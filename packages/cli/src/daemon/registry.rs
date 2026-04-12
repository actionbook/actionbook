use std::collections::HashMap;
use std::fmt;
use std::process::Child;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::action_result::ActionResult;
use crate::browser::observation::snapshot_transform::RefCache;
use crate::browser::session::provider::{ProviderSession, normalize_provider_name};
use crate::daemon::bridge::SharedBridgeState;
use crate::daemon::cdp_session::CdpSession;
use crate::error::CliError;
use crate::types::{Mode, SessionId, TabId};

/// Tab metadata. `id` is the short user-facing ID (e.g. "t1"). `native_id` is Chrome's CDP target ID.
#[derive(Debug, Clone)]
pub struct TabEntry {
    pub id: TabId,
    pub native_id: String,
    pub url: String,
    pub title: String,
}

/// Session entry in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Starting,
    Running,
    /// A `browser close` is in flight for this entry. Set atomically under the
    /// registry lock at the start of `close::execute` so concurrent close calls
    /// on the same session short-circuit instead of issuing a second provider
    /// API stop (which races against the first stop's success).
    Closing,
    Closed,
}

impl SessionState {
    /// Active means "holds live resources that reuse/health checks can target."
    /// `Closing` is intentionally NOT active: a session being torn down must
    /// not be selected by reuse lookups mid-close, otherwise the agent could
    /// attach to a handle that's about to disappear.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionState::Starting => write!(f, "starting"),
            SessionState::Running => write!(f, "running"),
            SessionState::Closing => write!(f, "closing"),
            SessionState::Closed => write!(f, "closed"),
        }
    }
}

pub struct SessionEntry {
    pub id: SessionId,
    pub mode: Mode,
    pub headless: bool,
    pub stealth: bool,
    /// Stealth user-agent string — needed when attaching new tabs so they get the same stealth injection.
    pub stealth_ua: Option<String>,
    pub profile: String,
    pub status: SessionState,
    pub cdp_port: Option<u16>,
    pub ws_url: String,
    pub tabs: Vec<TabEntry>,
    pub chrome_process: Option<Child>,
    /// Win32 Job Object that owns Chrome's main process and all its helpers.
    /// `TerminateJobObject` kills the entire process group atomically on close.
    #[cfg(windows)]
    pub job_object: Option<crate::daemon::chrome_reaper::ChromeJobObject>,
    /// Persistent CDP connection for this session.
    pub cdp: Option<CdpSession>,
    /// Original CDP endpoint for cloud sessions (used for reuse matching & restart).
    pub cdp_endpoint: Option<String>,
    /// Custom headers for cloud CDP connections (e.g. auth tokens).
    pub headers: Vec<(String, String)>,
    /// Launch-time provider name for provider-backed cloud sessions.
    pub provider: Option<String>,
    /// Provider-managed remote session metadata used for cleanup.
    pub provider_session: Option<ProviderSession>,
    /// Counter for assigning short tab IDs (t1, t2, ...).
    pub next_tab_id: u32,
}

impl Drop for SessionEntry {
    fn drop(&mut self) {
        // Last-resort backstop: kill Chrome if it wasn't cleaned up explicitly.
        // On Windows, ChromeJobObject::Drop (called when job_object field drops)
        // already calls TerminateJobObject, which kills the entire Chrome process
        // group.  kill_and_reap_option below reaps the main process exit status.
        crate::daemon::chrome_reaper::kill_and_reap_option(&mut self.chrome_process);
    }
}

impl SessionEntry {
    pub fn starting(
        id: SessionId,
        mode: Mode,
        headless: bool,
        stealth: bool,
        profile: String,
    ) -> Self {
        Self {
            id,
            mode,
            headless,
            stealth,
            stealth_ua: None,
            profile,
            status: SessionState::Starting,
            cdp_port: None,
            ws_url: String::new(),
            tabs: Vec::new(),
            chrome_process: None,
            #[cfg(windows)]
            job_object: None,
            cdp: None,
            cdp_endpoint: None,
            headers: Vec::new(),
            provider: None,
            provider_session: None,
            next_tab_id: 1,
        }
    }

    pub fn tabs_count(&self) -> usize {
        self.tabs.len()
    }

