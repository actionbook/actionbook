use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chromiumoxide::browser::Browser;
use chromiumoxide::handler::Handler;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::launcher::BrowserLauncher;
use super::stealth::StealthProfile;
use crate::config::{Config, ProfileConfig};
use crate::error::{ActionbookError, Result};

/// Page info from CDP /json/list endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub web_socket_debugger_url: Option<String>,
}

/// Session state persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionState {
    profile_name: String,
    cdp_port: u16,
    pid: Option<u32>,
    cdp_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_page_id: Option<String>,
    /// Path to custom application (for Electron apps launched via app launch)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    custom_app_path: Option<String>,
    /// Current frame ID for iframe context (None = main frame)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_frame_id: Option<String>,
    /// Optional HTTP headers to send during WebSocket handshake (e.g. SigV4 auth for AgentCore)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ws_headers: Option<std::collections::HashMap<String, String>>,
}

impl SessionState {
    /// Local CDP sessions expose devtools browser websocket on loopback
    /// and support localhost HTTP endpoints (/json/version, /json/list).
    ///
    /// Remote sessions (e.g. AgentCore wss://.../automation) must not use
    /// localhost HTTP fallback.
    fn uses_local_http_endpoints(&self) -> bool {
        let Some(host) = extract_ws_host(&self.cdp_url) else {
            return false;
        };

        let is_loopback = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
        is_loopback && self.cdp_url.contains("/devtools/browser/")
    }
}

/// Stealth configuration for session manager
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct StealthConfig {
    /// Whether stealth mode is enabled
    pub enabled: bool,
    /// Whether to run headless
    pub headless: bool,
    /// Stealth profile configuration
    pub profile: StealthProfile,
}

fn extract_ws_host(ws_url: &str) -> Option<String> {
    let authority = ws_url.split("://").nth(1)?.split('/').next()?;

    // Strip userinfo if present: user:pass@host:port
    let authority = authority.rsplit('@').next().unwrap_or(authority);

    // IPv6 in brackets: [::1]:9222
    if let Some(rest) = authority.strip_prefix('[') {
        let host = rest.split(']').next()?;
        return Some(host.to_ascii_lowercase());
    }

    // host:port or host
    Some(
        authority
            .split(':')
            .next()
            .unwrap_or(authority)
            .to_ascii_lowercase(),
    )
}

fn derive_page_ws_url(browser_ws_url: &str, target_id: &str) -> Option<String> {
    let marker = "/devtools/browser/";
    let idx = browser_ws_url.find(marker)?;
    let prefix = &browser_ws_url[..idx];
    Some(format!("{}/devtools/page/{}", prefix, target_id))
}

fn is_reusable_initial_blank_page_url(url: &str) -> bool {
    matches!(
        url,
        "" | "about:blank" | "chrome://newtab/" | "chrome://new-tab-page/"
    )
}

/// Sanitize a profile or session name to prevent path traversal.
/// Only allows alphanumeric characters, dashes, and underscores.
/// Returns "default" if the sanitized result is empty.
fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

fn format_page_load_timeout(timeout: Duration, url: &str) -> String {
    let timeout_text = if timeout.as_secs() >= 1 {
        let secs = timeout.as_secs();
        if secs == 1 {
            "1 second".to_string()
        } else {
            format!("{} seconds", secs)
        }
    } else {
        format!("{}ms", timeout.as_millis())
    };

    format!("Page load timed out after {}: {}", timeout_text, url)
}

/// Manages browser sessions across CLI invocations
pub struct SessionManager {
    config: Config,
    sessions_dir: PathBuf,
    stealth_config: Option<StealthConfig>,
    /// When true, CDP commands are routed through the per-profile daemon
    /// (persistent WS connection). Set by `--daemon` CLI flag.
    daemon_enabled: bool,
    /// CLI-selected profile override. When set, `resolve_profile_name(None)`
    /// returns this instead of the config default. This ensures that router
    /// methods (which pass `None`) use the profile the user actually requested.
    active_profile: Option<String>,
    /// Multi-session name. When set, session state is stored as
    /// `{profile}@{session}.json` instead of `{profile}.json`.
    /// Each session binds to its own tab independently.
    active_session: Option<String>,
}

impl SessionManager {
    pub fn new(config: Config) -> Self {
        Self::with_sessions_dir(config, Self::default_sessions_dir())
    }

    /// Create session manager with stealth configuration
    pub fn with_stealth(config: Config, stealth_config: StealthConfig) -> Self {
        Self {
            config,
            sessions_dir: Self::default_sessions_dir(),
            stealth_config: Some(stealth_config),
            daemon_enabled: false,
            active_profile: None,
            active_session: None,
        }
    }

    /// Create session manager with a custom sessions directory.
    ///
    /// This is primarily useful for tests and embedded callers that need
    /// isolated state instead of writing into `~/.actionbook/sessions`.
    pub fn with_sessions_dir(config: Config, sessions_dir: PathBuf) -> Self {
        Self {
            config,
            sessions_dir,
            stealth_config: None,
            daemon_enabled: false,
            active_profile: None,
            active_session: None,
        }
    }

    /// Set the active profile for this session manager.
    /// When set, all CDP commands route through this profile's session
    /// instead of the config default.
    pub fn set_active_profile(&mut self, profile: &str) {
        self.active_profile = Some(profile.to_string());
    }

    /// Enable daemon routing for CDP commands.
    /// When enabled, `send_cdp_command` routes through the per-profile daemon.
    pub fn set_daemon_enabled(&mut self, enabled: bool) {
        self.daemon_enabled = enabled;
    }

    /// Set the active session name for multi-session support.
    /// When set, session state is stored as `{profile}@{session}.json`.
    ///
    /// Session names are sanitized to prevent path traversal: only alphanumeric,
    /// dash, and underscore characters are allowed. Everything else is stripped.
    pub fn set_active_session(&mut self, session: &str) {
        self.active_session = Some(sanitize_name(session));
    }

    /// Get the active session name (None = "default").
    #[allow(dead_code)]
    pub fn active_session(&self) -> Option<&str> {
        self.active_session.as_deref()
    }

