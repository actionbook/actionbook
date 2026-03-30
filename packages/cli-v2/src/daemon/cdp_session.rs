//! Persistent per-session CDP connection with request multiplexing.
//!
//! One browser-level WebSocket connection per session. Commands target specific
//! tabs via CDP flat sessions (Target.attachToTarget + sessionId). Concurrent
//! requests are multiplexed using incrementing message IDs.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http;

use crate::error::CliError;

type PendingResponseTx = oneshot::Sender<Result<Value, CliError>>;
type PendingRequests = Arc<Mutex<HashMap<u64, PendingResponseTx>>>;
type EventSubs = Arc<Mutex<HashMap<String, Vec<mpsc::Sender<Value>>>>>;
/// Per-tab in-flight network request counter, keyed by CDP flat-session ID.
type TabNetPending = Arc<Mutex<HashMap<String, i64>>>;

/// Persistent CDP connection for a single browser session.
///
/// All fields are `Arc`/`Sender` so `CdpSession` is cheaply `Clone`able.
/// The background reader task is spawned on `connect()` and routes incoming
/// WebSocket messages to the appropriate pending request by message ID.
#[derive(Clone)]
pub struct CdpSession {
    /// Channel to send raw WS text messages to the writer task.
    writer_tx: mpsc::Sender<String>,
    /// In-flight requests keyed by message ID.
    pending: PendingRequests,
    /// Atomic counter for generating unique message IDs.
    next_id: Arc<AtomicU64>,
    /// Mapping from CDP target_id → CDP sessionId (from Target.attachToTarget).
    tab_sessions: Arc<Mutex<HashMap<String, String>>>,
    /// Event subscribers keyed by `"{cdp_session_id}:{method}"`.
    event_subs: EventSubs,
    /// In-flight Network request count per CDP flat-session ID.
    /// Maintained by reader_loop from Network domain events; Network.enable is
    /// called in attach() so tracking starts before any user commands run.
    tab_net_pending: TabNetPending,
}

impl CdpSession {
    /// Connect to a browser-level WebSocket endpoint and spawn background tasks.
    pub async fn connect(ws_url: &str) -> Result<Self, CliError> {
        Self::connect_with_headers(ws_url, &[]).await
    }

    /// Connect with custom headers (for cloud mode auth).
    pub async fn connect_with_headers(
        ws_url: &str,
        headers: &[(String, String)],
    ) -> Result<Self, CliError> {
        let mut request = ws_url
            .into_client_request()
            .map_err(|e| CliError::CdpConnectionFailed(format!("invalid WS URL: {e}")))?;

        for (key, value) in headers {
            let header_name = key.parse::<http::HeaderName>().map_err(|e| {
                CliError::InvalidArgument(format!("invalid header name '{key}': {e}"))
            })?;
            let header_value = http::HeaderValue::from_str(value).map_err(|e| {
                CliError::InvalidArgument(format!("invalid header value for '{key}': {e}"))
            })?;
            request.headers_mut().append(header_name, header_value);
        }

        let (ws, _) = connect_async(request)
            .await
            .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

        let (ws_writer, ws_reader) = ws.split();
        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicU64::new(1));
        let (writer_tx, writer_rx) = mpsc::channel::<String>(64);
        let event_subs: EventSubs = Arc::new(Mutex::new(HashMap::new()));
        let tab_net_pending: TabNetPending = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(Self::writer_loop(ws_writer, writer_rx));
        let pending_clone = pending.clone();
        let event_subs_clone = event_subs.clone();
        let tab_net_pending_clone = tab_net_pending.clone();
        tokio::spawn(Self::reader_loop(
            ws_reader,
            pending_clone,
            event_subs_clone,
            tab_net_pending_clone,
        ));

