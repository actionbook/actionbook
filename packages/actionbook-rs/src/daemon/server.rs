use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot, watch, Mutex};

use super::lifecycle;
use super::protocol::{self, DaemonRequest, DaemonResponse};
use crate::error::{ActionbookError, Result};

/// Default idle timeout: daemon exits if no UDS client connects within this duration.
/// Override with `ACTIONBOOK_DAEMON_IDLE_TIMEOUT_MS` environment variable.
const DEFAULT_IDLE_TIMEOUT_MS: u64 = 600_000; // 10 minutes

/// Base WS reconnect delay (doubles on each consecutive failure).
const WS_RECONNECT_BASE_MS: u64 = 1_000; // 1 second

/// Maximum WS reconnect delay cap.
const WS_RECONNECT_MAX_MS: u64 = 16_000; // 16 seconds

/// Maximum WS reconnect attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// Read the idle timeout from env or use the default.
fn idle_timeout() -> Duration {
    let ms = std::env::var("ACTIONBOOK_DAEMON_IDLE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_MS);
    Duration::from_millis(ms)
}

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// Run the daemon event loop for a given profile.
///
/// 1. Bind UDS at `~/.actionbook/daemons/{profile}.sock`
/// 2. Write PID file
/// 3. Load session state → establish persistent WS to CDP endpoint
/// 4. Accept UDS connections, route CDP requests through the persistent WS
/// 5. Exit on idle timeout or SIGTERM
pub async fn run(profile: &str) -> Result<()> {
    let config = crate::config::Config::load()?;

    // Resolve actual profile name
    let resolved_profile = if profile.is_empty() {
        config.effective_default_profile_name()
    } else {
        profile.to_string()
    };

    // Prepare UDS socket
    let sock_path = lifecycle::socket_path(&resolved_profile);
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Remove stale socket file if it exists
    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).map_err(|e| {
        ActionbookError::DaemonError(format!("Failed to bind UDS at {}: {}", sock_path.display(), e))
    })?;

    // Write PID file
    lifecycle::write_pid(&resolved_profile, std::process::id())?;

    tracing::info!(
        "Daemon started for profile '{}' (PID {}), socket: {}",
        resolved_profile,
        std::process::id(),
        sock_path.display()
    );

    // Set up SIGTERM handler for graceful shutdown
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_signal = shutdown.clone();

    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|e| ActionbookError::DaemonError(format!("Signal handler failed: {}", e)))?;
        tokio::spawn(async move {
            sigterm.recv().await;
            shutdown_signal.notify_waiters();
        });
    }

    // Connect to the persistent WS
    let ws_state = Arc::new(WsState::new(resolved_profile.clone()));
    let ws_state_clone = ws_state.clone();

    // Spawn WS connection manager
    let profile_for_ws = resolved_profile.clone();
    let shutdown_for_ws = shutdown.clone();
    tokio::spawn(async move {
        ws_connection_loop(
            &profile_for_ws,
            ws_state_clone,
            shutdown_for_ws,
        )
        .await;
    });

    // Accept UDS connections with idle timeout
    let idle_reset = Arc::new(tokio::sync::Notify::new());

    loop {
        let idle_reset_clone = idle_reset.clone();

        tokio::select! {
            // Accept a new UDS client
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        idle_reset.notify_one();
                        let ws_state = ws_state.clone();
                        let idle_notify = idle_reset_clone;
                        tokio::spawn(async move {
                            if let Err(e) = handle_uds_client(stream, ws_state, idle_notify).await {
                                tracing::warn!("UDS client error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("UDS accept error: {}", e);
                    }
                }
            }

            // Idle timeout
            _ = async {
                let timeout = idle_timeout();
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => break,
                        _ = idle_reset.notified() => continue,
                    }
                }
            } => {
                tracing::info!("Daemon idle timeout reached, shutting down");
                break;
            }

            // Shutdown signal
            _ = shutdown.notified() => {
                tracing::info!("Daemon received shutdown signal");
                break;
            }
        }
    }

    // Cleanup
    lifecycle::cleanup_files(&resolved_profile);
    tracing::info!("Daemon for profile '{}' exiting", resolved_profile);
    Ok(())
}

