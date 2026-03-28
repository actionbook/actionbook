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
use crate::browser::cdp_types::{JavascriptDialogOpeningEvent, PendingDialog};
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
#[allow(dead_code)]
pub async fn run(profile: &str) -> Result<()> {
    run_with_session(profile, None).await
}

/// Run the daemon for a specific profile.
/// One daemon per profile; all sessions share the same browser via the sessions HashMap.
/// The `session` parameter is accepted for CLI compatibility but ignored — session
/// routing is handled internally by the daemon's session routing table.
pub async fn run_with_session(profile: &str, _session: Option<&str>) -> Result<()> {
    let config = crate::config::Config::load()?;

    // Resolve actual profile name
    let resolved_profile = if profile.is_empty() {
        config.effective_default_profile_name()
    } else {
        profile.to_string()
    };

    // Prepare UDS socket — profile-level (one daemon per profile)
    let sock_path = lifecycle::socket_path(&resolved_profile);
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Remove stale socket file if it exists
    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).map_err(|e| {
        ActionbookError::DaemonError(format!(
            "Failed to bind UDS at {}: {}",
            sock_path.display(),
            e
        ))
    })?;

    // Write PID file — profile-level
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
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
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
        ws_connection_loop(&profile_for_ws, ws_state_clone, shutdown_for_ws).await;
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

    // Cleanup — profile-level
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
        let authority = self
            .cdp_url
            .split("://")
            .nth(1)
            .and_then(|s| s.split('/').next());
        let Some(authority) = authority else {
            return false;
        };
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = if host.starts_with('[') {
            host.split(']')
                .next()
                .unwrap_or(host)
                .trim_start_matches('[')
        } else {
            host.split(':').next().unwrap_or(host)
        };
        let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
        is_loopback && self.cdp_url.contains("/devtools/browser/")
    }
}