    /// Append a tab with an auto-assigned short ID (t1, t2, ...).
    pub fn push_tab(&mut self, native_id: String, url: String, title: String) {
        let short_id = format!("t{}", self.next_tab_id);
        self.next_tab_id += 1;
        self.tabs.push(TabEntry {
            id: TabId(short_id),
            native_id,
            url,
            title,
        });
    }

    /// Append a tab with a caller-specified short ID.
    /// Returns the assigned ID on success, or an error if the ID is already taken.
    pub fn push_tab_with_id(
        &mut self,
        custom_id: String,
        native_id: String,
        url: String,
        title: String,
    ) -> Result<String, ActionResult> {
        if self.tabs.iter().any(|t| t.id.0 == custom_id) {
            return Err(ActionResult::fatal_with_hint(
                "TAB_ID_CONFLICT",
                format!("tab ID '{}' already exists in this session", custom_id),
                "choose a different --tab (--set-tab-id) value or omit it for auto-assignment",
            ));
        }
        self.tabs.push(TabEntry {
            id: TabId(custom_id.clone()),
            native_id,
            url,
            title,
        });
        Ok(custom_id)
    }
}

/// Thread-safe session registry.
pub struct SessionRegistry {
    sessions: HashMap<String, SessionEntry>,
    /// Tab-scoped RefCache for stable snapshot refs. Key: "session_id\0tab_id"
    ref_caches: HashMap<String, RefCache>,
    /// Last known cursor position per tab. Key: "session_id\0tab_id"
    cursor_positions: HashMap<String, (f64, f64)>,
    /// Extension bridge state (set by daemon on startup, `None` if bridge unavailable).
    bridge_state: Option<SharedBridgeState>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry {
            sessions: HashMap::new(),
            ref_caches: HashMap::new(),
            cursor_positions: HashMap::new(),
            bridge_state: None,
        }
    }

    /// Set the extension bridge state handle.
    pub fn set_bridge_state(&mut self, state: SharedBridgeState) {
        self.bridge_state = Some(state);
    }

    /// Get a reference to the bridge state (if bridge is running).
    pub fn bridge_state(&self) -> Option<&SharedBridgeState> {
        self.bridge_state.as_ref()
    }

    fn has_active_session_id(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .is_some_and(|entry| entry.status.is_active())
    }

    pub fn find_local_session_by_profile(
        &self,
        profile: &str,
        mode: Mode,
    ) -> Option<&SessionEntry> {
        self.sessions.values().find(|entry| {
            entry.mode == mode && entry.profile == profile && entry.status.is_active()
        })
    }

    pub fn find_cloud_session_by_endpoint(&self, endpoint: &str) -> Option<&SessionEntry> {
        self.sessions.values().find(|entry| {
            entry.mode == Mode::Cloud
                && entry.status.is_active()
                && entry.cdp_endpoint.as_deref() == Some(endpoint)
        })
    }

    /// Return an active cloud session that was launched via `--provider <name>` and
    /// for the given profile. Only provider-minted sessions are eligible for reuse:
    /// stateless WS-URL overrides (DRIVER_DEV_WS_URL / BROWSER_USE_WS_URL) are stored
    /// with `provider = Some(...)` but `provider_session = None`, and must be
    /// reconnected every time because their config comes from the current shell env.
    /// Reusing them would silently ignore URL/credential changes between starts.
    pub fn find_cloud_session_by_provider(
        &self,
        provider: &str,
        profile: &str,
    ) -> Option<&SessionEntry> {
        let normalized = normalize_provider_name(provider).unwrap_or(provider);
        self.sessions.values().find(|entry| {
            entry.mode == Mode::Cloud
                && entry.status.is_active()
                && entry
                    .provider
                    .as_deref()
                    .unwrap_or_default()
                    == normalized
                && entry.profile == profile
                // Stateless WS-URL override sessions carry no provider handle;
                // force them to reconnect so env changes take effect.
                && entry.provider_session.is_some()
        })
    }

    /// Return the maximum N among active sessions with the given `PREFIX-` pattern.
    fn max_active_prefix_n(&self, prefix: &str) -> u32 {
        self.sessions
            .values()
            .filter(|e| e.status.is_active())
            .filter_map(|e| {
                e.id.as_str()
                    .strip_prefix(prefix)
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0)
    }

    pub fn generate_session_id(
        &mut self,
        set_id: Option<&str>,
    ) -> Result<SessionId, crate::error::CliError> {
        if let Some(id) = set_id {
            let sid = SessionId::new(id)
                .map_err(|e| crate::error::CliError::InvalidSessionId(e.to_string()))?;
            if self.has_active_session_id(sid.as_str()) {
                return Err(crate::error::CliError::SessionIdAlreadyExists(
                    sid.to_string(),
                ));
            }
            return Ok(sid);
        }
        let max_n = self.max_active_prefix_n("s");
        let start = if max_n >= 10000 { 1 } else { max_n + 1 };
        let mut n = start;
        loop {
            let candidate = SessionId::auto_generate(n);
            if !self.has_active_session_id(candidate.as_str()) {
                return Ok(candidate);
            }
            n = if n >= 10000 { 1 } else { n + 1 };
            if n == start {
                return Err(crate::error::CliError::Internal(
                    "all session ID slots exhausted".to_string(),
                ));
            }
        }
    }

    pub fn reserve_session_start(
        &mut self,
        set_id: Option<&str>,
        _requested_profile: Option<&str>,
        resolved_profile: &str,
        mode: Mode,
        headless: bool,
        stealth: bool,
    ) -> Result<SessionId, CliError> {
        if mode == Mode::Local
            && let Some(existing_id) = self
                .find_local_session_by_profile(resolved_profile, mode)
                .map(|entry| entry.id.to_string())
        {
            return Err(CliError::SessionAlreadyExists {
                profile: resolved_profile.to_string(),
                existing_session: existing_id,
            });
        }

        let session_id = self.generate_session_id(set_id)?;
        self.insert(SessionEntry::starting(
            session_id.clone(),
            mode,
            headless,
            stealth,
            resolved_profile.to_string(),
        ));
        Ok(session_id)
    }

    pub fn insert(&mut self, entry: SessionEntry) {
        self.sessions.insert(entry.id.as_str().to_string(), entry);
    }

    pub fn get(&self, session_id: &str) -> Option<&SessionEntry> {
        self.sessions.get(session_id)
    }

    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut SessionEntry> {
        self.sessions.get_mut(session_id)
    }

    pub fn remove(&mut self, session_id: &str) -> Option<SessionEntry> {
        self.sessions.remove(session_id)
    }

    pub fn list(&self) -> Vec<&SessionEntry> {
        self.sessions.values().collect()
    }

    /// Returns `true` if any session is in Starting or Running state.
    pub fn has_active_sessions(&self) -> bool {
        self.sessions.values().any(|entry| entry.status.is_active())
    }

    /// Get url and title for a tab.
    pub fn get_tab_url_title(
        &self,
        session_id: &str,
        tab_id: &str,
    ) -> (Option<String>, Option<String>) {
        self.get(session_id)
            .and_then(|entry| entry.tabs.iter().find(|t| t.id.0 == tab_id))
            .map(|tab| (Some(tab.url.clone()), Some(tab.title.clone())))
            .unwrap_or((None, None))
    }

    /// Get or create a tab-scoped RefCache for stable snapshot refs.
    pub fn take_ref_cache(&mut self, session_id: &str, tab_id: &str) -> RefCache {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.remove(&key).unwrap_or_default()
    }

    /// Store a tab-scoped RefCache back after snapshot.
    pub fn put_ref_cache(&mut self, session_id: &str, tab_id: &str, cache: RefCache) {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.insert(key, cache);
    }

    /// Read-only access to a tab-scoped RefCache (no take/put needed).
    pub fn peek_ref_cache(&self, session_id: &str, tab_id: &str) -> Option<&RefCache> {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.get(&key)
    }

    /// Clear the RefCache for a tab (call on navigation/reload/back/forward).
    /// When the page changes, old backendNodeIds are no longer valid.
    pub fn clear_ref_cache(&mut self, session_id: &str, tab_id: &str) {
        let key = format!("{}\0{}", session_id, tab_id);
        self.ref_caches.remove(&key);
    }

    /// Clear all RefCaches for a session (call on session close/restart).
    pub fn clear_session_ref_caches(&mut self, session_id: &str) {
        let prefix = format!("{}\0", session_id);
        self.ref_caches.retain(|k, _| !k.starts_with(&prefix));
    }

    /// Store the cursor position for a tab.
    pub fn set_cursor_position(&mut self, session_id: &str, tab_id: &str, x: f64, y: f64) {
        let key = format!("{}\0{}", session_id, tab_id);
        self.cursor_positions.insert(key, (x, y));
    }

    /// Get the cursor position for a tab.
    pub fn get_cursor_position(&self, session_id: &str, tab_id: &str) -> Option<(f64, f64)> {
        let key = format!("{}\0{}", session_id, tab_id);
        self.cursor_positions.get(&key).copied()
    }
}