/// Session state loaded from disk — mirrors the essential fields of
/// `browser::session::SessionState` without depending on its private type.
#[derive(Debug, serde::Deserialize)]
struct SessionInfo {
    cdp_url: String,
    #[serde(default)]
    active_page_id: Option<String>,
    #[serde(default)]
    ws_headers: Option<HashMap<String, String>>,
}

impl SessionInfo {
    /// Whether this session uses local HTTP endpoints for page discovery.
    fn uses_local_http_endpoints(&self) -> bool {
        let authority = self.cdp_url.split("://").nth(1).and_then(|s| s.split('/').next());
        let Some(authority) = authority else { return false };
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = if host.starts_with('[') {
            host.split(']').next().unwrap_or(host).trim_start_matches('[')
        } else {
            host.split(':').next().unwrap_or(host)
        };
        let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
        is_loopback && self.cdp_url.contains("/devtools/browser/")
    }
}

/// Shared WS state: a sender channel for outgoing CDP messages,
/// and a pending response map keyed by request ID.
struct WsState {
    /// Channel to send WsCommands to the WS writer task.
    tx: Mutex<Option<mpsc::Sender<WsCommand>>>,
    /// Pending request map: ws_id → oneshot sender for the response.
    pending: PendingMap,
    /// Monotonically increasing CDP message ID (WS-level).
    next_ws_id: AtomicU64,
    /// The CDP session ID obtained from Target.attachToTarget, if attached.
    session_id: Mutex<Option<String>>,
    /// The target ID we're currently attached to.
    attached_target_id: Mutex<Option<String>>,
    /// Serializes the entire detach→attach sequence in maybe_reattach().
    /// Without this, concurrent UDS clients could each trigger a separate
    /// detach+attach race, orphaning the intermediate target sessions.
    reattach_lock: Mutex<()>,
    /// Profile name for re-reading session state.
    profile: String,
    /// State-based readiness: `true` once `connect_and_run` finishes initial
    /// attach and sets session_id + tx. Reset to `false` on WS disconnect.
    /// Unlike `Notify`, `watch` always reflects the latest value — late
    /// arrivals see the current state without missing a signal.
    ready_tx: watch::Sender<bool>,
    ready_rx: watch::Receiver<bool>,
}

/// Command sent to the WS writer task.
struct WsCommand {
    ws_id: u64,
    method: String,
    params: Value,
    /// If Some, include "sessionId" in the CDP frame (page-scoped command).
    session_id: Option<String>,
}

impl WsState {
    fn new(profile: String) -> Self {
        let (ready_tx, ready_rx) = watch::channel(false);
        Self {
            tx: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_ws_id: AtomicU64::new(1),
            session_id: Mutex::new(None),
            attached_target_id: Mutex::new(None),
            reattach_lock: Mutex::new(()),
            profile,
            ready_tx,
            ready_rx,
        }
    }

    /// Send a CDP command through the persistent WS and wait for the response.
    ///
    /// For page-scoped methods (Runtime.*, DOM.*, Page.*, Input.*, etc.),
    /// the command is sent through the attached target session.
    ///
    /// Before each page-scoped command, re-reads `active_page_id` from the
    /// session file on disk. If it changed (e.g. after `browser switch`),
    /// detaches from the old target and re-attaches to the new one.
    async fn send_cdp(
        &self,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, String> {
        // Wait for initial attach to complete (connect_and_run sets tx + session_id
        // then sends `true` on ready_tx). The watch channel is state-based: if
        // connect_and_run already completed, the current value is `true` and
        // wait_for returns immediately.
        if self.tx.lock().await.is_none() {
            let is_ready = tokio::time::timeout(Duration::from_secs(10), async {
                let mut rx = self.ready_rx.clone();
                // Check current value first
                if *rx.borrow() {
                    return true;
                }
                // Wait for changes until ready
                loop {
                    if rx.changed().await.is_err() {
                        return false; // channel closed
                    }
                    if *rx.borrow() {
                        return true;
                    }
                }
            })
            .await;
            match is_ready {
                Ok(true) => {}
                Ok(false) => {
                    return Err("Daemon readiness channel closed".to_string());
                }
                Err(_) => {
                    return Err(
                        "Daemon WS not ready after 10s (initial connect still in progress)"
                            .to_string(),
                    );
                }
            }
        }

        let ws_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);

        // Determine if this method needs a target session
        let session_id = if is_browser_level_method(method) {
            None
        } else {
            // Check if the active page has changed since our last attach
            self.maybe_reattach().await?;

            let guard = self.session_id.lock().await;
            match guard.as_ref() {
                Some(sid) => Some(sid.clone()),
                None => {
                    return Err(format!(
                        "No target attached for page-scoped method '{}'. \
                         The daemon has not yet attached to a browser target.",
                        method
                    ));
                }
            }
        };

        // Register the oneshot for this request
        let (resp_tx, resp_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(ws_id, resp_tx);
        }

        // Send through the writer channel
        let tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };

        let tx = match tx {
            Some(tx) => tx,
            None => {
                // Clean up the pending entry before returning
                self.pending.lock().await.remove(&ws_id);
                return Err("Daemon WS not connected".to_string());
            }
        };
        if let Err(_) = tx.send(WsCommand {
            ws_id,
            method: method.to_string(),
            params,
            session_id,
        })
        .await
        {
            // Writer channel closed — clean up the pending entry
            self.pending.lock().await.remove(&ws_id);
            return Err("Daemon WS writer channel closed".to_string());
        }

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(30), resp_rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err("Response channel dropped".to_string()),
            Err(_) => {
                // Clean up pending entry
                self.pending.lock().await.remove(&ws_id);
                Err("CDP command timed out after 30s".to_string())
            }
        }
    }

    /// Check if `active_page_id` on disk differs from the currently attached target.
    /// If so, detach from the old target and re-attach to the new one via the
    /// persistent WS connection.
    ///
    /// Serialized by `reattach_lock` — concurrent callers wait rather than
    /// each triggering their own detach+attach cycle.
    async fn maybe_reattach(&self) -> std::result::Result<(), String> {
        // Acquire the reattach lock so only one caller runs the
        // detach→attach sequence at a time. Others will wait and then
        // see the updated attached_target_id, hitting the early return.
        let _guard = self.reattach_lock.lock().await;

        let current_target = self.attached_target_id.lock().await.clone();
        let Some(current_target) = current_target else {
            // Not attached yet — connect_and_run hasn't finished attach
            return Ok(());
        };

        // Re-read active_page_id from disk
        let session_info = load_session_info(&self.profile);
        let desired_target = session_info
            .as_ref()
            .and_then(|s| s.active_page_id.as_deref())
            .unwrap_or_default();

        if desired_target.is_empty() || desired_target == current_target {
            return Ok(()); // No change
        }

        // Grab the old sessionId before we replace it — needed for detach
        let old_session_id = self.session_id.lock().await.clone();

        tracing::info!(
            "Active page changed: {} → {}, re-attaching",
            current_target,
            desired_target
        );

        let tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };
        let tx = tx.ok_or_else(|| "WS not connected during re-attach".to_string())?;

        // Step 1: Detach from the old target session to avoid orphaned sessions.
        // Best-effort — if detach fails (e.g. target already closed), we still
        // proceed with attach.
        if let Some(old_sid) = &old_session_id {
            let detach_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);
            let (detach_tx, detach_rx) = oneshot::channel();
            {
                let mut pending = self.pending.lock().await;
                pending.insert(detach_id, detach_tx);
            }

            let _ = tx
                .send(WsCommand {
                    ws_id: detach_id,
                    method: "Target.detachFromTarget".to_string(),
                    params: serde_json::json!({
                        "sessionId": old_sid
                    }),
                    session_id: None,
                })
                .await;

            // Wait briefly — don't block re-attach on a slow detach
            match tokio::time::timeout(Duration::from_secs(3), detach_rx).await {
                Ok(Ok(resp)) => {
                    if let Some(err) = resp.get("error") {
                        tracing::debug!(
                            "Detach from old session {} failed (non-fatal): {}",
                            old_sid,
                            err
                        );
                    } else {
                        tracing::debug!("Detached old session {}", old_sid);
                    }
                }
                _ => {
                    tracing::debug!("Detach from old session {} timed out (non-fatal)", old_sid);
                    self.pending.lock().await.remove(&detach_id);
                }
            }
        }

        // Step 2: Attach to the new target
        let ws_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);
        let (resp_tx, resp_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(ws_id, resp_tx);
        }

        // Send attach as a browser-level command (no sessionId)
        tx.send(WsCommand {
            ws_id,
            method: "Target.attachToTarget".to_string(),
            params: serde_json::json!({
                "targetId": desired_target,
                "flatten": true
            }),
            session_id: None,
        })
        .await
        .map_err(|_| "WS writer closed during re-attach".to_string())?;

        // Wait for response
        let result = tokio::time::timeout(Duration::from_secs(10), resp_rx)
            .await
            .map_err(|_| "Re-attach timed out".to_string())?
            .map_err(|_| "Re-attach response channel dropped".to_string())?;

        // Extract new sessionId
        if let Some(err) = result.get("error") {
            return Err(format!("Re-attach failed: {}", err));
        }

        let new_session_id = result
            .get("sessionId")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No sessionId in re-attach response".to_string())?;

        // Update cached state
        *self.session_id.lock().await = Some(new_session_id.clone());
        *self.attached_target_id.lock().await = Some(desired_target.to_string());

        tracing::info!(
            "Re-attached to target {} with sessionId: {}",
            desired_target,
            new_session_id
        );

        Ok(())
    }
}