/// Per-session entry tracking the CDP sessionId and target for one named session.
struct SessionEntry {
    cdp_session_id: String,
    target_id: String,
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
    /// Multi-session routing table: session_name → SessionEntry.
    /// The "default" session is created on initial connect.
    sessions: Mutex<HashMap<String, SessionEntry>>,
    /// Profile name for re-reading session state.
    profile: String,
    /// State-based readiness: `true` once `connect_and_run` finishes initial
    /// attach and sets session_id + tx. Reset to `false` on WS disconnect.
    /// Unlike `Notify`, `watch` always reflects the latest value — late
    /// arrivals see the current state without missing a signal.
    ready_tx: watch::Sender<bool>,
    ready_rx: watch::Receiver<bool>,
    /// Tracks currently open JavaScript dialogs per CDP session ID.
    /// Updated by CDP events Page.javascriptDialogOpening / Page.javascriptDialogClosed.
    /// Keyed by CDP sessionId so multi-session flows don't cross-contaminate.
    pending_dialogs: Arc<Mutex<HashMap<String, PendingDialog>>>,
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
            sessions: Mutex::new(HashMap::new()),
            profile,
            ready_tx,
            ready_rx,
            pending_dialogs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Send a CDP command through the persistent WS for a specific named session.
    ///
    /// For page-scoped methods (Runtime.*, DOM.*, Page.*, Input.*, etc.),
    /// the command is sent through the attached target session.
    ///
    /// Before each page-scoped command, re-reads `active_page_id` from the
    /// session file on disk. If it changed (e.g. after `browser switch`),
    /// detaches from the old target and re-attaches to the new one.
    async fn send_cdp(
        &self,
        session_name: &str,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, String> {
        // Handle internal control methods
        if method.starts_with("__actionbook.") {
            return self
                .handle_internal_method(session_name, method, &params)
                .await;
        }

        self.send_cdp_to_ws(session_name, method, params).await
    }

    /// Send a CDP command directly through the persistent WS (no internal method dispatch).
    /// Used by `send_cdp` for regular commands and by internal methods that need
    /// to issue real CDP commands (e.g. `Page.handleJavaScriptDialog`).
    async fn send_cdp_to_ws(
        &self,
        session_name: &str,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, String> {
        // Wait for initial attach to complete (connect_and_run sets tx + sessions
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
            // Ensure we have a session entry, lazy-attaching if needed
            self.ensure_session(session_name).await?;

            // Check if the active page has changed since our last attach
            self.maybe_reattach(session_name).await?;

            let sessions = self.sessions.lock().await;
            match sessions.get(session_name) {
                Some(entry) => Some(entry.cdp_session_id.clone()),
                None => {
                    return Err(format!(
                        "No target attached for session '{}', method '{}'. \
                         The daemon has not yet attached to a browser target.",
                        session_name, method
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
        if let Err(_) = tx
            .send(WsCommand {
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

        // Wait for response with timeout.
        // Screenshot and PDF commands produce large payloads (PNG/PDF encoding +
        // base64 + WS transfer) and routinely exceed the default 30s on heavy pages.
        let timeout_secs = cdp_timeout_secs(method);
        match tokio::time::timeout(Duration::from_secs(timeout_secs), resp_rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err("Response channel dropped".to_string()),
            Err(_) => {
                // Clean up pending entry
                self.pending.lock().await.remove(&ws_id);
                Err(format!("CDP command timed out after {}s", timeout_secs))
            }
        }
    }

    /// Ensure a named session exists in the routing table.
    /// If it doesn't exist, lazy-attach to the appropriate target.
    async fn ensure_session(&self, session_name: &str) -> std::result::Result<(), String> {
        {
            let sessions = self.sessions.lock().await;
            if sessions.contains_key(session_name) {
                return Ok(());
            }
        }

        // Session not yet attached — lazy attach
        let session_info = load_session_info_for_session(&self.profile, session_name);

        let target_id = if let Some(ref info) = session_info {
            if let Some(ref page_id) = info.active_page_id {
                page_id.clone()
            } else {
                // No active_page_id in session file — create a new independent tab
                // (Don't fall back to default's target, which would cause cross-session conflicts)
                tracing::info!(
                    "Session '{}' has no active_page_id, creating new tab",
                    session_name
                );
                self.create_new_target().await?
            }
        } else {
            // No session file — create a new tab
            self.create_new_target().await?
        };

        // Attach to the target
        self.attach_session(session_name, &target_id).await
    }

    /// Create a new browser tab via Target.createTarget, returning its targetId.
    async fn create_new_target(&self) -> std::result::Result<String, String> {
        let tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };
        let tx = tx.ok_or_else(|| "WS not connected for new tab creation".to_string())?;

        let ws_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);
        let (resp_tx, resp_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(ws_id, resp_tx);
        }

        tx.send(WsCommand {
            ws_id,
            method: "Target.createTarget".to_string(),
            params: serde_json::json!({"url": "about:blank"}),
            session_id: None,
        })
        .await
        .map_err(|_| "WS writer closed during createTarget".to_string())?;

        let result = tokio::time::timeout(Duration::from_secs(10), resp_rx)
            .await
            .map_err(|_| "createTarget timed out".to_string())?
            .map_err(|_| "createTarget response channel dropped".to_string())?;

        if let Some(err) = result.get("error") {
            return Err(format!("Target.createTarget failed: {}", err));
        }

        result
            .get("targetId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No targetId in createTarget response".to_string())
    }

    /// Attach a named session to a specific target, storing the entry in the routing table.
    async fn attach_session(
        &self,
        session_name: &str,
        target_id: &str,
    ) -> std::result::Result<(), String> {
        let tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };
        let tx = tx.ok_or_else(|| "WS not connected during attach".to_string())?;

        let ws_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);
        let (resp_tx, resp_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(ws_id, resp_tx);
        }

        tx.send(WsCommand {
            ws_id,
            method: "Target.attachToTarget".to_string(),
            params: serde_json::json!({
                "targetId": target_id,
                "flatten": true
            }),
            session_id: None,
        })
        .await
        .map_err(|_| "WS writer closed during attach".to_string())?;

        let result = tokio::time::timeout(Duration::from_secs(10), resp_rx)
            .await
            .map_err(|_| "Attach timed out".to_string())?
            .map_err(|_| "Attach response channel dropped".to_string())?;

        if let Some(err) = result.get("error") {
            return Err(format!("Target.attachToTarget failed: {}", err));
        }

        let cdp_session_id = result
            .get("sessionId")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No sessionId in attach response".to_string())?;

        tracing::info!(
            "Attached session '{}' to target {} (cdpSessionId: {})",
            session_name,
            target_id,
            cdp_session_id
        );

        let cdp_session_id_clone = cdp_session_id.clone();
        {
            let mut sessions = self.sessions.lock().await;
            sessions.insert(
                session_name.to_string(),
                SessionEntry {
                    cdp_session_id,
                    target_id: target_id.to_string(),
                },
            );
        }

        // Enable Page domain events on the new session for dialog tracking.
        // Fire-and-forget — don't block command flow if this fails.
        {
            let page_enable_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);
            let (pe_tx, _pe_rx) = oneshot::channel();
            {
                let mut pending = self.pending.lock().await;
                pending.insert(page_enable_id, pe_tx);
            }
            let _ = tx
                .send(WsCommand {
                    ws_id: page_enable_id,
                    method: "Page.enable".to_string(),
                    params: serde_json::json!({}),
                    session_id: Some(cdp_session_id_clone),
                })
                .await;
        }

        Ok(())
    }

    /// Check if `active_page_id` on disk differs from the currently attached target
    /// for a specific named session. If so, detach from the old target and re-attach
    /// to the new one via the persistent WS connection.
    async fn maybe_reattach(&self, session_name: &str) -> std::result::Result<(), String> {
        // We need to acquire the per-session reattach lock.
        // First check if the session exists and get current state.
        let (current_target, old_session_id) = {
            let sessions = self.sessions.lock().await;
            match sessions.get(session_name) {
                Some(entry) => (entry.target_id.clone(), entry.cdp_session_id.clone()),
                None => return Ok(()), // Not attached yet
            }
        };

        // Re-read active_page_id from disk for this session
        let session_info = load_session_info_for_session(&self.profile, session_name);
        let desired_target = session_info
            .as_ref()
            .and_then(|s| s.active_page_id.as_deref())
            .unwrap_or_default();

        if desired_target.is_empty() || desired_target == current_target {
            return Ok(()); // No change
        }

        // Note: We don't use per-session reattach locks here because the
        // sessions HashMap lock already serializes access. The check above
        // ensures we only proceed if the target actually changed.

        tracing::info!(
            "Session '{}' active page changed: {} → {}, re-attaching",
            session_name,
            current_target,
            desired_target
        );

        let tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };
        let tx = tx.ok_or_else(|| "WS not connected during re-attach".to_string())?;

        // Step 1: Detach from the old target session (best-effort)
        {
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
                        "sessionId": old_session_id
                    }),
                    session_id: None,
                })
                .await;

