//! Cloud backend: connect to a remote browser via `wss://` with auth headers.
//!
//! Uses `tokio-tungstenite` with TLS for secure WebSocket communication.
//! Unlike the local backend, cloud has no HTTP endpoints — all operations
//! (health, list_targets, exec) go through the single WSS connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use futures::SinkExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use super::types::*;
use super::{BackendSession, BrowserBackendFactory};
use crate::browser::cdp_types::CdpResponse;
use crate::daemon::backend_op::BackendOp;
use crate::error::{ActionbookError, Result};

/// Type alias for the WebSocket stream used by cloud backend.
type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Reconnection configuration for cloud connections.
struct ReconnectConfig {
    /// Maximum number of reconnection attempts.
    max_attempts: u32,
    /// Initial backoff delay.
    initial_delay: Duration,
    /// Maximum backoff delay.
    max_delay: Duration,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}

// ---------------------------------------------------------------------------
// CloudBackendFactory
// ---------------------------------------------------------------------------

/// Factory that creates [`CloudBackendSession`]s by connecting to remote
/// browser endpoints over WSS.
pub struct CloudBackendFactory;

#[async_trait]
impl BrowserBackendFactory for CloudBackendFactory {
    fn kind(&self) -> BackendKind {
        BackendKind::Cloud
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_launch: false,
            can_attach: true,
            can_resume: true,
            supports_headless: true,
        }
    }

    async fn start(&self, _spec: StartSpec) -> Result<Box<dyn BackendSession>> {
        Err(ActionbookError::InvalidOperation(
            "Cloud backend cannot start a new browser — use attach() to connect to an existing endpoint".into(),
        ))
    }

    async fn attach(&self, spec: AttachSpec) -> Result<Box<dyn BackendSession>> {
        let headers = spec.headers.unwrap_or_default();
        let cancel = CancellationToken::new();
        let (ws, event_rx) =
            connect_wss_and_monitor(spec.ws_url.clone(), &headers, cancel.clone()).await?;

        let session = CloudBackendSession {
            ws,
            ws_url: spec.ws_url,
            headers,
            cmd_id: Arc::new(AtomicI64::new(1)),
            event_rx: Some(event_rx),
            cancel,
            attached_targets: HashMap::new(),
        };

        Ok(Box::new(session))
    }

    async fn resume(&self, cp: Checkpoint) -> Result<Box<dyn BackendSession>> {
        let headers = cp.headers.unwrap_or_default();
        let cancel = CancellationToken::new();
        let (ws, event_rx) =
            connect_wss_with_retry(cp.ws_url.clone(), &headers, cancel.clone()).await?;

        let session = CloudBackendSession {
            ws,
            ws_url: cp.ws_url,
            headers,
            cmd_id: Arc::new(AtomicI64::new(1)),
            event_rx: Some(event_rx),
            cancel,
            attached_targets: HashMap::new(),
        };

        Ok(Box::new(session))
    }
}

// ---------------------------------------------------------------------------
// CloudBackendSession
// ---------------------------------------------------------------------------

/// A live WSS connection to a cloud-hosted browser instance.
pub struct CloudBackendSession {
    /// WebSocket connection to the remote browser.
    ws: WsStream,
    /// The WSS endpoint URL.
    ws_url: String,
    /// Auth headers used for the WSS handshake.
    headers: HashMap<String, String>,
    /// Monotonically increasing CDP command ID.
    cmd_id: Arc<AtomicI64>,
    /// Receiver for backend events from the WS monitor task.
    #[allow(dead_code)]
    event_rx: Option<mpsc::UnboundedReceiver<BackendEvent>>,
    /// Token to cancel the background monitor task on shutdown.
    cancel: CancellationToken,
    /// Mapping from CDP target_id to flattened sessionId.
    attached_targets: HashMap<String, String>,
}

#[async_trait]
impl BackendSession for CloudBackendSession {
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

        if let Some(sid) = session_id {
            cmd["sessionId"] = serde_json::Value::String(sid);
        }

        self.ws
            .send(Message::Text(cmd.to_string().into()))
            .await
            .map_err(|e| ActionbookError::CdpConnectionFailed(format!("WSS send failed: {e}")))?;