/// Returns true for CDP methods that operate at the browser level
/// (don't need a target session). Everything else is page-scoped.
fn is_browser_level_method(method: &str) -> bool {
    matches!(
        method,
        "Target.getTargets"
            | "Target.createTarget"
            | "Target.closeTarget"
            | "Target.attachToTarget"
            | "Target.detachFromTarget"
            | "Target.activateTarget"
            | "Target.setDiscoverTargets"
            | "Browser.getVersion"
            | "Browser.close"
    )
}

/// Persistent WS connection loop with auto-reconnect.
///
/// On each connect:
/// 1. Reads SessionState from disk (picks up fresh auth headers from `connect` command)
/// 2. Establishes WS connection to the browser endpoint
/// 3. Resolves the active page target and calls Target.attachToTarget
/// 4. Stores the sessionId for subsequent page-scoped commands
async fn ws_connection_loop(
    profile: &str,
    ws_state: Arc<WsState>,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let mut reconnect_attempts = 0u32;

    loop {
        // Load fresh session info from disk
        let session_info = match load_session_info(profile) {
            Some(info) => info,
            None => {
                tracing::warn!("No session state found for profile '{}', retrying...", profile);
                // Compute delay *before* incrementing so first attempt uses base delay (1s)
                let delay_ms = (WS_RECONNECT_BASE_MS << reconnect_attempts.min(4))
                    .min(WS_RECONNECT_MAX_MS);
                reconnect_attempts += 1;
                if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                    tracing::error!("Max reconnect attempts reached (no session state), daemon exiting");
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => continue,
                    _ = shutdown.notified() => break,
                }
            }
        };

        tracing::info!(
            "Connecting to WS: {} (attempt {})",
            session_info.cdp_url,
            reconnect_attempts + 1
        );

        match connect_and_run(&session_info, &ws_state, &shutdown).await {
            Ok(()) => {
                // Graceful shutdown requested
                break;
            }
            Err(e) => {
                // Check if the WS was fully connected (session_id set) before
                // clearing state — distinguishes "connection failed" from
                // "connection dropped after running".
                let was_connected = ws_state.session_id.lock().await.is_some();

                tracing::warn!("WS connection lost: {}", e);
                *ws_state.tx.lock().await = None;
                *ws_state.session_id.lock().await = None;
                *ws_state.attached_target_id.lock().await = None;
                // Reset readiness so new callers wait for reconnect
                let _ = ws_state.ready_tx.send(false);

                // Drain all pending requests immediately so callers get an
                // error right away instead of waiting until their timeout.
                {
                    let mut pending = ws_state.pending.lock().await;
                    let count = pending.len();
                    for (_, tx) in pending.drain() {
                        let _ = tx.send(serde_json::json!({"error": "WS connection lost"}));
                    }
                    if count > 0 {
                        tracing::info!("Drained {} pending requests after WS disconnect", count);
                    }
                }

                // Reset counter when the connection was previously working —
                // only count *consecutive* connection failures.
                if was_connected {
                    reconnect_attempts = 0;
                }

                // Compute delay *before* incrementing: 1s → 2s → 4s → 8s → 16s (capped)
                let delay_ms = (WS_RECONNECT_BASE_MS << reconnect_attempts.min(4))
                    .min(WS_RECONNECT_MAX_MS);

                if !was_connected {
                    reconnect_attempts += 1;
                }
                if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                    tracing::error!("Max reconnect attempts reached, daemon exiting");
                    break;
                }
                tracing::info!("Reconnecting in {}ms...", delay_ms);
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => continue,
                    _ = shutdown.notified() => break,
                }
            }
        }
    }
}