            match tokio::time::timeout(Duration::from_secs(3), detach_rx).await {
                Ok(Ok(resp)) => {
                    if let Some(err) = resp.get("error") {
                        tracing::debug!(
                            "Detach session '{}' from {} failed (non-fatal): {}",
                            session_name,
                            old_session_id,
                            err
                        );
                    } else {
                        tracing::debug!(
                            "Detached session '{}' from {}",
                            session_name,
                            old_session_id
                        );
                    }
                }
                _ => {
                    tracing::debug!("Detach session '{}' timed out (non-fatal)", session_name);
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

        let result = tokio::time::timeout(Duration::from_secs(10), resp_rx)
            .await
            .map_err(|_| "Re-attach timed out".to_string())?
            .map_err(|_| "Re-attach response channel dropped".to_string())?;

        if let Some(err) = result.get("error") {
            return Err(format!("Re-attach failed: {}", err));
        }

        let new_session_id = result
            .get("sessionId")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No sessionId in re-attach response".to_string())?;

        // Update the session entry
        let mut sessions = self.sessions.lock().await;
        sessions.insert(
            session_name.to_string(),
            SessionEntry {
                cdp_session_id: new_session_id.clone(),
                target_id: desired_target.to_string(),
            },
        );
        drop(sessions);

        // Enable Page domain events on the re-attached session for dialog tracking.
        {
            let page_enable_id = self.next_ws_id.fetch_add(1, Ordering::Relaxed);
            let (pe_tx, _pe_rx) = oneshot::channel();
            {
                let mut pending = self.pending.lock().await;
                pending.insert(page_enable_id, pe_tx);
            }
            let _ = tx
                .send(WsCommand {
                    ws_id: page_enable_id,
                    method: "Page.enable".to_string(),
                    params: serde_json::json!({}),
                    session_id: Some(new_session_id.clone()),
                })
                .await;
        }

        tracing::info!(
            "Session '{}' re-attached to target {} with sessionId: {}",
            session_name,
            desired_target,
            new_session_id
        );

        Ok(())
    }

    /// Handle internal `__actionbook.*` control methods.
    async fn handle_internal_method(
        &self,
        session_name: &str,
        method: &str,
        params: &Value,
    ) -> std::result::Result<Value, String> {
        match method {
            "__actionbook.listSessions" => {
                let sessions = self.sessions.lock().await;
                let list: Vec<Value> = sessions
                    .iter()
                    .map(|(name, entry)| {
                        serde_json::json!({
                            "name": name,
                            "targetId": entry.target_id,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({"sessions": list}))
            }
            "__actionbook.destroySession" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing 'name' param for destroySession".to_string())?;

                let entry = {
                    let mut sessions = self.sessions.lock().await;
                    sessions.remove(name)
                };

                if let Some(entry) = entry {
                    // Best-effort detach
                    let tx = {
                        let guard = self.tx.lock().await;
                        guard.clone()
                    };
                    if let Some(tx) = tx {
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
                                params: serde_json::json!({"sessionId": entry.cdp_session_id}),
                                session_id: None,
                            })
                            .await;
                        let _ = tokio::time::timeout(Duration::from_secs(3), detach_rx).await;
                    }
                    Ok(serde_json::json!({"destroyed": name, "targetId": entry.target_id}))
                } else {
                    Err(format!("Session '{}' not found", name))
                }
            }
            "__actionbook.dialogStatus" => {
                // Look up dialog for the requesting session's CDP session ID
                let cdp_session_id = {
                    let sessions = self.sessions.lock().await;
                    sessions
                        .get(session_name)
                        .map(|e| e.cdp_session_id.clone())
                };
                let dialogs = self.pending_dialogs.lock().await;
                let dialog = cdp_session_id
                    .as_deref()
                    .and_then(|sid| dialogs.get(sid));
                match dialog {
                    Some(d) => Ok(serde_json::json!({
                        "hasDialog": true,
                        "type": d.dialog_type,
                        "message": d.message,
                        "url": d.url,
                        "defaultPrompt": d.default_prompt,
                    })),
                    None => Ok(serde_json::json!({ "hasDialog": false })),
                }
            }
            "__actionbook.handleDialog" => {
                let accept = params
                    .get("accept")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let prompt_text = params
                    .get("promptText")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let mut cdp_params = serde_json::json!({ "accept": accept });
                if let Some(text) = prompt_text {
                    cdp_params["promptText"] = serde_json::json!(text);
                }

                // Send Page.handleJavaScriptDialog through the normal CDP path
                let result = self
                    .send_cdp_to_ws(session_name, "Page.handleJavaScriptDialog", cdp_params)
                    .await?;

                // Clear pending dialog state for this session
                {
                    let sessions = self.sessions.lock().await;
                    if let Some(entry) = sessions.get(session_name) {
                        self.pending_dialogs
                            .lock()
                            .await
                            .remove(&entry.cdp_session_id);
                    }
                }

                Ok(serde_json::json!({ "handled": true, "accepted": accept, "result": result }))
            }
            _ => Err(format!("Unknown internal method: {}", method)),
        }
    }

    /// Build dialog warning string for a specific session, if a dialog is pending.
    async fn dialog_warning_for_session(&self, session_name: &str) -> Option<String> {
        let cdp_session_id = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_name)
                .map(|e| e.cdp_session_id.clone())
        };
        let dialogs = self.pending_dialogs.lock().await;
        cdp_session_id
            .as_deref()
            .and_then(|sid| dialogs.get(sid))
            .map(|d| {
                format!(
                    "A JavaScript {} dialog is blocking the page: \"{}\" — use `browser dialog accept` or `browser dialog dismiss` to resolve it",
                    d.dialog_type, d.message
                )
            })
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

/// Returns the CDP command timeout in seconds based on the method name.
///
/// Most CDP commands complete well within 30s. However, commands that produce
/// large binary payloads (screenshot PNG, PDF) require Chrome to render, encode,
/// base64-encode, and transfer the data over WS — which can exceed 30s on
/// heavy pages or slow machines.
fn cdp_timeout_secs(method: &str) -> u64 {
    match method {
        "Page.captureScreenshot" | "Page.printToPDF" => 120,
        _ => 30,
    }
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
        // Load all session candidates from disk, ordered by priority (default first).
        // Try each until one connects, so a stale default doesn't block live named sessions.
        let candidates = find_all_session_infos(profile);
        if candidates.is_empty() {
            tracing::warn!(
                "No session state found for profile '{}', retrying...",
                profile
            );
            let delay_ms =
                (WS_RECONNECT_BASE_MS << reconnect_attempts.min(4)).min(WS_RECONNECT_MAX_MS);
            reconnect_attempts += 1;
            if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                tracing::error!(
                    "Max reconnect attempts reached (no session state), daemon exiting"
                );
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => continue,
                _ = shutdown.notified() => break,
            }
        }

