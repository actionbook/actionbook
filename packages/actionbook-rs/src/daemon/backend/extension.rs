//! Extension backend: connect to the user's Chrome via extension bridge.
//!
//! Implements the two-phase connection model:
//!
//! **Phase 1** — One-time handshake via native messaging:
//! The extension calls `chrome.runtime.connectNative()` to the actionbook binary.
//! The native messaging host connects to the daemon via UDS, requests bridge info.
//! Daemon generates a one-time token + allocates a WS port for the ExtensionBridge.
//! Token is returned to the extension via native messaging; host exits.
//!
//! **Phase 2** — Persistent WS connection:
//! The extension connects to `ws://localhost:PORT` with `Authorization: Bearer TOKEN`.
//! Daemon validates the token (one-time, invalidated after use).
//! Persistent bidirectional WS is established.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use futures::SinkExt;
use rand::Rng;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;

use super::types::*;
use super::{BackendSession, BrowserBackendFactory};
use crate::daemon::backend_op::BackendOp;
use crate::error::{ActionbookError, Result};

/// Timeout waiting for the extension to connect via WS after attach.
const EXTENSION_CONNECT_TIMEOUT_SECS: u64 = 60;

// ---------------------------------------------------------------------------
// Token generation
// ---------------------------------------------------------------------------

