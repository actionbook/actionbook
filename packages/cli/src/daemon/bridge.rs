//! Extension bridge: WS relay between Chrome extension and daemon CdpSession.
//!
//! The bridge runs as a tokio task inside the daemon, listening on a fixed TCP
//! port. Two types of clients connect:
//!
//! 1. **Extension** — Chrome extension connects with a hello handshake. Origin
//!    is validated against known extension IDs. One extension connection at a time.
//!
//! 2. **CDP client** (daemon CdpSession) — connects for transparent CDP relay.
//!    First message is inspected: if it contains `"type":"hello"` it's an
//!    extension; otherwise it's treated as a CDP client and all messages are
//!    relayed bidirectionally to the extension.
//!
//! The bridge is spawned from `run_daemon()`. Binding the fixed port is
//! attempted with bounded exponential backoff so transient contention
//! (old daemon still releasing the socket, rapid restart, brief third-party
//! use of 19222) does not permanently break extension mode. If every attempt
//! fails the daemon still starts — only extension mode is unavailable.

use std::sync::Arc;
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::StatusCode;
use tracing::{error, info, warn};

// ─── Constants ──────────────────────────────────────────────────────────

/// Default bridge port. Must match the extension's hardcoded `ws://127.0.0.1:19222`.
pub const BRIDGE_PORT: u16 = 19222;

/// Delays (in ms) between retry attempts when binding the bridge port fails.
/// 6 total attempts (1 immediate + 5 retries), max wait ≈ 8.6s.
/// Sized to comfortably cover: kernel socket cleanup after a previous daemon
/// exit, rapid daemon restart races, and brief third-party port use.
const BIND_RETRY_DELAYS_MS: &[u64] = &[100, 500, 1_000, 2_000, 5_000];

/// Protocol version for the hello handshake.
const PROTOCOL_VERSION: &str = "0.2.0";

/// Known Actionbook Chrome extension IDs.
const EXTENSION_ID_CWS: &str = "bebchpafpemheedhcdabookaifcijmfo";
const EXTENSION_ID_DEV: &str = "dpfioflkmnkklgjldmaggkodhlidkdcd";
const EXTENSION_IDS: &[&str] = &[EXTENSION_ID_CWS, EXTENSION_ID_DEV];

// ─── Shared State ───────────────────────────────────────────────────────

/// Bridge state shared across connections.
pub struct BridgeState {
    /// Send commands TO the extension WebSocket.
    extension_tx: Option<mpsc::UnboundedSender<String>>,
    /// Send messages TO the CDP client (daemon CdpSession) WebSocket.
    cdp_tx: Option<mpsc::UnboundedSender<String>>,
    /// Monotonically increasing connection id to distinguish extension connections.
    connection_id: u64,
    /// Last activity timestamp.
    last_activity: Instant,
}