/// Load session info from disk.
fn load_session_info(profile: &str) -> Option<SessionInfo> {
    let sessions_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".actionbook")
        .join("sessions");
    let session_file = sessions_dir.join(format!("{}.json", profile));
    let content = std::fs::read_to_string(session_file).ok()?;
    serde_json::from_str(&content).ok()
}

/// Resolve the active page target ID.
///
/// For local sessions, uses HTTP `/json/list`.
/// For remote sessions, uses `Target.getTargets` over the WS.
async fn resolve_active_target(
    session_info: &SessionInfo,
    ws_write: &Mutex<futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        tokio_tungstenite::tungstenite::Message,
    >>,
    ws_read: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    >,
) -> std::result::Result<String, String> {
    if session_info.uses_local_http_endpoints() {
        // Extract port from cdp_url
        let port = session_info
            .cdp_url
            .split("://")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .and_then(|hp| hp.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(9222);

        let url = format!("http://127.0.0.1:{}/json/list", port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let resp = client.get(&url).send().await.map_err(|e| format!("HTTP /json/list failed: {}", e))?;
        let pages: Vec<Value> = resp.json().await.map_err(|e| format!("Parse /json/list failed: {}", e))?;

        // Find the active page or fall back to first "page" type
        let active_id = session_info.active_page_id.as_deref();
        for page in &pages {
            if page.get("type").and_then(|v| v.as_str()) != Some("page") {
                continue;
            }
            let id = page.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            if active_id.is_some_and(|aid| aid == id) {
                return Ok(id.to_string());
            }
        }
        // Fall back to first page
        pages
            .iter()
            .find(|p| p.get("type").and_then(|v| v.as_str()) == Some("page"))
            .and_then(|p| p.get("id").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .ok_or_else(|| "No page targets found via /json/list".to_string())
    } else {
        // Remote: use Target.getTargets over the existing WS
        let cmd = serde_json::json!({
            "id": 0,
            "method": "Target.getTargets",
            "params": {}
        });
        {
            let mut writer = ws_write.lock().await;
            writer
                .send(tokio_tungstenite::tungstenite::Message::Text(cmd.to_string().into()))
                .await
                .map_err(|e| format!("WS send Target.getTargets failed: {}", e))?;
        }

        // Read response for id=0
        while let Some(msg) = ws_read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    if let Ok(obj) = serde_json::from_str::<Value>(text.as_str()) {
                        if obj.get("id").and_then(|v| v.as_u64()) == Some(0) {
                            let targets = obj
                                .get("result")
                                .and_then(|r| r.get("targetInfos"))
                                .and_then(|t| t.as_array());

                            if let Some(targets) = targets {
                                let active_id = session_info.active_page_id.as_deref();
                                // Try active page first
                                if let Some(aid) = active_id {
                                    if let Some(t) = targets.iter().find(|t| {
                                        t.get("targetId").and_then(|v| v.as_str()) == Some(aid)
                                            && t.get("type").and_then(|v| v.as_str()) == Some("page")
                                    }) {
                                        return Ok(t["targetId"].as_str().unwrap().to_string());
                                    }
                                }
                                // Fall back to first page
                                if let Some(t) = targets.iter().find(|t| {
                                    t.get("type").and_then(|v| v.as_str()) == Some("page")
                                }) {
                                    return Ok(
                                        t.get("targetId")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default()
                                            .to_string(),
                                    );
                                }
                            }
                            return Err("No page targets found via Target.getTargets".to_string());
                        }
                    }
                    // Skip events and other responses
                }
                Err(e) => return Err(format!("WS read error during target resolution: {}", e)),
                _ => {} // ping/pong/binary
            }
        }
        Err("WS stream ended without Target.getTargets response".to_string())
    }
}