        tokio::time::timeout(Duration::from_secs(30), self.read_response(id))
            .await
            .map_err(|_| {
                ActionbookError::CdpConnectionFailed("CDP response timeout (30s)".into())
            })?
    }

    /// List targets via WSS using Target.getTargets (no HTTP endpoint for cloud).
    ///
    /// Opens a separate short-lived WSS connection to avoid interleaving with
    /// the main command/response stream.
    async fn list_targets(&self) -> Result<Vec<TargetInfo>> {
        let mut probe_ws = connect_wss(&self.ws_url, &self.headers).await?;
        let cmd = serde_json::json!({
            "id": 1,
            "method": "Target.getTargets",
            "params": {},
        });
        probe_ws
            .send(Message::Text(cmd.to_string().into()))
            .await
            .map_err(|e| ActionbookError::CdpConnectionFailed(format!("WSS send failed: {e}")))?;

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            read_single_response(&mut probe_ws, 1),
        )
        .await
        .map_err(|_| ActionbookError::CdpConnectionFailed("Target.getTargets timeout".into()))??;

        let _ = probe_ws.close(None).await;

        let target_infos = result
            .value
            .get("targetInfos")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|t| TargetInfo {
                        target_id: t["targetId"].as_str().unwrap_or("").to_string(),
                        target_type: t["type"].as_str().unwrap_or("").to_string(),
                        title: t["title"].as_str().unwrap_or("").to_string(),
                        url: t["url"].as_str().unwrap_or("").to_string(),
                        attached: t["attached"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(target_infos)
    }

    async fn checkpoint(&self) -> Result<Checkpoint> {
        Ok(Checkpoint {
            kind: BackendKind::Cloud,
            pid: None,
            ws_url: self.ws_url.clone(),
            cdp_port: None,
            user_data_dir: None,
            headers: if self.headers.is_empty() {
                None
            } else {
                Some(self.headers.clone())
            },
        })
    }

    /// Health check via Browser.getVersion over the main WSS connection.
    ///
    /// Note: this takes &self but we need to send over WS. We use a separate
    /// short-lived connection for the health probe to avoid interleaving with
    /// the command stream.
    async fn health(&self) -> Result<Health> {
        match connect_wss(&self.ws_url, &self.headers).await {
            Ok(mut probe_ws) => {
                let cmd = serde_json::json!({
                    "id": 1,
                    "method": "Browser.getVersion",
                    "params": {},
                });
                if probe_ws
                    .send(Message::Text(cmd.to_string().into()))
                    .await
                    .is_err()
                {
                    return Ok(Health {
                        connected: false,
                        browser_version: None,
                        uptime_secs: None,
                    });
                }

                match tokio::time::timeout(
                    Duration::from_secs(5),
                    read_single_response(&mut probe_ws, 1),
                )
                .await
                {
                    Ok(Ok(result)) => {
                        let version = result
                            .value
                            .get("product")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        Ok(Health {
                            connected: true,
                            browser_version: version,
                            uptime_secs: None,
                        })
                    }
                    _ => Ok(Health {
                        connected: false,
                        browser_version: None,
                        uptime_secs: None,
                    }),
                }
            }
            Err(_) => Ok(Health {
                connected: false,
                browser_version: None,
                uptime_secs: None,
            }),
        }
    }

    async fn shutdown(&mut self, policy: ShutdownPolicy) -> Result<()> {
        self.cancel.cancel();

        match policy {
            ShutdownPolicy::Graceful => {
                // Send Browser.close via WSS
                let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);
                let cmd = serde_json::json!({
                    "id": id,
                    "method": "Browser.close",
                    "params": {},
                });
                let _ = self.ws.send(Message::Text(cmd.to_string().into())).await;
            }
            ShutdownPolicy::ForceKill => {
                // No-op for cloud — we cannot kill a remote process.
            }
        }

        // Close the WSS connection
        let _ = self.ws.close(None).await;
        Ok(())
    }
}

impl CloudBackendSession {
    /// Ensure a target is attached via `Target.attachToTarget` with flattened session mode.
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
            .map_err(|e| ActionbookError::CdpConnectionFailed(format!("WSS send failed: {e}")))?;