    fn default_sessions_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".actionbook")
            .join("sessions")
    }

    /// Check if stealth mode is enabled
    pub fn is_stealth_enabled(&self) -> bool {
        self.stealth_config
            .as_ref()
            .map(|c| c.enabled)
            .unwrap_or(false)
    }

    fn resolve_profile_name(&self, profile_name: Option<&str>) -> String {
        match profile_name.map(str::trim).filter(|s| !s.is_empty()) {
            Some(name) => name.to_string(),
            None => self
                .active_profile
                .clone()
                .unwrap_or_else(|| self.config.effective_default_profile_name()),
        }
    }

    /// Get the session state file path for a profile.
    ///
    /// With multi-session support, files are named `{profile}@{session}.json`.
    /// When `active_session` is None, uses `{profile}@default.json`.
    /// For backward compatibility, migrates legacy `{profile}.json` to the new path.
    fn session_file(&self, profile_name: &str) -> PathBuf {
        let session = self.active_session.as_deref().unwrap_or("default");
        let safe_profile = sanitize_name(profile_name);
        let safe_session = sanitize_name(session);
        let new_path = self
            .sessions_dir
            .join(format!("{}@{}.json", safe_profile, safe_session));

        // Migration: if the new-style file doesn't exist but the old-style does,
        // copy it to the new path (only for "default" session).
        if session == "default" && !new_path.exists() {
            for legacy_path in self.legacy_session_paths(profile_name) {
                if legacy_path.exists() {
                    if let Ok(content) = fs::read_to_string(&legacy_path) {
                        let _ = fs::write(&new_path, &content);
                        tracing::debug!(
                            "Migrated session file: {} → {}",
                            legacy_path.display(),
                            new_path.display()
                        );
                        break;
                    }
                }
            }
        }

        new_path
    }

    fn legacy_session_paths(&self, profile_name: &str) -> Vec<PathBuf> {
        let mut paths = vec![self.sessions_dir.join(format!("{}.json", profile_name))];
        let safe_profile = sanitize_name(profile_name);
        let safe_path = self.sessions_dir.join(format!("{}.json", safe_profile));
        if !paths.iter().any(|p| p == &safe_path) {
            paths.push(safe_path);
        }
        paths
    }

    /// Get the legacy session file path (pre-multi-session).
    /// Used for cleanup and backward compatibility checks.
    #[allow(dead_code)]
    fn legacy_session_file(&self, profile_name: &str) -> PathBuf {
        self.legacy_session_paths(profile_name)
            .into_iter()
            .next()
            .unwrap_or_else(|| self.sessions_dir.join(format!("{}.json", profile_name)))
    }

    /// Load session state from disk
    fn load_session_state(&self, profile_name: &str) -> Option<SessionState> {
        let path = self.session_file(profile_name);
        Self::load_session_state_from_path(&path)
    }

    /// Load session state as raw JSON Value (public, for callers that need
    /// session data without coupling to SessionState internals).
    /// Respects sanitization, multi-session naming, and legacy migration.
    pub fn load_session_json(&self, profile_name: &str) -> Option<serde_json::Value> {
        let path = self.session_file(profile_name);
        if path.exists() {
            let content = fs::read_to_string(&path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    fn load_session_state_from_path(path: &Path) -> Option<SessionState> {
        if path.exists() {
            let content = fs::read_to_string(path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    /// Save session state to disk
    fn save_session_state(&self, state: &SessionState) -> Result<()> {
        fs::create_dir_all(&self.sessions_dir)?;
        let path = self.session_file(&state.profile_name);
        let content = serde_json::to_string_pretty(state)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Check if the session for the given profile is a remote session with auth headers.
    /// Remote sessions use `ws_headers` and cannot be opened via chromiumoxide `Browser::connect`.
    ///
    /// For a brand-new named session (file doesn't exist yet), checks the first *alive*
    /// shareable session (the one `ensure_session_state_for_cdp` would actually fork from).
    /// This mirrors the fork logic exactly: skip stale candidates, pick the first alive one.
    pub async fn is_remote_session(&self, profile_name: Option<&str>) -> bool {
        let profile_name = self.resolve_profile_name(profile_name);

        fn has_ws_headers(state: &SessionState) -> bool {
            state.ws_headers.as_ref().is_some_and(|h| !h.is_empty())
        }

        // Check current session file first — but only trust it if the session is alive.
        // A stale remote session file would cause us to return true here, but
        // ensure_session_state_for_cdp would delete it and fork from a different (possibly
        // local) session, causing a mismatch.
        if let Some(state) = self.load_session_state(&profile_name) {
            if self.is_session_alive(&state).await {
                return has_ws_headers(&state);
            }
            // Stale — fall through to check shareable candidates
        }

        // Current session is stale or doesn't exist — find the first alive shareable session
        // (mirrors ensure_session_state_for_cdp's fork logic: skip stale, pick first alive)
        for candidate in self.find_shareable_session_states(&profile_name) {
            if self.is_session_alive(&candidate).await {
                return has_ws_headers(&candidate);
            }
        }

        false
    }

    /// Find shareable session states for the given profile, ordered by priority:
    /// 1. default session, 2. legacy file, 3. any other named session.
    /// Returns all candidates so callers can skip stale ones.
    fn find_shareable_session_states(&self, profile_name: &str) -> Vec<SessionState> {
        let safe_profile = sanitize_name(profile_name);
        let mut candidates = Vec::new();

        let default_path = self
            .sessions_dir
            .join(format!("{}@default.json", safe_profile));
        if let Some(state) = Self::load_session_state_from_path(&default_path) {
            candidates.push(state);
        }

        for legacy_path in self.legacy_session_paths(profile_name) {
            if let Some(state) = Self::load_session_state_from_path(&legacy_path) {
                // Avoid duplicate if legacy was already loaded as default
                if candidates.is_empty() {
                    candidates.push(state);
                }
                break;
            }
        }

        let prefix = format!("{}@", safe_profile);
        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(&prefix) && name_str.ends_with(".json") {
                    // Skip default — already checked above
                    if name_str.as_ref() == format!("{}@default.json", safe_profile) {
                        continue;
                    }
                    if let Some(state) = Self::load_session_state_from_path(&entry.path()) {
                        candidates.push(state);
                    }
                }
            }
        }

        candidates
    }

    async fn refresh_local_session_ws_url_if_needed(&self, state: &mut SessionState) -> Result<()> {
        if state.uses_local_http_endpoints() {
            if let Some(fresh_url) = self.fetch_browser_ws_url(state.cdp_port).await {
                if fresh_url != state.cdp_url {
                    tracing::debug!("CDP WebSocket URL changed, updating session");
                    state.cdp_url = fresh_url;
                    self.save_session_state(state)?;
                }
            }
        }

        Ok(())
    }

    async fn ensure_session_state_for_cdp(&self, profile_name: &str) -> Result<SessionState> {
        if let Some(mut state) = self.load_session_state(profile_name) {
            if self.is_session_alive(&state).await {
                self.refresh_local_session_ws_url_if_needed(&mut state)
                    .await?;
                return Ok(state);
            }

            tracing::debug!("Session for profile {} is dead, removing", profile_name);
            self.remove_session_state(profile_name)?;
        }

        if self
            .active_session
            .as_deref()
            .is_some_and(|s| s != "default")
        {
            for shared_state in self.find_shareable_session_states(profile_name) {
                if self.is_session_alive(&shared_state).await {
                    // Inherit the parent's active_page_id so the forked session
                    // targets the same tab, not an arbitrary first page from
                    // get_active_page_info's fallback.  Once the user runs
                    // `browser open -S <name>`, a dedicated tab is created and
                    // persisted, replacing this inherited value.
                    let forked_state = SessionState {
                        profile_name: profile_name.to_string(),
                        current_frame_id: None,
                        ..shared_state
                    };
                    self.save_session_state(&forked_state)?;
                    tracing::info!(
                        "Named session '{}' forked from shared session for profile '{}'",
                        self.active_session.as_deref().unwrap_or("?"),
                        profile_name
                    );
                    return Ok(forked_state);
                }
            }
        }

        Err(ActionbookError::BrowserNotRunning)
    }

    fn saved_session_state_for_reuse(&self, profile_name: &str) -> Option<SessionState> {
        if let Some(state) = self.load_session_state(profile_name) {
            return Some(state);
        }

        if self
            .active_session
            .as_deref()
            .is_some_and(|session| session != "default")
        {
            return self
                .find_shareable_session_states(profile_name)
                .into_iter()
                .next();
        }

        None
    }

    fn initial_blank_launch_artifact_id(
        state: &SessionState,
        pages: &[PageInfo],
    ) -> Option<String> {
        if state.active_page_id.is_some() || pages.len() != 1 {
            return None;
        }

        pages
            .first()
            .filter(|page| is_reusable_initial_blank_page_url(&page.url))
            .map(|page| page.id.clone())
    }

    /// Remove session state from disk
    fn remove_session_state(&self, profile_name: &str) -> Result<()> {
        let path = self.session_file(profile_name);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Save session state for an externally connected browser (via `connect` command)
    pub fn save_external_session(
        &self,
        profile_name: &str,
        cdp_port: u16,
        cdp_url: &str,
    ) -> Result<()> {
        self.save_external_session_full(profile_name, cdp_port, cdp_url, None, None)
    }

    /// Save session state for an externally connected app with optional custom app path
    pub fn save_external_session_with_app(
        &self,
        profile_name: &str,
        cdp_port: u16,
        cdp_url: &str,
        custom_app_path: Option<String>,
    ) -> Result<()> {
        self.save_external_session_full(profile_name, cdp_port, cdp_url, custom_app_path, None)
    }

    /// Save session state with all optional fields
    pub fn save_external_session_full(
        &self,
        profile_name: &str,
        cdp_port: u16,
        cdp_url: &str,
        custom_app_path: Option<String>,
        ws_headers: Option<std::collections::HashMap<String, String>>,
    ) -> Result<()> {
        let state = SessionState {
            profile_name: profile_name.to_string(),
            cdp_port,
            pid: None,
            cdp_url: cdp_url.to_string(),
            active_page_id: None,
            custom_app_path,
            current_frame_id: None,
            ws_headers,
        };
        self.save_session_state(&state)
    }

    /// Check if a session is still alive
    async fn is_session_alive(&self, state: &SessionState) -> bool {
        if state.uses_local_http_endpoints() {
            // Local CDP mode: use localhost probe
            let url = format!("http://127.0.0.1:{}/json/version", state.cdp_port);
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            client.get(&url).send().await.is_ok()
        } else {
            // Remote WS/WSS mode: probe via websocket handshake with auth headers
            self.is_websocket_alive(&state.cdp_url, state.ws_headers.as_ref())
                .await
        }
    }

    /// Check if the saved session for a profile is still reachable.
    ///
    /// Uses `is_session_alive` which checks HTTP for local sessions (tolerates
    /// browser WS URL rotation) and WS for remote sessions.
    pub async fn is_session_reachable(&self, profile_name: &str) -> bool {
        match self.load_session_state(profile_name) {
            Some(state) => self.is_session_alive(&state).await,
            None => false,
        }
    }

    /// Whether there is saved session state for the current session, or a shareable
    /// parent session when running a named session that has not been materialized yet.
    pub fn has_saved_session_state(&self, profile_name: Option<&str>) -> bool {
        let profile_name = self.resolve_profile_name(profile_name);
        self.saved_session_state_for_reuse(&profile_name).is_some()
    }

    /// Whether the saved session for a profile uses a remote browser websocket.
    pub fn session_uses_remote_ws(&self, profile_name: Option<&str>) -> bool {
        let profile_name = self.resolve_profile_name(profile_name);
        self.saved_session_state_for_reuse(&profile_name)
            .is_some_and(|state| !state.uses_local_http_endpoints())
    }

    /// Probe a WebSocket endpoint to check if it's reachable and auth succeeds.
    /// Returns true on successful handshake, false on any error or timeout.
    pub async fn is_websocket_reachable(
        &self,
        ws_url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> bool {
        self.is_websocket_alive(ws_url, headers).await
    }

    async fn is_websocket_alive(
        &self,
        ws_url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> bool {
        match tokio::time::timeout(
            Duration::from_secs(5),
            Self::connect_ws_with_headers(ws_url, headers),
        )
        .await
        {
            Ok(Ok(mut ws)) => {
                let _ = ws.close(None).await;
                true
            }
            _ => false,
        }
    }

    /// Connect to a WebSocket URL with optional authentication headers from session state.
    async fn connect_ws_with_headers(
        ws_url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> std::result::Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        ActionbookError,
    > {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = ws_url.into_client_request().map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Bad WebSocket URL: {}", e))
        })?;

        if let Some(hdrs) = headers.filter(|h| !h.is_empty()) {
            for (key, value) in hdrs {
                request.headers_mut().insert(
                    tokio_tungstenite::tungstenite::http::HeaderName::try_from(key.as_str())
                        .map_err(|e| {
                            ActionbookError::CdpConnectionFailed(format!("Bad header name: {}", e))
                        })?,
                    tokio_tungstenite::tungstenite::http::HeaderValue::from_str(value).map_err(
                        |e| {
                            ActionbookError::CdpConnectionFailed(format!("Bad header value: {}", e))
                        },
                    )?,
                );
            }
        }

        let (ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| {
                ActionbookError::CdpConnectionFailed(format!("WebSocket connection failed: {}", e))
            })?;
        Ok(ws)
    }

    /// Fetch the current browser WebSocket URL from a CDP port via /json/version.
    /// Returns `None` if the port is unreachable or the response is malformed.
    async fn fetch_browser_ws_url(&self, cdp_port: u16) -> Option<String> {
        let url = format!("http://127.0.0.1:{}/json/version", cdp_port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let resp = client.get(&url).send().await.ok()?;
        let info: serde_json::Value = resp.json().await.ok()?;
        info.get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Refresh the saved WS URL for a local session by querying /json/version.
    /// No-op if the session is remote or if the HTTP probe fails.
    pub async fn refresh_session_ws_url(&self, profile_name: &str) {
        let Some(mut state) = self.load_session_state(profile_name) else {
            return;
        };
        if !state.uses_local_http_endpoints() {
            return;
        }
        if let Some(fresh_url) = self.fetch_browser_ws_url(state.cdp_port).await {
            if fresh_url != state.cdp_url {
                tracing::debug!(
                    "Refreshed WS URL for '{}': {} → {}",
                    profile_name,
                    state.cdp_url,
                    fresh_url
                );
                state.cdp_url = fresh_url;
                let _ = self.save_session_state(&state);
            }
        }
    }

    /// Get or create a browser session for the given profile
    pub async fn get_or_create_session(
        &self,
        profile_name: Option<&str>,
    ) -> Result<(Browser, Handler)> {
        let profile_name = self.resolve_profile_name(profile_name);

        // Check for existing session state
        if let Some(mut state) = self.load_session_state(&profile_name) {
            if self.is_session_alive(&state).await {
                self.refresh_local_session_ws_url_if_needed(&mut state)
                    .await?;
                tracing::debug!("Reusing existing session for profile: {}", profile_name);
                return self.connect_to_session(&state).await;
            } else {
                tracing::debug!("Session for profile {} is dead, removing", profile_name);
                self.remove_session_state(&profile_name)?;
            }
        }

        // Multi-session: if this is a named session (not "default") and the default
        // session's browser is already running, reuse it instead of launching a new Chrome.
        // Chrome only allows one instance per user-data-dir, so named sessions must share
        // the same browser process and use separate tabs.
        if self
            .active_session
            .as_deref()
            .is_some_and(|s| s != "default")
        {
            for shared_state in self.find_shareable_session_states(&profile_name) {
                if self.is_session_alive(&shared_state).await {
                    // Clone the shared session's CDP connection.  Inherit
                    // active_page_id so that commands before `browser open`
                    // target the same tab as the parent (not an arbitrary
                    // first page from get_active_page_info's fallback).
                    // The daemon will still create a dedicated tab via
                    // lazy_attach_session when it first routes a command
                    // for this session.
                    let forked_state = SessionState {
                        profile_name: profile_name.to_string(),
                        current_frame_id: None,
                        ..shared_state
                    };
                    self.save_session_state(&forked_state)?;
                    tracing::info!(
                        "Named session '{}' forked from shared session (sharing browser at {})",
                        self.active_session.as_deref().unwrap_or("?"),
                        forked_state.cdp_url
                    );
                    return self.connect_to_session(&forked_state).await;
                }
            }
        }

        // No existing browser found, create new session
        tracing::debug!(
            "No existing browser found, creating new session for profile: {}",
            profile_name
        );
        let profile = self.config.get_profile(&profile_name)?;
        self.create_session(&profile_name, &profile).await
    }

    /// Create a new browser session
    async fn create_session(
        &self,
        profile_name: &str,
        profile: &ProfileConfig,
    ) -> Result<(Browser, Handler)> {
        let stealth_enabled = self.is_stealth_enabled();

        let mut launcher =
            BrowserLauncher::from_profile(profile_name, profile)?.with_stealth(stealth_enabled);

        let (_child, cdp_url) = launcher.launch_and_wait().await?;

        // Save session state
        let state = SessionState {
            profile_name: profile_name.to_string(),
            cdp_port: launcher.get_cdp_port(),
            pid: None, // TODO: get actual PID
            cdp_url: cdp_url.clone(),
            active_page_id: None,
            custom_app_path: None,
            current_frame_id: None,
            ws_headers: None,
        };
        self.save_session_state(&state)?;

        // Also write a "default" session file when a named session creates the browser.
        // This lets other named sessions fork from it without launching a second Chrome.
        if self
            .active_session
            .as_deref()
            .is_some_and(|s| s != "default")
        {
            let safe_profile = sanitize_name(profile_name);
            let default_path = self
                .sessions_dir
                .join(format!("{}@default.json", safe_profile));
            if !default_path.exists() {
                let _ = fs::create_dir_all(&self.sessions_dir);
                let _ = fs::write(
                    &default_path,
                    serde_json::to_string_pretty(&state).unwrap_or_default(),
                );
                tracing::debug!(
                    "Created default session file for profile '{}' so other sessions can fork",
                    profile_name
                );
            }
        }

        // Connect to the browser
        let result = self.connect_to_session(&state).await?;

        // Always apply stealth JS overrides
        self.apply_stealth_js(&state).await;

        Ok(result)
    }

    /// Launch a custom app (Electron/etc.) and connect to its CDP endpoint
    pub async fn launch_custom_app(
        &self,
        profile_name: &str,
        executable_path: &str,
        extra_args: Vec<String>,
        port: Option<u16>,
    ) -> Result<(Browser, Handler)> {
        use std::process::{Command, Stdio};

        // Determine CDP port
        let cdp_port = port.unwrap_or_else(|| {
            // Find a free port if not specified
            if super::launcher::is_port_available(9222) {
                9222
            } else {
                super::launcher::find_free_port().unwrap_or(9223)
            }
        });

        // Build command with CDP port
        let mut args = vec![format!("--remote-debugging-port={}", cdp_port)];
        args.extend(extra_args);

        tracing::info!(
            "Launching custom app: {} with args: {:?}",
            executable_path,
            args
        );

        // Spawn the process
        let _child = Command::new(executable_path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                ActionbookError::BrowserLaunchFailed(format!(
                    "Failed to launch custom app {}: {}",
                    executable_path, e
                ))
            })?;

        // Wait for CDP to be ready
        let cdp_url = self.wait_for_cdp_ready(cdp_port).await?;

        // Save session state
        let state = SessionState {
            profile_name: profile_name.to_string(),
            cdp_port,
            pid: None,
            cdp_url: cdp_url.clone(),
            active_page_id: None,
            custom_app_path: Some(executable_path.to_string()),
            current_frame_id: None,
            ws_headers: None,
        };
        self.save_session_state(&state)?;

        // Connect to the app
        let result = self.connect_to_session(&state).await?;

        // Apply stealth JS overrides
        self.apply_stealth_js(&state).await;

        Ok(result)
    }

    /// Wait for CDP endpoint to become available (reused from launcher pattern)
    async fn wait_for_cdp_ready(&self, cdp_port: u16) -> Result<String> {
        use tokio::time::sleep;

        let url = format!("http://127.0.0.1:{}/json/version", cdp_port);

        // Build client with NO_PROXY for localhost
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Try for up to 10 seconds
        for i in 0..20 {
            sleep(Duration::from_millis(500)).await;

            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    let json: serde_json::Value = response.json().await.map_err(|e| {
                        ActionbookError::CdpConnectionFailed(format!(
                            "Failed to parse CDP response: {}",
                            e
                        ))
                    })?;

                    if let Some(ws_url) = json.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                    {
                        tracing::info!("CDP ready at: {}", ws_url);
                        return Ok(ws_url.to_string());
                    }
                }
                Ok(_) => {
                    tracing::debug!("CDP not ready yet (attempt {})", i + 1);
                }
                Err(e) => {
                    tracing::debug!("CDP connection attempt {} failed: {}", i + 1, e);
                }
            }
        }

        Err(ActionbookError::CdpConnectionFailed(
            "Timeout waiting for CDP to be ready".to_string(),
        ))
    }

    /// Apply stealth JavaScript overrides to the browser via CDP
    async fn apply_stealth_js(&self, state: &SessionState) {
        // Inject stealth JS on all existing pages
        let js = r#"
            // Remove webdriver flag
            Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
            delete navigator.__proto__.webdriver;

            // Fix chrome runtime
            window.chrome = { runtime: {} };

            // Override permissions query
            const originalQuery = window.navigator.permissions.query;
            window.navigator.permissions.query = (parameters) => (
                parameters.name === 'notifications'
                    ? Promise.resolve({ state: Notification.permission })
                    : originalQuery(parameters)
            );

            // Add realistic plugins
            Object.defineProperty(navigator, 'plugins', {
                get: () => [
                    { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer' },
                    { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai' },
                    { name: 'Native Client', filename: 'internal-nacl-plugin' }
                ]
            });

            // Fix languages
            Object.defineProperty(navigator, 'languages', {
                get: () => ['en-US', 'en']
            });
        "#;

        // Inject existing pages only when local /json/list is available.
        // Remote ws/wss sessions must not fallback to localhost HTTP.
        if state.uses_local_http_endpoints() {
            let pages_url = format!("http://127.0.0.1:{}/json/list", state.cdp_port);
            let client = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());

            if let Ok(response) = client.get(&pages_url).send().await {
                if let Ok(pages) = response.json::<Vec<PageInfo>>().await {
                    for page in pages.iter().filter(|p| p.page_type == "page") {
                        if let Some(ref ws_url) = page.web_socket_debugger_url {
                            if let Err(e) = self.inject_stealth_to_page(ws_url, js).await {
                                tracing::debug!(
                                    "Failed to inject stealth to page {}: {}",
                                    page.id,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        // Also inject via browser-level CDP for new pages
        let browser_ws_url = &state.cdp_url;
        let _ = self.inject_new_document_script(browser_ws_url, js).await;

        tracing::info!("Stealth JS overrides applied");
    }

    #[cfg(feature = "stealth")]
    pub(crate) async fn apply_stealth_to_active_page(
        &self,
        profile_name: Option<&str>,
    ) -> Result<()> {
        let Some(profile) = self.get_stealth_profile() else {
            return Ok(());
        };

        let script = super::stealth::build_stealth_script(profile);

        self.send_cdp_command(
            profile_name,
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({ "source": script.clone() }),
        )
        .await?;

        self.send_cdp_command(
            profile_name,
            "Runtime.evaluate",
            serde_json::json!({
                "expression": script,
                "returnByValue": true
            }),
        )
        .await?;

        Ok(())
    }

    /// Inject stealth JS into a specific page via its WebSocket URL
    async fn inject_stealth_to_page(&self, ws_url: &str, js: &str) -> Result<()> {
        use futures::SinkExt;
        use tokio_tungstenite::connect_async;

        let (mut ws, _) = connect_async(ws_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("WebSocket failed: {}", e))
        })?;

        let cmd = serde_json::json!({
            "id": 1,
            "method": "Runtime.evaluate",
            "params": { "expression": js }
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Send failed: {}", e)))?;

        Ok(())
    }

    /// Register stealth JS to run on every new document (page/navigation)
    async fn inject_new_document_script(&self, browser_ws_url: &str, js: &str) -> Result<()> {
        use futures::SinkExt;
        use tokio_tungstenite::connect_async;

        let (mut ws, _) = connect_async(browser_ws_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Browser WS failed: {}", e))
        })?;

        // Page.addScriptToEvaluateOnNewDocument ensures stealth runs on every new page
        let cmd = serde_json::json!({
            "id": 1,
            "method": "Page.addScriptToEvaluateOnNewDocument",
            "params": { "source": js }
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Send failed: {}", e)))?;

        Ok(())
    }

    /// Get the stealth profile if enabled
    #[cfg(feature = "stealth")]
    pub fn get_stealth_profile(&self) -> Option<&StealthProfile> {
        self.stealth_config
            .as_ref()
            .filter(|c| c.enabled)
            .map(|c| &c.profile)
    }

    /// Connect to an existing browser session.
    ///
    /// Note: chromiumoxide `Browser::connect` does not support custom WS headers.
    /// For remote sessions with auth headers, this will fail — callers should use
    /// CDP commands via `send_cdp_command` instead.
    async fn connect_to_session(&self, state: &SessionState) -> Result<(Browser, Handler)> {
        if state.ws_headers.as_ref().is_some_and(|h| !h.is_empty()) {
            return Err(ActionbookError::CdpConnectionFailed(
                "Cannot use chromiumoxide for remote sessions with auth headers. \
                 Use CDP commands (goto, eval, click) instead of open/restart."
                    .to_string(),
            ));
        }

        let (browser, handler) = Browser::connect(&state.cdp_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Failed to connect to browser: {}", e))
        })?;

        Ok((browser, handler))
    }

    /// Close a browser session
    pub async fn close_session(&self, profile_name: Option<&str>) -> Result<()> {
        let profile_name = self.resolve_profile_name(profile_name);

        if let Some(state) = self.load_session_state(&profile_name) {
            // For remote sessions with auth headers, use Browser.close via CDP
            if state.ws_headers.as_ref().is_some_and(|h| !h.is_empty()) {
                let close_result = self
                    .send_cdp_command(Some(&profile_name), "Browser.close", serde_json::json!({}))
                    .await;
                match close_result {
                    Ok(_) => {
                        self.remove_session_state(&profile_name)?;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to close remote browser: {}", e);
                        // Don't delete session files — browser may still be running
                        return Err(ActionbookError::CdpConnectionFailed(format!(
                            "Failed to close remote browser: {}",
                            e
                        )));
                    }
                }
                return Ok(());
            }

            // Local/no-headers path: use chromiumoxide
            let connected = self.connect_to_session(&state).await;
            match connected {
                Ok((mut browser, mut handler)) => {
                    tokio::spawn(async move { while handler.next().await.is_some() {} });
                    let _ = browser.close().await;
                    self.remove_session_state(&profile_name)?;
                }
                Err(e) if !state.uses_local_http_endpoints() => {
                    // Remote session without headers: connection failed, don't
                    // delete session — browser may still be running remotely
                    tracing::warn!(
                        "Cannot connect to remote browser for close, keeping session state"
                    );
                    return Err(ActionbookError::CdpConnectionFailed(format!(
                        "Failed to close remote browser: {}",
                        e
                    )));
                }
                Err(_) => {
                    // Local session that's unreachable — safe to clean up
                    self.remove_session_state(&profile_name)?;
                }
            }
        }

        Ok(())
    }

    /// Get list of pages from the browser
    pub async fn get_pages(&self, profile_name: Option<&str>) -> Result<Vec<PageInfo>> {
        let profile_name = self.resolve_profile_name(profile_name);
        let state = self.ensure_session_state_for_cdp(&profile_name).await?;

        if state.uses_local_http_endpoints() {
            let url = format!("http://127.0.0.1:{}/json/list", state.cdp_port);
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());

            // Try HTTP first, but fall back to WS Target.getTargets if HTTP
            // fails (Chrome M146+ chrome://inspect mode has no HTTP API).
            match client.get(&url).send().await {
                Ok(response) => match response.json::<Vec<PageInfo>>().await {
                    Ok(pages) => {
                        return Ok(pages
                            .into_iter()
                            .filter(|p| p.page_type == "page")
                            .collect());
                    }
                    Err(e) => {
                        tracing::debug!("HTTP /json/list parse failed, falling back to WS: {}", e);
                    }
                },
                Err(e) => {
                    tracing::debug!("HTTP /json/list failed, falling back to WS: {}", e);
                }
            }
        }

        self.get_pages_via_ws_targets(&state.cdp_url, state.ws_headers.as_ref())
            .await
    }

    async fn get_pages_via_ws_targets(
        &self,
        browser_ws_url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<Vec<PageInfo>> {
        let mut ws = Self::connect_ws_with_headers(browser_ws_url, headers).await?;

        let cmd = serde_json::json!({
            "id": 1,
            "method": "Target.getTargets",
            "params": {}
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send CDP command: {}", e)))?;

        while let Some(msg) = ws.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                    if response.get("id") == Some(&serde_json::json!(1)) {
                        if let Some(error) = response.get("error") {
                            return Err(ActionbookError::CdpConnectionFailed(format!(
                                "Target.getTargets failed: {}",
                                error
                            )));
                        }

                        let pages = response
                            .get("result")
                            .and_then(|r| r.get("targetInfos"))
                            .and_then(|t| t.as_array())
                            .map(|targets| {
                                targets
                                    .iter()
                                    .filter(|t| {
                                        t.get("type").and_then(|v| v.as_str()) == Some("page")
                                    })
                                    .filter_map(|target| {
                                        let id = target
                                            .get("targetId")
                                            .and_then(|v| v.as_str())?
                                            .to_string();
                                        let title = target
                                            .get("title")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let url = target
                                            .get("url")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("about:blank")
                                            .to_string();

                                        Some(PageInfo {
                                            id: id.clone(),
                                            title,
                                            url,
                                            page_type: "page".to_string(),
                                            web_socket_debugger_url: derive_page_ws_url(
                                                browser_ws_url,
                                                &id,
                                            ),
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();

                        return Ok(pages);
                    }
                }
                Ok(_) => continue,
                Err(e) => {
                    return Err(ActionbookError::CdpConnectionFailed(format!(
                        "WebSocket error while reading targets: {}",
                        e
                    )));
                }
            }
        }

        Err(ActionbookError::CdpConnectionFailed(
            "No response received for Target.getTargets".to_string(),
        ))
    }

    /// Get the active page info (first page in the list)
    pub async fn get_active_page_info(&self, profile_name: Option<&str>) -> Result<PageInfo> {
        let profile_name = self.resolve_profile_name(profile_name);
        let pages = self.get_pages(Some(&profile_name)).await?;

        // Try to get persisted active page
        if let Some(state) = self.load_session_state(&profile_name) {
            if let Some(active_id) = state.active_page_id {
                if let Some(page) = pages.iter().find(|p| p.id == active_id) {
                    return Ok(page.clone());
                }
            }
        }

        // Fallback to first page
        pages
            .into_iter()
            .next()
            .ok_or(ActionbookError::BrowserNotRunning)
    }

    /// Switch to a specific page by ID and persist the active page
    pub async fn switch_to_page(
        &self,
        profile_name: Option<&str>,
        page_id: &str,
    ) -> Result<PageInfo> {
        let profile_name = self.resolve_profile_name(profile_name);

        // Validate page exists
        let pages = self.get_pages(Some(&profile_name)).await?;
        let target_page = pages
            .iter()
            .find(|p| p.id == page_id)
            .ok_or_else(|| ActionbookError::PageNotFound(page_id.to_string()))?
            .clone();

        // Update session state with new active page ID
        let mut state = self
            .load_session_state(&profile_name)
            .ok_or(ActionbookError::BrowserNotRunning)?;
        state.active_page_id = Some(page_id.to_string());
        self.save_session_state(&state)?;

        Ok(target_page)
    }

    /// Create a new page/tab (or window if `new_window` is true) in the browser
    pub async fn new_page(
        &self,
        profile_name: Option<&str>,
        url: Option<&str>,
        new_window: bool,
    ) -> Result<PageInfo> {
        self.new_page_with_timeout(profile_name, url, new_window, Duration::from_secs(30))
            .await
    }

    async fn new_page_with_timeout(
        &self,
        profile_name: Option<&str>,
        url: Option<&str>,
        new_window: bool,
        create_target_timeout: Duration,
    ) -> Result<PageInfo> {
        let profile_name = self.resolve_profile_name(profile_name);
        let page_url = url.unwrap_or("about:blank");
        let state = self.ensure_session_state_for_cdp(&profile_name).await?;

        let sole_blank_page_id = if new_window {
            self.get_pages(Some(&profile_name))
                .await
                .ok()
                .and_then(|pages| Self::initial_blank_launch_artifact_id(&state, &pages))
        } else {
            None
        };

        match tokio::time::timeout(create_target_timeout, async {
            let mut params = serde_json::json!({
                "url": page_url
            });
            if new_window {
                params["newWindow"] = serde_json::json!(true);
            }

            let result = self
                .send_browser_command(Some(&profile_name), "Target.createTarget", params)
                .await?;
            let target_id = result
                .get("targetId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ActionbookError::CdpError("No targetId in response".to_string()))?;

            tokio::time::sleep(Duration::from_millis(500)).await;

            let pages = self.get_pages(Some(&profile_name)).await?;
            let mut new_page = pages
                .iter()
                .find(|p| p.id == target_id)
                .ok_or_else(|| ActionbookError::PageNotFound(target_id.to_string()))?
                .clone();

            self.switch_to_page(Some(&profile_name), &new_page.id)
                .await?;

            #[cfg(feature = "stealth")]
            if self.is_stealth_enabled() {
                if let Err(e) = self.apply_stealth_to_active_page(Some(&profile_name)).await {
                    tracing::warn!("Failed to apply stealth profile: {}", e);
                } else {
                    tracing::info!("Applied stealth profile to page");
                }
            }

            // If a real URL was requested, wait for navigation to complete so the
            // caller gets a meaningful title instead of an empty string.
            if url.is_some_and(|u| u != "about:blank") {
                let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
                while tokio::time::Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let pages = self.get_pages(Some(&profile_name)).await?;
                    if let Some(p) = pages.iter().find(|p| p.id == target_id) {
                        if !p.title.is_empty() && p.title != "about:blank" {
                            new_page = p.clone();
                            break;
                        }
                    }
                }
            }

            if let Some(blank_id) = sole_blank_page_id {
                let _ = self.close_page(Some(&profile_name), &blank_id).await;
            }

            Ok(new_page)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ActionbookError::Timeout(format_page_load_timeout(
                create_target_timeout,
                page_url,
            ))),
        }
    }

    /// Close a specific page/tab
    pub async fn close_page(&self, profile_name: Option<&str>, page_id: &str) -> Result<()> {
        let profile_name = self.resolve_profile_name(profile_name);

        // Validate page exists
        let pages = self.get_pages(Some(&profile_name)).await?;
        if !pages.iter().any(|p| p.id == page_id) {
            return Err(ActionbookError::PageNotFound(page_id.to_string()));
        }

        // Cannot close last page
        if pages.len() == 1 {
            return Err(ActionbookError::InvalidOperation(
                "Cannot close the last tab. Use 'browser close' to close the browser.".to_string(),
            ));
        }

        // Send CDP command Target.closeTarget
        let params = serde_json::json!({ "targetId": page_id });
        self.send_cdp_command(Some(&profile_name), "Target.closeTarget", params)
            .await?;

        // If we closed the active page, switch to first remaining page
        if let Some(state) = self.load_session_state(&profile_name) {
            if state.active_page_id.as_ref() == Some(&page_id.to_string()) {
                let remaining_pages = self.get_pages(Some(&profile_name)).await?;
                if let Some(first_page) = remaining_pages.first() {
                    self.switch_to_page(Some(&profile_name), &first_page.id)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Execute JavaScript on the active page using direct CDP via WebSocket
    pub async fn eval_on_page(
        &self,
        profile_name: Option<&str>,
        expression: &str,
    ) -> Result<serde_json::Value> {
        use futures::SinkExt;
        use tokio_tungstenite::connect_async;

        // When daemon is enabled, route through send_cdp_command so the
        // daemon's persistent WS + Target.attachToTarget session is used,
        // rather than opening a one-off page WebSocket.
        if self.daemon_enabled {
            // Handle iframe context: get execution context via createIsolatedWorld
            let frame_id = self.get_current_frame_id(profile_name);
            let mut params = serde_json::json!({
                "expression": expression,
                "returnByValue": true
            });

            if let Some(fid) = &frame_id {
                let world_result = self
                    .send_cdp_command(
                        profile_name,
                        "Page.createIsolatedWorld",
                        serde_json::json!({ "frameId": fid }),
                    )
                    .await;

                if let Ok(world) = world_result {
                    if let Some(ctx_id) = world.get("executionContextId").and_then(|c| c.as_i64()) {
                        params
                            .as_object_mut()
                            .unwrap()
                            .insert("contextId".to_string(), serde_json::json!(ctx_id));
                    }
                }
            }

            let result = self
                .send_cdp_command(profile_name, "Runtime.evaluate", params)
                .await?;

            let value = result
                .get("result")
                .and_then(|r| r.get("value"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            return Ok(value);
        }

        let page_info = self.get_active_page_info(profile_name).await?;
        let Some(ws_url) = page_info.web_socket_debugger_url.as_deref() else {
            let result = self
                .send_cdp_command(
                    profile_name,
                    "Runtime.evaluate",
                    serde_json::json!({
                        "expression": expression,
                        "returnByValue": true
                    }),
                )
                .await?;

            let value = result
                .get("result")
                .and_then(|r| r.get("value"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            return Ok(value);
        };

        // Connect to page WebSocket
        let (mut ws, _) = connect_async(ws_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("WebSocket connection failed: {}", e))
        })?;

        // Check if we need to evaluate in a specific frame
        let frame_id = self.get_current_frame_id(profile_name);
        let mut execution_context_id: Option<i64> = None;

        if let Some(fid) = &frame_id {
            // Create isolated world in the target frame to get execution context
            let create_world_cmd = serde_json::json!({
                "id": 1,
                "method": "Page.createIsolatedWorld",
                "params": {
                    "frameId": fid
                }
            });

            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                create_world_cmd.to_string().into(),
            ))
            .await
            .map_err(|e| ActionbookError::Other(format!("Failed to send command: {}", e)))?;

            // Read response to get execution context ID
            use futures::stream::StreamExt;
            while let Some(msg) = ws.next().await {
                if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                    let response: serde_json::Value = serde_json::from_str(&text)?;
                    if response.get("id") == Some(&serde_json::json!(1)) {
                        if let Some(ctx_id) = response
                            .get("result")
                            .and_then(|r| r.get("executionContextId"))
                            .and_then(|c| c.as_i64())
                        {
                            execution_context_id = Some(ctx_id);
                        }
                        break;
                    }
                }
            }
        }

        // Send Runtime.evaluate command with optional contextId
        let mut params = serde_json::json!({
            "expression": expression,
            "returnByValue": true
        });

        if let Some(ctx_id) = execution_context_id {
            params
                .as_object_mut()
                .unwrap()
                .insert("contextId".to_string(), serde_json::json!(ctx_id));
        }

        let cmd = serde_json::json!({
            "id": 2,
            "method": "Runtime.evaluate",
            "params": params
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send command: {}", e)))?;

        // Read response
        use futures::stream::StreamExt;
        while let Some(msg) = ws.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                    if response.get("id") == Some(&serde_json::json!(2)) {
                        if let Some(result) = response.get("result").and_then(|r| r.get("result")) {
                            if let Some(value) = result.get("value") {
                                return Ok(value.clone());
                            }
                            // Return the whole result if no value
                            return Ok(result.clone());
                        }
                        if let Some(error) = response.get("error") {
                            return Err(ActionbookError::JavaScriptError(error.to_string()));
                        }
                        return Ok(serde_json::Value::Null);
                    }
                }
                Ok(_) => continue,
                Err(e) => return Err(ActionbookError::Other(format!("WebSocket error: {}", e))),
            }
        }

        Err(ActionbookError::Other("No response received".to_string()))
    }

    /// Helper to send a CDP command and get response (public for snapshot/blocking)
    ///
    /// When a daemon is running for this profile (indicated by an active UDS socket),
    /// commands are routed through the daemon's persistent WS connection instead of
    /// opening a new WS per command.
    pub async fn send_cdp_command(
        &self,
        profile_name: Option<&str>,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let resolved_profile = self.resolve_profile_name(profile_name);

        // Try routing through daemon if its socket exists
        #[cfg(unix)]
        {
            if let Some(result) = self
                .try_send_via_daemon(&resolved_profile, method, params.clone())
                .await
            {
                return result;
            }
        }

        let state = self.ensure_session_state_for_cdp(&resolved_profile).await?;

        let page_info = self.get_active_page_info(Some(&resolved_profile)).await?;

        if let Some(ws_url) = page_info.web_socket_debugger_url.as_deref() {
            // Only use direct page WS for local loopback sessions where page-level
            // ws URLs are directly connectable without auth headers.
            // For ALL remote endpoints (with or without headers), use the
            // attach-target path through the browser WS, because:
            // - Remote page WS URLs may be synthesized and not directly accessible
            // - Some remote services only expose the browser WS endpoint
            // - Auth headers (if any) need to be sent on the browser WS handshake
            if state.uses_local_http_endpoints() {
                return self
                    .send_cdp_command_over_page_ws(ws_url, method, params)
                    .await;
            }
        }

        // Remote ws/wss endpoints: use browser websocket + Target.attachToTarget(sessionId)
        // with optional auth headers.
        self.send_cdp_command_via_attached_target(
            &state.cdp_url,
            &page_info.id,
            method,
            params,
            state.ws_headers.as_ref(),
        )
        .await
    }

    async fn send_browser_command(
        &self,
        profile_name: Option<&str>,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let resolved_profile = self.resolve_profile_name(profile_name);

        #[cfg(unix)]
        {
            if let Some(result) = self
                .try_send_via_daemon(&resolved_profile, method, params.clone())
                .await
            {
                return result;
            }
        }

        let mut state = self
            .load_session_state(&resolved_profile)
            .ok_or(ActionbookError::BrowserNotRunning)?;

        if state.uses_local_http_endpoints() {
            if let Some(fresh_url) = self.fetch_browser_ws_url(state.cdp_port).await {
                if fresh_url != state.cdp_url {
                    tracing::debug!(
                        "Refreshed browser WS URL for browser-level command: {} -> {}",
                        state.cdp_url,
                        fresh_url
                    );
                    state.cdp_url = fresh_url;
                    self.save_session_state(&state)?;
                }
            }
        }

        self.send_browser_command_over_ws(&state.cdp_url, method, params, state.ws_headers.as_ref())
            .await
    }

    /// Try to send a CDP command through the daemon for this profile.
    ///
    /// Only attempts daemon routing when `daemon_enabled` is true AND the daemon
    /// socket exists. Returns `Some(Ok(value))` on success, `None` on any failure
    /// (falls through to direct WS path). This ensures the daemon layer never
    /// blocks commands that would succeed via the direct path.
    #[cfg(unix)]
    async fn try_send_via_daemon(
        &self,
        profile: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Option<Result<serde_json::Value>> {
        if !self.daemon_enabled {
            return None;
        }

        // Always check the profile-level socket — the daemon is per-profile,
        // not per-session. Session routing happens inside the daemon via the
        // request's session field.
        let sock_path = crate::daemon::lifecycle::socket_path(profile);
        if !sock_path.exists() {
            return None;
        }

        // Retry loop: the daemon socket may be up before it has attached to a
        // browser target (WS connect + resolve target + attach takes time).
        // "No target attached" means the command was never forwarded — safe to retry.
        // Budget: 15 × 300ms = 4.5s, aligned with ensure_daemon's 5s startup timeout.
        let max_retries = 14;
        let mut last_err = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }

            let client = crate::daemon::client::DaemonClient::with_session(
                profile.to_string(),
                self.active_session.clone(),
            );
            match client.send_cdp(method, params.clone()).await {
                Ok(value) => return Some(Ok(value)),
                Err(ActionbookError::DaemonNotRunning(msg)) => {
                    // Pre-send failure: daemon unreachable or write failed before the
                    // command was delivered. Safe to fall back to direct WS.
                    tracing::debug!("Daemon not reachable, falling back to direct WS: {}", msg);
                    return None;
                }
                Err(ref e) if e.to_string().contains("No target attached") => {
                    // Daemon received the request but hasn't attached to a browser
                    // target yet. The command was NOT forwarded — safe to retry.
                    tracing::debug!(
                        "Daemon target not ready (attempt {}/{}), retrying...",
                        attempt + 1,
                        max_retries + 1
                    );
                    last_err = Some(ActionbookError::DaemonError(e.to_string()));
                    continue;
                }
                Err(e) => {
                    // Post-send failure (DaemonError): the command may have already been
                    // forwarded to the browser. Do NOT fall back — that would risk
                    // duplicating non-idempotent operations (click, navigate, etc.).
                    tracing::warn!("Daemon error after command may have been sent: {}", e);
                    return Some(Err(e));
                }
            }
        }

        // All retries exhausted — fall back to direct WS
        tracing::warn!(
            "Daemon target not ready after {} retries, falling back to direct WS",
            max_retries + 1,
        );
        if let Some(err) = last_err {
            tracing::debug!("Last daemon error: {}", err);
        }
        None
    }

    async fn send_cdp_command_over_page_ws(
        &self,
        ws_url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::browser::cdp_types::CdpResponse;
        use tokio_tungstenite::connect_async;

        let (mut ws, _) = connect_async(ws_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("WebSocket connection failed: {}", e))
        })?;

        let cmd = serde_json::json!({
            "id": 1,
            "method": method,
            "params": params
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send command: {}", e)))?;

        use futures::stream::StreamExt;
        let mut parse_failures = 0u8; // Track consecutive parse failures

        while let Some(msg) = ws.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    // Phase 2a optimization: Single-pass typed deserialization
                    // Try parsing directly as CdpResponse (the common case for responses).
                    // CdpResponse requires an "id" field, so CDP Events (which lack "id")
                    // will fail to parse and fall through to the lightweight event check.
                    match serde_json::from_str::<CdpResponse>(&text) {
                        Ok(response) => {
                            if response.id == 1 {
                                if let Some(err) = response.error {
                                    return Err(ActionbookError::Other(format!(
                                        "CDP error: {}",
                                        err
                                    )));
                                }
                                return Ok(response.result.unwrap_or(serde_json::Value::Null));
                            }
                            // Not our response (different id), keep waiting
                            tracing::trace!(
                                "Received CDP Response with id={}, waiting for id=1",
                                response.id
                            );
                        }
                        Err(_) => {
                            // Not a valid CdpResponse — check if it's a CDP Event or malformed
                            // Use lightweight string checks to avoid a full Value parse
                            if text.contains("\"method\"") && !text.contains("\"id\"") {
                                // CDP Event: {"method": "...", "params": {...}}
                                tracing::trace!("Skipping CDP Event");
                                continue;
                            }
                            // Unexpected message structure
                            tracing::warn!(
                                "Unexpected CDP message, length: {}, first 80 chars: {}",
                                text.len(),
                                text.chars().take(80).collect::<String>()
                            );
                            parse_failures += 1;
                            if parse_failures > 5 {
                                return Err(ActionbookError::Other(format!(
                                    "Too many CDP parse failures ({})",
                                    parse_failures
                                )));
                            }
                            continue;
                        }
                    }
                }
                Ok(_) => continue, // Ignore non-text messages (ping/pong/binary)
                Err(e) => return Err(ActionbookError::Other(format!("WebSocket error: {}", e))),
            }
        }

        Err(ActionbookError::Other("No response received".to_string()))
    }

    async fn send_cdp_command_via_attached_target(
        &self,
        browser_ws_url: &str,
        target_id: &str,
        method: &str,
        params: serde_json::Value,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<serde_json::Value> {
        let mut ws = Self::connect_ws_with_headers(browser_ws_url, headers).await?;

        // 1) Attach to target (flatten=true gives sessionId)
        let attach_cmd = serde_json::json!({
            "id": 1,
            "method": "Target.attachToTarget",
            "params": {
                "targetId": target_id,
                "flatten": true
            }
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            attach_cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send attach command: {}", e)))?;

        let mut session_id: Option<String> = None;
        while let Some(msg) = ws.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                    if response.get("id") == Some(&serde_json::json!(1)) {
                        if let Some(error) = response.get("error") {
                            return Err(ActionbookError::Other(format!(
                                "CDP attach failed: {}",
                                error
                            )));
                        }
                        session_id = response
                            .get("result")
                            .and_then(|r| r.get("sessionId"))
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string());
                        break;
                    }
                }
                Ok(_) => continue,
                Err(e) => {
                    return Err(ActionbookError::Other(format!(
                        "WebSocket error while attaching: {}",
                        e
                    )));
                }
            }
        }

        let session_id = session_id.ok_or_else(|| {
            ActionbookError::Other("No sessionId returned by attachToTarget".to_string())
        })?;

        // 2) Send command to attached target session
        let cmd = serde_json::json!({
            "id": 2,
            "sessionId": session_id,
            "method": method,
            "params": params
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send command: {}", e)))?;

        while let Some(msg) = ws.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                    if response.get("id") == Some(&serde_json::json!(2)) {
                        if let Some(error) = response.get("error") {
                            return Err(ActionbookError::Other(format!("CDP error: {}", error)));
                        }
                        return Ok(response
                            .get("result")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null));
                    }
                }
                Ok(_) => continue,
                Err(e) => return Err(ActionbookError::Other(format!("WebSocket error: {}", e))),
            }
        }

        Err(ActionbookError::Other("No response received".to_string()))
    }

    async fn send_browser_command_over_ws(
        &self,
        browser_ws_url: &str,
        method: &str,
        params: serde_json::Value,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<serde_json::Value> {
        let mut ws = Self::connect_ws_with_headers(browser_ws_url, headers).await?;

        let cmd = serde_json::json!({
            "id": 1,
            "method": method,
            "params": params
        });

        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send command: {}", e)))?;

        while let Some(msg) = ws.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                    if response.get("id") == Some(&serde_json::json!(1)) {
                        if let Some(error) = response.get("error") {
                            return Err(ActionbookError::Other(format!("CDP error: {}", error)));
                        }
                        return Ok(response
                            .get("result")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null));
                    }
                }
                Ok(_) => continue,
                Err(e) => return Err(ActionbookError::Other(format!("WebSocket error: {}", e))),
            }
        }

        Err(ActionbookError::Other("No response received".to_string()))
    }

    /// Returns JavaScript that defines `__findElement(selector)` function.
    /// Supports CSS selectors, XPath (starts with //), @eN and [ref=eN] snapshot references.
    fn find_element_js() -> &'static str {
        r#"
        function __findInShadowDOM(selector) {
            // Split by ::shadow-root separator
            const parts = selector.split('::shadow-root');
            if (parts.length < 2) {
                return null;
            }

            // Find the host element
            const hostSelector = parts[0].trim();
            let currentElement;

            // Handle ref-based selection for host
            if (/^@e\d+$/.test(hostSelector) || /^\[ref=e\d+\]$/.test(hostSelector)) {
                currentElement = __findElement(hostSelector);
            } else {
                currentElement = document.querySelector(hostSelector);
            }

            if (!currentElement) {
                console.warn('Shadow DOM host element not found:', hostSelector);
                return null;
            }

            // Access shadow root
            const shadowRoot = currentElement.shadowRoot;
            if (!shadowRoot) {
                console.warn('Element has no shadow root:', hostSelector);
                return null;
            }

            // Query inside shadow DOM
            const innerSelector = parts[1].trim().replace(/^>\s*/, '').trim();
            if (!innerSelector) {
                // No inner selector, return shadow root's first child or host
                return shadowRoot.firstElementChild || currentElement;
            }

            // Support nested shadow DOM: inner::shadow-root > button
            if (innerSelector.includes('::shadow-root')) {
                // Recursively handle nested shadow roots
                // For this, we need to query in current shadow root first
                const nestedParts = innerSelector.split('::shadow-root');
                const nextHost = shadowRoot.querySelector(nestedParts[0].trim());
                if (!nextHost) return null;

                const nextShadowRoot = nextHost.shadowRoot;
                if (!nextShadowRoot) return null;

                const finalSelector = nestedParts.slice(1).join('::shadow-root').trim().replace(/^>\s*/, '').trim();
                return nextShadowRoot.querySelector(finalSelector);
            }

            return shadowRoot.querySelector(innerSelector);
        }

        function __findElement(selector) {
            // Handle Shadow DOM syntax: element::shadow-root > inner-selector
            if (selector.includes('::shadow-root')) {
                return __findInShadowDOM(selector);
            }

            // Normalize [ref=eN] format (from snapshot output) to @eN
            const refMatch = selector.match(/^\[ref=(e\d+)\]$/);
            if (refMatch) selector = '@' + refMatch[1];
            if (/^@e\d+$/.test(selector)) {
                const targetNum = parseInt(selector.slice(2));
                const SKIP_TAGS = new Set(['script','style','noscript','template','svg','path','defs','clippath','lineargradient','stop','meta','link','br','wbr']);
                const INLINE_TAGS = new Set(['strong','b','em','i','code','span','small','sup','sub','abbr','mark','u','s','del','ins','time','q','cite','dfn','var','samp','kbd']);
                const INTERACTIVE_ROLES = new Set(['button','link','textbox','checkbox','radio','combobox','listbox','menuitem','menuitemcheckbox','menuitemradio','option','searchbox','slider','spinbutton','switch','tab','treeitem']);
                const CONTENT_ROLES = new Set(['heading','cell','gridcell','columnheader','rowheader','listitem','article','region','main','navigation','img']);
                function getRole(el) {
                    const explicit = el.getAttribute('role');
                    if (explicit) return explicit.toLowerCase();
                    const tag = el.tagName.toLowerCase();
                    if (INLINE_TAGS.has(tag)) return tag;
                    const roleMap = {
                        'a': el.hasAttribute('href') ? 'link' : 'generic',
                        'button': 'button', 'input': getInputRole(el), 'select': 'combobox', 'textarea': 'textbox', 'img': 'img',
                        'h1':'heading','h2':'heading','h3':'heading','h4':'heading','h5':'heading','h6':'heading',
                        'nav':'navigation','main':'main','header':'banner','footer':'contentinfo','aside':'complementary',
                        'form':'form','table':'table','thead':'rowgroup','tbody':'rowgroup','tfoot':'rowgroup',
                        'tr':'row','th':'columnheader','td':'cell','ul':'list','ol':'list','li':'listitem',
                        'details':'group','summary':'button','dialog':'dialog',
                        'section': el.hasAttribute('aria-label') || el.hasAttribute('aria-labelledby') ? 'region' : 'generic',
                        'article':'article'
                    };
                    return roleMap[tag] || 'generic';
                }
                function getInputRole(el) {
                    const type = (el.getAttribute('type') || 'text').toLowerCase();
                    const map = {'text':'textbox','email':'textbox','password':'textbox','search':'searchbox','tel':'textbox','url':'textbox','number':'spinbutton','checkbox':'checkbox','radio':'radio','submit':'button','reset':'button','button':'button','range':'slider'};
                    return map[type] || 'textbox';
                }
                function getAccessibleName(el) {
                    const ariaLabel = el.getAttribute('aria-label');
                    if (ariaLabel) return ariaLabel.trim();
                    const labelledBy = el.getAttribute('aria-labelledby');
                    if (labelledBy) { const label = document.getElementById(labelledBy); if (label) return label.textContent?.trim()?.substring(0, 100) || ''; }
                    const tag = el.tagName.toLowerCase();
                    if (tag === 'img') return el.getAttribute('alt') || '';
                    if (tag === 'input' || tag === 'textarea' || tag === 'select') {
                        if (el.id) { const label = document.querySelector('label[for="' + el.id + '"]'); if (label) return label.textContent?.trim()?.substring(0, 100) || ''; }
                        return el.getAttribute('placeholder') || el.getAttribute('title') || '';
                    }
                    if (tag === 'a' || tag === 'button' || tag === 'summary') return '';
                    if (['h1','h2','h3','h4','h5','h6'].includes(tag)) return el.textContent?.trim()?.substring(0, 150) || '';
                    const title = el.getAttribute('title');
                    if (title) return title.trim();
                    return '';
                }
                function isHidden(el) {
                    if (el.hidden) return true;
                    if (el.getAttribute('aria-hidden') === 'true') return true;
                    const style = el.style;
                    if (style.display === 'none' || style.visibility === 'hidden') return true;
                    if (el.offsetParent === null && el.tagName.toLowerCase() !== 'body' && getComputedStyle(el).position !== 'fixed' && getComputedStyle(el).position !== 'sticky') {
                        const cs = getComputedStyle(el);
                        if (cs.display === 'none' || cs.visibility === 'hidden') return true;
                    }
                    return false;
                }
                let refCounter = 0;
                function walkFind(el, depth) {
                    if (depth > 15) return null;
                    const tag = el.tagName.toLowerCase();
                    if (SKIP_TAGS.has(tag)) return null;
                    if (isHidden(el)) return null;
                    const role = getRole(el);
                    const name = getAccessibleName(el);
                    const isInteractive = INTERACTIVE_ROLES.has(role);
                    const isContent = CONTENT_ROLES.has(role);
                    const shouldRef = isInteractive || (isContent && name);
                    if (shouldRef) {
                        refCounter++;
                        if (refCounter === targetNum) return el;
                    }
                    for (const child of el.children) {
                        const found = walkFind(child, depth + 1);
                        if (found) return found;
                    }
                    return null;
                }
                return walkFind(document.body, 0);
            }
            if (selector.startsWith('//') || selector.startsWith('(//')) {
                const result = document.evaluate(selector, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null);
                return result.singleNodeValue;
            }
            // Extended selector support: :has-text("...") and :nth(N)
            // These are Playwright-style pseudo-selectors not in native CSS
            const hasTextRe = /:has-text\(['"](.+?)['"]\)/;
            const nthRe = /:nth\((\d+)\)$/;
            const hasTextM = selector.match(hasTextRe);
            const nthM = selector.match(nthRe);
            if (hasTextM || nthM) {
                let base = selector;
                let textFilter = null;
                let nthIdx = null;
                if (hasTextM) {
                    textFilter = hasTextM[1];
                    base = base.replace(hasTextRe, '');
                }
                if (nthM) {
                    nthIdx = parseInt(nthM[1]);
                    base = base.replace(nthRe, '');
                }
                base = base.trim() || '*';
                let els = Array.from(document.querySelectorAll(base));
                if (textFilter) {
                    els = els.filter(el => el.textContent && el.textContent.includes(textFilter));
                }
                if (nthIdx !== null) return els[nthIdx] || null;
                return els[0] || null;
            }
            return document.querySelector(selector);
        }
        "#
    }

    /// Click an element on the active page
    pub async fn click_on_page(&self, profile_name: Option<&str>, selector: &str) -> Result<()> {
        // Find the element, scroll it into view, and get its center coordinates
        // Supports CSS selectors, XPath (starts with //), @eN and [ref=eN] snapshot references
        let selector_json = serde_json::to_string(selector)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return null;",
            "el.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });",
            "const rect = el.getBoundingClientRect();",
            "return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };",
            "})()",
        ]
        .join("\n");

        let coords = self.eval_on_page(profile_name, &js).await?;

        if coords.is_null() {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }

        let x = coords
            .get("x")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ActionbookError::Other("Invalid coordinates".to_string()))?;
        let y = coords
            .get("y")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ActionbookError::Other("Invalid coordinates".to_string()))?;

        // Move mouse to target first so the browser updates its hit-test target,
        // then press and release. Without mouseMoved, CDP may not dispatch the
        // click to the correct DOM element.
        self.send_cdp_command(
            profile_name,
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": "mouseMoved",
                "x": x,
                "y": y
            }),
        )
        .await?;

        self.send_cdp_command(
            profile_name,
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
        )
        .await?;

        self.send_cdp_command(
            profile_name,
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
        )
        .await?;

        Ok(())
    }

    /// Type text into an element on the active page
    pub async fn type_on_page(
        &self,
        profile_name: Option<&str>,
        selector: &str,
        text: &str,
    ) -> Result<()> {
        // Focus the element first (supports CSS, XPath, and @eN refs)
        let selector_json = serde_json::to_string(selector)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return false;",
            "el.focus();",
            "return true;",
            "})()",
        ]
        .join("\n");

        let focused = self.eval_on_page(profile_name, &js).await?;
        if !focused.as_bool().unwrap_or(false) {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }

        // Type each character
        for c in text.chars() {
            self.send_cdp_command(
                profile_name,
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyDown",
                    "text": c.to_string()
                }),
            )
            .await?;

            self.send_cdp_command(
                profile_name,
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyUp",
                    "text": c.to_string()
                }),
            )
            .await?;
        }

        Ok(())
    }

    /// Fill an input element (clear and type)
    pub async fn fill_on_page(
        &self,
        profile_name: Option<&str>,
        selector: &str,
        text: &str,
    ) -> Result<()> {
        // Clear and set value directly via JS, then dispatch input event (supports CSS, XPath, and @eN refs)
        let selector_json = serde_json::to_string(selector)?;
        let text_json = serde_json::to_string(text)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return false;",
            "el.focus();",
            &format!("el.value = {text_json};"),
            "el.dispatchEvent(new Event('input', { bubbles: true }));",
            "el.dispatchEvent(new Event('change', { bubbles: true }));",
            "return true;",
            "})()",
        ]
        .join("\n");

        let filled = self.eval_on_page(profile_name, &js).await?;
        if !filled.as_bool().unwrap_or(false) {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }

        Ok(())
    }

    /// Take a screenshot of the active page
    pub async fn screenshot_page(&self, profile_name: Option<&str>) -> Result<Vec<u8>> {
        let result = self
            .send_cdp_command(
                profile_name,
                "Page.captureScreenshot",
                serde_json::json!({
                    "format": "png"
                }),
            )
            .await?;

        let data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionbookError::Other("No screenshot data".to_string()))?;

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| ActionbookError::Other(format!("Failed to decode screenshot: {}", e)))
    }

    /// Export the active page as PDF
    pub async fn pdf_page(&self, profile_name: Option<&str>) -> Result<Vec<u8>> {
        let result = self
            .send_cdp_command(profile_name, "Page.printToPDF", serde_json::json!({}))
            .await?;

        let data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionbookError::Other("No PDF data".to_string()))?;

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| ActionbookError::Other(format!("Failed to decode PDF: {}", e)))
    }

    /// Take a full-page screenshot
    pub async fn screenshot_full_page(&self, profile_name: Option<&str>) -> Result<Vec<u8>> {
        // Get page dimensions
        let metrics = self
            .send_cdp_command(profile_name, "Page.getLayoutMetrics", serde_json::json!({}))
            .await?;

        let content_size = metrics
            .get("contentSize")
            .ok_or_else(|| ActionbookError::Other("No content size".to_string()))?;

        let width = content_size
            .get("width")
            .and_then(|v| v.as_f64())
            .unwrap_or(1920.0);
        let height = content_size
            .get("height")
            .and_then(|v| v.as_f64())
            .unwrap_or(1080.0);

        let result = self
            .send_cdp_command(
                profile_name,
                "Page.captureScreenshot",
                serde_json::json!({
                    "format": "png",
                    "clip": {
                        "x": 0,
                        "y": 0,
                        "width": width,
                        "height": height,
                        "scale": 1
                    },
                    "captureBeyondViewport": true
                }),
            )
            .await?;

        let data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionbookError::Other("No screenshot data".to_string()))?;

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| ActionbookError::Other(format!("Failed to decode screenshot: {}", e)))
    }

    /// Navigate to URL on current page
    pub async fn goto(&self, profile_name: Option<&str>, url: &str) -> Result<()> {
        self.send_cdp_command(
            profile_name,
            "Page.navigate",
            serde_json::json!({ "url": url }),
        )
        .await?;
        Ok(())
    }

    /// Go back in history
    pub async fn go_back(&self, profile_name: Option<&str>) -> Result<()> {
        let history = self
            .send_cdp_command(
                profile_name,
                "Page.getNavigationHistory",
                serde_json::json!({}),
            )
            .await?;

        let current_index = history
            .get("currentIndex")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if current_index > 0 {
            let entries = history.get("entries").and_then(|v| v.as_array());
            if let Some(entries) = entries {
                if let Some(entry) = entries.get((current_index - 1) as usize) {
                    if let Some(entry_id) = entry.get("id").and_then(|v| v.as_i64()) {
                        self.send_cdp_command(
                            profile_name,
                            "Page.navigateToHistoryEntry",
                            serde_json::json!({ "entryId": entry_id }),
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Go forward in history
    pub async fn go_forward(&self, profile_name: Option<&str>) -> Result<()> {
        let history = self
            .send_cdp_command(
                profile_name,
                "Page.getNavigationHistory",
                serde_json::json!({}),
            )
            .await?;

        let current_index = history
            .get("currentIndex")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let entries = history.get("entries").and_then(|v| v.as_array());
        if let Some(entries) = entries {
            if let Some(entry) = entries.get((current_index + 1) as usize) {
                if let Some(entry_id) = entry.get("id").and_then(|v| v.as_i64()) {
                    self.send_cdp_command(
                        profile_name,
                        "Page.navigateToHistoryEntry",
                        serde_json::json!({ "entryId": entry_id }),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    /// Reload current page
    pub async fn reload(&self, profile_name: Option<&str>) -> Result<()> {
        self.send_cdp_command(profile_name, "Page.reload", serde_json::json!({}))
            .await?;
        Ok(())
    }

    /// Wait for element to appear
    pub async fn wait_for_element(
        &self,
        profile_name: Option<&str>,
        selector: &str,
        timeout_ms: u64,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        let selector_json = serde_json::to_string(selector)?;

        loop {
            let js = [
                "(function() {",
                Self::find_element_js(),
                &format!("return __findElement({selector_json}) !== null;"),
                "})()",
            ]
            .join("\n");
            let found = self.eval_on_page(profile_name, &js).await?;

            if found.as_bool().unwrap_or(false) {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(ActionbookError::Timeout(format!(
                    "Element '{}' not found within {}ms",
                    selector, timeout_ms
                )));
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Wait for navigation to complete
    pub async fn wait_for_navigation(
        &self,
        profile_name: Option<&str>,
        timeout_ms: u64,
    ) -> Result<String> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        // Get initial URL
        let initial_url = self
            .eval_on_page(profile_name, "document.location.href")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string();

        loop {
            // Check document ready state
            let ready_state = self
                .eval_on_page(profile_name, "document.readyState")
                .await?;

            let current_url = self
                .eval_on_page(profile_name, "document.location.href")
                .await?
                .as_str()
                .unwrap_or("")
                .to_string();

            if ready_state.as_str() == Some("complete") && current_url != initial_url {
                return Ok(current_url);
            }

            if start.elapsed() > timeout {
                return Err(ActionbookError::Timeout("Navigation timeout".to_string()));
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Select an option from dropdown
    pub async fn select_on_page(
        &self,
        profile_name: Option<&str>,
        selector: &str,
        value: &str,
    ) -> Result<()> {
        let selector_json = serde_json::to_string(selector)?;
        let value_json = serde_json::to_string(value)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el || el.tagName !== 'SELECT') return false;",
            &format!("el.value = {value_json};"),
            "el.dispatchEvent(new Event('change', { bubbles: true }));",
            "return true;",
            "})()",
        ]
        .join("\n");

        let selected = self.eval_on_page(profile_name, &js).await?;
        if !selected.as_bool().unwrap_or(false) {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }
        Ok(())
    }

    /// Hover over an element
    pub async fn hover_on_page(&self, profile_name: Option<&str>, selector: &str) -> Result<()> {
        // Get element coordinates (supports CSS, XPath, and @eN refs)
        let selector_json = serde_json::to_string(selector)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return null;",
            "const rect = el.getBoundingClientRect();",
            "return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };",
            "})()",
        ]
        .join("\n");

        let coords = self.eval_on_page(profile_name, &js).await?;
        if coords.is_null() {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }

        let x = coords.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = coords.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);

        self.send_cdp_command(
            profile_name,
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": "mouseMoved",
                "x": x,
                "y": y
            }),
        )
        .await?;

        Ok(())
    }

    /// Focus on an element
    pub async fn focus_on_page(&self, profile_name: Option<&str>, selector: &str) -> Result<()> {
        let selector_json = serde_json::to_string(selector)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return false;",
            "el.focus();",
            "return true;",
            "})()",
        ]
        .join("\n");

        let focused = self.eval_on_page(profile_name, &js).await?;
        if !focused.as_bool().unwrap_or(false) {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }
        Ok(())
    }

    /// Press a keyboard key
    pub async fn press_key(&self, profile_name: Option<&str>, key: &str) -> Result<()> {
        // Map common key names to CDP key codes, code, and windowsVirtualKeyCode
        // Virtual key codes follow the Windows VK standard — cross-platform in CDP
        let (key_value, code, text, vk) = match key.to_lowercase().as_str() {
            "enter" | "return" => ("Enter", "Enter", "\r", 13),
            "tab" => ("Tab", "Tab", "\t", 9),
            "escape" | "esc" => ("Escape", "Escape", "", 27),
            "backspace" => ("Backspace", "Backspace", "", 8),
            "delete" => ("Delete", "Delete", "", 46),
            "arrowup" | "up" => ("ArrowUp", "ArrowUp", "", 38),
            "arrowdown" | "down" => ("ArrowDown", "ArrowDown", "", 40),
            "arrowleft" | "left" => ("ArrowLeft", "ArrowLeft", "", 37),
            "arrowright" | "right" => ("ArrowRight", "ArrowRight", "", 39),
            "home" => ("Home", "Home", "", 36),
            "end" => ("End", "End", "", 35),
            "pageup" => ("PageUp", "PageUp", "", 33),
            "pagedown" => ("PageDown", "PageDown", "", 34),
            "space" => (" ", "Space", " ", 32),
            "f1" => ("F1", "F1", "", 112),
            "f2" => ("F2", "F2", "", 113),
            "f3" => ("F3", "F3", "", 114),
            "f4" => ("F4", "F4", "", 115),
            "f5" => ("F5", "F5", "", 116),
            "f6" => ("F6", "F6", "", 117),
            "f7" => ("F7", "F7", "", 118),
            "f8" => ("F8", "F8", "", 119),
            "f9" => ("F9", "F9", "", 120),
            "f10" => ("F10", "F10", "", 121),
            "f11" => ("F11", "F11", "", 122),
            "f12" => ("F12", "F12", "", 123),
            "insert" => ("Insert", "Insert", "", 45),
            _ => (key, key, key, 0),
        };

        let mut key_down = serde_json::json!({
            "type": "keyDown",
            "key": key_value,
            "code": code,
            "windowsVirtualKeyCode": vk,
        });
        if !text.is_empty() {
            key_down["text"] = serde_json::json!(text);
        }

        self.send_cdp_command(profile_name, "Input.dispatchKeyEvent", key_down)
            .await?;

        self.send_cdp_command(
            profile_name,
            "Input.dispatchKeyEvent",
            serde_json::json!({
                "type": "keyUp",
                "key": key_value,
                "code": code,
                "windowsVirtualKeyCode": vk,
            }),
        )
        .await?;

        Ok(())
    }

    /// Send keyboard hotkey (e.g., Ctrl+A, Ctrl+Shift+ArrowRight)
    /// keys format: ["Control", "A"] or ["Control", "Shift", "ArrowRight"]
    pub async fn send_hotkey(&self, profile_name: Option<&str>, keys: &[&str]) -> Result<()> {
        if keys.is_empty() {
            return Err(ActionbookError::Other("Empty key sequence".to_string()));
        }

        // Map modifier key names to their codes and modifiers flag
        let get_modifier_info = |key: &str| -> Option<(&str, &str, i32, i32)> {
            match key.to_lowercase().as_str() {
                "control" | "ctrl" => Some(("Control", "ControlLeft", 17, 2)),
                "shift" => Some(("Shift", "ShiftLeft", 16, 8)),
                "alt" => Some(("Alt", "AltLeft", 18, 1)),
                "meta" | "command" | "cmd" => Some(("Meta", "MetaLeft", 91, 4)),
                _ => None,
            }
        };

        let modifiers_count = keys.len() - 1;
        let main_key = keys[keys.len() - 1];

        // Calculate modifiers bitmask
        let mut modifiers_mask = 0;
        for key in &keys[..modifiers_count] {
            if let Some((_, _, _, mask)) = get_modifier_info(key) {
                modifiers_mask |= mask;
            }
        }

        // Press all modifier keys
        for key in &keys[..modifiers_count] {
            if let Some((key_value, code, vk, _)) = get_modifier_info(key) {
                self.send_cdp_command(
                    profile_name,
                    "Input.dispatchKeyEvent",
                    serde_json::json!({
                        "type": "keyDown",
                        "key": key_value,
                        "code": code,
                        "windowsVirtualKeyCode": vk,
                        "modifiers": modifiers_mask,
                    }),
                )
                .await?;
            }
        }

        // Press and release main key with modifiers
        let (key_value, code, text, vk) = match main_key.to_lowercase().as_str() {
            "a" => ("a", "KeyA", "a", 65),
            "b" => ("b", "KeyB", "b", 66),
            "c" => ("c", "KeyC", "c", 67),
            "d" => ("d", "KeyD", "d", 68),
            "e" => ("e", "KeyE", "e", 69),
            "f" => ("f", "KeyF", "f", 70),
            "g" => ("g", "KeyG", "g", 71),
            "h" => ("h", "KeyH", "h", 72),
            "i" => ("i", "KeyI", "i", 73),
            "j" => ("j", "KeyJ", "j", 74),
            "k" => ("k", "KeyK", "k", 75),
            "l" => ("l", "KeyL", "l", 76),
            "m" => ("m", "KeyM", "m", 77),
            "n" => ("n", "KeyN", "n", 78),
            "o" => ("o", "KeyO", "o", 79),
            "p" => ("p", "KeyP", "p", 80),
            "q" => ("q", "KeyQ", "q", 81),
            "r" => ("r", "KeyR", "r", 82),
            "s" => ("s", "KeyS", "s", 83),
            "t" => ("t", "KeyT", "t", 84),
            "u" => ("u", "KeyU", "u", 85),
            "v" => ("v", "KeyV", "v", 86),
            "w" => ("w", "KeyW", "w", 87),
            "x" => ("x", "KeyX", "x", 88),
            "y" => ("y", "KeyY", "y", 89),
            "z" => ("z", "KeyZ", "z", 90),
            "arrowleft" | "left" => ("ArrowLeft", "ArrowLeft", "", 37),
            "arrowright" | "right" => ("ArrowRight", "ArrowRight", "", 39),
            "arrowup" | "up" => ("ArrowUp", "ArrowUp", "", 38),
            "arrowdown" | "down" => ("ArrowDown", "ArrowDown", "", 40),
            "enter" | "return" => ("Enter", "Enter", "\r", 13),
            "tab" => ("Tab", "Tab", "\t", 9),
            "backspace" => ("Backspace", "Backspace", "", 8),
            "delete" => ("Delete", "Delete", "", 46),
            _ => (main_key, main_key, main_key, 0),
        };

        let mut key_down = serde_json::json!({
            "type": "keyDown",
            "key": key_value,
            "code": code,
            "windowsVirtualKeyCode": vk,
            "modifiers": modifiers_mask,
        });
        if !text.is_empty() {
            key_down["text"] = serde_json::json!(text);
        }

        self.send_cdp_command(profile_name, "Input.dispatchKeyEvent", key_down)
            .await?;

        self.send_cdp_command(
            profile_name,
            "Input.dispatchKeyEvent",
            serde_json::json!({
                "type": "keyUp",
                "key": key_value,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "modifiers": modifiers_mask,
            }),
        )
        .await?;

        // Release all modifier keys in reverse order
        for key in keys[..modifiers_count].iter().rev() {
            if let Some((key_value, code, vk, _)) = get_modifier_info(key) {
                self.send_cdp_command(
                    profile_name,
                    "Input.dispatchKeyEvent",
                    serde_json::json!({
                        "type": "keyUp",
                        "key": key_value,
                        "code": code,
                        "windowsVirtualKeyCode": vk,
                        "modifiers": 0,
                    }),
                )
                .await?;
            }
        }

        Ok(())
    }

    /// Dispatch a single character key event (for human-like typing)
    pub async fn dispatch_key_char(&self, profile_name: Option<&str>, ch: char) -> Result<()> {
        let text = ch.to_string();
        self.send_cdp_command(
            profile_name,
            "Input.dispatchKeyEvent",
            serde_json::json!({
                "type": "keyDown",
                "key": &text,
                "text": &text,
            }),
        )
        .await?;
        self.send_cdp_command(
            profile_name,
            "Input.dispatchKeyEvent",
            serde_json::json!({
                "type": "keyUp",
                "key": &text,
            }),
        )
        .await?;
        Ok(())
    }

    /// Get page HTML
    pub async fn get_html(
        &self,
        profile_name: Option<&str>,
        selector: Option<&str>,
    ) -> Result<String> {
        let js = match selector {
            Some(sel) => {
                let sel_json = serde_json::to_string(sel)?;
                [
                    "(function() {",
                    Self::find_element_js(),
                    &format!("const el = __findElement({sel_json});"),
                    "return el ? el.outerHTML : null;",
                    "})()",
                ]
                .join("\n")
            }
            None => "document.documentElement.outerHTML".to_string(),
        };

        let html = self.eval_on_page(profile_name, &js).await?;
        match html {
            serde_json::Value::String(s) => Ok(s),
            serde_json::Value::Null => Err(ActionbookError::ElementNotFound(
                selector.unwrap_or("document").to_string(),
            )),
            _ => Ok(html.to_string()),
        }
    }

    /// Get page text content
    pub async fn get_text(
        &self,
        profile_name: Option<&str>,
        selector: Option<&str>,
    ) -> Result<String> {
        let js = match selector {
            Some(sel) => {
                let sel_json = serde_json::to_string(sel)?;
                [
                    "(function() {",
                    Self::find_element_js(),
                    &format!("const el = __findElement({sel_json});"),
                    "return el ? el.innerText : null;",
                    "})()",
                ]
                .join("\n")
            }
            None => "document.body.innerText".to_string(),
        };

        let text = self.eval_on_page(profile_name, &js).await?;
        match text {
            serde_json::Value::String(s) => Ok(s),
            serde_json::Value::Null => Err(ActionbookError::ElementNotFound(
                selector.unwrap_or("body").to_string(),
            )),
            _ => Ok(text.to_string()),
        }
    }

    /// Get all cookies
    pub async fn get_cookies(&self, profile_name: Option<&str>) -> Result<Vec<serde_json::Value>> {
        let result = self
            .send_cdp_command(profile_name, "Network.getAllCookies", serde_json::json!({}))
            .await?;

        let cookies = result
            .get("cookies")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(cookies)
    }

    /// Set a cookie
    pub async fn set_cookie(
        &self,
        profile_name: Option<&str>,
        name: &str,
        value: &str,
        domain: Option<&str>,
    ) -> Result<()> {
        let mut params = serde_json::json!({
            "name": name,
            "value": value
        });

        if let Some(d) = domain {
            params["domain"] = serde_json::json!(d);
        } else {
            // Get current domain
            let url = self
                .eval_on_page(profile_name, "document.location.href")
                .await?;
            if let Some(url_str) = url.as_str() {
                params["url"] = serde_json::json!(url_str);
            }
        }

        self.send_cdp_command(profile_name, "Network.setCookie", params)
            .await?;
        Ok(())
    }

    /// Delete a cookie
    pub async fn delete_cookie(&self, profile_name: Option<&str>, name: &str) -> Result<()> {
        // Get current URL for domain
        let url = self
            .eval_on_page(profile_name, "document.location.href")
            .await?;
        let url_str = url.as_str().unwrap_or("");

        self.send_cdp_command(
            profile_name,
            "Network.deleteCookies",
            serde_json::json!({
                "name": name,
                "url": url_str
            }),
        )
        .await?;
        Ok(())
    }

    /// Clear all cookies
    pub async fn clear_cookies(&self, profile_name: Option<&str>) -> Result<()> {
        self.send_cdp_command(
            profile_name,
            "Network.clearBrowserCookies",
            serde_json::json!({}),
        )
        .await?;
        Ok(())
    }

    /// Get viewport dimensions
    pub async fn get_viewport(&self, profile_name: Option<&str>) -> Result<(f64, f64)> {
        let js = r#"
            (function() {
                return {
                    width: window.innerWidth,
                    height: window.innerHeight
                };
            })()
        "#;

        let result = self.eval_on_page(profile_name, js).await?;
        let width = result.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let height = result.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);

        Ok((width, height))
    }

    /// Inspect DOM element at coordinates
    pub async fn inspect_at(
        &self,
        profile_name: Option<&str>,
        x: f64,
        y: f64,
    ) -> Result<serde_json::Value> {
        // First, move mouse to the coordinates
        self.send_cdp_command(
            profile_name,
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": "mouseMoved",
                "x": x,
                "y": y
            }),
        )
        .await?;

        // Then inspect the element
        let js = format!(
            r#"
            (function() {{
                const x = {x};
                const y = {y};
                const element = document.elementFromPoint(x, y);

                if (!element) {{
                    return {{
                        found: false,
                        message: 'No element found at coordinates'
                    }};
                }}

                // Get computed style for interactivity check
                const computedStyles = window.getComputedStyle(element);

                // Get bounding box
                const rect = element.getBoundingClientRect();

                // Get parent hierarchy for selector context (up to 3 levels)
                const parents = [];
                let parent = element.parentElement;
                let level = 0;
                while (parent && level < 3) {{
                    const textContent = parent.textContent?.trim() || '';
                    parents.push({{
                        tagName: parent.tagName.toLowerCase(),
                        className: parent.className || '',
                        id: parent.id || '',
                        textContent: textContent.length > 50 ? textContent.substring(0, 50) + '...' : textContent,
                    }});
                    parent = parent.parentElement;
                    level++;
                }}

                // Get all attributes for comprehensive selectors
                const attributes = {{}};
                for (const attr of element.attributes) {{
                    attributes[attr.name] = attr.value;
                }}

                const elementOuterHTML = element.outerHTML;
                const elementTextContent = element.textContent?.trim() || '';

                // Build suggested selectors
                const selectors = [];
                if (element.id) {{
                    selectors.push('#' + element.id);
                }}
                if (element.getAttribute('data-testid')) {{
                    selectors.push('[data-testid=\"' + element.getAttribute('data-testid') + '\"]');
                }}
                if (element.getAttribute('aria-label')) {{
                    selectors.push('[aria-label=\"' + element.getAttribute('aria-label') + '\"]');
                }}
                if (element.className && typeof element.className === 'string') {{
                    const classes = element.className.split(' ').filter(c => c.length > 0);
                    if (classes.length > 0) {{
                        selectors.push(element.tagName.toLowerCase() + '.' + classes.join('.'));
                    }}
                }}

                return {{
                    found: true,
                    tagName: element.tagName.toLowerCase(),
                    id: element.id || null,
                    className: element.className || null,
                    textContent: elementTextContent.length > 200 ? elementTextContent.substring(0, 200) + '...' : elementTextContent,
                    attributes: attributes,
                    outerHTML: elementOuterHTML.length > 2000 ? elementOuterHTML.substring(0, 2000) + '...' : elementOuterHTML,
                    boundingBox: {{
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height
                    }},
                    isInteractive: ['a', 'button', 'input', 'select', 'textarea', 'label'].includes(element.tagName.toLowerCase()) ||
                                  element.onclick !== null ||
                                  element.role === 'button' ||
                                  element.hasAttribute('onclick') ||
                                  computedStyles.cursor === 'pointer',
                    suggestedSelectors: selectors,
                    parents: parents
                }};
            }})()
            "#,
            x = x,
            y = y
        );

        self.eval_on_page(profile_name, &js).await
    }

    /// Get browser status for a profile
    pub async fn get_status(&self, profile_name: Option<&str>) -> SessionStatus {
        let profile_name = self.resolve_profile_name(profile_name);

        if let Some(state) = self.load_session_state(&profile_name) {
            if self.is_session_alive(&state).await {
                SessionStatus::Running {
                    profile: profile_name.clone(),
                    cdp_port: state.cdp_port,
                    cdp_url: state.cdp_url,
                }
            } else {
                SessionStatus::Stale {
                    profile: profile_name.clone(),
                }
            }
        } else {
            SessionStatus::NotRunning {
                profile: profile_name,
            }
        }
    }

    // ========== G5: Fingerprint Rotation ==========

    /// Generate and apply a new browser fingerprint dynamically.
    /// Updates UA, platform, screen dimensions, hardware concurrency, and device memory.
    pub async fn rotate_fingerprint(
        &self,
        profile_name: Option<&str>,
        fingerprint: &super::stealth_enhanced::EnhancedStealthProfile,
    ) -> Result<()> {
        // 1. Set User-Agent override via Emulation
        self.send_cdp_command(
            profile_name,
            "Emulation.setUserAgentOverride",
            serde_json::json!({
                "userAgent": fingerprint.user_agent,
                "platform": fingerprint.platform,
                "acceptLanguage": fingerprint.language,
            }),
        )
        .await?;

        // 2. Inject screen/hardware overrides via JS
        let screen_js = format!(
            r#"(function() {{
                Object.defineProperty(screen, 'width', {{ get: () => {} }});
                Object.defineProperty(screen, 'height', {{ get: () => {} }});
                Object.defineProperty(screen, 'availWidth', {{ get: () => {} }});
                Object.defineProperty(screen, 'availHeight', {{ get: () => {} }});
                Object.defineProperty(screen, 'colorDepth', {{ get: () => {} }});
                Object.defineProperty(navigator, 'hardwareConcurrency', {{ get: () => {} }});
                Object.defineProperty(navigator, 'deviceMemory', {{ get: () => {} }});
            }})()"#,
            fingerprint.screen_width,
            fingerprint.screen_height,
            fingerprint.avail_width,
            fingerprint.avail_height,
            fingerprint.color_depth,
            fingerprint.hardware_concurrency,
            fingerprint.device_memory,
        );

        self.eval_on_page(profile_name, &screen_js).await?;

        // 3. Register for future pages too
        self.send_cdp_command(
            profile_name,
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({ "source": &screen_js }),
        )
        .await?;

        Ok(())
    }

    // ========== G2: Global Animation Disabling ==========

    /// Disable all CSS animations and transitions on the current and future pages.
    /// Injects CSS via `Page.addScriptToEvaluateOnNewDocument` and applies
    /// `Emulation.setEmulatedMedia` with `prefers-reduced-motion: reduce`.
    pub async fn disable_animations(&self, profile_name: Option<&str>) -> Result<()> {
        let css = r#"*, *::before, *::after { animation: none !important; animation-duration: 0s !important; transition: none !important; transition-duration: 0s !important; scroll-behavior: auto !important; }"#;

        let inject_js = format!(
            r#"(function() {{ var s = document.createElement('style'); s.textContent = {}; document.head.appendChild(s); }})()"#,
            serde_json::to_string(css).unwrap_or_default()
        );

        // 1. Inject CSS on current page immediately
        self.eval_on_page(profile_name, &inject_js).await?;

        // 2. Register script for all future page loads
        self.send_cdp_command(
            profile_name,
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({ "source": &inject_js }),
        )
        .await?;

        // 3. Set prefers-reduced-motion media feature
        self.send_cdp_command(
            profile_name,
            "Emulation.setEmulatedMedia",
            serde_json::json!({
                "features": [
                    { "name": "prefers-reduced-motion", "value": "reduce" }
                ]
            }),
        )
        .await?;

        Ok(())
    }

    // ========== F3: Resource Blocking ==========

    /// Block resource loading by URL patterns via CDP Network.setBlockedURLs
    pub async fn set_resource_blocking(
        &self,
        profile_name: Option<&str>,
        level: ResourceBlockLevel,
    ) -> Result<()> {
        let patterns = level.patterns();
        if patterns.is_empty() {
            return Ok(());
        }

        // Enable Network domain first
        self.send_cdp_command(profile_name, "Network.enable", serde_json::json!({}))
            .await?;

        self.send_cdp_command(
            profile_name,
            "Network.setBlockedURLs",
            serde_json::json!({ "urls": patterns }),
        )
        .await?;

        Ok(())
    }

    // ========== F4: Readability Text Extraction ==========

    /// Get readable text content from the page using readability extraction
    pub async fn get_readable_text(
        &self,
        profile_name: Option<&str>,
        mode: TextExtractionMode,
    ) -> Result<String> {
        let js = match mode {
            TextExtractionMode::Raw => "document.body.innerText".to_string(),
            TextExtractionMode::Readability => super::readability::READABILITY_JS.to_string(),
        };

        let result = self.eval_on_page(profile_name, &js).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    // ========== F1: CDP Accessibility Tree ==========

    /// Get the full accessibility tree via CDP Accessibility.getFullAXTree
    ///
    /// Enables DOM and Accessibility domains first to ensure computed accessible
    /// names are populated (required for links, headings, etc.).
    pub async fn get_accessibility_tree(
        &self,
        profile_name: Option<&str>,
    ) -> Result<serde_json::Value> {
        // Enable domains so CDP computes full accessible names
        let _ = self
            .send_cdp_command(profile_name, "DOM.enable", serde_json::json!({}))
            .await;
        let _ = self
            .send_cdp_command(profile_name, "Accessibility.enable", serde_json::json!({}))
            .await;
        self.send_cdp_command(
            profile_name,
            "Accessibility.getFullAXTree",
            serde_json::json!({}),
        )
        .await
    }

    /// Get the backendNodeId of an element matching a CSS selector
    pub async fn get_backend_node_id(
        &self,
        profile_name: Option<&str>,
        selector: &str,
    ) -> Result<Option<i64>> {
        // Get document root
        let doc = self
            .send_cdp_command(profile_name, "DOM.getDocument", serde_json::json!({}))
            .await?;
        let root_id = doc
            .get("root")
            .and_then(|r| r.get("nodeId"))
            .and_then(|n| n.as_i64())
            .unwrap_or(1);

        // Query selector
        let result = self
            .send_cdp_command(
                profile_name,
                "DOM.querySelector",
                serde_json::json!({ "nodeId": root_id, "selector": selector }),
            )
            .await?;
        let node_id = result.get("nodeId").and_then(|n| n.as_i64()).unwrap_or(0);
        if node_id == 0 {
            return Ok(None);
        }

        // Describe node to get backendNodeId
        let desc = self
            .send_cdp_command(
                profile_name,
                "DOM.describeNode",
                serde_json::json!({ "nodeId": node_id }),
            )
            .await?;
        let backend_id = desc
            .get("node")
            .and_then(|n| n.get("backendNodeId"))
            .and_then(|b| b.as_i64());

        Ok(backend_id)
    }

    // ========== F2: Node-based actions ==========

    /// Resolve a backendNodeId to a JS remote object, then call a function on it
    pub async fn resolve_and_call(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
        function_declaration: &str,
    ) -> Result<serde_json::Value> {
        // When daemon is enabled, route each CDP step through send_cdp_command
        // so the daemon's persistent WS is reused (4 sequential UDS round-trips
        // instead of 1 one-off page WS with 4 commands).
        if self.daemon_enabled {
            return self
                .resolve_and_call_via_cdp(profile_name, backend_node_id, function_declaration)
                .await;
        }

        use tokio_tungstenite::connect_async;

        let page_info = self.get_active_page_info(profile_name).await?;
        let ws_url = page_info
            .web_socket_debugger_url
            .ok_or_else(|| ActionbookError::CdpConnectionFailed("No WebSocket URL".to_string()))?;

        let (mut ws, _) = connect_async(&ws_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("WebSocket connection failed: {}", e))
        })?;

        // Helper to send a command and wait for its response on the same connection
        async fn send_and_recv(
            ws: &mut tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            id: u64,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value> {
            use futures::stream::StreamExt;
            use futures::SinkExt;

            let cmd = serde_json::json!({ "id": id, "method": method, "params": params });
            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                cmd.to_string().into(),
            ))
            .await
            .map_err(|e| ActionbookError::Other(format!("Failed to send {}: {}", method, e)))?;

            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                        if response.get("id") == Some(&serde_json::json!(id)) {
                            if let Some(error) = response.get("error") {
                                return Err(ActionbookError::Other(format!(
                                    "CDP error: {}",
                                    error
                                )));
                            }
                            return Ok(response
                                .get("result")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null));
                        }
                        // Not our response, skip (could be events)
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        return Err(ActionbookError::Other(format!("WebSocket error: {}", e)))
                    }
                }
            }
            Err(ActionbookError::Other(format!(
                "No response for {}",
                method
            )))
        }

        // All commands on the same WebSocket connection:
        // 1. Enable DOM domain
        let _ = send_and_recv(&mut ws, 1, "DOM.enable", serde_json::json!({})).await;
        // 2. Get document root (populates internal DOM state)
        let _ = send_and_recv(&mut ws, 2, "DOM.getDocument", serde_json::json!({})).await;
        // 3. Resolve backendNodeId to remote object
        let resolved = send_and_recv(
            &mut ws,
            3,
            "DOM.resolveNode",
            serde_json::json!({ "backendNodeId": backend_node_id }),
        )
        .await?;

        let object_id = resolved
            .get("object")
            .and_then(|o| o.get("objectId"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                ActionbookError::ElementNotFound(format!(
                    "Could not resolve backendNodeId {}",
                    backend_node_id
                ))
            })?;

        // 4. Call function on the resolved object
        let result = send_and_recv(
            &mut ws,
            4,
            "Runtime.callFunctionOn",
            serde_json::json!({
                "objectId": object_id,
                "functionDeclaration": function_declaration,
                "returnByValue": true,
            }),
        )
        .await?;

        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Daemon-aware resolve_and_call: routes each CDP step through send_cdp_command
    /// so the daemon's persistent WS is reused instead of opening a one-off page WS.
    async fn resolve_and_call_via_cdp(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
        function_declaration: &str,
    ) -> Result<serde_json::Value> {
        // 1. Enable DOM domain
        let _ = self
            .send_cdp_command(profile_name, "DOM.enable", serde_json::json!({}))
            .await;
        // 2. Get document root
        let _ = self
            .send_cdp_command(profile_name, "DOM.getDocument", serde_json::json!({}))
            .await;
        // 3. Resolve backendNodeId to remote object
        let resolved = self
            .send_cdp_command(
                profile_name,
                "DOM.resolveNode",
                serde_json::json!({ "backendNodeId": backend_node_id }),
            )
            .await?;

        let object_id = resolved
            .get("object")
            .and_then(|o| o.get("objectId"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                ActionbookError::ElementNotFound(format!(
                    "Could not resolve backendNodeId {}",
                    backend_node_id
                ))
            })?;

        // 4. Call function on the resolved object
        let result = self
            .send_cdp_command(
                profile_name,
                "Runtime.callFunctionOn",
                serde_json::json!({
                    "objectId": object_id,
                    "functionDeclaration": function_declaration,
                    "returnByValue": true,
                }),
            )
            .await?;

        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Get the center coordinates of an element by backendNodeId (scrolls into view)
    pub async fn get_element_center_by_node_id(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
    ) -> Result<(f64, f64)> {
        let coords = self
            .resolve_and_call(
                profile_name,
                backend_node_id,
                "function() { this.scrollIntoView({ behavior: 'instant', block: 'center' }); \
                 const r = this.getBoundingClientRect(); \
                 return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; }",
            )
            .await?;
        let x = coords.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = coords.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        Ok((x, y))
    }

    /// Get the center coordinates of an element by CSS selector (scrolls into view)
    pub async fn get_element_center(
        &self,
        profile_name: Option<&str>,
        selector: &str,
    ) -> Result<(f64, f64)> {
        let js = format!(
            r#"(function() {{
                var el = document.querySelector({sel});
                if (!el) return null;
                el.scrollIntoView({{ behavior: 'instant', block: 'center' }});
                var r = el.getBoundingClientRect();
                return {{ x: r.left + r.width / 2, y: r.top + r.height / 2 }};
            }})()"#,
            sel = serde_json::to_string(selector).unwrap_or_else(|_| format!("\"{}\"", selector))
        );
        let result = self.eval_on_page(profile_name, &js).await?;
        let x = result.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = result.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        Ok((x, y))
    }

    /// Click an element by backendNodeId
    pub async fn click_by_node_id(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
    ) -> Result<()> {
        // Scroll into view and get coordinates
        let (x, y) = self
            .get_element_center_by_node_id(profile_name, backend_node_id)
            .await?;

        // Dispatch click events
        for event_type in &["mouseMoved", "mousePressed", "mouseReleased"] {
            let mut params = serde_json::json!({ "type": event_type, "x": x, "y": y });
            if *event_type != "mouseMoved" {
                params["button"] = serde_json::json!("left");
                params["clickCount"] = serde_json::json!(1);
            }
            self.send_cdp_command(profile_name, "Input.dispatchMouseEvent", params)
                .await?;
        }
        Ok(())
    }

    // ========== File Upload via DOM.setFileInputFiles ==========

    /// Set files on a file input element located by CSS selector.
    ///
    /// Uses a single WebSocket connection to:
    /// 1. DOM.enable + DOM.getDocument
    /// 2. DOM.querySelector to find the element
    /// 3. DOM.setFileInputFiles to set the file paths
    /// 4. Dispatch change + input events via Runtime.callFunctionOn
    pub async fn set_file_input_files(
        &self,
        profile_name: Option<&str>,
        selector: &str,
        files: &[String],
    ) -> Result<()> {
        // When daemon is enabled, route each CDP step through send_cdp_command
        if self.daemon_enabled {
            return self
                .set_file_input_files_via_cdp(profile_name, selector, files)
                .await;
        }

        use tokio_tungstenite::connect_async;

        let page_info = self.get_active_page_info(profile_name).await?;
        let ws_url = page_info
            .web_socket_debugger_url
            .ok_or_else(|| ActionbookError::CdpConnectionFailed("No WebSocket URL".to_string()))?;

        let (mut ws, _) = connect_async(&ws_url).await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("WebSocket connection failed: {}", e))
        })?;

        // Reuse the same send_and_recv helper pattern from resolve_and_call
        async fn send_and_recv(
            ws: &mut tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            id: u64,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value> {
            use futures::stream::StreamExt;
            use futures::SinkExt;

            let cmd = serde_json::json!({ "id": id, "method": method, "params": params });
            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                cmd.to_string().into(),
            ))
            .await
            .map_err(|e| ActionbookError::Other(format!("Failed to send {}: {}", method, e)))?;

            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        let response: serde_json::Value = serde_json::from_str(text.as_str())?;
                        if response.get("id") == Some(&serde_json::json!(id)) {
                            if let Some(error) = response.get("error") {
                                return Err(ActionbookError::Other(format!(
                                    "CDP error: {}",
                                    error
                                )));
                            }
                            return Ok(response
                                .get("result")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null));
                        }
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        return Err(ActionbookError::Other(format!("WebSocket error: {}", e)))
                    }
                }
            }
            Err(ActionbookError::Other(format!(
                "No response for {}",
                method
            )))
        }

        // 1. Enable DOM
        let _ = send_and_recv(&mut ws, 1, "DOM.enable", serde_json::json!({})).await;

        // 2. Get document root
        let doc = send_and_recv(&mut ws, 2, "DOM.getDocument", serde_json::json!({})).await?;
        let root_id = doc
            .get("root")
            .and_then(|r| r.get("nodeId"))
            .and_then(|n| n.as_i64())
            .unwrap_or(1);

        // 3. querySelector to find the file input
        let qs_result = send_and_recv(
            &mut ws,
            3,
            "DOM.querySelector",
            serde_json::json!({ "nodeId": root_id, "selector": selector }),
        )
        .await?;
        let node_id = qs_result
            .get("nodeId")
            .and_then(|n| n.as_i64())
            .unwrap_or(0);
        if node_id == 0 {
            return Err(ActionbookError::ElementNotFound(format!(
                "File input not found: {}",
                selector
            )));
        }

        // 4. DOM.setFileInputFiles
        let _ = send_and_recv(
            &mut ws,
            4,
            "DOM.setFileInputFiles",
            serde_json::json!({ "files": files, "nodeId": node_id }),
        )
        .await?;

        // 5. Resolve node to object for event dispatch
        let resolved = send_and_recv(
            &mut ws,
            5,
            "DOM.resolveNode",
            serde_json::json!({ "nodeId": node_id }),
        )
        .await?;

        let object_id = resolved
            .get("object")
            .and_then(|o| o.get("objectId"))
            .and_then(|id| id.as_str());

        // 6. Dispatch change + input events (best-effort)
        if let Some(oid) = object_id {
            let _ = send_and_recv(
                &mut ws,
                6,
                "Runtime.callFunctionOn",
                serde_json::json!({
                    "objectId": oid,
                    "functionDeclaration": "function() { this.dispatchEvent(new Event('input', { bubbles: true })); this.dispatchEvent(new Event('change', { bubbles: true })); }",
                    "returnByValue": true,
                }),
            )
            .await;
        }

        Ok(())
    }

    /// Daemon-aware set_file_input_files: routes each CDP step through send_cdp_command.
    async fn set_file_input_files_via_cdp(
        &self,
        profile_name: Option<&str>,
        selector: &str,
        files: &[String],
    ) -> Result<()> {
        // 1. Enable DOM
        let _ = self
            .send_cdp_command(profile_name, "DOM.enable", serde_json::json!({}))
            .await;

        // 2. Get document root
        let doc = self
            .send_cdp_command(profile_name, "DOM.getDocument", serde_json::json!({}))
            .await?;
        let root_id = doc
            .get("root")
            .and_then(|r| r.get("nodeId"))
            .and_then(|n| n.as_i64())
            .unwrap_or(1);

        // 3. querySelector to find the file input
        let qs_result = self
            .send_cdp_command(
                profile_name,
                "DOM.querySelector",
                serde_json::json!({ "nodeId": root_id, "selector": selector }),
            )
            .await?;
        let node_id = qs_result
            .get("nodeId")
            .and_then(|n| n.as_i64())
            .unwrap_or(0);
        if node_id == 0 {
            return Err(ActionbookError::ElementNotFound(format!(
                "File input not found: {}",
                selector
            )));
        }

        // 4. DOM.setFileInputFiles
        let _ = self
            .send_cdp_command(
                profile_name,
                "DOM.setFileInputFiles",
                serde_json::json!({ "files": files, "nodeId": node_id }),
            )
            .await?;

        // 5. Resolve node to object for event dispatch
        let resolved = self
            .send_cdp_command(
                profile_name,
                "DOM.resolveNode",
                serde_json::json!({ "nodeId": node_id }),
            )
            .await?;

        let object_id = resolved
            .get("object")
            .and_then(|o| o.get("objectId"))
            .and_then(|id| id.as_str());

        // 6. Dispatch change + input events (best-effort)
        if let Some(oid) = object_id {
            let _ = self
                .send_cdp_command(
                    profile_name,
                    "Runtime.callFunctionOn",
                    serde_json::json!({
                        "objectId": oid,
                        "functionDeclaration": "function() { this.dispatchEvent(new Event('input', { bubbles: true })); this.dispatchEvent(new Event('change', { bubbles: true })); }",
                        "returnByValue": true,
                    }),
                )
                .await;
        }

        Ok(())
    }

    /// Set files on a file input element located by backendNodeId.
    ///
    /// Uses DOM.setFileInputFiles with backendNodeId, then dispatches events via resolve_and_call.
    pub async fn set_file_input_files_by_node_id(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
        files: &[String],
    ) -> Result<()> {
        // 1. Set files using backendNodeId
        self.send_cdp_command(
            profile_name,
            "DOM.setFileInputFiles",
            serde_json::json!({ "files": files, "backendNodeId": backend_node_id }),
        )
        .await?;

        // 2. Dispatch change + input events via resolve_and_call
        let _ = self
            .resolve_and_call(
                profile_name,
                backend_node_id,
                "function() { this.dispatchEvent(new Event('input', { bubbles: true })); this.dispatchEvent(new Event('change', { bubbles: true })); }",
            )
            .await;

        Ok(())
    }

    /// Focus an element by backendNodeId
    pub async fn focus_by_node_id(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
    ) -> Result<()> {
        self.send_cdp_command(
            profile_name,
            "DOM.focus",
            serde_json::json!({ "backendNodeId": backend_node_id }),
        )
        .await?;
        Ok(())
    }

    /// Type text into an element by backendNodeId (focus + dispatchKeyEvent)
    pub async fn type_by_node_id(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
        text: &str,
    ) -> Result<()> {
        self.focus_by_node_id(profile_name, backend_node_id).await?;
        for ch in text.chars() {
            self.send_cdp_command(
                profile_name,
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyDown",
                    "text": ch.to_string(),
                }),
            )
            .await?;
            self.send_cdp_command(
                profile_name,
                "Input.dispatchKeyEvent",
                serde_json::json!({ "type": "keyUp" }),
            )
            .await?;
        }
        Ok(())
    }

    /// Fill (clear + set value) an element by backendNodeId
    pub async fn fill_by_node_id(
        &self,
        profile_name: Option<&str>,
        backend_node_id: i64,
        text: &str,
    ) -> Result<()> {
        self.focus_by_node_id(profile_name, backend_node_id).await?;
        let text_json = serde_json::to_string(text)?;
        self.resolve_and_call(
            profile_name,
            backend_node_id,
            &format!(
                "function() {{ this.value = {text_json}; \
                 this.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                 this.dispatchEvent(new Event('change', {{ bubbles: true }})); }}"
            ),
        )
        .await?;
        Ok(())
    }

    // ========== F5: Human-like input ==========

    // ========== H1: Console Log Capture ==========

    /// Capture console log entries from the page via CDP Runtime.evaluate
    /// This fetches any existing console entries via performance logs.
    pub async fn capture_console_logs(
        &self,
        profile_name: Option<&str>,
    ) -> Result<Vec<serde_json::Value>> {
        let js = r#"(function() {
            if (!window.__ab_console_logs) return [];
            return window.__ab_console_logs.splice(0);
        })()"#;

        let result = self.eval_on_page(profile_name, js).await?;
        let empty = vec![];
        let logs = result.as_array().unwrap_or(&empty);
        Ok(logs.clone())
    }

    /// Install console log interceptor on the current page
    pub async fn install_console_interceptor(&self, profile_name: Option<&str>) -> Result<()> {
        let js = r#"(function() {
            if (window.__ab_console_installed) return;
            window.__ab_console_installed = true;
            window.__ab_console_logs = [];
            const MAX = 200;
            ['log','warn','error','info','debug'].forEach(function(level) {
                var orig = console[level];
                console[level] = function() {
                    var args = Array.from(arguments).map(function(a) {
                        try { return typeof a === 'object' ? JSON.stringify(a) : String(a); }
                        catch(e) { return String(a); }
                    });
                    window.__ab_console_logs.push({
                        level: level,
                        text: args.join(' '),
                        timestamp: Date.now()
                    });
                    if (window.__ab_console_logs.length > MAX) {
                        window.__ab_console_logs = window.__ab_console_logs.slice(-MAX);
                    }
                    orig.apply(console, arguments);
                };
            });
        })()"#;

        self.eval_on_page(profile_name, js).await?;

        // Also register for future pages
        self.send_cdp_command(
            profile_name,
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({ "source": js }),
        )
        .await?;

        Ok(())
    }

    // ========== H2: Network Idle Wait ==========

    /// Wait for network to become idle (no pending requests for `idle_ms` milliseconds)
    pub async fn wait_for_network_idle(
        &self,
        profile_name: Option<&str>,
        timeout_ms: u64,
        idle_ms: u64,
    ) -> Result<()> {
        // Install a network request counter via JS
        let setup_js = r#"(function() {
            if (window.__ab_net_installed) return;
            window.__ab_net_installed = true;
            window.__ab_pending_requests = 0;
            window.__ab_last_activity = Date.now();
            var origFetch = window.fetch;
            window.fetch = function() {
                window.__ab_pending_requests++;
                window.__ab_last_activity = Date.now();
                return origFetch.apply(this, arguments).finally(function() {
                    window.__ab_pending_requests--;
                    window.__ab_last_activity = Date.now();
                });
            };
            var origOpen = XMLHttpRequest.prototype.open;
            var origSend = XMLHttpRequest.prototype.send;
            XMLHttpRequest.prototype.open = function() {
                this.__ab_tracked = true;
                return origOpen.apply(this, arguments);
            };
            XMLHttpRequest.prototype.send = function() {
                if (this.__ab_tracked) {
                    window.__ab_pending_requests++;
                    window.__ab_last_activity = Date.now();
                    this.addEventListener('loadend', function() {
                        window.__ab_pending_requests--;
                        window.__ab_last_activity = Date.now();
                    });
                }
                return origSend.apply(this, arguments);
            };
        })()"#;

        self.eval_on_page(profile_name, setup_js).await?;

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        loop {
            let status_js = "(function() { return { pending: window.__ab_pending_requests || 0, lastActivity: window.__ab_last_activity || 0 }; })()";
            let status = self.eval_on_page(profile_name, status_js).await?;
            let pending = status.get("pending").and_then(|v| v.as_i64()).unwrap_or(0);
            let last_activity = status
                .get("lastActivity")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            if pending == 0 {
                // Check JS-side idle time
                let now_js = self.eval_on_page(profile_name, "Date.now()").await?;
                let now = now_js.as_f64().unwrap_or(0.0);
                let idle_since = now - last_activity;
                if idle_since >= idle_ms as f64 {
                    return Ok(());
                }
            }

            if start.elapsed() > timeout {
                return Err(ActionbookError::Timeout(format!(
                    "Network not idle within {}ms ({} requests pending)",
                    timeout_ms, pending
                )));
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    // ========== H3: Dialog Auto-Handling ==========

    /// Enable auto-dismissal of JavaScript dialogs (alert, confirm, prompt)
    pub async fn enable_dialog_auto_dismiss(&self, profile_name: Option<&str>) -> Result<()> {
        // Enable Page domain events
        self.send_cdp_command(profile_name, "Page.enable", serde_json::json!({}))
            .await?;

        // Use Runtime.evaluate to set up a handler that auto-accepts dialogs
        // We also need to use Page.handleJavaScriptDialog via CDP event listener,
        // but since we're using one-shot WS connections, we inject JS-level override instead
        let js = r#"(function() {
            if (window.__ab_dialog_installed) return;
            window.__ab_dialog_installed = true;
            window.__ab_dialog_log = [];
            window.alert = function(msg) {
                window.__ab_dialog_log.push({type:'alert', message:String(msg), timestamp:Date.now()});
            };
            var origConfirm = window.confirm;
            window.confirm = function(msg) {
                window.__ab_dialog_log.push({type:'confirm', message:String(msg), timestamp:Date.now()});
                return true;
            };
            var origPrompt = window.prompt;
            window.prompt = function(msg, def) {
                window.__ab_dialog_log.push({type:'prompt', message:String(msg), timestamp:Date.now()});
                return def || '';
            };
            window.onbeforeunload = null;
        })()"#;

        self.eval_on_page(profile_name, js).await?;

        // Register for future pages
        self.send_cdp_command(
            profile_name,
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({ "source": js }),
        )
        .await?;

        Ok(())
    }

    // ========== H4: Element Info ==========

    /// Get detailed information about an element by CSS selector
    pub async fn get_element_info(
        &self,
        profile_name: Option<&str>,
        selector: &str,
    ) -> Result<serde_json::Value> {
        let selector_json = serde_json::to_string(selector)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            r#"if (!el) return null;
            const rect = el.getBoundingClientRect();
            const cs = getComputedStyle(el);
            const attrs = {};
            for (const a of el.attributes) { attrs[a.name] = a.value; }
            const selectors = [];
            if (el.id) selectors.push('#' + el.id);
            if (el.getAttribute('data-testid')) selectors.push('[data-testid="' + el.getAttribute('data-testid') + '"]');
            if (el.getAttribute('aria-label')) selectors.push('[aria-label="' + el.getAttribute('aria-label') + '"]');
            if (el.className && typeof el.className === 'string') {
                const cls = el.className.trim().split(/\s+/).filter(Boolean);
                if (cls.length) selectors.push(el.tagName.toLowerCase() + '.' + cls.join('.'));
            }
            return {
                tagName: el.tagName.toLowerCase(),
                id: el.id || null,
                className: el.className || null,
                textContent: (el.textContent || '').trim().substring(0, 200),
                value: el.value !== undefined ? el.value : null,
                attributes: attrs,
                boundingBox: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
                computedStyle: {
                    display: cs.display,
                    visibility: cs.visibility,
                    position: cs.position,
                    color: cs.color,
                    backgroundColor: cs.backgroundColor,
                    fontSize: cs.fontSize,
                    cursor: cs.cursor,
                    opacity: cs.opacity
                },
                isVisible: rect.width > 0 && rect.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none',
                isInteractive: ['a','button','input','select','textarea'].includes(el.tagName.toLowerCase()) || el.getAttribute('role') === 'button' || cs.cursor === 'pointer',
                suggestedSelectors: selectors
            };"#,
            "})()",
        ]
        .join("\n");

        let result = self.eval_on_page(profile_name, &js).await?;
        if result.is_null() {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }
        Ok(result)
    }

    // ========== H6: Device Emulation ==========

    /// Emulate a device by setting viewport, UA, and device scale factor
    pub async fn emulate_device(
        &self,
        profile_name: Option<&str>,
        width: u32,
        height: u32,
        device_scale_factor: f64,
        mobile: bool,
        user_agent: Option<&str>,
    ) -> Result<()> {
        self.send_cdp_command(
            profile_name,
            "Emulation.setDeviceMetricsOverride",
            serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": device_scale_factor,
                "mobile": mobile,
            }),
        )
        .await?;

        if let Some(ua) = user_agent {
            self.send_cdp_command(
                profile_name,
                "Emulation.setUserAgentOverride",
                serde_json::json!({ "userAgent": ua }),
            )
            .await?;
        }

        // Touch events for mobile
        if mobile {
            self.send_cdp_command(
                profile_name,
                "Emulation.setTouchEmulationEnabled",
                serde_json::json!({ "enabled": true }),
            )
            .await?;
        }

        Ok(())
    }

    // ========== H7: Wait for JS Condition ==========

    /// Wait for a JavaScript expression to return a truthy value
    pub async fn wait_for_function(
        &self,
        profile_name: Option<&str>,
        expression: &str,
        timeout_ms: u64,
        interval_ms: u64,
    ) -> Result<serde_json::Value> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        loop {
            let result = self.eval_on_page(profile_name, expression).await?;

            // Check for truthy value
            let is_truthy = match &result {
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::Number(n) => n.as_f64().map_or(false, |f| f != 0.0),
                serde_json::Value::String(s) => !s.is_empty(),
                serde_json::Value::Null => false,
                serde_json::Value::Array(a) => !a.is_empty(),
                serde_json::Value::Object(_) => true,
            };

            if is_truthy {
                return Ok(result);
            }

            if start.elapsed() > timeout {
                return Err(ActionbookError::Timeout(format!(
                    "Expression did not become truthy within {}ms: {}",
                    timeout_ms, expression
                )));
            }

            tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
        }
    }

    /// Dispatch a sequence of mouse move events following a bezier curve
    pub async fn dispatch_mouse_moves(
        &self,
        profile_name: Option<&str>,
        points: &[(f64, f64)],
    ) -> Result<()> {
        for (x, y) in points {
            self.send_cdp_command(
                profile_name,
                "Input.dispatchMouseEvent",
                serde_json::json!({ "type": "mouseMoved", "x": x, "y": y }),
            )
            .await?;
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }
        Ok(())
    }

    /// Switch to an iframe context
    pub async fn switch_to_frame(
        &self,
        profile_name: Option<&str>,
        selector: &str,
    ) -> Result<String> {
        // Find the iframe element
        let selector_json = serde_json::to_string(selector)?;
        let js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return null;",
            "if (el.tagName.toLowerCase() !== 'iframe') return { error: 'Not an iframe' };",
            "return { success: true };",
            "})()",
        ]
        .join("\n");

        let result = self.eval_on_page(profile_name, &js).await?;

        if result.is_null() {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }

        if let Some(error) = result.get("error").and_then(|e| e.as_str()) {
            return Err(ActionbookError::Other(format!(
                "Element is not an iframe: {}",
                error
            )));
        }

        // Get the iframe's frame ID via DOM.describeNode
        let selector_json = serde_json::to_string(selector)?;
        let find_js = [
            "(function() {",
            Self::find_element_js(),
            &format!("const el = __findElement({selector_json});"),
            "if (!el) return null;",
            "return el;",
            "})()",
        ]
        .join("\n");

        let eval_result = self
            .send_cdp_command(
                profile_name,
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": find_js,
                    "returnByValue": false,
                }),
            )
            .await?;

        let object_id = eval_result
            .get("result")
            .and_then(|r| r.get("objectId"))
            .and_then(|o| o.as_str())
            .ok_or_else(|| ActionbookError::Other("Failed to get element objectId".to_string()))?;

        let describe_result = self
            .send_cdp_command(
                profile_name,
                "DOM.describeNode",
                serde_json::json!({
                    "objectId": object_id
                }),
            )
            .await?;

        let frame_id = describe_result
            .get("node")
            .and_then(|n| n.get("frameId"))
            .and_then(|f| f.as_str())
            .ok_or_else(|| {
                ActionbookError::Other("Element has no frameId (not an iframe)".to_string())
            })?;

        // Store the frame ID in session state
        let profile = self.resolve_profile_name(profile_name);
        let session_file = self.session_file(&profile);

        if session_file.exists() {
            let content = fs::read_to_string(&session_file)?;
            let mut state: serde_json::Value = serde_json::from_str(&content)?;
            state["current_frame_id"] = serde_json::json!(frame_id);
            fs::write(&session_file, serde_json::to_string_pretty(&state)?)?;
        }

        Ok(frame_id.to_string())
    }

    /// Switch to parent frame
    pub async fn switch_to_parent_frame(&self, profile_name: Option<&str>) -> Result<()> {
        // For now, just switch to main frame (null)
        // TODO: Implement proper parent frame tracking
        self.switch_to_default_frame(profile_name).await
    }

    /// Switch to main (default) frame
    pub async fn switch_to_default_frame(&self, profile_name: Option<&str>) -> Result<()> {
        let profile = self.resolve_profile_name(profile_name);
        let session_file = self.session_file(&profile);

        if session_file.exists() {
            let content = fs::read_to_string(&session_file)?;
            let mut state: serde_json::Value = serde_json::from_str(&content)?;
            state["current_frame_id"] = serde_json::Value::Null;
            fs::write(&session_file, serde_json::to_string_pretty(&state)?)?;
        }

        Ok(())
    }

    /// Get current frame ID (None = main frame)
    pub fn get_current_frame_id(&self, profile_name: Option<&str>) -> Option<String> {
        let profile = self.resolve_profile_name(profile_name);
        let session_file = self.session_file(&profile);

        if session_file.exists() {
            if let Ok(content) = fs::read_to_string(&session_file) {
                if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
                    return state
                        .get("current_frame_id")
                        .and_then(|f| f.as_str())
                        .map(|s| s.to_string());
                }
            }
        }

        None
    }
}