/// Establish a WS connection, attach to the active page target, and run reader/writer loops.
///
/// Returns `Ok(())` on graceful shutdown, `Err(msg)` on connection failure.
async fn connect_and_run(
    session_info: &SessionInfo,
    ws_state: &WsState,
    shutdown: &tokio::sync::Notify,
) -> std::result::Result<(), String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut request = session_info
        .cdp_url
        .as_str()
        .into_client_request()
        .map_err(|e| format!("Bad WS URL: {}", e))?;

    if let Some(hdrs) = session_info.ws_headers.as_ref().filter(|h| !h.is_empty()) {
        for (key, value) in hdrs {
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::HeaderName::try_from(key.as_str())
                    .map_err(|e| format!("Bad header name: {}", e))?,
                tokio_tungstenite::tungstenite::http::HeaderValue::from_str(value)
                    .map_err(|e| format!("Bad header value: {}", e))?,
            );
        }
    }

    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("WS connection failed: {}", e))?;

    let (ws_write, mut ws_read) = ws.split();
    let ws_write = Arc::new(Mutex::new(ws_write));

    // Step 1: Resolve the active page target ID
    let target_id = resolve_active_target(session_info, &ws_write, &mut ws_read).await?;
    tracing::info!("Resolved active target: {}", target_id);

    // Step 2: Attach to the target (flatten=true gives us a sessionId)
    // Use a distinct reserved ID that won't collide with resolve_active_target's id=0.
    // Must stay below Number.MAX_SAFE_INTEGER (2^53 - 1) to avoid JSON precision loss.
    let attach_ws_id = 999_999_999u64;
    let attach_cmd = serde_json::json!({
        "id": attach_ws_id,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    });

    {
        let mut writer = ws_write.lock().await;
        writer
            .send(tokio_tungstenite::tungstenite::Message::Text(
                attach_cmd.to_string().into(),
            ))
            .await
            .map_err(|e| format!("WS send attach failed: {}", e))?;
    }

    // Wait for attach response
    let mut session_id: Option<String> = None;
    while let Some(msg) = ws_read.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(obj) = serde_json::from_str::<Value>(text.as_str()) {
                    if obj.get("id").and_then(|v| v.as_u64()) == Some(attach_ws_id) {
                        if let Some(err) = obj.get("error") {
                            return Err(format!("Target.attachToTarget failed: {}", err));
                        }
                        session_id = obj
                            .get("result")
                            .and_then(|r| r.get("sessionId"))
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string());
                        break;
                    }
                }
                // Skip events during attach
            }
            Err(e) => return Err(format!("WS read error during attach: {}", e)),
            _ => {}
        }
    }

    let session_id = session_id
        .ok_or_else(|| "No sessionId returned by Target.attachToTarget".to_string())?;
    tracing::info!("Attached to target {} with sessionId: {}", target_id, session_id);

    // Store session_id and target_id in WsState
    *ws_state.session_id.lock().await = Some(session_id);
    *ws_state.attached_target_id.lock().await = Some(target_id);

    // Create writer channel
    let (tx, mut rx) = mpsc::channel::<WsCommand>(64);
    *ws_state.tx.lock().await = Some(tx);

    // Signal that the daemon is ready to accept commands.
    // Any send_cdp() calls waiting on ready_rx.wait_for() will proceed.
    // Late arrivals also see `true` immediately (watch is state-based).
    let _ = ws_state.ready_tx.send(true);

    // Writer task: receives WsCommand and sends CDP JSON over WS
    let ws_write_clone = ws_write.clone();
    let writer_handle = tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            let mut frame = serde_json::json!({
                "id": cmd.ws_id,
                "method": cmd.method,
                "params": cmd.params,
            });
            // For page-scoped commands, include the sessionId
            if let Some(sid) = cmd.session_id {
                frame.as_object_mut().unwrap().insert(
                    "sessionId".to_string(),
                    Value::String(sid),
                );
            }
            let msg = tokio_tungstenite::tungstenite::Message::Text(
                frame.to_string().into(),
            );
            let mut writer = ws_write_clone.lock().await;
            if let Err(e) = writer.send(msg).await {
                tracing::error!("WS write error: {}", e);
                break;
            }
        }
    });

    // Reader task: reads WS messages and routes responses by ID
    let pending = ws_state.pending.clone();
    let reader_result: std::result::Result<(), String> = loop {
        tokio::select! {
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        // Try to parse as a CDP response with an "id" field
                        if let Ok(obj) = serde_json::from_str::<Value>(text.as_str()) {
                            if let Some(id) = obj.get("id").and_then(|v| v.as_u64()) {
                                // This is a response — route to pending
                                let mut pending = pending.lock().await;
                                if let Some(tx) = pending.remove(&id) {
                                    let result = if let Some(err) = obj.get("error") {
                                        serde_json::json!({"error": err})
                                    } else {
                                        obj.get("result").cloned().unwrap_or(Value::Null)
                                    };
                                    let _ = tx.send(result);
                                }
                            }
                            // else: CDP Event (has "method" but no "id") — discard
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                        break Err("WS connection closed by remote".to_string());
                    }
                    Some(Err(e)) => {
                        break Err(format!("WS read error: {}", e));
                    }
                    None => {
                        break Err("WS stream ended".to_string());
                    }
                    _ => {} // Ignore ping/pong/binary
                }
            }
            _ = shutdown.notified() => {
                // Graceful shutdown: close the WS
                let mut writer = ws_write.lock().await;
                let _ = writer.close().await;
                break Ok(());
            }
        }
    };

    writer_handle.abort();
    reader_result
}

