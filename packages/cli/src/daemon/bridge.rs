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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
///
/// Bumped to `0.3.0` when multi-tab concurrent debug landed:
/// - every CDP request now carries a root-level `tabId`
/// - extension uses `attachedTabs: Set<number>` instead of a single attach
/// - older extensions (0.2.x) are rejected and asked to reload — the
///   single-attach protocol cannot be mixed with the multi-attach client.
const PROTOCOL_VERSION: &str = "0.3.0";

/// Known Actionbook Chrome extension IDs.
const EXTENSION_ID_CWS: &str = "bebchpafpemheedhcdabookaifcijmfo";
const EXTENSION_ID_DEV: &str = "dpfioflkmnkklgjldmaggkodhlidkdcd";
const EXTENSION_IDS: &[&str] = &[EXTENSION_ID_CWS, EXTENSION_ID_DEV];

/// Plain HTTP health check served on the same port as the WS bridge so the
/// extension can probe readiness without emitting a failed WebSocket error.
const HEALTH_CHECK_PATH: &str = "/healthz";
const HEALTH_CHECK_RESPONSE: &[u8] = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n\r\n";

// ─── Shared State ───────────────────────────────────────────────────────

/// Observable state of the bridge TCP listener.
///
/// Exposed so callers (e.g. `browser start --mode extension`) can distinguish
/// "still binding, keep waiting" from "bind failed, stop waiting" without
/// having to watch a task JoinHandle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeListenerStatus {
    /// Bind attempt in progress (initial state; stays here during backoff retries).
    Binding,
    /// Listener is bound and accepting connections.
    Listening,
    /// Every retry failed; extension mode is permanently unavailable on this daemon.
    Failed,
}

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
    /// Listener bind state (updated by the background bind task).
    listener_status: BridgeListenerStatus,
}

impl BridgeState {
    fn new() -> Self {
        Self {
            extension_tx: None,
            cdp_tx: None,
            connection_id: 0,
            last_activity: Instant::now(),
            listener_status: BridgeListenerStatus::Binding,
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

    /// Current listener status.
    pub fn listener_status(&self) -> BridgeListenerStatus {
        self.listener_status
    }

    fn set_listener_status(&mut self, status: BridgeListenerStatus) {
        self.listener_status = status;
    }
}

pub type SharedBridgeState = Arc<Mutex<BridgeState>>;

/// Create a new shared bridge state.
pub fn new_bridge_state() -> SharedBridgeState {
    Arc::new(Mutex::new(BridgeState::new()))
}

// ─── Bridge errors ──────────────────────────────────────────────────────

/// Information about a process holding a port we tried to bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortHolder {
    pub pid: u32,
    /// Process name, or `<unknown>` if the process owner is another user
    /// or the OS denied the lookup.
    pub command: String,
}

/// Error returned by [`ensure_bridge`].
#[derive(Debug)]
pub enum BridgeError {
    /// Every retry of `bind_with_retry` failed.
    BindFailed {
        port: u16,
        source: std::io::Error,
        /// Best-effort holder identification (None on lookup failure).
        holder: Option<PortHolder>,
    },
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::BindFailed {
                port,
                source,
                holder,
            } => {
                if let Some(h) = holder {
                    write!(
                        f,
                        "extension bridge failed to bind port {port} (held by {} pid {}): {source}",
                        h.command, h.pid
                    )
                } else {
                    write!(f, "extension bridge failed to bind port {port}: {source}")
                }
            }
        }
    }
}

impl std::error::Error for BridgeError {}

/// Lazily ensure the extension bridge is bound and listening on
/// `BRIDGE_PORT`. Production entry point — see [`ensure_bridge_on_port`]
/// for the underlying logic.
pub async fn ensure_bridge(
    reg: &crate::daemon::registry::SharedRegistry,
) -> Result<SharedBridgeState, BridgeError> {
    ensure_bridge_on_port(reg, BRIDGE_PORT).await
}