        // Try each candidate until one connects successfully.
        // If a candidate connects and then drops, we break out to the outer loop
        // which will re-scan disk and try all candidates again.
        let mut any_connected = false;
        let mut should_shutdown = false;
        for (initial_session_name, session_info) in &candidates {
            tracing::info!(
                "Trying session '{}' at WS: {} (attempt {})",
                initial_session_name,
                session_info.cdp_url,
                reconnect_attempts + 1
            );

            match connect_and_run(session_info, &ws_state, &shutdown, initial_session_name).await {
                Ok(()) => {
                    // Graceful shutdown requested
                    should_shutdown = true;
                    break;
                }
                Err(e) => {
                    let was_connected = !ws_state.sessions.lock().await.is_empty();

                    tracing::warn!(
                        "WS connection to session '{}' lost: {}",
                        initial_session_name,
                        e
                    );
                    *ws_state.tx.lock().await = None;
                    ws_state.sessions.lock().await.clear();
                    let _ = ws_state.ready_tx.send(false);
                    ws_state.pending_dialogs.lock().await.clear();

                    // Drain pending requests
                    {
                        let mut pending = ws_state.pending.lock().await;
                        let count = pending.len();
                        for (_, tx) in pending.drain() {
                            let _ = tx.send(serde_json::json!({"error": "WS connection lost"}));
                        }
                        if count > 0 {
                            tracing::info!(
                                "Drained {} pending requests after WS disconnect",
                                count
                            );
                        }
                    }

                    if was_connected {
                        // Connection was working then dropped — restart scan from top
                        reconnect_attempts = 0;
                        any_connected = true;
                        break;
                    }

                    // Connection never succeeded — try next candidate
                    tracing::debug!(
                        "Session '{}' not reachable, trying next candidate",
                        initial_session_name
                    );
                    continue;
                }
            }
        }