/// Handle a single UDS client connection.
///
/// Reads JSON-line requests, routes them through the persistent WS,
/// and writes JSON-line responses back.
async fn handle_uds_client(
    stream: tokio::net::UnixStream,
    ws_state: Arc<WsState>,
    idle_notify: Arc<tokio::sync::Notify>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        idle_notify.notify_one();

        let request: DaemonRequest = match protocol::decode_line(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = DaemonResponse::err(0, format!("Invalid request: {}", e));
                let encoded = protocol::encode_line(&resp).unwrap_or_default();
                let _ = writer.write_all(encoded.as_bytes()).await;
                continue;
            }
        };

        let id = request.id;
        let result = ws_state.send_cdp(&request.method, request.params).await;

        let resp = match result {
            Ok(value) => {
                // Check if the value itself contains an error
                if let Some(err) = value.get("error") {
                    DaemonResponse::err(id, format!("CDP error: {}", err))
                } else {
                    DaemonResponse::ok(id, value)
                }
            }
            Err(e) => DaemonResponse::err(id, e),
        };

        let encoded = protocol::encode_line(&resp)
            .map_err(|e| ActionbookError::DaemonError(format!("Failed to encode response: {}", e)))?;
        writer.write_all(encoded.as_bytes()).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_level_methods_are_classified_correctly() {
        assert!(is_browser_level_method("Target.getTargets"));
        assert!(is_browser_level_method("Target.createTarget"));
        assert!(is_browser_level_method("Browser.getVersion"));
        assert!(!is_browser_level_method("Runtime.evaluate"));
        assert!(!is_browser_level_method("DOM.getDocument"));
        assert!(!is_browser_level_method("Page.navigate"));
        assert!(!is_browser_level_method("Input.dispatchMouseEvent"));
        assert!(!is_browser_level_method("Accessibility.getFullAXTree"));
    }

    #[test]
    fn session_info_local_detection() {
        let local = SessionInfo {
            cdp_url: "ws://127.0.0.1:9222/devtools/browser/abc-123".to_string(),
            active_page_id: None,
            ws_headers: None,
        };
        assert!(local.uses_local_http_endpoints());

        let remote = SessionInfo {
            cdp_url: "wss://agent.example.com/automation".to_string(),
            active_page_id: None,
            ws_headers: None,
        };
        assert!(!remote.uses_local_http_endpoints());
    }

    /// Verify that the attach_ws_id used in connect_and_run is within
    /// JSON Number.MAX_SAFE_INTEGER (2^53 - 1), preventing precision loss
    /// when Chrome echoes the ID back in its JSON response.
    #[test]
    fn attach_ws_id_is_json_safe() {
        let attach_ws_id = 999_999_999u64;
        let max_safe_integer: u64 = (1u64 << 53) - 1; // 9007199254740991
        assert!(
            attach_ws_id <= max_safe_integer,
            "attach_ws_id ({}) must be <= Number.MAX_SAFE_INTEGER ({})",
            attach_ws_id,
            max_safe_integer
        );

        // Verify JSON round-trip preserves the exact value
        let json = serde_json::json!({"id": attach_ws_id});
        let parsed_id = json.get("id").and_then(|v| v.as_u64()).unwrap();
        assert_eq!(parsed_id, attach_ws_id);
    }

    /// Ensure attach_ws_id doesn't collide with resolve_active_target's id=0
    #[test]
    fn attach_ws_id_does_not_collide_with_resolve_target_id() {
        let resolve_target_id = 0u64;
        let attach_ws_id = 999_999_999u64;
        assert_ne!(attach_ws_id, resolve_target_id);
    }
}