/// Same as [`ensure_bridge`] but binds an arbitrary port — exists so unit
/// tests can exercise the idempotency / recovery contract without competing
/// for the global 19222.
///
/// Idempotent: concurrent first-callers are serialized through Registry's
/// `bridge_init_lock`; subsequent callers observe `Listening` and return
/// immediately. A previously `Failed` bridge is retried (allowing recovery
/// after the holding process releases the port).
pub(crate) async fn ensure_bridge_on_port(
    reg: &crate::daemon::registry::SharedRegistry,
    port: u16,
) -> Result<SharedBridgeState, BridgeError> {
    // Fast path: bridge already Listening, skip locking.
    if let Some(bs) = current_listening(reg).await {
        return Ok(bs);
    }

    // Serialize first-start / recovery via the init lock so concurrent
    // callers bind exactly once. Acquired without holding the registry lock
    // so other commands stay responsive.
    let init_lock = reg.lock().await.bridge_init_lock();
    let _guard = init_lock.lock().await;

    // Re-check under the init lock — another caller may have completed bind
    // while we were waiting.
    if let Some(bs) = current_listening(reg).await {
        return Ok(bs);
    }

    let addr = format!("127.0.0.1:{port}");
    let listener = match bind_with_retry(&addr, BIND_RETRY_DELAYS_MS).await {
        Ok(l) => l,
        Err(e) => {
            // Record Failed so the BRIDGE_BIND_FAILED hint path in start.rs
            // can pick up the existing state via `bridge_state()`. Future
            // `ensure_bridge` callers will re-enter this branch and retry
            // (covering "holder eventually releases" recovery).
            let failed = new_bridge_state();
            failed
                .lock()
                .await
                .set_listener_status(BridgeListenerStatus::Failed);
            reg.lock().await.set_bridge_state(failed);
            // netstat2 + ps are blocking syscalls; offload them so the
            // tokio reactor stays responsive (CLAUDE.md: no blocking IO
            // in async context). Cold path — only on retry exhaustion.
            let holder = tokio::task::spawn_blocking(move || diagnose_port_holder(port))
                .await
                .ok()
                .flatten();
            return Err(BridgeError::BindFailed {
                port,
                holder,
                source: e,
            });
        }
    };
    info!("extension bridge listening on ws://{addr}");

    let state = new_bridge_state();
    state
        .lock()
        .await
        .set_listener_status(BridgeListenerStatus::Listening);
    let state_for_task = state.clone();
    tokio::spawn(async move {
        accept_loop(listener, state_for_task).await;
    });

    reg.lock().await.set_bridge_state(state.clone());
    Ok(state)
}

/// Returns the bridge state if it's currently Listening, else None.
async fn current_listening(
    reg: &crate::daemon::registry::SharedRegistry,
) -> Option<SharedBridgeState> {
    let bs = {
        let r = reg.lock().await;
        r.bridge_state().cloned()
    }?;
    if bs.lock().await.listener_status() == BridgeListenerStatus::Listening {
        Some(bs)
    } else {
        None
    }
}

/// Identify the process listening on `port` (best effort).
///
/// Returns `None` when the port is free, or when the lookup fails (no `lsof`
/// available, holder owned by another user, etc).
///
/// Implemented via `lsof -F pc` on Unix (avoids pulling a bindgen-using
/// crate just for an error-hint feature; `lsof` ships by default on macOS
/// and most Linux distros). Windows returns `None` for now — the BRIDGE_BIND_FAILED
/// hint there will fall back to "run lsof... or netstat".
pub fn diagnose_port_holder(port: u16) -> Option<PortHolder> {
    diagnose_port_holder_impl(port)
}