        if should_shutdown {
            break;
        }

        // All candidates failed (or one connected then dropped)
        if !any_connected {
            reconnect_attempts += 1;
        }
        if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
            tracing::error!("Max reconnect attempts reached, daemon exiting");
            break;
        }
        let delay_ms = (WS_RECONNECT_BASE_MS << reconnect_attempts.min(4)).min(WS_RECONNECT_MAX_MS);
        tracing::info!("Reconnecting in {}ms...", delay_ms);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => continue,
            _ = shutdown.notified() => break,
        }
    }
}

/// Sanitize a name for safe use in file paths (same logic as lifecycle::sanitize).
fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if s.is_empty() {
        "default".to_string()
    } else {
        s
    }
}

fn legacy_session_paths(sessions_dir: &std::path::Path, profile: &str) -> Vec<std::path::PathBuf> {
    let mut paths = vec![sessions_dir.join(format!("{}.json", profile))];
    let safe_profile = sanitize_name(profile);
    let safe_path = sessions_dir.join(format!("{}.json", safe_profile));
    if !paths.iter().any(|p| p == &safe_path) {
        paths.push(safe_path);
    }
    paths
}

/// Load session info for a specific named session from disk.
/// Tries `{profile}@{session}.json` first, then falls back to legacy `{profile}.json`
/// if the session is "default".
fn load_session_info_for_session(profile: &str, session_name: &str) -> Option<SessionInfo> {
    let safe_profile = sanitize_name(profile);
    let safe_session = sanitize_name(session_name);
    let sessions_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".actionbook")
        .join("sessions");

    // Try session-specific file
    let session_file = sessions_dir.join(format!("{}@{}.json", safe_profile, safe_session));
    if let Ok(content) = std::fs::read_to_string(&session_file) {
        if let Ok(info) = serde_json::from_str::<SessionInfo>(&content) {
            return Some(info);
        }
    }

    // Fall back to legacy file only for "default" session
    if safe_session == "default" {
        for legacy_file in legacy_session_paths(&sessions_dir, profile) {
            if let Ok(content) = std::fs::read_to_string(&legacy_file) {
                return serde_json::from_str(&content).ok();
            }
        }
    }

    None
}