        let result = tokio::time::timeout(Duration::from_secs(10), self.read_response(id))
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
        read_single_response(&mut self.ws, expected_id).await
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a WSS connection with auth headers.
async fn connect_wss(ws_url: &str, headers: &HashMap<String, String>) -> Result<WsStream> {
    let mut request = ws_url
        .into_client_request()
        .map_err(|e| ActionbookError::CdpConnectionFailed(format!("Bad WebSocket URL: {e}")))?;

    if !headers.is_empty() {
        for (key, value) in headers {
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::HeaderName::try_from(key.as_str()).map_err(
                    |e| ActionbookError::CdpConnectionFailed(format!("Bad header name: {e}")),
                )?,
                tokio_tungstenite::tungstenite::http::HeaderValue::from_str(value).map_err(
                    |e| ActionbookError::CdpConnectionFailed(format!("Bad header value: {e}")),
                )?,
            );
        }
    }

    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("WSS connection to {ws_url} failed: {e}"))
        })?;

    Ok(ws)
}

/// Connect to WSS and spawn a monitor task that periodically probes the
/// connection by sending Browser.getVersion over a separate short-lived WS.
async fn connect_wss_and_monitor(
    ws_url: String,
    headers: &HashMap<String, String>,
    cancel: CancellationToken,
) -> Result<(WsStream, mpsc::UnboundedReceiver<BackendEvent>)> {
    let ws = connect_wss(&ws_url, headers).await?;
    let (tx, rx) = mpsc::unbounded_channel();

    // Monitor task: periodically probe via a separate WSS connection.
    // On failure, attempt reconnection with exponential backoff before
    // declaring the browser disconnected.
    let probe_url = ws_url.clone();
    let probe_headers = headers.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            }

            // Try a health probe on a separate connection
            let probe_ok = match connect_wss(&probe_url, &probe_headers).await {
                Ok(mut probe_ws) => {
                    let cmd = serde_json::json!({
                        "id": 1,
                        "method": "Browser.getVersion",
                        "params": {},
                    });
                    if probe_ws
                        .send(Message::Text(cmd.to_string().into()))
                        .await
                        .is_err()
                    {
                        false
                    } else {
                        tokio::time::timeout(
                            Duration::from_secs(5),
                            read_single_response(&mut probe_ws, 1),
                        )
                        .await
                        .is_ok_and(|r| r.is_ok())
                    }
                }
                Err(_) => false,
            };

            if !probe_ok {
                // Attempt reconnection with exponential backoff before giving up.
                if !attempt_reconnect_probe(&probe_url, &probe_headers).await {
                    let _ = tx.send(BackendEvent::Disconnected {
                        reason: "Cloud browser unreachable after reconnect retries".into(),
                    });
                    break;
                }
            }
        }
    });

    Ok((ws, rx))
}

/// Connect to WSS with exponential backoff retry (for resume).
#[allow(dead_code)]
async fn connect_wss_with_retry(
    ws_url: String,
    headers: &HashMap<String, String>,
    cancel: CancellationToken,
) -> Result<(WsStream, mpsc::UnboundedReceiver<BackendEvent>)> {
    let config = ReconnectConfig::default();
    let mut delay = config.initial_delay;

    for attempt in 1..=config.max_attempts {
        match connect_wss_and_monitor(ws_url.clone(), headers, cancel.clone()).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt == config.max_attempts {
                    return Err(ActionbookError::CdpConnectionFailed(format!(
                        "WSS reconnection failed after {} attempts: {e}",
                        config.max_attempts
                    )));
                }
                tracing::warn!(
                    attempt,
                    max = config.max_attempts,
                    delay_ms = delay.as_millis(),
                    "WSS connection failed, retrying: {e}"
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(config.max_delay);
            }
        }
    }

    unreachable!()
}