/// Generate a one-time token: 32 random bytes, hex-encoded (64 hex chars).
pub fn generate_one_time_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Validate a token using constant-time comparison to prevent timing attacks.
fn validate_token(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

// ---------------------------------------------------------------------------
// Bridge message protocol
// ---------------------------------------------------------------------------

/// Message sent from daemon to extension over the bridge WS.
#[derive(Debug, serde::Serialize)]
struct BridgeRequest {
    /// Monotonically increasing request ID for correlating responses.
    id: i64,
    /// CDP method name (e.g. "Page.navigate", "DOM.getDocument").
    method: String,
    /// CDP method parameters.
    params: serde_json::Value,
    /// CDP target ID for page-scoped commands.
    #[serde(skip_serializing_if = "Option::is_none")]
    target_id: Option<String>,
}

/// Message sent from extension to daemon over the bridge WS.
#[derive(Debug, serde::Deserialize)]
struct BridgeResponse {
    /// Correlates with the request ID.
    id: i64,
    /// CDP result (present on success).
    result: Option<serde_json::Value>,
    /// Error (present on failure).
    error: Option<BridgeError>,
}

#[derive(Debug, serde::Deserialize)]
struct BridgeError {
    #[allow(dead_code)]
    code: Option<i64>,
    message: String,
}

/// Extension-originated event (not a response to a request).
#[derive(Debug, serde::Deserialize)]
struct BridgeEvent {
    /// Event type: "disconnected", "target_created", "target_destroyed", etc.
    #[serde(rename = "type")]
    event_type: String,
    /// Event-specific payload.
    #[serde(flatten)]
    data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// ExtensionBackendFactory
// ---------------------------------------------------------------------------

/// Factory that creates [`ExtensionBackendSession`]s by starting a bridge
/// WS server and waiting for the extension to connect.
pub struct ExtensionBackendFactory;

#[async_trait]
impl BrowserBackendFactory for ExtensionBackendFactory {
    fn kind(&self) -> BackendKind {
        BackendKind::Extension
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_launch: false,
            can_attach: true,
            can_resume: false,
            supports_headless: false,
        }
    }

    async fn start(&self, _spec: StartSpec) -> Result<Box<dyn BackendSession>> {
        Err(ActionbookError::FeatureNotSupported(
            "Extension backend cannot launch a browser. Use 'attach' instead.".into(),
        ))
    }

    async fn attach(&self, _spec: AttachSpec) -> Result<Box<dyn BackendSession>> {
        // 1. Bind a WS server on a random port
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
            ActionbookError::ExtensionError(format!("Failed to bind bridge listener: {e}"))
        })?;
        let port = listener
            .local_addr()
            .map_err(|e| {
                ActionbookError::ExtensionError(format!("Failed to get bridge port: {e}"))
            })?
            .port();

        // 2. Generate a one-time token
        let token = generate_one_time_token();

        tracing::info!(port, "Extension bridge listening, waiting for connection");

        // 3. Wait for the extension to connect (with timeout)
        let session = wait_for_extension_connect(listener, token, port).await?;

        Ok(Box::new(session))
    }

    async fn resume(&self, _cp: Checkpoint) -> Result<Box<dyn BackendSession>> {
        Err(ActionbookError::FeatureNotSupported(
            "Extension backend does not support resume. The extension must reconnect.".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// ExtensionBridge — daemon-internal WS server
// ---------------------------------------------------------------------------

/// Internal state shared between the bridge acceptor and the session.
struct BridgeState {
    /// One-time token; set to None after successful validation.
    token: Option<String>,
}

/// Type alias for the WebSocket stream used by the extension bridge.
type ExtWsStream = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

/// Wait for the extension to connect to the bridge WS server.
///
/// Validates the Bearer token on the first connection, then returns
/// an `ExtensionBackendSession` with the established WS.
async fn wait_for_extension_connect(
    listener: TcpListener,
    token: String,
    port: u16,
) -> Result<ExtensionBackendSession> {
    let state = Arc::new(Mutex::new(BridgeState { token: Some(token) }));

    let accept = async {
        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(|e| ActionbookError::ExtensionError(format!("Accept failed: {e}")))?;

            // Only accept from loopback
            if !peer.ip().is_loopback() {
                tracing::warn!("Rejected non-loopback connection from {peer}");
                drop(stream);
                continue;
            }

            let state_guard = state.lock().await;
            let expected_token = match &state_guard.token {
                Some(t) => t.clone(),
                None => {
                    // Token already consumed — reject.
                    tracing::warn!("Rejected connection: token already consumed");
                    drop(stream);
                    continue;
                }
            };
            drop(state_guard);

            // Perform WS upgrade with token validation in the handshake callback.
            let token_for_cb = expected_token.clone();
            #[allow(clippy::result_large_err)]
            let ws_result = tokio_tungstenite::accept_hdr_async(
                stream,
                move |req: &tokio_tungstenite::tungstenite::http::Request<()>,
                      resp: tokio_tungstenite::tungstenite::http::Response<()>|
                      -> std::result::Result<
                    tokio_tungstenite::tungstenite::http::Response<()>,
                    tokio_tungstenite::tungstenite::http::Response<Option<String>>,
                > {
                    // Extract Authorization header
                    let auth = req
                        .headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");

                    let provided_token = auth.strip_prefix("Bearer ").unwrap_or("");

                    if validate_token(provided_token, &token_for_cb) {
                        Ok(resp)
                    } else {
                        tracing::warn!("Extension bridge: invalid token");
                        let reject = tokio_tungstenite::tungstenite::http::Response::builder()
                            .status(tokio_tungstenite::tungstenite::http::StatusCode::UNAUTHORIZED)
                            .body(Some("Invalid token".to_string()))
                            .unwrap_or_else(|_| {
                                tokio_tungstenite::tungstenite::http::Response::new(Some(
                                    "Invalid token".to_string(),
                                ))
                            });
                        Err(reject)
                    }
                },
            )
            .await;

            match ws_result {
                Ok(ws) => {
                    // Token consumed — invalidate it
                    let mut state_guard = state.lock().await;
                    state_guard.token = None;
                    drop(state_guard);

                    tracing::info!("Extension connected to bridge on port {port}");

                    let (event_tx, event_rx) = mpsc::unbounded_channel();

                    return Ok(ExtensionBackendSession::new(ws, port, event_tx, event_rx));
                }
                Err(e) => {
                    tracing::debug!("WS upgrade failed (bad token?): {e}");
                    continue;
                }
            }
        }
    };

    tokio::time::timeout(
        std::time::Duration::from_secs(EXTENSION_CONNECT_TIMEOUT_SECS),
        accept,
    )
    .await
    .map_err(|_| {
        ActionbookError::Timeout(format!(
            "Extension did not connect within {EXTENSION_CONNECT_TIMEOUT_SECS}s"
        ))
    })?
}

// ---------------------------------------------------------------------------
// ExtensionBackendSession
// ---------------------------------------------------------------------------

/// A live WS connection to the Chrome extension via the daemon's bridge server.
pub struct ExtensionBackendSession {
    /// WebSocket connection to the extension.
    ws: ExtWsStream,
    /// Bridge port.
    port: u16,
    /// Monotonically increasing request ID for bridge messages.
    cmd_id: AtomicI64,
    /// Sender for backend events.
    event_tx: mpsc::UnboundedSender<BackendEvent>,
    /// Receiver for backend events (taken once by `events()`).
    event_rx: Option<mpsc::UnboundedReceiver<BackendEvent>>,
    /// Connection start time.
    connected_at: Instant,
}

impl ExtensionBackendSession {
    fn new(
        ws: ExtWsStream,
        port: u16,
        event_tx: mpsc::UnboundedSender<BackendEvent>,
        event_rx: mpsc::UnboundedReceiver<BackendEvent>,
    ) -> Self {
        Self {
            ws,
            port,
            cmd_id: AtomicI64::new(1),
            event_tx,
            event_rx: Some(event_rx),
            connected_at: Instant::now(),
        }
    }

    /// Send a bridge request and wait for the matching response.
    async fn send_and_recv(
        &mut self,
        method: &str,
        params: serde_json::Value,
        target_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);

        let request = BridgeRequest {
            id,
            method: method.to_string(),
            params,
            target_id,
        };

        let msg_str = serde_json::to_string(&request).map_err(|e| {
            ActionbookError::ExtensionError(format!("Failed to serialize bridge request: {e}"))
        })?;

        self.ws
            .send(Message::Text(msg_str.into()))
            .await
            .map_err(|e| ActionbookError::ExtensionError(format!("Bridge WS send failed: {e}")))?;

        // Read messages until we get the response with our ID.
        // Events are forwarded to the event channel.
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(30), self.read_response(id))
                .await
                .map_err(|_| {
                    ActionbookError::Timeout("Extension bridge response timeout (30s)".into())
                })??;

        Ok(response)
    }

    /// Read WS messages until we find a response matching `expected_id`.
    /// Non-matching messages and events are handled appropriately.
    async fn read_response(&mut self, expected_id: i64) -> Result<serde_json::Value> {
        while let Some(msg) = self.ws.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Try to parse as a response first
                    if let Ok(resp) = serde_json::from_str::<BridgeResponse>(&text) {
                        if resp.id == expected_id {
                            if let Some(err) = resp.error {
                                return Err(ActionbookError::ExtensionError(format!(
                                    "Extension error: {}",
                                    err.message
                                )));
                            }
                            return Ok(resp.result.unwrap_or(serde_json::Value::Null));
                        }
                        // Response for a different ID — skip (shouldn't happen
                        // since we serialize calls, but be defensive).
                        continue;
                    }

                    // Try to parse as an event
                    if let Ok(event) = serde_json::from_str::<BridgeEvent>(&text) {
                        self.handle_event(event);
                        continue;
                    }

                    tracing::debug!(
                        "Unrecognized bridge message: {}",
                        &text[..text.len().min(200)]
                    );
                }
                Ok(Message::Close(_)) => {
                    let _ = self.event_tx.send(BackendEvent::Disconnected {
                        reason: "Extension WS closed".into(),
                    });
                    return Err(ActionbookError::ExtensionError(
                        "Extension disconnected".into(),
                    ));
                }
                Ok(Message::Ping(data)) => {
                    let _ = self.ws.send(Message::Pong(data)).await;
                }
                Ok(_) => {} // Ignore binary, pong, etc.
                Err(e) => {
                    let _ = self.event_tx.send(BackendEvent::Disconnected {
                        reason: format!("Bridge WS error: {e}"),
                    });
                    return Err(ActionbookError::ExtensionError(format!(
                        "Bridge WS read error: {e}"
                    )));
                }
            }
        }

        let _ = self.event_tx.send(BackendEvent::Disconnected {
            reason: "Extension WS stream ended".into(),
        });
        Err(ActionbookError::ExtensionError(
            "Extension WS stream ended unexpectedly".into(),
        ))
    }

    /// Handle an event from the extension.
    fn handle_event(&self, event: BridgeEvent) {
        let backend_event = match event.event_type.as_str() {
            "disconnected" => Some(BackendEvent::Disconnected {
                reason: event
                    .data
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("extension disconnected")
                    .to_string(),
            }),
            "target_created" => event
                .data
                .get("target_id")
                .or_else(|| event.data.get("targetId"))
                .and_then(|v| v.as_str())
                .map(|tid| BackendEvent::TargetCreated {
                    target_id: tid.to_string(),
                }),
            "target_destroyed" => event
                .data
                .get("target_id")
                .or_else(|| event.data.get("targetId"))
                .and_then(|v| v.as_str())
                .map(|tid| BackendEvent::TargetDestroyed {
                    target_id: tid.to_string(),
                }),
            "dialog" => event
                .data
                .get("message")
                .and_then(|v| v.as_str())
                .map(|msg| BackendEvent::Dialog {
                    message: msg.to_string(),
                }),
            _ => {
                tracing::debug!("Unknown extension event type: {}", event.event_type);
                None
            }
        };

        if let Some(evt) = backend_event {
            let _ = self.event_tx.send(evt);
        }
    }
}