        Ok(CdpSession {
            writer_tx,
            pending,
            next_id,
            tab_sessions: Arc::new(Mutex::new(HashMap::new())),
            event_subs,
            tab_net_pending,
        })
    }

    /// Attach to a CDP target (tab) using flat session mode.
    ///
    /// Sends `Target.attachToTarget` with `flatten: true` and stores the
    /// returned `sessionId` for future `execute_on_tab` calls.
    /// Idempotent: if already attached, returns the existing sessionId.
    pub async fn attach(&self, target_id: &str) -> Result<String, CliError> {
        // Check if already attached (idempotent)
        if let Some(existing) = self.tab_sessions.lock().await.get(target_id).cloned() {
            return Ok(existing);
        }

        let resp = self
            .execute(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
                None,
            )
            .await?;

        let session_id = resp
            .get("result")
            .and_then(|r| r.get("sessionId"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CliError::CdpError(format!(
                    "Target.attachToTarget did not return sessionId: {resp}"
                ))
            })?
            .to_string();

        self.tab_sessions
            .lock()
            .await
            .insert(target_id.to_string(), session_id.clone());

        // Enable the Network domain immediately so reader_loop tracks in-flight
        // requests from tab birth, before any user command (including wait network-idle)
        // is invoked.  Idempotent if called again on an already-enabled session.
        let _ = self
            .execute("Network.enable", json!({}), Some(&session_id))
            .await;

        Ok(session_id)
    }

    /// Detach from a CDP target (tab).
    pub async fn detach(&self, target_id: &str) -> Result<(), CliError> {
        let session_id = self
            .tab_sessions
            .lock()
            .await
            .remove(target_id)
            .ok_or_else(|| CliError::CdpError(format!("no session for target '{target_id}'")))?;

        self.execute(
            "Target.detachFromTarget",
            json!({ "sessionId": session_id }),
            None,
        )
        .await?;

        // Clean up the pending counter for this session.
        self.tab_net_pending.lock().await.remove(&session_id);

        Ok(())
    }

    /// Execute a CDP command on a specific tab (by target_id).
    ///
    /// Looks up the CDP sessionId for the target and includes it in the message.
    pub async fn execute_on_tab(
        &self,
        target_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, CliError> {
        let session_id = self
            .tab_sessions
            .lock()
            .await
            .get(target_id)
            .cloned()
            .ok_or_else(|| {
                CliError::CdpError(format!("no CDP session for target '{target_id}'"))
            })?;

        self.execute(method, params, Some(&session_id)).await
    }

    /// Execute a browser-level CDP command (no sessionId).
    pub async fn execute_browser(&self, method: &str, params: Value) -> Result<Value, CliError> {
        self.execute(method, params, None).await
    }

    /// Return the CDP flat-session ID for a target, or `None` if not attached.
    pub async fn get_cdp_session_id(&self, target_id: &str) -> Option<String> {
        self.tab_sessions.lock().await.get(target_id).cloned()
    }

    /// Return the current in-flight Network request count for a tab's CDP session.
    ///
    /// This counter is maintained by `reader_loop` from the moment `attach()` is
    /// called (which enables the Network domain), so it reflects ALL requests since
    /// tab attachment — not just those that started after `wait network-idle` was
    /// invoked.
    pub async fn network_pending(&self, cdp_session_id: &str) -> i64 {
        *self
            .tab_net_pending
            .lock()
            .await
            .get(cdp_session_id)
            .unwrap_or(&0)
    }

    /// Subscribe to a CDP event for a specific flat-session.
    ///
    /// Returns a channel receiver that yields each matching event message.
    /// Subscribe BEFORE enabling the relevant CDP domain to avoid races.
    /// Dead receivers are removed lazily on the next event dispatch.
    pub async fn subscribe_events(
        &self,
        cdp_session_id: &str,
        method: &str,
    ) -> mpsc::Receiver<Value> {
        let key = format!("{cdp_session_id}:{method}");
        let (tx, rx) = mpsc::channel(256);
        self.event_subs
            .lock()
            .await
            .entry(key)
            .or_default()
            .push(tx);
        rx
    }

    /// Low-level: send a CDP command and wait for its response.
    pub async fn execute(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, CliError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let mut msg = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(sid) = session_id {
            msg["sessionId"] = json!(sid);
        }

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        if self.writer_tx.send(msg.to_string()).await.is_err() {
            // Clean up pending entry to avoid leak
            self.pending.lock().await.remove(&id);
            return Err(CliError::CdpError("writer channel closed".to_string()));
        }

        let resp = rx
            .await
            .map_err(|_| CliError::CdpError("response channel dropped".to_string()))??;

        // Surface CDP-level errors (e.g., method not found, target crashed)
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown CDP error");
            return Err(CliError::CdpError(format!("CDP error {code}: {message}")));
        }

        Ok(resp)
    }

    /// Background task: read WS messages and route responses/events to callers.
    async fn reader_loop<S>(
        mut reader: S,
        pending: PendingRequests,
        event_subs: EventSubs,
        tab_net_pending: TabNetPending,
    ) where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        while let Some(raw) = reader.next().await {
            let msg = match raw {
                Ok(Message::Text(t)) => t.to_string(),
                Ok(_) => continue,
                Err(_) => break,
            };

            let resp: Value = match serde_json::from_str(&msg) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(id) = resp.get("id").and_then(|v| v.as_u64()) {
                // Response: route to the pending caller by message ID.
                let mut map = pending.lock().await;
                if let Some(tx) = map.remove(&id) {
                    let _ = tx.send(Ok(resp));
                }
            } else if let Some(method) = resp.get("method").and_then(|v| v.as_str()) {
                // Event: extract sessionId (empty string for browser-level events).
                let session_id = resp.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");

                // Maintain per-tab Network pending counter.
                if !session_id.is_empty() {
                    let delta: i64 = match method {
                        "Network.requestWillBeSent" => 1,
                        "Network.loadingFinished"
                        | "Network.loadingFailed"
                        | "Network.requestServedFromCache" => -1,
                        _ => 0,
                    };
                    if delta != 0 {
                        let mut tp = tab_net_pending.lock().await;
                        let count = tp.entry(session_id.to_string()).or_insert(0);
                        *count = (*count + delta).max(0);
                    }
                }

                // Route to external event subscribers keyed by "{sessionId}:{method}".
                let key = format!("{session_id}:{method}");
                let mut subs = event_subs.lock().await;
                if let Some(txs) = subs.get_mut(&key) {
                    // try_send is non-blocking; retain removes closed receivers lazily.
                    txs.retain(|tx| tx.try_send(resp.clone()).is_ok());
                }
            }
        }

        // Connection dropped — fail all pending requests.
        // Use generic CdpError here; cdp_error_to_result will upgrade to
        // CloudConnectionLost for cloud sessions based on session mode.
        let mut map = pending.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(CliError::CdpError("connection closed".to_string())));
        }
    }

    /// Background task: forward channel messages to WS writer.
    async fn writer_loop<S>(mut writer: S, mut rx: mpsc::Receiver<String>)
    where
        S: SinkExt<Message> + Unpin,
    {
        while let Some(text) = rx.recv().await {
            if writer.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    }
}