/// Attempt reconnection with exponential backoff (1s, 2s, 4s, ..., max 30s).
/// Returns `true` if a health probe succeeded within the retry window.
async fn attempt_reconnect_probe(ws_url: &str, headers: &HashMap<String, String>) -> bool {
    let config = ReconnectConfig::default();
    let mut delay = config.initial_delay;

    for attempt in 1..=config.max_attempts {
        tracing::warn!(
            attempt,
            max = config.max_attempts,
            delay_ms = delay.as_millis(),
            "Cloud backend reconnect probe attempt"
        );
        tokio::time::sleep(delay).await;

        if let Ok(mut probe_ws) = connect_wss(ws_url, headers).await {
            let cmd = serde_json::json!({
                "id": 1,
                "method": "Browser.getVersion",
                "params": {},
            });
            if probe_ws
                .send(Message::Text(cmd.to_string().into()))
                .await
                .is_ok()
                && tokio::time::timeout(
                    Duration::from_secs(5),
                    read_single_response(&mut probe_ws, 1),
                )
                .await
                .is_ok_and(|r| r.is_ok())
            {
                tracing::info!(attempt, "Cloud backend reconnect probe succeeded");
                return true;
            }
        }

        delay = (delay * 2).min(config.max_delay);
    }

    false
}