/// Find all session infos for the given profile, ordered by priority:
/// 1. default session (new-style), 2. legacy file, 3. any other named sessions.
/// Returns `Vec<(session_name, SessionInfo)>` so callers can try each until one connects.
fn find_all_session_infos(profile: &str) -> Vec<(String, SessionInfo)> {
    let safe_profile = sanitize_name(profile);
    let sessions_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".actionbook")
        .join("sessions");

    let mut candidates = Vec::new();

    // 1. Try default session (new-style)
    let default_file = sessions_dir.join(format!("{}@default.json", safe_profile));
    if let Ok(content) = std::fs::read_to_string(&default_file) {
        if let Ok(info) = serde_json::from_str::<SessionInfo>(&content) {
            candidates.push(("default".to_string(), info));
        }
    }

    // 2. Try legacy file (only if default wasn't found)
    if candidates.is_empty() {
        for legacy_file in legacy_session_paths(&sessions_dir, profile) {
            if let Ok(content) = std::fs::read_to_string(&legacy_file) {
                if let Ok(info) = serde_json::from_str::<SessionInfo>(&content) {
                    candidates.push(("default".to_string(), info));
                    break;
                }
            }
        }
    }

    // 3. Any other named sessions
    let prefix = format!("{}@", safe_profile);
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let fname_str = fname.to_string_lossy();
            if fname_str.starts_with(&prefix) && fname_str.ends_with(".json") {
                // Skip default — already checked above
                if fname_str.as_ref() == format!("{}@default.json", safe_profile) {
                    continue;
                }
                let session_name = fname_str
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".json"))
                    .unwrap_or("default")
                    .to_string();
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(info) = serde_json::from_str::<SessionInfo>(&content) {
                        tracing::debug!(
                            "Found session info from {} (session={})",
                            fname_str,
                            session_name
                        );
                        candidates.push((session_name, info));
                    }
                }
            }
        }
    }

    candidates
}