// ─── Helper ──────────────────────────────────────────────────────────

/// Extract CdpSession and tab target_id from the registry.
///
/// Returns `ActionResult` errors for SESSION_NOT_FOUND, TAB_NOT_FOUND,
/// or missing CDP connection.
/// Extract CdpSession from the registry and verify the tab exists.
/// Since tab_id IS the native target_id, just return it directly.
pub async fn get_cdp_and_target(
    registry: &crate::daemon::registry::SharedRegistry,
    session_id: &str,
    tab_id: &str,
) -> Result<(CdpSession, String), crate::action_result::ActionResult> {
    let reg = registry.lock().await;
    let entry = reg.get(session_id).ok_or_else(|| {
        crate::action_result::ActionResult::fatal(
            "SESSION_NOT_FOUND",
            format!("session '{session_id}' not found"),
        )
    })?;
    let cdp = entry.cdp.clone().ok_or_else(|| {
        crate::action_result::ActionResult::fatal(
            "INTERNAL_ERROR",
            format!("no CDP connection for session '{session_id}'"),
        )
    })?;
    if !entry.tabs.iter().any(|t| t.id.0 == tab_id) {
        return Err(crate::action_result::ActionResult::fatal(
            "TAB_NOT_FOUND",
            format!("tab '{tab_id}' not found"),
        ));
    }
    // tab_id IS the native target_id
    Ok((cdp, tab_id.to_string()))
}

