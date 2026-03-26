//! Local backend: launch and control a Chrome process via CDP over `ws://`.
//!
//! Uses the existing [`BrowserLauncher`] for process management and
//! `tokio-tungstenite` for WebSocket communication.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use futures::SinkExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use super::types::*;
use super::{BackendSession, BrowserBackendFactory};
use crate::browser::cdp_types::CdpResponse;
use crate::browser::launcher::BrowserLauncher;
use crate::daemon::backend_op::BackendOp;
use crate::error::{ActionbookError, Result};

/// Type alias for the WebSocket stream used by local backend.
type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// LocalBackendFactory
// ---------------------------------------------------------------------------

/// Factory that creates [`LocalBackendSession`]s by launching Chrome.
pub struct LocalBackendFactory;

#[async_trait]
impl BrowserBackendFactory for LocalBackendFactory {
    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_launch: true,
            can_attach: true,
            can_resume: true,
            supports_headless: true,
        }
    }

    async fn start(&self, spec: StartSpec) -> Result<Box<dyn BackendSession>> {
        let mut launch = BrowserLauncher::new()?;

        // Apply headless setting
        if spec.headless {
            launch = launch.headless(true);
        }

        // Apply extra args
        if !spec.extra_args.is_empty() {
            launch = launch.extra_args(spec.extra_args);
        }

        let (child, ws_url) = launch.launch_and_wait().await?;
        let pid = child.id();
        let cdp_port = launch.get_cdp_port();
        let user_data_dir = BrowserLauncher::default_user_data_dir(&spec.profile);

        let cancel = CancellationToken::new();
        let (ws, event_rx) = connect_and_monitor(ws_url.clone(), cancel.clone()).await?;

        let mut session = LocalBackendSession {
            ws,
            ws_url,
            pid: Some(pid),
            cdp_port,
            user_data_dir,
            cmd_id: Arc::new(AtomicI64::new(1)),
            event_rx: Some(event_rx),
            cancel,
            attached_targets: HashMap::new(),
        };

        // Navigate to the initial URL if specified
        if let Some(ref url) = spec.open_url {
            let targets = session.list_targets().await?;
            if let Some(page_target) = targets.iter().find(|t| t.target_type == "page") {
                let op = BackendOp::Navigate {
                    target_id: page_target.target_id.clone(),
                    url: url.clone(),
                };
                if let Err(e) = session.exec(op).await {
                    tracing::warn!(url, "failed to navigate to open_url: {e}");
                }
            }
        }

        Ok(Box::new(session))
    }

    async fn attach(&self, spec: AttachSpec) -> Result<Box<dyn BackendSession>> {
        let cancel = CancellationToken::new();
        let (ws, event_rx) = connect_and_monitor(spec.ws_url.clone(), cancel.clone()).await?;

        // Try to extract port from ws_url (ws://127.0.0.1:PORT/...)
        let cdp_port = extract_port_from_ws_url(&spec.ws_url).unwrap_or(0);

        let session = LocalBackendSession {
            ws,
            ws_url: spec.ws_url,
            pid: None,
            cdp_port,
            user_data_dir: PathBuf::new(),
            cmd_id: Arc::new(AtomicI64::new(1)),
            event_rx: Some(event_rx),
            cancel,
            attached_targets: HashMap::new(),
        };

        Ok(Box::new(session))
    }

    async fn resume(&self, cp: Checkpoint) -> Result<Box<dyn BackendSession>> {
        let cancel = CancellationToken::new();
        let (ws, event_rx) = connect_and_monitor(cp.ws_url.clone(), cancel.clone()).await?;

        let session = LocalBackendSession {
            ws,
            ws_url: cp.ws_url,
            pid: cp.pid,
            cdp_port: cp.cdp_port.unwrap_or(0),
            user_data_dir: cp.user_data_dir.unwrap_or_default(),
            cmd_id: Arc::new(AtomicI64::new(1)),
            event_rx: Some(event_rx),
            cancel,
            attached_targets: HashMap::new(),
        };

        Ok(Box::new(session))
    }
}