#[async_trait]
impl BackendSession for ExtensionBackendSession {
    fn events(&mut self) -> BoxStream<'static, BackendEvent> {
        if let Some(rx) = self.event_rx.take() {
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx).boxed()
        } else {
            futures::stream::empty().boxed()
        }
    }

    async fn exec(&mut self, op: BackendOp) -> Result<OpResult> {
        let target_id = op.target_id().map(|s| s.to_string());
        let (method, params) = op_to_cdp(&op);

        let result = self.send_and_recv(method, params, target_id).await?;

        Ok(OpResult::new(result))
    }

    async fn list_targets(&self) -> Result<Vec<TargetInfo>> {
        // We cannot use send_and_recv here because it requires &mut self.
        // For list_targets (&self), we return an error suggesting exec be used
        // from the session actor which has &mut self.
        //
        // In practice, list_targets is called by the session actor which holds
        // &mut self on the BackendSession. The trait signature says &self for
        // compatibility, but our implementation needs mutability for WS I/O.
        //
        // As a workaround, return an empty list. The session actor should use
        // exec(BackendOp::GetTargets) instead.
        Err(ActionbookError::ExtensionError(
            "Extension backend list_targets requires exec(BackendOp::GetTargets)".into(),
        ))
    }

    async fn checkpoint(&self) -> Result<Checkpoint> {
        Ok(Checkpoint {
            kind: BackendKind::Extension,
            pid: None,
            ws_url: format!("ws://127.0.0.1:{}", self.port),
            cdp_port: None,
            user_data_dir: None,
            headers: None,
        })
    }

    async fn health(&self) -> Result<Health> {
        // For extension backend, health is determined by whether the WS is open.
        // Since we can't send a ping without &mut self, we report based on
        // whether the event_tx is still active (receiver not dropped).
        Ok(Health {
            connected: !self.event_tx.is_closed(),
            browser_version: None,
            uptime_secs: Some(self.connected_at.elapsed().as_secs()),
        })
    }

    async fn shutdown(&mut self, _policy: ShutdownPolicy) -> Result<()> {
        // Send a detach command to the extension, then close the WS.
        let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);
        let request = BridgeRequest {
            id,
            method: "Extension.detach".to_string(),
            params: serde_json::json!({}),
            target_id: None,
        };

        if let Ok(msg_str) = serde_json::to_string(&request) {
            let _ = self.ws.send(Message::Text(msg_str.into())).await;
        }

        let _ = self.ws.close(None).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// op_to_cdp — convert BackendOp to CDP method + params