/// Convert a CliError from CDP operations into an ActionResult.
/// For cloud sessions, connection drops are surfaced as CLOUD_CONNECTION_LOST.
/// For local sessions, they use the default_code.
pub fn cdp_error_to_result(e: CliError, default_code: &str) -> crate::action_result::ActionResult {
    match &e {
        CliError::CloudConnectionLost(_) => crate::action_result::ActionResult::fatal_with_hint(
            "CLOUD_CONNECTION_LOST",
            e.to_string(),
            "cloud connection lost — retry or run `actionbook browser start --mode cloud ...` to reconnect",
        ),
        _ => crate::action_result::ActionResult::fatal(default_code, e.to_string()),
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream::SplitSink;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    /// Start a mock WebSocket server. Returns the URL and a channel that
    /// yields each accepted connection's (reader, writer) pair.
    async fn mock_ws_server() -> (
        String,
        mpsc::Receiver<(
            futures_util::stream::SplitStream<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
            >,
            SplitSink<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, Message>,
        )>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let url = format!("ws://127.0.0.1:{}", addr.port());

        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (writer, reader) = ws.split();
                if tx.send((reader, writer)).await.is_err() {
                    break;
                }
            }
        });

        (url, rx)
    }

    /// Helper: read one JSON message from the mock server's reader.
    async fn read_json<S>(reader: &mut S) -> Value
    where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        loop {
            let msg = reader.next().await.unwrap().unwrap();
            if let Message::Text(t) = msg {
                return serde_json::from_str(t.as_ref()).unwrap();
            }
        }
    }

    /// Helper: send a JSON response from the mock server.
    async fn send_json<S>(writer: &mut S, value: Value)
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        writer
            .send(Message::Text(value.to_string().into()))
            .await
            .unwrap();
    }

    // ── 1. test_message_id_increment ─────────────────────────────────

    #[tokio::test]
    async fn test_message_id_increment() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Send 3 sequential requests, verify IDs are 1, 2, 3
        for expected_id in 1..=3u64 {
            let cdp = cdp.clone();
            let method = format!("Test.method{expected_id}");
            let handle = tokio::spawn(async move { cdp.execute(&method, json!({}), None).await });

            let msg = read_json(&mut reader).await;
            assert_eq!(msg["id"], expected_id, "message id should be {expected_id}");
            assert_eq!(msg["method"], format!("Test.method{expected_id}"));
            assert!(
                msg.get("sessionId").is_none(),
                "no sessionId for browser-level"
            );

            // Reply
            send_json(&mut writer, json!({"id": expected_id, "result": {}})).await;
            handle.await.unwrap().unwrap();
        }
    }

    // ── 2. test_request_response_matching ─────────────────────────────

    #[tokio::test]
    async fn test_request_response_matching() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Send 2 requests concurrently
        let cdp1 = cdp.clone();
        let h1 = tokio::spawn(async move { cdp1.execute("Method.A", json!({}), None).await });
        let cdp2 = cdp.clone();
        let h2 = tokio::spawn(async move { cdp2.execute("Method.B", json!({}), None).await });

        // Read both requests
        let msg1 = read_json(&mut reader).await;
        let msg2 = read_json(&mut reader).await;
        let id1 = msg1["id"].as_u64().unwrap();
        let id2 = msg2["id"].as_u64().unwrap();

        // Reply in REVERSE order (id2 first, then id1)
        send_json(&mut writer, json!({"id": id2, "result": {"value": "B"}})).await;
        send_json(&mut writer, json!({"id": id1, "result": {"value": "A"}})).await;

        // Each caller gets the correct response
        let r1 = h1.await.unwrap().unwrap();
        let r2 = h2.await.unwrap().unwrap();
        assert_eq!(r1["result"]["value"], "A");
        assert_eq!(r2["result"]["value"], "B");
    }

    // ── 3. test_attach_detach ─────────────────────────────────────────

    #[tokio::test]
    async fn test_attach_detach() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Attach
        let cdp_clone = cdp.clone();
        let attach_handle = tokio::spawn(async move { cdp_clone.attach("TARGET_ABC").await });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.attachToTarget");
        assert_eq!(msg["params"]["targetId"], "TARGET_ABC");
        assert_eq!(msg["params"]["flatten"], true);

        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"sessionId": "CDP_SESS_1"}}),
        )
        .await;

        // attach() enables Network domain immediately after storing the session.
        let net_msg = read_json(&mut reader).await;
        assert_eq!(net_msg["method"], "Network.enable");
        assert_eq!(net_msg["sessionId"], "CDP_SESS_1");
        send_json(&mut writer, json!({"id": net_msg["id"], "result": {}})).await;

        let session_id = attach_handle.await.unwrap().unwrap();
        assert_eq!(session_id, "CDP_SESS_1");

        // execute_on_tab should include the sessionId
        let cdp_clone = cdp.clone();
        let exec_handle = tokio::spawn(async move {
            cdp_clone
                .execute_on_tab(
                    "TARGET_ABC",
                    "Runtime.evaluate",
                    json!({"expression": "1+1"}),
                )
                .await
        });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["sessionId"], "CDP_SESS_1");
        assert_eq!(msg["method"], "Runtime.evaluate");
        send_json(
            &mut writer,
            json!({"id": msg["id"], "result": {"result": {"value": 2}}}),
        )
        .await;
        exec_handle.await.unwrap().unwrap();

        // Detach
        let cdp_clone = cdp.clone();
        let detach_handle = tokio::spawn(async move { cdp_clone.detach("TARGET_ABC").await });

        let msg = read_json(&mut reader).await;
        assert_eq!(msg["method"], "Target.detachFromTarget");
        assert_eq!(msg["params"]["sessionId"], "CDP_SESS_1");
        send_json(&mut writer, json!({"id": msg["id"], "result": {}})).await;
        detach_handle.await.unwrap().unwrap();

        // After detach, execute_on_tab should fail
        let result = cdp.execute_on_tab("TARGET_ABC", "Test", json!({})).await;
        assert!(result.is_err());
    }

    // ── 4. test_concurrent_requests ───────────────────────────────────

    #[tokio::test]
    async fn test_concurrent_requests() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (mut reader, mut writer) = conns.recv().await.unwrap();

        // Pre-populate tab_sessions for 3 tabs (skip attach handshake)
        {
            let mut sessions = cdp.tab_sessions.lock().await;
            sessions.insert("T1".to_string(), "S1".to_string());
            sessions.insert("T2".to_string(), "S2".to_string());
            sessions.insert("T3".to_string(), "S3".to_string());
        }

        // Spawn 3 concurrent requests
        let handles: Vec<_> = ["T1", "T2", "T3"]
            .iter()
            .map(|tid| {
                let cdp = cdp.clone();
                let tid = tid.to_string();
                tokio::spawn(async move {
                    cdp.execute_on_tab(&tid, "Runtime.evaluate", json!({"expression": "1"}))
                        .await
                })
            })
            .collect();

        // Read all 3 requests from mock server
        let mut requests = Vec::new();
        for _ in 0..3 {
            requests.push(read_json(&mut reader).await);
        }

        // Verify each has a unique id and correct sessionId
        let ids: Vec<u64> = requests.iter().map(|r| r["id"].as_u64().unwrap()).collect();
        assert_eq!(ids.len(), 3);
        assert!(
            ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2],
            "IDs must be unique"
        );

        let session_ids: Vec<&str> = requests
            .iter()
            .map(|r| r["sessionId"].as_str().unwrap())
            .collect();
        assert!(session_ids.contains(&"S1"));
        assert!(session_ids.contains(&"S2"));
        assert!(session_ids.contains(&"S3"));

        // Reply to all 3
        for req in &requests {
            let id = req["id"].as_u64().unwrap();
            let sid = req["sessionId"].as_str().unwrap();
            send_json(&mut writer, json!({"id": id, "result": {"value": sid}})).await;
        }

        // Verify each handle resolves with correct value
        for handle in handles {
            let resp = handle.await.unwrap().unwrap();
            assert!(resp["result"]["value"].is_string());
        }
    }

    // ── 5. test_connection_drop ───────────────────────────────────────

    #[tokio::test]
    async fn test_connection_drop() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (reader, writer) = conns.recv().await.unwrap();

        // Start a request, then drop the full server-side connection
        let cdp_clone = cdp.clone();
        let handle =
            tokio::spawn(async move { cdp_clone.execute("Test.method", json!({}), None).await });

        // Give a moment for the request to be sent
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Drop both sides → fully closes the WS connection
        drop(reader);
        drop(writer);

        // Caller should get an error, not hang forever
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("should not timeout")
            .unwrap();

        assert!(result.is_err(), "should return error when connection drops");
    }

    // ── 6. test_execute_on_unknown_tab ────────────────────────────────

    #[tokio::test]
    async fn test_execute_on_unknown_tab() {
        let (url, mut _conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();

        // execute_on_tab with non-attached target should fail immediately
        let result = cdp
            .execute_on_tab("UNKNOWN_TARGET", "Runtime.evaluate", json!({}))
            .await;

        assert!(result.is_err(), "should fail for unknown target");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("no CDP session for target"),
            "error should mention missing session, got: {err}"
        );
    }

    // ── 7. test_network_pending_counter ──────────────────────────────

    #[tokio::test]
    async fn test_network_pending_counter() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        // Pre-populate a tab session (simulates attach having stored the session).
        cdp.tab_sessions
            .lock()
            .await
            .insert("T_NET".to_string(), "S_NET".to_string());

        // Initially 0
        assert_eq!(cdp.network_pending("S_NET").await, 0);

        // Simulate Network.requestWillBeSent event (no id field)
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_NET",
                "params": { "requestId": "r1" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 1);

        // Second request
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "S_NET",
                "params": { "requestId": "r2" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 2);

        // First finishes
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_NET",
                "params": { "requestId": "r1" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 1);

        // Second fails
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFailed",
                "sessionId": "S_NET",
                "params": { "requestId": "r2" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 0);

        // Does not go negative on extra terminal events
        send_json(
            &mut writer,
            json!({
                "method": "Network.loadingFinished",
                "sessionId": "S_NET",
                "params": { "requestId": "r_unknown" }
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(cdp.network_pending("S_NET").await, 0);
    }

    // ── 9. test_event_routing ─────────────────────────────────────────

    #[tokio::test]
    async fn test_event_routing() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        // Pre-populate a tab session to simulate attach
        cdp.tab_sessions
            .lock()
            .await
            .insert("TARGET_EV".to_string(), "SESSION_EV".to_string());

        // Subscribe before the event arrives
        let mut rx = cdp
            .subscribe_events("SESSION_EV", "Network.requestWillBeSent")
            .await;

        // Server sends a CDP event (no id field)
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "SESSION_EV",
                "params": { "requestId": "req-42" }
            }),
        )
        .await;

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for event")
            .expect("channel closed");

        assert_eq!(event["method"], "Network.requestWillBeSent");
        assert_eq!(event["params"]["requestId"], "req-42");
    }

    // ── 10. test_event_not_routed_to_wrong_session ────────────────────

    #[tokio::test]
    async fn test_event_not_routed_to_wrong_session() {
        let (url, mut conns) = mock_ws_server().await;
        let cdp = CdpSession::connect(&url).await.unwrap();
        let (_, mut writer) = conns.recv().await.unwrap();

        // Subscribe to SESSION_A events
        let mut rx_a = cdp
            .subscribe_events("SESSION_A", "Network.requestWillBeSent")
            .await;

        // Send event for SESSION_B (different session)
        send_json(
            &mut writer,
            json!({
                "method": "Network.requestWillBeSent",
                "sessionId": "SESSION_B",
                "params": { "requestId": "req-99" }
            }),
        )
        .await;

        // Allow reader loop to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // SESSION_A subscriber must NOT receive SESSION_B event
        assert!(
            rx_a.try_recv().is_err(),
            "should not receive event destined for a different session"
        );
    }
}