// ---------------------------------------------------------------------------
// LocalBackendSession
// ---------------------------------------------------------------------------

/// A live CDP connection to a locally-launched Chrome process.
pub struct LocalBackendSession {
    /// WebSocket connection to the browser.
    ws: WsStream,
    /// The WebSocket URL we connected to.
    ws_url: String,
    /// Chrome process ID (None if we attached to an existing process).
    pid: Option<u32>,
    /// CDP debugging port.
    cdp_port: u16,
    /// Chrome user data directory.
    user_data_dir: PathBuf,
    /// Monotonically increasing CDP command ID.
    cmd_id: Arc<AtomicI64>,
    /// Receiver for backend events from the WS monitor task.
    event_rx: Option<mpsc::UnboundedReceiver<BackendEvent>>,
    /// Token to cancel the background health-probe task on shutdown.
    cancel: CancellationToken,
    /// Mapping from CDP target_id to flattened sessionId (for page-scoped commands).
    attached_targets: HashMap<String, String>,
}

#[async_trait]
impl BackendSession for LocalBackendSession {
    fn events(&mut self) -> BoxStream<'static, BackendEvent> {
        if let Some(rx) = self.event_rx.take() {
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx).boxed()
        } else {
            futures::stream::empty().boxed()
        }
    }

    async fn exec(&mut self, op: BackendOp) -> Result<OpResult> {
        let target_id = op.target_id();
        let is_page_scoped = op.is_page_scoped();

        // For page-scoped commands, ensure the target is attached and get the sessionId.
        let session_id = if is_page_scoped {
            if let Some(tid) = target_id {
                Some(self.ensure_attached(tid).await?)
            } else {
                None
            }
        } else {
            None
        };

        let (method, params) = op_to_cdp(&op);
        let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);

        let mut cmd = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });

        // Include sessionId for page-scoped commands routed via flattened session.
        if let Some(sid) = session_id {
            cmd["sessionId"] = serde_json::Value::String(sid);
        }

        self.ws
            .send(Message::Text(cmd.to_string().into()))
            .await
            .map_err(|e| ActionbookError::CdpConnectionFailed(format!("WS send failed: {e}")))?;

        // Read until we get the response with our ID, with a 30s timeout.
        tokio::time::timeout(std::time::Duration::from_secs(30), self.read_response(id))
            .await
            .map_err(|_| {
                ActionbookError::CdpConnectionFailed("CDP response timeout (30s)".into())
            })?
    }

    async fn list_targets(&self) -> Result<Vec<TargetInfo>> {
        // Use HTTP endpoint for local targets -- avoids interleaving with
        // the command/response WS stream.
        let url = format!("http://127.0.0.1:{}/json/list", self.cdp_port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let resp = client.get(&url).send().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Failed to list targets: {e}"))
        })?;

        let pages: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Failed to parse targets: {e}"))
        })?;

        Ok(pages
            .into_iter()
            .map(|p| TargetInfo {
                target_id: p["id"].as_str().unwrap_or("").to_string(),
                target_type: p["type"].as_str().unwrap_or("").to_string(),
                title: p["title"].as_str().unwrap_or("").to_string(),
                url: p["url"].as_str().unwrap_or("").to_string(),
                attached: false,
            })
            .collect())
    }

    async fn checkpoint(&self) -> Result<Checkpoint> {
        Ok(Checkpoint {
            kind: BackendKind::Local,
            pid: self.pid,
            ws_url: self.ws_url.clone(),
            cdp_port: Some(self.cdp_port),
            user_data_dir: Some(self.user_data_dir.clone()),
            headers: None,
        })
    }

    async fn health(&self) -> Result<Health> {
        let url = format!("http://127.0.0.1:{}/json/version", self.cdp_port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        match client.get(&url).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(info) => {
                    let version = info["Browser"].as_str().map(|s| s.to_string());
                    Ok(Health {
                        connected: true,
                        browser_version: version,
                        uptime_secs: None,
                    })
                }
                Err(e) => {
                    tracing::debug!("health check: failed to parse JSON: {e}");
                    Ok(Health {
                        connected: false,
                        browser_version: None,
                        uptime_secs: None,
                    })
                }
            },
            Err(_) => Ok(Health {
                connected: false,
                browser_version: None,
                uptime_secs: None,
            }),
        }
    }

    async fn shutdown(&mut self, policy: ShutdownPolicy) -> Result<()> {
        // Stop the background health-probe task.
        self.cancel.cancel();

        match policy {
            ShutdownPolicy::Graceful => {
                // Send Browser.close via WS
                let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);
                let cmd = serde_json::json!({
                    "id": id,
                    "method": "Browser.close",
                    "params": {},
                });
                let _ = self.ws.send(Message::Text(cmd.to_string().into())).await;

                // Wait briefly, then kill if still alive
                if let Some(pid) = self.pid {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    kill_process(pid);
                }
            }
            ShutdownPolicy::ForceKill => {
                if let Some(pid) = self.pid {
                    kill_process(pid);
                }
            }
        }

        // Close the WS connection
        let _ = self.ws.close(None).await;
        Ok(())
    }
}