impl BridgeState {
    fn new() -> Self {
        Self {
            extension_tx: None,
            cdp_tx: None,
            connection_id: 0,
            last_activity: Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Whether an extension is currently connected (channel is open).
    pub fn is_extension_connected(&self) -> bool {
        self.extension_tx
            .as_ref()
            .map(|tx| !tx.is_closed())
            .unwrap_or(false)
    }
}

pub type SharedBridgeState = Arc<Mutex<BridgeState>>;

/// Create a new shared bridge state.
pub fn new_bridge_state() -> SharedBridgeState {
    Arc::new(Mutex::new(BridgeState::new()))
}

// ─── Public API ─────────────────────────────────────────────────────────

/// Spawn the bridge server as a background tokio task.
///
/// Returns the bridge state handle on success. Returns `None` if every bind
/// attempt fails (daemon still starts, only extension mode is unavailable).
pub async fn spawn_bridge() -> Option<SharedBridgeState> {
    let addr = format!("127.0.0.1:{BRIDGE_PORT}");
    let listener = match bind_with_retry(&addr, BIND_RETRY_DELAYS_MS).await {
        Ok(l) => {
            info!("extension bridge listening on ws://{addr}");
            l
        }
        Err(e) => {
            warn!(
                "extension bridge: failed to bind {addr} after {} attempts: {e} — extension mode unavailable",
                BIND_RETRY_DELAYS_MS.len() + 1
            );
            return None;
        }
    };

    let state = new_bridge_state();
    let state_clone = state.clone();

    tokio::spawn(async move {
        accept_loop(listener, state_clone).await;
    });

    Some(state)
}

/// Bind `addr` with bounded retry. First attempt is immediate; on failure,
/// waits `delays_ms[i]` then retries, for a total of `delays_ms.len() + 1`
/// attempts. Returns the last error if every attempt fails.
async fn bind_with_retry(addr: &str, delays_ms: &[u64]) -> std::io::Result<TcpListener> {
    let mut last_err = match TcpListener::bind(addr).await {
        Ok(l) => return Ok(l),
        Err(e) => e,
    };
    let total = delays_ms.len() + 1;
    for (i, &delay_ms) in delays_ms.iter().enumerate() {
        info!(
            "extension bridge: bind {addr} attempt {}/{total} failed ({last_err}) — retrying in {delay_ms}ms",
            i + 1
        );
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        match TcpListener::bind(addr).await {
            Ok(l) => {
                info!(
                    "extension bridge: bound {addr} on attempt {}/{total}",
                    i + 2
                );
                return Ok(l);
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

// ─── Accept Loop ────────────────────────────────────────────────────────

async fn accept_loop(listener: TcpListener, state: SharedBridgeState) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let peer_ip = peer.ip();
                if !peer_ip.is_loopback() {
                    warn!("bridge: rejected non-loopback connection from {peer}");
                    continue;
                }
                let state = state.clone();
                tokio::spawn(async move {
                    handle_connection(stream, state).await;
                });
            }
            Err(e) => {
                error!("bridge: accept error: {e}");
            }
        }
    }
}

// ─── Connection Handler ─────────────────────────────────────────────────

async fn handle_connection(stream: TcpStream, state: SharedBridgeState) {
    // Capture origin during WS upgrade for extension ID validation.
    let captured_origin: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));
    let origin_capture = Arc::clone(&captured_origin);

    let ws = match tokio_tungstenite::accept_hdr_async(
        stream,
        #[allow(clippy::result_large_err)] // accept_hdr_async requires this exact signature
        move |req: &tokio_tungstenite::tungstenite::http::Request<()>,
              resp: tokio_tungstenite::tungstenite::http::Response<()>|
              -> std::result::Result<
            tokio_tungstenite::tungstenite::http::Response<()>,
            tokio_tungstenite::tungstenite::http::Response<Option<String>>,
        > {
            let origin = req
                .headers()
                .get("origin")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_lowercase());

            if !is_origin_allowed(origin.as_deref()) {
                let rejection = tokio_tungstenite::tungstenite::http::Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Some("Forbidden origin".to_string()))
                    .unwrap();
                return Err(rejection);
            }

            *origin_capture.lock().unwrap() = origin;
            Ok(resp)
        },
    )
    .await
    {
        Ok(ws) => ws,
        Err(_) => return, // TCP probe or failed handshake
    };

    let connection_origin = captured_origin.lock().unwrap().take();
    let (write, mut read) = ws.split();

    // Read first message to determine client role.
    let first_msg = match tokio::time::timeout(std::time::Duration::from_secs(5), read.next()).await
    {
        Ok(Some(Ok(Message::Text(text)))) => text.to_string(),
        _ => return,
    };

    let parsed: serde_json::Value = match serde_json::from_str(&first_msg) {
        Ok(v) => v,
        Err(_) => return,
    };

    let msg_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");

    if msg_type == "hello" {
        handle_extension(write, read, parsed, connection_origin, state).await;
    } else {
        // Not a hello → assume CDP client (daemon CdpSession).
        handle_cdp_client(write, read, first_msg, state).await;
    }
}

// ─── Extension Handler ──────────────────────────────────────────────────