pub type SharedRegistry = Arc<Mutex<SessionRegistry>>;

pub fn new_shared_registry() -> SharedRegistry {
    Arc::new(Mutex::new(SessionRegistry::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_starting(
        registry: &mut SessionRegistry,
        id: &str,
        mode: Mode,
        profile: &str,
        active: bool,
    ) {
        let mut entry = SessionEntry::starting(
            SessionId::new_unchecked(id),
            mode,
            true,
            true,
            profile.to_string(),
        );
        if !active {
            entry.status = SessionState::Closed;
        }
        registry.insert(entry);
    }

    #[test]
    fn reserve_session_start_auto_ids_use_global_counter_not_mode() {
        let mut registry = SessionRegistry::new();

        let s1 = registry
            .reserve_session_start(None, Some("work"), "work", Mode::Local, true, true)
            .expect("reserve first session");
        let s2 = registry
            .reserve_session_start(None, Some("personal"), "personal", Mode::Local, true, true)
            .expect("reserve second session");
        let s3 = registry
            .reserve_session_start(None, Some("shared"), "shared", Mode::Cloud, true, true)
            .expect("reserve third session");
        let s4 = registry
            .reserve_session_start(
                None,
                Some("assistant"),
                "assistant",
                Mode::Extension,
                true,
                true,
            )
            .expect("reserve fourth session");

        assert_eq!(s1.as_str(), "s1");
        assert_eq!(s2.as_str(), "s2");
        assert_eq!(s3.as_str(), "s3");
        assert_eq!(s4.as_str(), "s4");
    }

    #[test]
    fn reserve_session_start_uses_global_max_plus_one() {
        let mut registry = SessionRegistry::new();
        insert_starting(&mut registry, "s7", Mode::Local, "local-7", true);

        let next = registry
            .reserve_session_start(None, Some("fresh"), "fresh", Mode::Cloud, true, true)
            .expect("next session after s7");

        assert_eq!(next.as_str(), "s8");
    }

    #[test]
    fn reserve_session_start_wraps_at_10000_and_skips_collisions() {
        let mut registry = SessionRegistry::new();
        insert_starting(&mut registry, "s10000", Mode::Local, "maxed", true);
        insert_starting(&mut registry, "s1", Mode::Local, "occupied", true);

        let sid = registry
            .reserve_session_start(None, Some("wrap"), "wrap", Mode::Local, true, true)
            .expect("wrapped session");

        assert_eq!(sid.as_str(), "s2");
    }

    #[test]
    fn reserve_session_start_preserves_manual_set_session_id() {
        let mut registry = SessionRegistry::new();

        let sid = registry
            .reserve_session_start(
                Some("manual-session"),
                Some("profile-that-should-not-matter"),
                "profile-that-should-not-matter",
                Mode::Local,
                true,
                true,
            )
            .expect("manual id should bypass auto generation");

        assert_eq!(sid.as_str(), "manual-session");
    }

    #[test]
    fn reserve_session_start_rejects_second_placeholder_for_same_local_profile() {
        let mut registry = SessionRegistry::new();

        let session_id = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true, true)
            .expect("reserve first placeholder");

        let entry = registry
            .get(session_id.as_str())
            .expect("placeholder entry should exist");
        assert_eq!(entry.status, SessionState::Starting);

        let err = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true, true)
            .expect_err("second placeholder should be rejected");

        assert_eq!(err.error_code(), "SESSION_ALREADY_EXISTS");
    }

    #[test]
    fn reserve_session_start_allows_retry_after_placeholder_cleanup() {
        let mut registry = SessionRegistry::new();

        let first = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true, true)
            .expect("reserve first placeholder");
        registry.remove(first.as_str());

        let second = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true, true)
            .expect("retry after cleanup should succeed");

        assert_eq!(
            registry.get(second.as_str()).map(|entry| entry.status),
            Some(SessionState::Starting)
        );
    }

    #[test]
    fn reserve_session_start_rejects_set_id_when_profile_occupied() {
        let mut registry = SessionRegistry::new();

        // Create first session with profile "myprofile"
        registry
            .reserve_session_start(
                Some("first-id"),
                Some("myprofile"),
                "myprofile",
                Mode::Local,
                true,
                true,
            )
            .expect("reserve first session");

        // Try to create second session with SAME profile but DIFFERENT set-session-id
        let err = registry
            .reserve_session_start(
                Some("second-id"),
                Some("myprofile"),
                "myprofile",
                Mode::Local,
                true,
                true,
            )
            .expect_err("should reject: profile already occupied");

        assert_eq!(err.error_code(), "SESSION_ALREADY_EXISTS");
    }

    #[test]
    fn reserve_session_start_ignores_closed_sessions_for_uniqueness() {
        let mut registry = SessionRegistry::new();
        insert_starting(&mut registry, "s1", Mode::Local, "testrace", false);

        let next = registry
            .reserve_session_start(None, Some("testrace"), "testrace", Mode::Local, true, true)
            .expect("closed entry should not block new start");

        assert_eq!(next.as_str(), "s1");
        assert_eq!(
            registry.get(next.as_str()).map(|entry| entry.status),
            Some(SessionState::Starting)
        );
    }

    #[test]
    fn has_active_sessions_empty_registry() {
        let registry = SessionRegistry::new();
        assert!(!registry.has_active_sessions());
    }

    #[test]
    fn has_active_sessions_with_starting_session() {
        let mut registry = SessionRegistry::new();
        registry
            .reserve_session_start(Some("s1"), Some("prof"), "prof", Mode::Local, true, true)
            .unwrap();
        assert!(registry.has_active_sessions());
    }

    #[test]
    fn has_active_sessions_all_closed() {
        let mut registry = SessionRegistry::new();
        let sid = registry
            .reserve_session_start(Some("s1"), Some("prof"), "prof", Mode::Local, true, true)
            .unwrap();
        // Transition to Closed
        if let Some(entry) = registry.get_mut(sid.as_str()) {
            entry.status = SessionState::Closed;
        }
        assert!(!registry.has_active_sessions());
    }

    #[test]
    fn push_tab_with_id_assigns_custom_id() {
        let mut entry = SessionEntry::starting(
            SessionId::new("test-session").unwrap(),
            Mode::Local,
            true,
            true,
            "profile".to_string(),
        );

        let id = entry
            .push_tab_with_id(
                "inbox".to_string(),
                "native-1".to_string(),
                "https://example.com".to_string(),
                "Example".to_string(),
            )
            .expect("should succeed");

        assert_eq!(id, "inbox");
        assert_eq!(entry.tabs.len(), 1);
        assert_eq!(entry.tabs[0].id.0, "inbox");
        assert_eq!(entry.tabs[0].native_id, "native-1");
    }

    #[test]
    fn push_tab_with_id_rejects_duplicate() {
        let mut entry = SessionEntry::starting(
            SessionId::new("test-session").unwrap(),
            Mode::Local,
            true,
            true,
            "profile".to_string(),
        );

        entry.push_tab(
            "native-1".to_string(),
            "https://a.com".to_string(),
            String::new(),
        );
        // The auto-assigned ID is "t1"
        assert_eq!(entry.tabs[0].id.0, "t1");

        let err = entry
            .push_tab_with_id(
                "t1".to_string(),
                "native-2".to_string(),
                "https://b.com".to_string(),
                String::new(),
            )
            .expect_err("should reject duplicate tab ID");

        match err {
            ActionResult::Fatal { code, .. } => assert_eq!(code, "TAB_ID_CONFLICT"),
            _ => panic!("expected Fatal with TAB_ID_CONFLICT"),
        }
        assert_eq!(entry.tabs.len(), 1, "no tab should be added on conflict");
    }

    #[test]
    fn push_tab_with_id_does_not_affect_auto_counter() {
        let mut entry = SessionEntry::starting(
            SessionId::new("test-session").unwrap(),
            Mode::Local,
            true,
            true,
            "profile".to_string(),
        );

        // Custom ID first
        entry
            .push_tab_with_id(
                "custom".to_string(),
                "native-1".to_string(),
                "https://a.com".to_string(),
                String::new(),
            )
            .unwrap();

        // Auto-assigned should still start at t1
        entry.push_tab(
            "native-2".to_string(),
            "https://b.com".to_string(),
            String::new(),
        );
        assert_eq!(entry.tabs[1].id.0, "t1");
    }

    #[test]
    fn drop_session_entry_kills_chrome_process() {
        use std::process::Command;

        // Spawn a long-lived process (cross-platform).
        #[cfg(unix)]
        let child = Command::new("sleep")
            .arg("3600")
            .spawn()
            .expect("spawn sleep");
        #[cfg(windows)]
        let child = Command::new("ping")
            .args(["-n", "3600", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn ping");

        let pid = child.id();

        // Verify process is alive.
        #[cfg(unix)]
        assert!(
            Command::new("kill")
                .args(["-0", &pid.to_string()])
                .output()
                .is_ok_and(|o| o.status.success()),
            "process should be alive before drop"
        );
        #[cfg(windows)]
        assert!(
            Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .is_ok_and(|o| { String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()) }),
            "process should be alive before drop"
        );

        // Create a SessionEntry with the child process and drop it
        {
            let mut entry = SessionEntry::starting(
                crate::types::SessionId::new("drop-test").unwrap(),
                Mode::Local,
                true,
                true,
                "test-profile".to_string(),
            );
            entry.chrome_process = Some(child);
            // entry is dropped here
        }

        // Give kill a moment to take effect on Windows.
        #[cfg(windows)]
        std::thread::sleep(std::time::Duration::from_millis(500));

        // After drop, the process must be dead.
        #[cfg(unix)]
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .is_ok_and(|o| o.status.success());
        #[cfg(windows)]
        let alive = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()));
        assert!(
            !alive,
            "Chrome process should be killed when SessionEntry is dropped"
        );
    }

    #[test]
    fn find_cloud_session_by_provider_ignores_ws_url_overrides() {
        // Stateless WS-URL overrides (DRIVER_DEV_WS_URL / BROWSER_USE_WS_URL)
        // store `provider = Some(...)` so list/status UIs still show the tag,
        // but MUST NOT be reused by `browser start -p <name>` — the user may
        // have pointed the env var at a new endpoint or rotated a key, and
        // silent reuse would keep attaching to the old remote browser.
        let mut registry = SessionRegistry::new();
        let mut entry = SessionEntry::starting(
            SessionId::new("override-1").unwrap(),
            Mode::Cloud,
            false,
            true,
            "actionbook".to_string(),
        );
        entry.status = SessionState::Running;
        entry.provider = Some("driver".to_string());
        entry.provider_session = None; // ← override path leaves this unset
        registry.insert(entry);

        assert!(
            registry
                .find_cloud_session_by_provider("driver", "actionbook")
                .is_none(),
            "override sessions (no provider_session handle) must not be reused"
        );

        // Sanity: a provider-minted session with a handle does get reused.
        let mut minted = SessionEntry::starting(
            SessionId::new("minted-1").unwrap(),
            Mode::Cloud,
            false,
            true,
            "actionbook".to_string(),
        );
        minted.status = SessionState::Running;
        minted.provider = Some("driver".to_string());
        minted.provider_session = Some(ProviderSession {
            provider: "driver".to_string(),
            session_id: "remote-abc".to_string(),
            provider_env: Default::default(),
        });
        registry.insert(minted);

        let found = registry
            .find_cloud_session_by_provider("driver", "actionbook")
            .expect("provider-minted session should be reusable");
        assert_eq!(found.id.as_str(), "minted-1");
    }
}