/// Resource blocking level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceBlockLevel {
    None,
    Images,
    Media,
}

impl ResourceBlockLevel {
    fn patterns(&self) -> Vec<&'static str> {
        match self {
            Self::None => vec![],
            Self::Images => vec![
                "*.png",
                "*.jpg",
                "*.jpeg",
                "*.gif",
                "*.webp",
                "*.svg",
                "*.ico",
                "*.bmp",
                "*.avif",
                "*.jfif",
                "*.tiff",
                "*imagedelivery.net*",
                "*images.unsplash.com*",
            ],
            Self::Media => vec![
                // Images
                "*.png",
                "*.jpg",
                "*.jpeg",
                "*.gif",
                "*.webp",
                "*.svg",
                "*.ico",
                "*.bmp",
                "*.avif",
                "*.jfif",
                "*.tiff",
                "*imagedelivery.net*",
                "*images.unsplash.com*",
                // Fonts
                "*.woff",
                "*.woff2",
                "*.ttf",
                "*.otf",
                "*.eot",
                // Video/Audio
                "*.mp4",
                "*.webm",
                "*.ogg",
                "*.mp3",
                "*.wav",
                "*.m3u8",
                // CSS
                "*.css",
            ],
        }
    }
}

/// Text extraction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextExtractionMode {
    Raw,
    Readability,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use futures::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Create a SessionManager with a temp directory for isolation
    fn test_session_manager(dir: &std::path::Path) -> SessionManager {
        SessionManager {
            config: Config::default(),
            sessions_dir: dir.to_path_buf(),
            stealth_config: None,
            daemon_enabled: false,
            active_profile: None,
            active_session: None,
        }
    }

    #[test]
    fn save_and_load_external_session() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        sm.save_external_session(
            "test-profile",
            9222,
            "ws://127.0.0.1:9222/devtools/browser/abc",
        )
        .unwrap();

        let state = sm.load_session_state("test-profile");
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.profile_name, "test-profile");
        assert_eq!(state.cdp_port, 9222);
        assert_eq!(state.cdp_url, "ws://127.0.0.1:9222/devtools/browser/abc");
        assert!(state.pid.is_none()); // External sessions have no PID
    }

    #[test]
    fn save_external_session_creates_sessions_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join("nested").join("sessions");
        let sm = SessionManager {
            config: Config::default(),
            sessions_dir: sessions_dir.clone(),
            stealth_config: None,
            daemon_enabled: false,
            active_profile: None,
            active_session: None,
        };

        assert!(!sessions_dir.exists());
        sm.save_external_session("default", 9222, "ws://localhost:9222")
            .unwrap();
        assert!(sessions_dir.exists());
        assert!(sessions_dir.join("default@default.json").exists());
    }

    #[test]
    fn load_nonexistent_session_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        let state = sm.load_session_state("nonexistent");
        assert!(state.is_none());
    }

    #[test]
    fn remove_session_state_deletes_file() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        sm.save_external_session("removeme", 9222, "ws://localhost:9222")
            .unwrap();
        assert!(sm.session_file("removeme").exists());

        sm.remove_session_state("removeme").unwrap();
        assert!(!sm.session_file("removeme").exists());
    }

    #[test]
    fn remove_nonexistent_session_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        // Should not error
        sm.remove_session_state("doesnotexist").unwrap();
    }

    #[test]
    fn save_overwrites_existing_session() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        sm.save_external_session("default", 9222, "ws://old-url")
            .unwrap();
        sm.save_external_session("default", 9333, "ws://new-url")
            .unwrap();

        let state = sm.load_session_state("default").unwrap();
        assert_eq!(state.cdp_port, 9333);
        assert_eq!(state.cdp_url, "ws://new-url");
    }

    #[test]
    fn multiple_profiles_are_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        sm.save_external_session("work", 9222, "ws://work-browser")
            .unwrap();
        sm.save_external_session("personal", 9333, "ws://personal-browser")
            .unwrap();

        let work = sm.load_session_state("work").unwrap();
        let personal = sm.load_session_state("personal").unwrap();

        assert_eq!(work.cdp_port, 9222);
        assert_eq!(personal.cdp_port, 9333);
        assert_eq!(work.cdp_url, "ws://work-browser");
        assert_eq!(personal.cdp_url, "ws://personal-browser");
    }

    #[test]
    fn session_file_path_uses_profile_and_session_name() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        // Default session (active_session = None → "default")
        let path = sm.session_file("my-profile");
        assert_eq!(path, dir.path().join("my-profile@default.json"));

        // Named session
        let mut sm2 = test_session_manager(dir.path());
        sm2.set_active_session("work");
        let path = sm2.session_file("my-profile");
        assert_eq!(path, dir.path().join("my-profile@work.json"));
    }

    #[test]
    fn session_file_migration_from_legacy() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        // Create a legacy session file
        let legacy_path = dir.path().join("migrate-profile.json");
        fs::write(
            &legacy_path,
            r#"{"profile_name":"migrate-profile","cdp_port":9222,"cdp_url":"ws://localhost:9222"}"#,
        )
        .unwrap();

        // session_file should find the legacy file and migrate
        let new_path = sm.session_file("migrate-profile");
        assert_eq!(new_path, dir.path().join("migrate-profile@default.json"));
        assert!(new_path.exists(), "Migrated file should exist");
        assert!(
            legacy_path.exists(),
            "Legacy file should still exist (copy, not move)"
        );
    }

    #[test]
    fn session_file_migration_from_sanitized_legacy() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        let legacy_path = dir.path().join("teamalpha.json");
        fs::write(
            &legacy_path,
            r#"{"profile_name":"team/alpha","cdp_port":9222,"cdp_url":"ws://localhost:9222"}"#,
        )
        .unwrap();

        let new_path = sm.session_file("team/alpha");
        assert_eq!(new_path, dir.path().join("teamalpha@default.json"));
        assert!(new_path.exists(), "Migrated file should exist");
        assert!(
            legacy_path.exists(),
            "Sanitized legacy file should still exist (copy, not move)"
        );
    }

    #[test]
    fn helper_extract_ws_host_handles_common_forms() {
        assert_eq!(
            extract_ws_host("ws://127.0.0.1:9222/devtools/browser/abc").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            extract_ws_host("wss://bedrock-agentcore.example.com/automation").as_deref(),
            Some("bedrock-agentcore.example.com")
        );
        assert_eq!(
            extract_ws_host("ws://[::1]:9222/devtools/browser/abc").as_deref(),
            Some("::1")
        );
    }

    #[test]
    fn session_state_local_http_detection() {
        let local = SessionState {
            profile_name: "local".to_string(),
            cdp_port: 9222,
            pid: None,
            cdp_url: "ws://127.0.0.1:9222/devtools/browser/abc".to_string(),
            active_page_id: None,
            custom_app_path: None,
            current_frame_id: None,
            ws_headers: None,
        };
        assert!(local.uses_local_http_endpoints());

        let remote = SessionState {
            profile_name: "remote".to_string(),
            cdp_port: 9222,
            pid: None,
            cdp_url: "wss://bedrock-agentcore.example.com/automation".to_string(),
            active_page_id: None,
            custom_app_path: None,
            current_frame_id: None,
            ws_headers: None,
        };
        assert!(!remote.uses_local_http_endpoints());

        // Even on loopback, non-devtools path should not use localhost HTTP fallback.
        let loopback_remote_style = SessionState {
            profile_name: "loopback-remote".to_string(),
            cdp_port: 9222,
            pid: None,
            cdp_url: "ws://127.0.0.1:9222/automation".to_string(),
            active_page_id: None,
            custom_app_path: None,
            current_frame_id: None,
            ws_headers: None,
        };
        assert!(!loopback_remote_style.uses_local_http_endpoints());
    }

    #[test]
    fn derive_page_ws_url_from_browser_ws() {
        let browser = "ws://127.0.0.1:9222/devtools/browser/abc";
        let page = derive_page_ws_url(browser, "target-1");
        assert_eq!(
            page.as_deref(),
            Some("ws://127.0.0.1:9222/devtools/page/target-1")
        );

        let non_standard = "wss://bedrock-agentcore.example.com/automation";
        assert!(derive_page_ws_url(non_standard, "target-1").is_none());
    }

    #[test]
    fn initial_blank_launch_artifact_requires_unbound_session() {
        let state = SessionState {
            profile_name: "blank".to_string(),
            cdp_port: 9222,
            pid: None,
            cdp_url: "ws://127.0.0.1:9222/devtools/browser/abc".to_string(),
            active_page_id: None,
            custom_app_path: None,
            current_frame_id: None,
            ws_headers: None,
        };
        let pages = vec![PageInfo {
            id: "page-1".to_string(),
            title: String::new(),
            url: "about:blank".to_string(),
            page_type: "page".to_string(),
            web_socket_debugger_url: None,
        }];

        assert_eq!(
            SessionManager::initial_blank_launch_artifact_id(&state, &pages).as_deref(),
            Some("page-1")
        );

        let bound_state = SessionState {
            active_page_id: Some("page-1".to_string()),
            ..state
        };
        assert!(SessionManager::initial_blank_launch_artifact_id(&bound_state, &pages).is_none());
    }

    #[test]
    fn initial_blank_launch_artifact_ignores_non_blank_or_multiple_pages() {
        let state = SessionState {
            profile_name: "blank".to_string(),
            cdp_port: 9222,
            pid: None,
            cdp_url: "ws://127.0.0.1:9222/devtools/browser/abc".to_string(),
            active_page_id: None,
            custom_app_path: None,
            current_frame_id: None,
            ws_headers: None,
        };

        let non_blank = vec![PageInfo {
            id: "page-1".to_string(),
            title: "Example".to_string(),
            url: "https://example.com".to_string(),
            page_type: "page".to_string(),
            web_socket_debugger_url: None,
        }];
        assert!(SessionManager::initial_blank_launch_artifact_id(&state, &non_blank).is_none());

        let multiple = vec![
            PageInfo {
                id: "page-1".to_string(),
                title: String::new(),
                url: "about:blank".to_string(),
                page_type: "page".to_string(),
                web_socket_debugger_url: None,
            },
            PageInfo {
                id: "page-2".to_string(),
                title: "Other".to_string(),
                url: "https://example.com".to_string(),
                page_type: "page".to_string(),
                web_socket_debugger_url: None,
            },
        ];
        assert!(SessionManager::initial_blank_launch_artifact_id(&state, &multiple).is_none());
    }

    #[tokio::test]
    async fn remote_get_pages_uses_target_get_targets() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

                if attempt == 0 {
                    let _ = ws.close(None).await;
                    continue;
                }

                while let Some(msg) = ws.next().await {
                    let msg = msg.unwrap();
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        let req: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                        if req.get("method").and_then(|m| m.as_str()) == Some("Target.getTargets") {
                            let resp = serde_json::json!({
                                "id": req.get("id").and_then(|v| v.as_i64()).unwrap_or(1),
                                "result": {
                                    "targetInfos": [
                                        {
                                            "targetId": "page-1",
                                            "type": "page",
                                            "title": "Remote Page",
                                            "url": "https://example.com"
                                        },
                                        {
                                            "targetId": "worker-1",
                                            "type": "service_worker",
                                            "title": "",
                                            "url": "https://example.com/sw.js"
                                        }
                                    ]
                                }
                            });

                            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                                resp.to_string().into(),
                            ))
                            .await
                            .unwrap();
                            break;
                        }
                    }
                }
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());
        let remote_ws = format!("ws://127.0.0.1:{}/automation", port);
        sm.save_external_session("remote", 9222, &remote_ws)
            .unwrap();

        let pages = sm.get_pages(Some("remote")).await.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, "page-1");
        assert_eq!(pages[0].title, "Remote Page");
        assert_eq!(pages[0].url, "https://example.com");
        assert!(pages[0].web_socket_debugger_url.is_none());

        server.abort();
    }

    #[tokio::test]
    async fn named_remote_session_forks_shared_state_for_direct_cdp() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

                if attempt == 0 {
                    let _ = ws.close(None).await;
                    continue;
                }

                while let Some(msg) = ws.next().await {
                    let msg = msg.unwrap();
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        let req: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                        if req.get("method").and_then(|m| m.as_str()) == Some("Target.getTargets") {
                            let resp = serde_json::json!({
                                "id": req.get("id").and_then(|v| v.as_i64()).unwrap_or(1),
                                "result": {
                                    "targetInfos": [{
                                        "targetId": "page-1",
                                        "type": "page",
                                        "title": "Forked Remote Page",
                                        "url": "https://example.com"
                                    }]
                                }
                            });
                            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                                resp.to_string().into(),
                            ))
                            .await
                            .unwrap();
                            break;
                        }
                    }
                }
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let remote_ws = format!("ws://127.0.0.1:{}/automation", port);
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test".to_string());

        let default_sm = test_session_manager(dir.path());
        default_sm
            .save_external_session_full("remote", 9222, &remote_ws, None, Some(headers.clone()))
            .unwrap();

        // Set a known active_page_id on the parent session so we can verify
        // that the forked session inherits it (not resets to None).
        {
            let mut parent = default_sm.load_session_state("remote").unwrap();
            parent.active_page_id = Some("parent-tab-42".to_string());
            default_sm.save_session_state(&parent).unwrap();
        }

        let mut named_sm = test_session_manager(dir.path());
        named_sm.set_active_session("work");

        let pages = named_sm.get_pages(Some("remote")).await.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].title, "Forked Remote Page");

        let forked_state = named_sm.load_session_state("remote").unwrap();
        assert_eq!(forked_state.cdp_url, remote_ws);
        assert_eq!(forked_state.ws_headers, Some(headers));
        // P1-1: forked session must inherit parent's active_page_id, not reset to None
        assert_eq!(
            forked_state.active_page_id.as_deref(),
            Some("parent-tab-42"),
            "Forked session should inherit parent's active_page_id"
        );

        server.abort();
    }

    #[test]
    fn named_session_detects_shareable_saved_remote_session_state() {
        let dir = tempfile::tempdir().unwrap();

        let default_sm = test_session_manager(dir.path());
        default_sm
            .save_external_session_full(
                "remote",
                9222,
                "wss://agent.example.com/automation",
                None,
                Some(std::collections::HashMap::from([(
                    "authorization".to_string(),
                    "Bearer test".to_string(),
                )])),
            )
            .unwrap();

        let mut named_sm = test_session_manager(dir.path());
        named_sm.set_active_session("work");

        assert!(named_sm.has_saved_session_state(Some("remote")));
        assert!(named_sm.session_uses_remote_ws(Some("remote")));
    }

    #[tokio::test]
    async fn is_remote_session_skips_stale_first_candidate() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

                if attempt == 0 {
                    let _ = ws.close(None).await;
                    continue;
                }

                while let Some(msg) = ws.next().await {
                    let msg = msg.unwrap();
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        let req: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                        if req.get("method").and_then(|m| m.as_str()) == Some("Target.getTargets") {
                            let resp = serde_json::json!({
                                "id": req.get("id").and_then(|v| v.as_i64()).unwrap_or(1),
                                "result": { "targetInfos": [] }
                            });
                            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                                resp.to_string().into(),
                            ))
                            .await
                            .unwrap();
                            break;
                        }
                    }
                }
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let stale_default = test_session_manager(dir.path());
        stale_default
            .save_external_session(
                "mixed",
                19999,
                "ws://127.0.0.1:19999/devtools/browser/stale-default",
            )
            .unwrap();

        let remote_ws = format!("ws://127.0.0.1:{}/automation", port);
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test".to_string());

        let mut shared_remote = test_session_manager(dir.path());
        shared_remote.set_active_session("shared");
        shared_remote
            .save_external_session_full("mixed", 9222, &remote_ws, None, Some(headers))
            .unwrap();

        let mut named_sm = test_session_manager(dir.path());
        named_sm.set_active_session("work");

        assert!(named_sm.is_remote_session(Some("mixed")).await);

        server.abort();
    }

    #[tokio::test]
    async fn dead_session_reports_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        // Save a session pointing to a port nothing is listening on
        sm.save_external_session("dead", 19999, "ws://127.0.0.1:19999")
            .unwrap();

        let status = sm.get_status(Some("dead")).await;
        assert!(matches!(status, SessionStatus::Stale { .. }));
    }

    #[tokio::test]
    async fn no_session_reports_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        let status = sm.get_status(Some("nonexistent")).await;
        assert!(matches!(status, SessionStatus::NotRunning { .. }));
    }

    #[tokio::test]
    async fn fetch_browser_ws_url_returns_none_for_unreachable_port() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        // Port 19998 is not listening — should return None, not panic
        let result = sm.fetch_browser_ws_url(19998).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn stale_ws_url_is_detected_via_session_state() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        // Simulate a stale session with an old WebSocket URL
        sm.save_external_session(
            "stale-test",
            19997,
            "ws://127.0.0.1:19997/devtools/browser/old-session-id",
        )
        .unwrap();

        let state = sm.load_session_state("stale-test").unwrap();
        assert_eq!(
            state.cdp_url,
            "ws://127.0.0.1:19997/devtools/browser/old-session-id"
        );

        // fetch_browser_ws_url returns None since port is not listening,
        // so the URL remains unchanged (no crash)
        let fresh = sm.fetch_browser_ws_url(state.cdp_port).await;
        assert!(fresh.is_none());
    }

    #[tokio::test]
    async fn new_page_refreshes_loopback_browser_ws_url_before_create_target() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        let pages = std::sync::Arc::new(tokio::sync::Mutex::new(vec![serde_json::json!({
            "id": "page-1",
            "title": "Existing Page",
            "url": "https://existing.example",
            "type": "page",
            "webSocketDebuggerUrl": serde_json::Value::Null
        })]));

        let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cdp_port = http_listener.local_addr().unwrap().port();
        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        let fresh_ws = format!("ws://127.0.0.1:{}/devtools/browser/fresh", ws_port);

        let http_pages = pages.clone();
        let http_fresh_ws = fresh_ws.clone();
        let http_server = tokio::spawn(async move {
            loop {
                let accepted =
                    tokio::time::timeout(Duration::from_secs(2), http_listener.accept()).await;
                let Ok(Ok((mut stream, _))) = accepted else {
                    break;
                };

                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);

                let (status, body) = if req.starts_with("GET /json/version ") {
                    (
                        "200 OK",
                        serde_json::json!({
                            "webSocketDebuggerUrl": http_fresh_ws
                        })
                        .to_string(),
                    )
                } else if req.starts_with("GET /json/list ") {
                    (
                        "200 OK",
                        serde_json::to_string(&*http_pages.lock().await).unwrap(),
                    )
                } else {
                    ("404 Not Found", "{}".to_string())
                };

                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let ws_pages = pages.clone();
        let ws_server = tokio::spawn(async move {
            let (stream, _) = ws_listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            while let Some(msg) = ws.next().await {
                let msg = msg.unwrap();
                if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                    let req: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                    if req.get("method").and_then(|m| m.as_str()) == Some("Target.createTarget") {
                        assert!(
                            req.get("sessionId").is_none(),
                            "browser-level Target.createTarget must not include sessionId"
                        );

                        let target_id = "page-2";
                        let url = req
                            .get("params")
                            .and_then(|params| params.get("url"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("about:blank");

                        ws_pages.lock().await.push(serde_json::json!({
                            "id": target_id,
                            "title": "Opened Page",
                            "url": url,
                            "type": "page",
                            "webSocketDebuggerUrl": serde_json::Value::Null
                        }));

                        let resp = serde_json::json!({
                            "id": req.get("id").and_then(|v| v.as_i64()).unwrap_or(1),
                            "result": {
                                "targetId": target_id
                            }
                        });

                        ws.send(tokio_tungstenite::tungstenite::Message::Text(
                            resp.to_string().into(),
                        ))
                        .await
                        .unwrap();
                        break;
                    }
                }
            }
        });

        sm.save_external_session(
            "local-stale",
            cdp_port,
            "ws://127.0.0.1:19997/devtools/browser/stale-session",
        )
        .unwrap();

        let new_page = sm
            .new_page(Some("local-stale"), Some("https://example.com"), false)
            .await
            .unwrap();

        assert_eq!(new_page.id, "page-2");
        assert_eq!(new_page.url, "https://example.com");

        let state = sm.load_session_state("local-stale").unwrap();
        assert_eq!(state.cdp_url, fresh_ws);

        ws_server.await.unwrap();
        http_server.await.unwrap();
    }

    #[tokio::test]
    async fn new_page_times_out_when_browser_ws_never_replies() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Accept multiple connections: the first is the liveness probe from
        // ensure_session_state_for_cdp → is_session_alive, the second is the
        // actual command from send_browser_command_over_ws.
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

                let mut got_create_target = false;
                while let Some(msg) = ws.next().await {
                    let msg = msg.unwrap();
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        let req: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                        if req.get("method").and_then(|m| m.as_str()) == Some("Target.createTarget")
                        {
                            // Stall so the caller times out
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            got_create_target = true;
                            break;
                        }
                    }
                }
                if got_create_target {
                    break;
                }
            }
        });

        let mut headers = std::collections::HashMap::new();
        headers.insert("x-test-auth".to_string(), "secret".to_string());
        let remote_ws = format!("ws://127.0.0.1:{port}/automation");
        sm.save_external_session_full("remote-timeout", 9222, &remote_ws, None, Some(headers))
            .unwrap();

        let err = sm
            .new_page_with_timeout(
                Some("remote-timeout"),
                Some("https://example.com"),
                false,
                Duration::from_millis(50),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(
                &err,
                ActionbookError::Timeout(msg)
                    if msg.contains("Page load timed out")
                        && msg.contains("https://example.com")
            ),
            "Expected Timeout error, got: {:?}",
            err
        );

        server.abort();
    }

    #[tokio::test]
    async fn new_page_times_out_when_get_targets_stalls_after_create_target() {
        let dir = tempfile::tempdir().unwrap();
        let sm = test_session_manager(dir.path());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Accept multiple connections: the first may be a liveness probe from
        // ensure_session_state_for_cdp → is_session_alive.
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

                let mut done = false;
                while let Some(msg) = ws.next().await {
                    let Ok(msg) = msg else {
                        break;
                    };
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        let req: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                        match req.get("method").and_then(|m| m.as_str()) {
                            Some("Target.createTarget") => {
                                let response = serde_json::json!({
                                    "id": req.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
                                    "result": {
                                        "targetId": "page-2"
                                    }
                                });
                                ws.send(tokio_tungstenite::tungstenite::Message::Text(
                                    response.to_string().into(),
                                ))
                                .await
                                .unwrap();
                            }
                            Some("Target.getTargets") => {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                                done = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                if done {
                    break;
                }
            }
        });

        let mut headers = std::collections::HashMap::new();
        headers.insert("x-test-auth".to_string(), "secret".to_string());
        let remote_ws = format!("ws://127.0.0.1:{port}/automation");
        sm.save_external_session_full("remote-timeout", 9222, &remote_ws, None, Some(headers))
            .unwrap();

        let err = sm
            .new_page_with_timeout(
                Some("remote-timeout"),
                Some("https://example.com"),
                false,
                Duration::from_millis(50),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(
                &err,
                ActionbookError::Timeout(msg)
                    if msg.contains("Page load timed out")
                        && msg.contains("https://example.com")
            ),
            "Expected Timeout error, got: {:?}",
            err
        );

        server.abort();
    }

    #[tokio::test]
    async fn none_profile_uses_configured_default_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.browser.default_profile = "team-default".to_string();
        let sm = SessionManager {
            config,
            sessions_dir: dir.path().to_path_buf(),
            stealth_config: None,
            daemon_enabled: false,
            active_profile: None,
            active_session: None,
        };

        let status = sm.get_status(None).await;
        assert!(matches!(
            status,
            SessionStatus::NotRunning { profile } if profile == "team-default"
        ));
    }

    #[test]
    fn active_profile_overrides_config_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut sm = test_session_manager(dir.path());

        // Without active_profile, resolve_profile_name(None) uses config default
        assert_eq!(sm.resolve_profile_name(None), "actionbook");

        // With active_profile set, resolve_profile_name(None) uses it instead
        sm.set_active_profile("twitter");
        assert_eq!(sm.resolve_profile_name(None), "twitter");
    }

    #[test]
    fn explicit_profile_name_overrides_active_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut sm = test_session_manager(dir.path());
        sm.set_active_profile("twitter");

        // Explicit profile_name always wins over active_profile
        assert_eq!(sm.resolve_profile_name(Some("arxiv")), "arxiv");
    }

    #[test]
    fn active_profile_does_not_affect_explicit_profile_routing() {
        let dir = tempfile::tempdir().unwrap();
        let mut sm = test_session_manager(dir.path());

        sm.save_external_session("twitter", 9401, "ws://127.0.0.1:9401/devtools/browser/aaa")
            .unwrap();
        sm.save_external_session("arxiv", 9402, "ws://127.0.0.1:9402/devtools/browser/bbb")
            .unwrap();

        sm.set_active_profile("twitter");

        // Explicit profile loads its own session, not the active one
        let arxiv_state = sm.load_session_state("arxiv").unwrap();
        assert_eq!(arxiv_state.cdp_port, 9402);
        assert_eq!(
            arxiv_state.cdp_url,
            "ws://127.0.0.1:9402/devtools/browser/bbb"
        );
    }
}

#[derive(Debug)]
pub enum SessionStatus {
    Running {
        profile: String,
        cdp_port: u16,
        cdp_url: String,
    },
    Stale {
        profile: String,
    },
    NotRunning {
        profile: String,
    },
}