/// Read WS messages until we get a response matching `expected_id`.
async fn read_single_response(ws: &mut WsStream, expected_id: i64) -> Result<OpResult> {
    let mut parse_failures = 0u8;

    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<CdpResponse>(&text) {
                    Ok(response) => {
                        if response.id == expected_id {
                            if let Some(err) = response.error {
                                return Err(ActionbookError::CdpError(format!("CDP error: {err}")));
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

/// Translate a [`BackendOp`] into a CDP method name and params JSON.
///
/// This is identical to the local backend's `op_to_cdp` — both backends
/// speak raw CDP, just over different transports.
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
    fn cloud_factory_kind() {
        let factory = CloudBackendFactory;
        assert_eq!(factory.kind(), BackendKind::Cloud);
    }

    #[test]
    fn cloud_factory_capabilities() {
        let factory = CloudBackendFactory;
        let caps = factory.capabilities();
        assert!(!caps.can_launch);
        assert!(caps.can_attach);
        assert!(caps.can_resume);
        assert!(caps.supports_headless);
    }

    #[tokio::test]
    async fn cloud_start_returns_error() {
        let factory = CloudBackendFactory;
        let spec = StartSpec {
            profile: "test".into(),
            headless: false,
            open_url: None,
            extra_args: vec![],
        };
        let result = factory.start(spec).await;
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("cannot start"), "got: {err}");
    }

    #[test]
    fn reconnect_config_defaults() {
        let config = ReconnectConfig::default();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(30));
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
    fn op_to_cdp_get_targets() {
        let (method, _) = op_to_cdp(&BackendOp::GetTargets);
        assert_eq!(method, "Target.getTargets");
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
    fn op_to_cdp_close_target() {
        let op = BackendOp::CloseTarget {
            target_id: "ABC".into(),
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Target.closeTarget");
        assert_eq!(params["targetId"], "ABC");
    }

    #[test]
    fn op_to_cdp_set_cookie_with_optional_fields() {
        let op = BackendOp::SetCookie {
            target_id: "T1".into(),
            name: "session".into(),
            value: "abc123".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: Some(true),
            http_only: Some(true),
            same_site: Some("Strict".into()),
            expires: Some(123.0),
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Network.setCookie");
        assert_eq!(params["secure"], true);
        assert_eq!(params["httpOnly"], true);
        assert_eq!(params["sameSite"], "Strict");
        assert_eq!(params["expires"], 123.0);
    }

    #[test]
    fn op_to_cdp_delete_cookies_and_dom_ops() {
        let delete = BackendOp::DeleteCookies {
            target_id: "T1".into(),
            name: "session".into(),
            domain: Some(".example.com".into()),
            path: Some("/".into()),
        };
        let (method, params) = op_to_cdp(&delete);
        assert_eq!(method, "Network.deleteCookies");
        assert_eq!(params["domain"], ".example.com");
        assert_eq!(params["path"], "/");

        let focus = BackendOp::DomFocus {
            target_id: "T1".into(),
            node_id: 11,
        };
        let (focus_method, focus_params) = op_to_cdp(&focus);
        assert_eq!(focus_method, "DOM.focus");
        assert_eq!(focus_params["nodeId"], 11);

        let files = BackendOp::SetFileInputFiles {
            target_id: "T1".into(),
            node_id: 12,
            files: vec!["/tmp/file.txt".into()],
        };
        let (file_method, file_params) = op_to_cdp(&files);
        assert_eq!(file_method, "DOM.setFileInputFiles");
        assert_eq!(file_params["files"][0], "/tmp/file.txt");
    }

    #[test]
    fn op_to_cdp_get_box_and_node_location() {
        let box_model = BackendOp::GetBoxModel {
            target_id: "T1".into(),
            node_id: 42,
        };
        let (box_method, box_params) = op_to_cdp(&box_model);
        assert_eq!(box_method, "DOM.getBoxModel");
        assert_eq!(box_params["nodeId"], 42);

        let node = BackendOp::GetNodeForLocation {
            target_id: "T1".into(),
            x: 9,
            y: 21,
        };
        let (node_method, node_params) = op_to_cdp(&node);
        assert_eq!(node_method, "DOM.getNodeForLocation");
        assert_eq!(node_params["x"], 9);
        assert_eq!(node_params["y"], 21);
    }

    #[test]
    fn checkpoint_round_trip_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), "Bearer tok".into());
        headers.insert("X-Custom".into(), "val".into());

        let cp = Checkpoint {
            kind: BackendKind::Cloud,
            pid: None,
            ws_url: "wss://cloud.example.com/browser/abc".into(),
            cdp_port: None,
            user_data_dir: None,
            headers: Some(headers),
        };

        let json = serde_json::to_string(&cp).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.kind, BackendKind::Cloud);
        assert!(decoded.pid.is_none());
        assert!(decoded.cdp_port.is_none());
        assert!(decoded.user_data_dir.is_none());
        let hdrs = decoded.headers.unwrap();
        assert_eq!(hdrs.get("Authorization").unwrap(), "Bearer tok");
        assert_eq!(hdrs.get("X-Custom").unwrap(), "val");
    }

    #[test]
    fn checkpoint_round_trip_no_headers() {
        let cp = Checkpoint {
            kind: BackendKind::Cloud,
            pid: None,
            ws_url: "wss://cloud.example.com/browser/abc".into(),
            cdp_port: None,
            user_data_dir: None,
            headers: None,
        };

        let json = serde_json::to_string(&cp).unwrap();
        assert!(!json.contains("headers"));
        let decoded: Checkpoint = serde_json::from_str(&json).unwrap();
        assert!(decoded.headers.is_none());
    }

    #[test]
    fn build_request_with_auth_headers() {
        let mut request = "wss://cloud.example.com/browser"
            .into_client_request()
            .unwrap();

        let headers_map: HashMap<String, String> = [
            ("Authorization".into(), "Bearer token123".into()),
            ("X-Session-Id".into(), "sess-abc".into()),
        ]
        .into();

        for (key, value) in &headers_map {
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::HeaderName::try_from(key.as_str()).unwrap(),
                tokio_tungstenite::tungstenite::http::HeaderValue::from_str(value).unwrap(),
            );
        }

        assert_eq!(
            request.headers().get("Authorization").unwrap(),
            "Bearer token123"
        );
        assert_eq!(request.headers().get("X-Session-Id").unwrap(), "sess-abc");
    }

    #[test]
    fn build_request_empty_headers() {
        let request = "wss://cloud.example.com/browser"
            .into_client_request()
            .unwrap();

        assert!(request.headers().get("Authorization").is_none());
    }

    #[test]
    fn backoff_progression() {
        let config = ReconnectConfig::default();
        let mut delay = config.initial_delay;
        let expected_secs = [1, 2, 4, 8, 16];

        for &exp in &expected_secs {
            assert_eq!(delay.as_secs(), exp);
            delay = (delay * 2).min(config.max_delay);
        }

        // After 16s, next would be 32s but capped at max_delay (30s)
        assert_eq!(delay.as_secs(), 30);
    }

    #[tokio::test]
    async fn cloud_attach_fails_with_bad_url() {
        let factory = CloudBackendFactory;
        let spec = AttachSpec {
            ws_url: "wss://nonexistent.invalid:9999/browser".into(),
            headers: Some(HashMap::from([(
                "Authorization".into(),
                "Bearer test".into(),
            )])),
        };
        let result = factory.attach(spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cloud_resume_fails_with_bad_url() {
        let factory = CloudBackendFactory;
        let cp = Checkpoint {
            kind: BackendKind::Cloud,
            pid: None,
            ws_url: "wss://nonexistent.invalid:9999/browser".into(),
            cdp_port: None,
            user_data_dir: None,
            headers: Some(HashMap::from([(
                "Authorization".into(),
                "Bearer test".into(),
            )])),
        };
        let result = factory.resume(cp).await;
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(
            err.contains("reconnection failed") || err.contains("WSS connection"),
            "got: {err}"
        );
    }

    #[test]
    fn op_to_cdp_remaining_variants() {
        // GetDocument
        let (method, _) = op_to_cdp(&BackendOp::GetDocument {
            target_id: "T1".into(),
        });
        assert_eq!(method, "DOM.getDocument");

        // QuerySelector
        let (method, params) = op_to_cdp(&BackendOp::QuerySelector {
            target_id: "T1".into(),
            node_id: 1,
            selector: ".btn".into(),
        });
        assert_eq!(method, "DOM.querySelector");
        assert_eq!(params["selector"], ".btn");
        assert_eq!(params["nodeId"], 1);

        // DispatchMouseEvent
        let (method, params) = op_to_cdp(&BackendOp::DispatchMouseEvent {
            target_id: "T1".into(),
            event_type: "mousePressed".into(),
            x: 100.0,
            y: 200.0,
            button: "left".into(),
            click_count: 1,
        });
        assert_eq!(method, "Input.dispatchMouseEvent");
        assert_eq!(params["type"], "mousePressed");
        assert_eq!(params["x"], 100.0);
        assert_eq!(params["y"], 200.0);
        assert_eq!(params["button"], "left");
        assert_eq!(params["clickCount"], 1);

        // DispatchKeyEvent
        let (method, params) = op_to_cdp(&BackendOp::DispatchKeyEvent {
            target_id: "T1".into(),
            event_type: "keyDown".into(),
            key: "Enter".into(),
            text: "\r".into(),
        });
        assert_eq!(method, "Input.dispatchKeyEvent");
        assert_eq!(params["key"], "Enter");
        assert_eq!(params["text"], "\r");

        // PrintToPdf
        let (method, _) = op_to_cdp(&BackendOp::PrintToPdf {
            target_id: "T1".into(),
        });
        assert_eq!(method, "Page.printToPDF");

        // GetAccessibilityTree
        let (method, _) = op_to_cdp(&BackendOp::GetAccessibilityTree {
            target_id: "T1".into(),
        });
        assert_eq!(method, "Accessibility.getFullAXTree");

        // GetCookies
        let (method, _) = op_to_cdp(&BackendOp::GetCookies {
            target_id: "T1".into(),
        });
        assert_eq!(method, "Network.getCookies");

        // CreateTarget
        let (method, params) = op_to_cdp(&BackendOp::CreateTarget {
            url: "https://example.com".into(),
            window_id: None,
            new_window: true,
        });
        assert_eq!(method, "Target.createTarget");
        assert_eq!(params["url"], "https://example.com");
        assert_eq!(params["newWindow"], true);
    }

    #[test]
    fn op_to_cdp_set_cookie_minimal_fields() {
        let op = BackendOp::SetCookie {
            target_id: "T1".into(),
            name: "uid".into(),
            value: "42".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: None,
            http_only: None,
            same_site: None,
            expires: None,
        };
        let (method, params) = op_to_cdp(&op);
        assert_eq!(method, "Network.setCookie");
        assert_eq!(params["name"], "uid");
        assert_eq!(params["value"], "42");
        assert!(params.get("secure").is_none());
        assert!(params.get("httpOnly").is_none());
        assert!(params.get("sameSite").is_none());
        assert!(params.get("expires").is_none());
    }
}