async fn handle_extension(
    mut write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    mut read: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    hello: serde_json::Value,
    origin: Option<String>,
    state: SharedBridgeState,
) {
    let client_version = hello
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");

    // Validate protocol version (>= 0.2.0).
    if !is_version_ok(client_version) {
        let err = json!({
            "type": "hello_error",
            "error": "version_mismatch",
            "message": format!("Minimum required: {PROTOCOL_VERSION}"),
            "required_version": PROTOCOL_VERSION,
        });
        let _ = write.send(Message::Text(err.to_string().into())).await;
        return;
    }

    // Validate extension origin.
    let origin_ok = EXTENSION_IDS.iter().any(|id| {
        let expected = format!("chrome-extension://{id}");
        origin
            .as_deref()
            .map(|o| o.eq_ignore_ascii_case(&expected))
            .unwrap_or(false)
    });
    if !origin_ok {
        let err = json!({
            "type": "hello_error",
            "error": "invalid_origin",
            "message": "Extension origin does not match any known Actionbook extension ID.",
        });
        let _ = write.send(Message::Text(err.to_string().into())).await;
        return;
    }

    // Reject if another extension is already connected.
    {
        let s = state.lock().await;
        if s.is_extension_connected() {
            drop(s);
            let err = json!({
                "type": "replaced",
                "message": "Another extension instance is already connected.",
            });
            let _ = write.send(Message::Text(err.to_string().into())).await;
            return;
        }
    }

    // Send hello_ack.
    let ack = json!({ "type": "hello_ack", "version": PROTOCOL_VERSION });
    if write
        .send(Message::Text(ack.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    info!("bridge: extension connected");

    // Create channel for sending commands TO this extension WS.
    let (ext_tx, mut ext_rx) = mpsc::unbounded_channel::<String>();

    let my_conn_id = {
        let mut s = state.lock().await;
        s.connection_id += 1;
        s.extension_tx = Some(ext_tx);
        s.touch();
        s.connection_id
    };

    // Writer task: channel → extension WS.
    let write = Arc::new(Mutex::new(write));
    let write_clone = write.clone();
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = ext_rx.recv().await {
            let mut w = write_clone.lock().await;
            if w.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Reader: extension WS → forward to CDP client (if connected).
    while let Some(frame) = read.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                let text_str = text.to_string();
                let mut s = state.lock().await;
                s.touch();
                if let Some(ref cdp_tx) = s.cdp_tx
                    && cdp_tx.send(text_str).is_err()
                {
                    warn!("bridge: failed to forward extension message to CDP client");
                }
                // If no CDP client, message is dropped (events before session start).
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    info!("bridge: extension disconnected");

    // Cleanup: only clear if we own the current connection.
    {
        let mut s = state.lock().await;
        if s.connection_id == my_conn_id {
            s.extension_tx = None;
        }
    }

    write_handle.abort();
}

// ─── CDP Client Handler (daemon CdpSession) ─────────────────────────────

async fn handle_cdp_client(
    write: futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>,
    mut read: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    first_message: String,
    state: SharedBridgeState,
) {
    // Reject if another CDP client is already connected. The bridge is a 1:1
    // relay — allowing a second client would silently steal extension responses
    // from the first session, causing it to stall/timeout.
    {
        let s = state.lock().await;
        if s.cdp_tx.as_ref().is_some_and(|tx| !tx.is_closed()) {
            warn!("bridge: rejected CDP client — another session is already connected");
            return;
        }
    }

    // Create channel for sending messages TO this CDP client WS.
    let (cdp_tx, mut cdp_rx) = mpsc::unbounded_channel::<String>();

    {
        let mut s = state.lock().await;
        s.cdp_tx = Some(cdp_tx);
        s.touch();
    }

    // Forward the first CDP message (already read) to extension.
    {
        let mut s = state.lock().await;
        s.touch();
        if let Some(ref ext_tx) = s.extension_tx
            && ext_tx.send(first_message).is_err()
        {
            warn!("bridge: failed to forward first CDP message to extension");
        }
    }

    // Writer task: channel → CDP client WS.
    let write = Arc::new(Mutex::new(write));
    let write_clone = write.clone();
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = cdp_rx.recv().await {
            let mut w = write_clone.lock().await;
            if w.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Reader: CDP client WS → forward to extension.
    while let Some(frame) = read.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                let text_str = text.to_string();
                let mut s = state.lock().await;
                s.touch();
                if let Some(ref ext_tx) = s.extension_tx
                    && ext_tx.send(text_str).is_err()
                {
                    warn!("bridge: failed to forward CDP message to extension");
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Cleanup CDP client channel.
    {
        let mut s = state.lock().await;
        s.cdp_tx = None;
    }

    write_handle.abort();
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Validate WS origin: allow chrome-extension:// and loopback HTTP.
fn is_origin_allowed(origin: Option<&str>) -> bool {
    let Some(o) = origin else { return true };
    let lower = o.to_lowercase();
    if lower.starts_with("chrome-extension://") {
        return true;
    }
    if lower.starts_with("http://") {
        let host = lower
            .strip_prefix("http://")
            .unwrap_or("")
            .trim_end_matches('/');
        let host_no_port = host.split(':').next().unwrap_or("");
        return matches!(host_no_port, "127.0.0.1" | "localhost" | "[::1]");
    }
    false
}

/// Check protocol version >= 0.2.0 (simple major.minor comparison).
fn is_version_ok(version: &str) -> bool {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() < 2 {
        return false;
    }
    // 0.2.0 minimum: major > 0, or major == 0 && minor >= 2
    parts[0] > 0 || (parts[0] == 0 && parts[1] >= 2)
}

// ─── Unit Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_origin_allowed() {
        assert!(is_origin_allowed(None));
        assert!(is_origin_allowed(Some(
            "chrome-extension://bebchpafpemheedhcdabookaifcijmfo"
        )));
        assert!(is_origin_allowed(Some("http://127.0.0.1")));
        assert!(is_origin_allowed(Some("http://localhost")));
        assert!(is_origin_allowed(Some("http://127.0.0.1:3000")));
        assert!(!is_origin_allowed(Some("https://evil.com")));
        assert!(!is_origin_allowed(Some("http://192.168.1.1")));
    }

    #[test]
    fn test_is_version_ok() {
        assert!(is_version_ok("0.2.0"));
        assert!(is_version_ok("0.3.0"));
        assert!(is_version_ok("1.0.0"));
        assert!(!is_version_ok("0.1.0"));
        assert!(!is_version_ok("0.0.1"));
        assert!(!is_version_ok("invalid"));
    }

    #[test]
    fn test_bridge_state_extension_not_connected_by_default() {
        let state = BridgeState::new();
        assert!(!state.is_extension_connected());
    }

    // ─── bind_with_retry ────────────────────────────────────────────────

    /// Grab a free port, drop the listener, and return the address string.
    /// The port *may* be racy if another process grabs it between drop and
    /// the caller's bind — for CI we accept the vanishingly small risk.
    async fn ephemeral_addr() -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        drop(l);
        format!("{addr}")
    }

    #[tokio::test]
    async fn bind_with_retry_succeeds_immediately_when_port_is_free() {
        let addr = ephemeral_addr().await;
        let listener = bind_with_retry(&addr, &[50, 100]).await.expect("bind");
        assert_eq!(listener.local_addr().unwrap().to_string(), addr);
    }

    #[tokio::test]
    async fn bind_with_retry_recovers_when_port_releases_during_backoff() {
        // Occupy a port, schedule release after first retry window.
        let blocker = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = blocker.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
            drop(blocker); // frees port before 2nd attempt at t=100ms
        });

        let started = std::time::Instant::now();
        let listener = bind_with_retry(&addr, &[100, 500, 1_000])
            .await
            .expect("retry should succeed after port release");
        // 2nd attempt fires at ~100ms; allow some slack for scheduler jitter.
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(90),
            "should have waited for at least one retry"
        );
        assert_eq!(listener.local_addr().unwrap().to_string(), addr);
    }

    #[tokio::test]
    async fn bind_with_retry_gives_up_when_port_stays_busy() {
        // Hold the port for the entire retry window.
        let _blocker = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = _blocker.local_addr().unwrap().to_string();

        let err = bind_with_retry(&addr, &[20, 30, 40])
            .await
            .expect_err("should fail while port is held");
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    }
}