#[cfg(unix)]
fn diagnose_port_holder_impl(port: u16) -> Option<PortHolder> {
    // -F pc → field-formatted output: "p<pid>\nc<command>" per file descriptor
    // -P → numeric ports (skip /etc/services lookup)
    // -n → numeric host (skip DNS)
    let out = std::process::Command::new("lsof")
        .args([
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-P",
            "-n",
            "-F",
            "pc",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut pid: Option<u32> = None;
    let mut command: Option<String> = None;
    // `lsof -F pc` groups records by process: the line starting with 'p' begins
    // a new record, then 'c' carries the command name. Multiple FD records may
    // follow; we only need the first matched (pid, command) pair.
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            pid = rest.trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix('c') {
            command = Some(rest.trim().to_string());
        }
        if pid.is_some() && command.is_some() {
            break;
        }
    }
    Some(PortHolder {
        pid: pid?,
        command: command.filter(|c| !c.is_empty())?,
    })
}

#[cfg(windows)]
fn diagnose_port_holder_impl(_port: u16) -> Option<PortHolder> {
    // Windows port-holder lookup needs GetExtendedTcpTable + GetModuleBaseName.
    // Out of scope for this PR; the hint falls back to "run netstat -ano".
    None
}

// ─── Internal binder ────────────────────────────────────────────────────

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

async fn handle_connection(mut stream: TcpStream, state: SharedBridgeState) {
    match maybe_serve_health_check(&mut stream).await {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            warn!("bridge: failed to serve health check: {e}");
            return;
        }
    }

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

fn is_health_check_request(buf: &[u8]) -> bool {
    buf.starts_with(format!("GET {HEALTH_CHECK_PATH} ").as_bytes())
        || buf.starts_with(format!("HEAD {HEALTH_CHECK_PATH} ").as_bytes())
}

async fn maybe_serve_health_check(stream: &mut TcpStream) -> std::io::Result<bool> {
    let mut buf = [0_u8; 256];
    let n = stream.peek(&mut buf).await?;
    if n == 0 || !is_health_check_request(&buf[..n]) {
        return Ok(false);
    }

    let mut consumed = Vec::with_capacity(n);
    while !consumed.windows(4).any(|w| w == b"\r\n\r\n") && consumed.len() < 8 * 1024 {
        let mut chunk = [0_u8; 512];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        consumed.extend_from_slice(&chunk[..read]);
    }

    stream.write_all(HEALTH_CHECK_RESPONSE).await?;
    let _ = stream.shutdown().await;
    Ok(true)
}

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

/// Check protocol version >= 0.3.0 (simple major.minor comparison).
fn is_version_ok(version: &str) -> bool {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() < 2 {
        return false;
    }
    // 0.3.0 minimum: major > 0, or major == 0 && minor >= 3
    parts[0] > 0 || (parts[0] == 0 && parts[1] >= 3)
}

// ─── Unit Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

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
        assert!(is_version_ok("0.3.0"));
        assert!(is_version_ok("0.3.5"));
        assert!(is_version_ok("1.0.0"));
        assert!(!is_version_ok("0.2.0"));
        assert!(!is_version_ok("0.2.9"));
        assert!(!is_version_ok("0.1.0"));
        assert!(!is_version_ok("0.0.1"));
        assert!(!is_version_ok("invalid"));
    }

    #[test]
    fn test_is_health_check_request() {
        assert!(is_health_check_request(b"GET /healthz HTTP/1.1\r\n"));
        assert!(is_health_check_request(b"HEAD /healthz HTTP/1.1\r\n"));
        assert!(!is_health_check_request(b"GET / HTTP/1.1\r\n"));
        assert!(!is_health_check_request(b"GET /health HTTP/1.1\r\n"));
    }

    #[test]
    fn test_bridge_state_extension_not_connected_by_default() {
        let state = BridgeState::new();
        assert!(!state.is_extension_connected());
    }

    #[test]
    fn test_bridge_state_listener_starts_in_binding() {
        // start --mode extension must treat a fresh state as "keep waiting",
        // not "fail fast" — the async bind task has not run yet.
        let state = BridgeState::new();
        assert_eq!(state.listener_status(), BridgeListenerStatus::Binding);
    }

    #[test]
    fn test_bridge_state_listener_status_transitions() {
        let mut state = BridgeState::new();
        state.set_listener_status(BridgeListenerStatus::Listening);
        assert_eq!(state.listener_status(), BridgeListenerStatus::Listening);
        state.set_listener_status(BridgeListenerStatus::Failed);
        assert_eq!(state.listener_status(), BridgeListenerStatus::Failed);
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

    // ─── ensure_bridge contract (Phase 3 lazy + recovery) ───────────────

    use crate::daemon::registry::{SharedRegistry, new_shared_registry};

    /// Pick a likely-free port (kernel-assigned, then released). Tests that
    /// exercise `ensure_bridge_on_port` use this so they don't fight with the
    /// real daemon on the global 19222.
    async fn ephemeral_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        port
    }

    /// Concurrent first-callers must observe the same `SharedBridgeState` —
    /// `ensure_bridge` binds at most once per daemon, no matter how many
    /// callers race in. The fix relies on `Registry::bridge_init_lock`.
    #[tokio::test]
    async fn ensure_idempotent_under_contention() {
        let reg: SharedRegistry = new_shared_registry();
        let port = ephemeral_port().await;
        let mut handles = Vec::new();
        for _ in 0..10 {
            let r = reg.clone();
            handles.push(tokio::spawn(async move {
                ensure_bridge_on_port(&r, port).await
            }));
        }
        let mut firsts: Vec<*const Mutex<BridgeState>> = Vec::new();
        for h in handles {
            let bs = h.await.expect("task panicked").expect("ensure_bridge ok");
            firsts.push(Arc::as_ptr(&bs));
        }
        let unique: std::collections::HashSet<_> = firsts.iter().collect();
        assert_eq!(
            unique.len(),
            1,
            "all concurrent callers must share one bridge state, got {} distinct",
            unique.len()
        );
    }

    /// When the bridge is already `Listening`, `ensure_bridge` must take the
    /// fast path and return the existing state — no second `bind_with_retry`.
    #[tokio::test]
    async fn ensure_skip_when_already_listening() {
        let reg: SharedRegistry = new_shared_registry();
        let port = ephemeral_port().await;
        // First call: binds.
        let first = ensure_bridge_on_port(&reg, port)
            .await
            .expect("first bind ok");
        // Mutate state externally to prove the second call returns the same Arc
        // rather than creating a fresh one.
        first.lock().await.connection_id = 999;
        let second = ensure_bridge_on_port(&reg, port)
            .await
            .expect("second call ok");
        assert!(
            Arc::ptr_eq(&first, &second),
            "second ensure must reuse listening bridge state"
        );
        assert_eq!(
            second.lock().await.connection_id,
            999,
            "marker preserved → same instance"
        );
    }

    /// A bridge previously left in `Failed` (port-was-busy at first call) must
    /// be recoverable: a later `ensure_bridge` re-enters the bind ladder and
    /// transitions to `Listening`. This is the behavior PR #517 still lacks.
    #[tokio::test]
    async fn ensure_recovers_from_failed() {
        let reg: SharedRegistry = new_shared_registry();
        let port = ephemeral_port().await;
        // Seed Failed manually (the production path that produces it is the
        // bind-retry-exhausted branch; we shortcut for unit-test brevity).
        {
            let stub = new_bridge_state();
            stub.lock()
                .await
                .set_listener_status(BridgeListenerStatus::Failed);
            reg.lock().await.set_bridge_state(stub);
        }
        let recovered = ensure_bridge_on_port(&reg, port)
            .await
            .expect("should recover");
        assert_eq!(
            recovered.lock().await.listener_status(),
            BridgeListenerStatus::Listening,
            "after recovery, status must be Listening"
        );
    }

    /// Unix-only: the Windows implementation of `diagnose_port_holder` is
    /// currently a `None` stub (tracked as a follow-up), so this test only
    /// runs on Unix where `lsof -F pc` provides real pid + command data.
    #[cfg(unix)]
    #[tokio::test]
    async fn diagnose_port_holder_returns_pid_for_occupied() {
        // Bind a real listener so the port has a known holder = this process.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let holder = diagnose_port_holder(port).expect("should find holder");
        assert_eq!(
            holder.pid,
            std::process::id(),
            "holder pid must match current test process"
        );
        assert!(
            !holder.command.is_empty(),
            "holder command must be populated (got empty)"
        );
        // Defensive: the placeholder for unknown owners is "<unknown>"; ensure
        // we got a real name when the port is owned by us.
        assert_ne!(
            holder.command, "<unknown>",
            "lookup should resolve current process command"
        );
    }

    #[tokio::test]
    async fn diagnose_port_holder_returns_none_for_free_port() {
        // Take a free port number from the kernel, then release it. The port is
        // very likely still free at the moment of the call (test is racy in
        // principle, but rare in practice because the kernel won't reissue this
        // port within the same process for a while).
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // Give the kernel a moment to fully release.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            diagnose_port_holder(port).is_none(),
            "free port must return None"
        );
    }

    async fn spawn_single_connection_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = new_bridge_state();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, state).await;
        });
        format!("127.0.0.1:{}", addr.port())
    }

    #[tokio::test]
    async fn handle_connection_serves_health_check() {
        let addr = spawn_single_connection_server().await;
        let mut stream = TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 204 No Content"));
        assert!(response.contains("Access-Control-Allow-Origin: *"));
    }

    #[tokio::test]
    async fn handle_connection_keeps_websocket_handshake_working() {
        let addr = spawn_single_connection_server().await;
        let url = format!("ws://{addr}");
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert(
            "Origin",
            HeaderValue::from_static("chrome-extension://bebchpafpemheedhcdabookaifcijmfo"),
        );

        let (mut ws, _) = connect_async(request).await.unwrap();
        ws.send(Message::Text(
            json!({
                "type": "hello",
                "role": "extension",
                "version": "0.3.0"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        let text = match msg {
            Message::Text(text) => text.to_string(),
            other => panic!("expected text hello_ack, got {other:?}"),
        };
        let ack: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(ack["type"], "hello_ack");
    }
}