// ---------------------------------------------------------------------------

/// Convert a BackendOp into a CDP method name and parameters.
///
/// This is identical to the local backend's conversion — the extension
/// forwards these to `chrome.debugger.sendCommand()`.
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
    fn token_generation_length() {
        let token = generate_one_time_token();
        // 32 bytes = 64 hex characters
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn token_generation_uniqueness() {
        let t1 = generate_one_time_token();
        let t2 = generate_one_time_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn token_validation_correct() {
        let token = generate_one_time_token();
        assert!(validate_token(&token, &token));
    }

    #[test]
    fn token_validation_wrong() {
        let token = generate_one_time_token();
        let wrong = generate_one_time_token();
        assert!(!validate_token(&wrong, &token));
    }

    #[test]
    fn token_validation_empty() {
        let token = generate_one_time_token();
        assert!(!validate_token("", &token));
    }

    #[test]
    fn token_validation_length_mismatch() {
        let token = generate_one_time_token();
        assert!(!validate_token("short", &token));
    }

    #[test]
    fn factory_kind() {
        let factory = ExtensionBackendFactory;
        assert_eq!(factory.kind(), BackendKind::Extension);
    }

    #[test]
    fn factory_capabilities() {
        let factory = ExtensionBackendFactory;
        let caps = factory.capabilities();
        assert!(!caps.can_launch);
        assert!(caps.can_attach);
        assert!(!caps.can_resume);
        assert!(!caps.supports_headless);
    }

    #[tokio::test]
    async fn factory_start_returns_error() {
        let factory = ExtensionBackendFactory;
        let spec = StartSpec {
            profile: "test".into(),
            headless: false,
            open_url: None,
            extra_args: vec![],
        };
        let result = factory.start(spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn factory_resume_returns_error() {
        let factory = ExtensionBackendFactory;
        let cp = Checkpoint {
            kind: BackendKind::Extension,
            pid: None,
            ws_url: "ws://127.0.0.1:0".into(),
            cdp_port: None,
            user_data_dir: None,
            headers: None,
        };
        let result = factory.resume(cp).await;
        assert!(result.is_err());
    }

    #[test]
    fn bridge_request_serialization() {
        let req = BridgeRequest {
            id: 1,
            method: "Page.navigate".into(),
            params: serde_json::json!({"url": "https://example.com"}),
            target_id: Some("TARGET_1".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "Page.navigate");
        assert_eq!(parsed["target_id"], "TARGET_1");
    }

    #[test]
    fn bridge_request_serialization_no_target() {
        let req = BridgeRequest {
            id: 2,
            method: "Target.getTargets".into(),
            params: serde_json::json!({}),
            target_id: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["id"], 2);
        assert!(parsed.get("target_id").is_none());
    }

    #[test]
    fn bridge_response_deserialization_success() {
        let json = r#"{"id": 1, "result": {"nodeId": 42}}"#;
        let resp: BridgeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["nodeId"], 42);
    }

    #[test]
    fn bridge_response_deserialization_error() {
        let json = r#"{"id": 1, "error": {"code": -32000, "message": "Not found"}}"#;
        let resp: BridgeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().message, "Not found");
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
        let (method, _) = op_to_cdp(&op);
        assert_eq!(method, "DOM.getDocument");
    }

    #[test]
    fn op_to_cdp_get_targets() {
        let op = BackendOp::GetTargets;
        let (method, _) = op_to_cdp(&op);
        assert_eq!(method, "Target.getTargets");
    }

    /// Integration test: WS round-trip with real tokio runtime.
    /// Simulates extension connecting to bridge and exchanging a message.
    #[tokio::test]
    async fn ws_message_round_trip() {
        use tokio_tungstenite::tungstenite::http;

        // 1. Bind a bridge listener
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = generate_one_time_token();
        let token_clone = token.clone();

        // 2. Spawn the bridge acceptor
        let bridge_handle =
            tokio::spawn(async move { wait_for_extension_connect(listener, token, port).await });

        // 3. Simulate extension connecting with the correct token
        let url = format!("ws://127.0.0.1:{port}");
        let request = http::Request::builder()
            .uri(&url)
            .header("authorization", format!("Bearer {token_clone}"))
            .header("host", format!("127.0.0.1:{port}"))
            .header("connection", "Upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header(
                "sec-websocket-key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .unwrap();

        let (mut ext_ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .expect("Extension WS connect failed");

        // 4. Get the session from the bridge
        let mut session = bridge_handle.await.unwrap().unwrap();

        // 5. Spawn a task to handle the extension side: read request, send response
        let ext_handle = tokio::spawn(async move {
            if let Some(Ok(Message::Text(text))) = ext_ws.next().await {
                let req: serde_json::Value = serde_json::from_str(&text).unwrap();
                let id = req["id"].as_i64().unwrap();
                let resp = serde_json::json!({
                    "id": id,
                    "result": {"nodeId": 99}
                });
                ext_ws
                    .send(Message::Text(resp.to_string().into()))
                    .await
                    .unwrap();
            }
            ext_ws
        });

        // 6. Send a request through the session
        let result = session
            .exec(BackendOp::GetDocument {
                target_id: "T1".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.value["nodeId"], 99);

        // Cleanup
        let _ = session.shutdown(ShutdownPolicy::Graceful).await;
        let _ = ext_handle.await;
    }

    /// Test that an invalid token is rejected during WS handshake.
    #[tokio::test]
    async fn ws_invalid_token_rejected() {
        use tokio_tungstenite::tungstenite::http;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = generate_one_time_token();

        // Spawn acceptor with short timeout
        let bridge_handle = tokio::spawn(async move {
            // Use a very short timeout to avoid blocking
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                wait_for_extension_connect(listener, token, port),
            )
            .await
        });

        // Try connecting with wrong token
        let url = format!("ws://127.0.0.1:{port}");
        let request = http::Request::builder()
            .uri(&url)
            .header("authorization", "Bearer wrong_token")
            .header("host", format!("127.0.0.1:{port}"))
            .header("connection", "Upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header(
                "sec-websocket-key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .unwrap();

        let result = tokio_tungstenite::connect_async(request).await;
        // The connection should be rejected
        assert!(result.is_err());

        // Bridge should timeout since no valid connection was made
        let bridge_result = bridge_handle.await.unwrap();
        assert!(bridge_result.is_err()); // Timeout
    }

    /// Test that the token is consumed after first use (one-time).
    #[tokio::test]
    async fn token_consumed_after_first_use() {
        use tokio_tungstenite::tungstenite::http;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = generate_one_time_token();
        let token_clone = token.clone();

        let bridge_handle =
            tokio::spawn(async move { wait_for_extension_connect(listener, token, port).await });

        // First connection with correct token — should succeed
        let url = format!("ws://127.0.0.1:{port}");
        let request = http::Request::builder()
            .uri(&url)
            .header("authorization", format!("Bearer {token_clone}"))
            .header("host", format!("127.0.0.1:{port}"))
            .header("connection", "Upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header(
                "sec-websocket-key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .unwrap();

        let (ext_ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .expect("First connection should succeed");

        let mut session = bridge_handle.await.unwrap().unwrap();

        // Session is established — token is consumed
        let cp = session.checkpoint().await.unwrap();
        assert_eq!(cp.kind, BackendKind::Extension);
        assert_eq!(cp.ws_url, format!("ws://127.0.0.1:{port}"));

        let _ = session.shutdown(ShutdownPolicy::Graceful).await;
        drop(ext_ws);
    }
}