/// Resolve the active page target ID.
///
/// For local sessions, uses HTTP `/json/list`.
/// For remote sessions, uses `Target.getTargets` over the WS.
async fn resolve_active_target(
    session_info: &SessionInfo,
    ws_write: &Mutex<
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Message,
        >,
    >,
    ws_read: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
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

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP /json/list failed: {}", e))?;
        let pages: Vec<Value> = resp
            .json()
            .await
            .map_err(|e| format!("Parse /json/list failed: {}", e))?;

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
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    cmd.to_string().into(),
                ))
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
                                            && t.get("type").and_then(|v| v.as_str())
                                                == Some("page")
                                    }) {
                                        return Ok(t["targetId"].as_str().unwrap().to_string());
                                    }
                                }
                                // Fall back to first page
                                if let Some(t) = targets.iter().find(|t| {
                                    t.get("type").and_then(|v| v.as_str()) == Some("page")
                                }) {
                                    return Ok(t
                                        .get("targetId")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string());
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
    initial_session_name: &str,
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

    let session_id =
        session_id.ok_or_else(|| "No sessionId returned by Target.attachToTarget".to_string())?;
    tracing::info!(
        "Attached to target {} with sessionId: {}",
        target_id,
        session_id
    );

    // Keep a copy for Page.enable below (session_id is moved into SessionEntry)
    let session_id_for_page_enable = session_id.clone();

    // Store the initial session in the routing table under its actual name
    {
        let mut sessions = ws_state.sessions.lock().await;
        sessions.insert(
            initial_session_name.to_string(),
            SessionEntry {
                cdp_session_id: session_id,
                target_id: target_id,
            },
        );
    }

    // Create writer channel
    let (tx, mut rx) = mpsc::channel::<WsCommand>(64);
    *ws_state.tx.lock().await = Some(tx);

    // Signal that the daemon is ready to accept commands.
    // Any send_cdp() calls waiting on ready_rx.wait_for() will proceed.
    // Late arrivals also see `true` immediately (watch is state-based).
    let _ = ws_state.ready_tx.send(true);

    // Enable Page domain events so we receive Page.javascriptDialogOpening/Closed.
    // This is fire-and-forget — if it fails, dialog tracking won't work but
    // other commands are unaffected.
    {
        let page_enable_id = ws_state.next_ws_id.fetch_add(1, Ordering::Relaxed);
        let (pe_tx, _pe_rx) = oneshot::channel();
        {
            let mut pending = ws_state.pending.lock().await;
            pending.insert(page_enable_id, pe_tx);
        }
        if let Some(ref tx) = *ws_state.tx.lock().await {
            let _ = tx
                .send(WsCommand {
                    ws_id: page_enable_id,
                    method: "Page.enable".to_string(),
                    params: serde_json::json!({}),
                    session_id: Some(session_id_for_page_enable.clone()),
                })
                .await;
        }
        // Don't wait for response — best effort
    }

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
                frame
                    .as_object_mut()
                    .unwrap()
                    .insert("sessionId".to_string(), Value::String(sid));
            }
            let msg = tokio_tungstenite::tungstenite::Message::Text(frame.to_string().into());
            let mut writer = ws_write_clone.lock().await;
            if let Err(e) = writer.send(msg).await {
                tracing::error!("WS write error: {}", e);
                break;
            }
        }
    });

    // Reader task: reads WS messages and routes responses by ID
    let pending = ws_state.pending.clone();
    let pending_dialogs = ws_state.pending_dialogs.clone();
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
                            } else if let Some(method) = obj.get("method").and_then(|v| v.as_str()) {
                                // CDP Event — check for dialog events.
                                // In flatten mode, events carry a top-level "sessionId".
                                let event_session_id = obj
                                    .get("sessionId")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let params = obj.get("params").cloned().unwrap_or(Value::Null);
                                match method {
                                    "Page.javascriptDialogOpening" => {
                                        if let Ok(event) = serde_json::from_value::<JavascriptDialogOpeningEvent>(params) {
                                            tracing::info!(
                                                "JavaScript {} dialog opened (session {}): \"{}\"",
                                                event.dialog_type, event_session_id, event.message
                                            );
                                            pending_dialogs.lock().await.insert(
                                                event_session_id,
                                                PendingDialog {
                                                    dialog_type: event.dialog_type,
                                                    message: event.message,
                                                    url: event.url,
                                                    default_prompt: event.default_prompt,
                                                },
                                            );
                                        }
                                    }
                                    "Page.javascriptDialogClosed" => {
                                        tracing::info!("JavaScript dialog closed (session {})", event_session_id);
                                        pending_dialogs.lock().await.remove(&event_session_id);
                                    }
                                    _ => {
                                        tracing::trace!("Skipping CDP Event: {}", method);
                                    }
                                }
                            }
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
        let method = request.method.clone();
        let session_name = request.session.as_deref().unwrap_or("default");
        let result = ws_state
            .send_cdp(session_name, &method, request.params)
            .await;

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

        // Inject dialog warning for non-dialog commands
        let resp = if !method.contains("dialog") && !method.contains("Dialog") {
            resp.with_warning(ws_state.dialog_warning_for_session(session_name).await)
        } else {
            resp
        };

        let encoded = protocol::encode_line(&resp).map_err(|e| {
            ActionbookError::DaemonError(format!("Failed to encode response: {}", e))
        })?;
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
    fn cdp_timeout_is_extended_for_large_payload_methods() {
        assert_eq!(cdp_timeout_secs("Page.captureScreenshot"), 120);
        assert_eq!(cdp_timeout_secs("Page.printToPDF"), 120);
        // All other methods use the default 30s
        assert_eq!(cdp_timeout_secs("Runtime.evaluate"), 30);
        assert_eq!(cdp_timeout_secs("Page.navigate"), 30);
        assert_eq!(cdp_timeout_secs("DOM.getDocument"), 30);
        assert_eq!(cdp_timeout_secs("Input.dispatchMouseEvent"), 30);
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