impl LocalBackendSession {
    /// Ensure a target is attached via `Target.attachToTarget` with flattened session mode.
    /// Returns the cached CDP sessionId for the target.
    async fn ensure_attached(&mut self, target_id: &str) -> Result<String> {
        if let Some(sid) = self.attached_targets.get(target_id) {
            return Ok(sid.clone());
        }

        let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);
        let cmd = serde_json::json!({
            "id": id,
            "method": "Target.attachToTarget",
            "params": {
                "targetId": target_id,
                "flatten": true,
            },
        });

        self.ws
            .send(Message::Text(cmd.to_string().into()))
            .await
            .map_err(|e| ActionbookError::CdpConnectionFailed(format!("WS send failed: {e}")))?;

        let result =
            tokio::time::timeout(std::time::Duration::from_secs(10), self.read_response(id))
                .await
                .map_err(|_| {
                    ActionbookError::CdpConnectionFailed("Target.attachToTarget timeout".into())
                })??;

        let session_id = result
            .value
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ActionbookError::CdpError("Target.attachToTarget response missing sessionId".into())
            })?
            .to_string();

        self.attached_targets
            .insert(target_id.to_string(), session_id.clone());
        Ok(session_id)
    }

    /// Read WS messages until we get a response matching `expected_id`.
    async fn read_response(&mut self, expected_id: i64) -> Result<OpResult> {
        let mut parse_failures = 0u8;

        while let Some(msg) = self.ws.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<CdpResponse>(&text) {
                        Ok(response) => {
                            if response.id == expected_id {
                                if let Some(err) = response.error {
                                    return Err(ActionbookError::CdpError(format!(
                                        "CDP error: {err}"
                                    )));
                                }
                                return Ok(OpResult::new(
                                    response.result.unwrap_or(serde_json::Value::Null),
                                ));
                            }
                            // Not our response, keep waiting
                        }
                        Err(_) => {
                            // Likely a CDP event (no "id" field)
                            if text.contains("\"method\"") && !text.contains("\"id\"") {
                                continue;
                            }
                            parse_failures += 1;
                            if parse_failures > 5 {
                                return Err(ActionbookError::CdpError(format!(
                                    "Too many CDP parse failures ({parse_failures})"
                                )));
                            }
                        }
                    }
                }
                Ok(_) => continue, // ping/pong/binary
                Err(e) => {
                    return Err(ActionbookError::CdpConnectionFailed(format!(
                        "WebSocket error: {e}"
                    )));
                }
            }
        }

        Err(ActionbookError::CdpConnectionFailed(
            "WebSocket closed before response received".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Connect to a browser WS URL and spawn a background task that monitors
/// for disconnection via periodic health probes.
///
/// The caller should store the returned [`CancellationToken`] and cancel it
/// during shutdown so the background task exits cleanly.
async fn connect_and_monitor(
    ws_url: String,
    cancel: CancellationToken,
) -> Result<(WsStream, mpsc::UnboundedReceiver<BackendEvent>)> {
    let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!(
                "WebSocket connection to {ws_url} failed: {e}"
            ))
        })?;

    let (tx, rx) = mpsc::unbounded_channel();

    // Monitor task: periodically probe the HTTP health endpoint.
    // This runs on a separate connection so it does not interfere with
    // the command/response WS stream.
    let probe_url = ws_url.clone();
    tokio::spawn(async move {
        let port = match extract_port_from_ws_url(&probe_url) {
            Some(p) => p,
            None => return,
        };
        let health_url = format!("http://127.0.0.1:{port}/json/version");
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            }
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => continue,
                _ => {
                    let _ = tx.send(BackendEvent::Disconnected {
                        reason: "Browser health check failed".into(),
                    });
                    break;
                }
            }
        }
    });

    Ok((ws, rx))
}

/// Extract port from a WebSocket URL like `ws://127.0.0.1:9222/devtools/browser/...`
fn extract_port_from_ws_url(url: &str) -> Option<u16> {
    let authority = url.split("://").nth(1)?.split('/').next()?;
    let port_str = authority.rsplit(':').next()?;
    port_str.parse().ok()
}

/// Kill a process by PID.
fn kill_process(pid: u32) {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
}

/// Translate a [`BackendOp`] into a CDP method name and params JSON.
fn op_to_cdp(op: &BackendOp) -> (&'static str, serde_json::Value) {
    match op {
        BackendOp::Navigate { target_id: _, url } => {
            ("Page.navigate", serde_json::json!({ "url": url }))
        }
        BackendOp::Evaluate {
            target_id: _,
            expression,
            return_by_value,
        } => (
            "Runtime.evaluate",
            serde_json::json!({
                "expression": expression,
                "returnByValue": return_by_value,
            }),
        ),
        BackendOp::GetDocument { target_id: _ } => ("DOM.getDocument", serde_json::json!({})),
        BackendOp::QuerySelector {
            target_id: _,
            node_id,
            selector,
        } => (
            "DOM.querySelector",
            serde_json::json!({
                "nodeId": node_id,
                "selector": selector,
            }),
        ),
        BackendOp::GetBoxModel {
            target_id: _,
            node_id,
        } => ("DOM.getBoxModel", serde_json::json!({ "nodeId": node_id })),
        BackendOp::DispatchMouseEvent {
            target_id: _,
            event_type,
            x,
            y,
            button,
            click_count,
        } => (
            "Input.dispatchMouseEvent",
            serde_json::json!({
                "type": event_type,
                "x": x,
                "y": y,
                "button": button,
                "clickCount": click_count,
            }),
        ),
        BackendOp::DispatchKeyEvent {
            target_id: _,
            event_type,
            key,
            text,
        } => (
            "Input.dispatchKeyEvent",
            serde_json::json!({
                "type": event_type,
                "key": key,
                "text": text,
            }),
        ),
        BackendOp::CaptureScreenshot {
            target_id: _,
            full_page,
        } => {
            let mut params = serde_json::json!({ "format": "png" });
            if *full_page {
                params["captureBeyondViewport"] = serde_json::json!(true);
            }
            ("Page.captureScreenshot", params)
        }
        BackendOp::PrintToPdf { target_id: _ } => ("Page.printToPDF", serde_json::json!({})),
        BackendOp::GetAccessibilityTree { target_id: _ } => {
            ("Accessibility.getFullAXTree", serde_json::json!({}))
        }
        BackendOp::GetCookies { target_id: _ } => ("Network.getCookies", serde_json::json!({})),
        BackendOp::SetCookie {
            target_id: _,
            name,
            value,
            domain,
            path,
            secure,
            http_only,
            same_site,
            expires,
        } => {
            let mut params = serde_json::json!({
                "name": name,
                "value": value,
                "domain": domain,
                "path": path,
            });
            if let Some(s) = secure {
                params["secure"] = serde_json::json!(s);
            }
            if let Some(h) = http_only {
                params["httpOnly"] = serde_json::json!(h);
            }
            if let Some(ss) = same_site {
                params["sameSite"] = serde_json::json!(ss);
            }
            if let Some(e) = expires {
                params["expires"] = serde_json::json!(e);
            }
            ("Network.setCookie", params)
        }
        BackendOp::GetTargets => ("Target.getTargets", serde_json::json!({})),
        BackendOp::CreateTarget {
            url,
            window_id: _,
            new_window,
        } => (
            "Target.createTarget",
            serde_json::json!({
                "url": url,
                "newWindow": new_window,
            }),
        ),
        BackendOp::CloseTarget { target_id } => (
            "Target.closeTarget",
            serde_json::json!({ "targetId": target_id }),
        ),
        BackendOp::DeleteCookies {
            target_id: _,
            name,
            domain,
            path,
        } => {
            let mut params = serde_json::json!({ "name": name });
            if let Some(d) = domain {
                params["domain"] = serde_json::json!(d);
            }
            if let Some(p) = path {
                params["path"] = serde_json::json!(p);
            }
            ("Network.deleteCookies", params)
        }
        BackendOp::GetNodeForLocation { target_id: _, x, y } => (
            "DOM.getNodeForLocation",
            serde_json::json!({ "x": x, "y": y }),
        ),
        BackendOp::DomFocus {
            target_id: _,
            node_id,
        } => ("DOM.focus", serde_json::json!({ "nodeId": node_id })),
        BackendOp::SetFileInputFiles {
            target_id: _,
            node_id,
            files,
        } => (
            "DOM.setFileInputFiles",
            serde_json::json!({
                "nodeId": node_id,
                "files": files,
            }),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_port_standard() {
        assert_eq!(
            extract_port_from_ws_url("ws://127.0.0.1:9222/devtools/browser/abc"),
            Some(9222)
        );
    }

    #[test]
    fn extract_port_different_port() {
        assert_eq!(
            extract_port_from_ws_url("ws://localhost:12345/devtools"),
            Some(12345)
        );
    }

    #[test]
    fn extract_port_no_port() {
        assert_eq!(extract_port_from_ws_url("ws://localhost/devtools"), None);
    }

    #[test]
    fn extract_port_wss() {
        assert_eq!(
            extract_port_from_ws_url("wss://cloud.example.com:8443/browser"),
            Some(8443)
        );
    }

    #[test]
    fn op_to_cdp_navigate() {
        let op = BackendOp::Navigate {
            target_id: "T1".into(),
            url: "https://example.com".into(),
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Page.navigate");
        assert_eq!(params["url"], "https://example.com");
    }

    #[test]
    fn op_to_cdp_evaluate() {
        let op = BackendOp::Evaluate {
            target_id: "T1".into(),
            expression: "1+1".into(),
            return_by_value: true,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Runtime.evaluate");
        assert_eq!(params["expression"], "1+1");
        assert_eq!(params["returnByValue"], true);
    }

    #[test]
    fn op_to_cdp_get_document() {
        let op = BackendOp::GetDocument {
            target_id: "T1".into(),
        };
        let (method, _params) = op_to_cdp(&op);
        assert_eq!(method, "DOM.getDocument");
    }

    #[test]
    fn op_to_cdp_query_selector() {
        let op = BackendOp::QuerySelector {
            target_id: "T1".into(),
            node_id: 1,
            selector: "#submit".into(),
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "DOM.querySelector");
        assert_eq!(params["nodeId"], 1);
        assert_eq!(params["selector"], "#submit");
    }

    #[test]
    fn op_to_cdp_dispatch_mouse() {
        let op = BackendOp::DispatchMouseEvent {
            target_id: "T1".into(),
            event_type: "mousePressed".into(),
            x: 100.0,
            y: 200.0,
            button: "left".into(),
            click_count: 1,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Input.dispatchMouseEvent");
        assert_eq!(params["type"], "mousePressed");
        assert_eq!(params["x"], 100.0);
        assert_eq!(params["y"], 200.0);
        assert_eq!(params["button"], "left");
        assert_eq!(params["clickCount"], 1);
    }

    #[test]
    fn op_to_cdp_dispatch_key() {
        let op = BackendOp::DispatchKeyEvent {
            target_id: "T1".into(),
            event_type: "keyDown".into(),
            key: "Enter".into(),
            text: "\r".into(),
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Input.dispatchKeyEvent");
        assert_eq!(params["type"], "keyDown");
        assert_eq!(params["key"], "Enter");
    }

    #[test]
    fn op_to_cdp_screenshot_full_page() {
        let op = BackendOp::CaptureScreenshot {
            target_id: "T1".into(),
            full_page: true,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Page.captureScreenshot");
        assert_eq!(params["captureBeyondViewport"], true);
    }

    #[test]
    fn op_to_cdp_screenshot_viewport() {
        let op = BackendOp::CaptureScreenshot {
            target_id: "T1".into(),
            full_page: false,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Page.captureScreenshot");
        assert!(params.get("captureBeyondViewport").is_none());
    }

    #[test]
    fn op_to_cdp_get_targets() {
        let (method, _) = op_to_cdp(&BackendOp::GetTargets);
        assert_eq!(method, "Target.getTargets");
    }

    #[test]
    fn op_to_cdp_create_target() {
        let op = BackendOp::CreateTarget {
            url: "about:blank".into(),
            window_id: Some(42),
            new_window: true,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Target.createTarget");
        assert_eq!(params["url"], "about:blank");
        assert_eq!(params["newWindow"], true);
    }

    #[test]
    fn op_to_cdp_close_target() {
        let op = BackendOp::CloseTarget {
            target_id: "ABC".into(),
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Target.closeTarget");
        assert_eq!(params["targetId"], "ABC");
    }

    #[test]
    fn op_to_cdp_get_cookies() {
        let op = BackendOp::GetCookies {
            target_id: "T1".into(),
        };
        let (method, _) = op_to_cdp(&op);
        assert_eq!(method, "Network.getCookies");
    }

    #[test]
    fn op_to_cdp_set_cookie() {
        let op = BackendOp::SetCookie {
            target_id: "T1".into(),
            name: "session".into(),
            value: "abc123".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: None,
            http_only: None,
            same_site: None,
            expires: None,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Network.setCookie");
        assert_eq!(params["name"], "session");
        assert_eq!(params["domain"], ".example.com");
    }

    #[test]
    fn op_to_cdp_print_pdf() {
        let op = BackendOp::PrintToPdf {
            target_id: "T1".into(),
        };
        let (method, _) = op_to_cdp(&op);
        assert_eq!(method, "Page.printToPDF");
    }

    #[test]
    fn op_to_cdp_get_accessibility_tree() {
        let op = BackendOp::GetAccessibilityTree {
            target_id: "T1".into(),
        };
        let (method, _) = op_to_cdp(&op);
        assert_eq!(method, "Accessibility.getFullAXTree");
    }

    #[test]
    fn op_to_cdp_get_box_model() {
        let op = BackendOp::GetBoxModel {
            target_id: "T1".into(),
            node_id: 42,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "DOM.getBoxModel");
        assert_eq!(params["nodeId"], 42);
    }

    #[test]
    fn local_factory_kind() {
        let factory = LocalBackendFactory;
        assert_eq!(factory.kind(), BackendKind::Local);
    }

    #[test]
    fn local_factory_capabilities() {
        let factory = LocalBackendFactory;
        let caps = factory.capabilities();
        assert!(caps.can_launch);
        assert!(caps.can_attach);
        assert!(caps.can_resume);
        assert!(caps.supports_headless);
    }
}
