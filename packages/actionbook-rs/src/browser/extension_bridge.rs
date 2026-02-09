use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures::{SinkExt, StreamExt};
use rand::Rng;
use subtle::ConstantTimeEq;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{ActionbookError, Result};

/// CDP method risk levels for the command allowlist.
/// L1 = read-only, L2 = page modification, L3 = high risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    L1,
    L2,
    L3,
}

impl RiskLevel {
    fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::L1 => "L1",
            RiskLevel::L2 => "L2",
            RiskLevel::L3 => "L3",
        }
    }
}

/// Look up the risk level for a CDP method.
/// Returns None if the method is not in the allowlist.
pub fn get_risk_level(method: &str) -> Option<RiskLevel> {
    match method {
        // L1 - Read only
        "Page.captureScreenshot"
        | "DOM.getDocument"
        | "DOM.querySelector"
        | "DOM.querySelectorAll"
        | "DOM.getOuterHTML"
        | "Network.getCookies" => Some(RiskLevel::L1),

        // L2 - Page modification (includes Runtime.evaluate which executes arbitrary JS)
        "Runtime.evaluate"
        | "Page.navigate"
        | "Page.reload"
        | "Input.dispatchMouseEvent"
        | "Input.dispatchKeyEvent"
        | "Emulation.setDeviceMetricsOverride"
        | "Page.printToPDF" => Some(RiskLevel::L2),

        // L3 - High risk
        "Network.setCookie"
        | "Network.deleteCookies"
        | "Network.clearBrowserCookies"
        | "Page.setDownloadBehavior"
        | "Storage.clearDataForOrigin" => Some(RiskLevel::L3),

        // Extension-internal methods (always allowed, L1)
        _ if method.starts_with("Extension.") => Some(RiskLevel::L1),

        // Unknown method - not allowed
        _ => None,
    }
}

/// Token prefix for all bridge session tokens.
const TOKEN_PREFIX: &str = "abk_";

/// Token idle timeout in seconds (30 minutes).
const TOKEN_TTL_SECS: u64 = 30 * 60;

/// Minimum protocol version we accept in hello handshake.
const PROTOCOL_VERSION: &str = "0.2.0";

/// Generate a new session token: `abk_` + 32 random hex characters.
pub fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    format!("{}{}", TOKEN_PREFIX, hex)
}

/// Path to the bridge token file: `~/.local/share/actionbook/bridge-token`
pub fn token_file_path() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir().ok_or_else(|| {
        ActionbookError::Other("Cannot determine local data directory".to_string())
    })?;
    Ok(data_dir.join("actionbook").join("bridge-token"))
}

/// Write the session token to disk with mode 0600.
/// Uses atomic write pattern: write to temp file with restricted permissions, then rename.
pub async fn write_token_file(token: &str) -> Result<()> {
    let path = token_file_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    #[cfg(unix)]
    {
        // Create temp file with 0600 permissions atomically (uses tokio's OpenOptionsExt)
        let tmp_path = path.with_extension("tmp");
        let mut opts = tokio::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);
        let mut file = opts.open(&tmp_path).await?;
        tokio::io::AsyncWriteExt::write_all(&mut file, token.as_bytes()).await?;
        tokio::io::AsyncWriteExt::flush(&mut file).await?;
        drop(file);
        // Atomic rename
        tokio::fs::rename(&tmp_path, &path).await?;
    }

    #[cfg(not(unix))]
    {
        tokio::fs::write(&path, token).await?;
    }

    Ok(())
}

/// Delete the token file if it exists.
pub async fn delete_token_file() {
    if let Ok(path) = token_file_path() {
        let _ = tokio::fs::remove_file(&path).await;
    }
}

/// Path to the bridge port file: `~/.local/share/actionbook/bridge-port`
pub fn port_file_path() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir().ok_or_else(|| {
        ActionbookError::Other("Cannot determine local data directory".to_string())
    })?;
    Ok(data_dir.join("actionbook").join("bridge-port"))
}

/// Write the bridge port to disk so native messaging and other tools can discover it.
pub async fn write_port_file(port: u16) -> Result<()> {
    let path = port_file_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, port.to_string()).await?;
    Ok(())
}

/// Read the bridge port from file. Returns None if file doesn't exist or is invalid.
pub async fn read_port_file() -> Option<u16> {
    let path = port_file_path().ok()?;
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    content.trim().parse().ok()
}

/// Delete the port file if it exists.
pub async fn delete_port_file() {
    if let Ok(path) = port_file_path() {
        let _ = tokio::fs::remove_file(&path).await;
    }
}

/// Read the token from the token file. Returns None if file doesn't exist.
pub async fn read_token_file() -> Option<String> {
    let path = token_file_path().ok()?;
    tokio::fs::read_to_string(&path).await.ok().map(|s| s.trim().to_string())
}

/// Shared state for the bridge server
struct BridgeState {
    /// Session token that clients must present in the hello handshake
    token: String,
    /// Channel to send commands to the connected extension
    extension_tx: Option<mpsc::UnboundedSender<String>>,
    /// Pending CLI requests waiting for extension responses, keyed by request id
    pending: HashMap<u64, oneshot::Sender<String>>,
    /// Monotonically increasing request id counter
    next_id: u64,
    /// Last activity timestamp (any message from any client resets this)
    last_activity: Instant,
}

impl BridgeState {
    fn new(token: String) -> Self {
        Self {
            token,
            extension_tx: None,
            pending: HashMap::new(),
            next_id: 1,
            last_activity: Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
}

/// Start the bridge WebSocket server on the given port with the given session token.
/// This function blocks until the server is shut down.
pub async fn serve(port: u16, token: String) -> Result<()> {
    // Clean up any stale files from a previous ungraceful shutdown before starting
    delete_port_file().await;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        ActionbookError::Other(format!("Failed to bind to {}: {}", addr, e))
    })?;

    let state = Arc::new(Mutex::new(BridgeState::new(token)));

    println!("Bridge server listening on ws://127.0.0.1:{}", port);
    println!("Waiting for extension connection...");

    // Write port file so native messaging can discover the actual port
    if let Err(e) = write_port_file(port).await {
        tracing::warn!("Failed to write port file: {}. Native messaging auto-pairing may not work.", e);
        eprintln!(
            "  Warning: Failed to write port file: {}. Auto-pairing may not work.",
            e
        );
    }

    // Spawn TTL watchdog
    let ttl_state = Arc::clone(&state);
    let ttl_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let mut s = ttl_state.lock().await;
            if s.last_activity.elapsed().as_secs() >= TOKEN_TTL_SECS {
                tracing::warn!("Token idle timeout reached ({}min). Generating new token.", TOKEN_TTL_SECS / 60);
                let new_token = generate_token();
                // Send token_expired notification before closing
                if let Some(ext_tx) = s.extension_tx.take() {
                    let expire_msg = serde_json::json!({
                        "type": "token_expired",
                        "message": "Session token expired due to inactivity"
                    });
                    let _ = ext_tx.send(expire_msg.to_string());
                    drop(ext_tx);
                }
                // Notify all pending CLI requests with their original IDs
                for (id, sender) in s.pending.drain() {
                    let err_msg = serde_json::json!({
                        "id": id,
                        "error": { "code": -32000, "message": "Session token expired" }
                    });
                    let _ = sender.send(err_msg.to_string());
                }
                println!(
                    "\n  {} Token expired due to inactivity. New token: {}\n",
                    colored::Colorize::yellow("!"),
                    new_token
                );
                // Write new token file
                let _ = write_token_file(&new_token).await;
                s.token = new_token;
                s.last_activity = Instant::now();
            }
        }
    });

    // Handle SIGINT/SIGTERM for graceful shutdown with file cleanup
    let shutdown = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
            tokio::select! {
                _ = sigint.recv() => tracing::info!("Received SIGINT"),
                _ = sigterm.recv() => tracing::info!("Received SIGTERM"),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
        }
    };

    let accept_loop = async {
        loop {
            let (stream, peer) = listener.accept().await.map_err(|e| {
                ActionbookError::Other(format!("Accept failed: {}", e))
            })?;

            tracing::debug!("New connection from {}", peer);

            // Validate origin at TCP level before upgrading to WebSocket.
            // Only accept connections from loopback addresses.
            let peer_ip = peer.ip();
            if !peer_ip.is_loopback() {
                tracing::warn!("Rejected non-loopback connection from {}", peer);
                drop(stream);
                continue;
            }

            let state = Arc::clone(&state);
            tokio::spawn(handle_connection(stream, state));
        }
    };

    let result: Result<()> = tokio::select! {
        r = accept_loop => r,
        _ = shutdown => {
            tracing::info!("Shutting down bridge server...");
            Ok(())
        }
    };

    // Cleanup always runs, whether shutdown was graceful or the loop exited
    delete_port_file().await;
    ttl_handle.abort();
    result
}

/// Parse an origin string into (scheme, host, optional_port).
fn parse_origin(origin: &str) -> Option<(&str, &str, Option<&str>)> {
    let (scheme, rest) = origin.split_once("://")?;
    if rest.is_empty() {
        return None;
    }
    // Handle IPv6 bracket notation e.g. [::1]:8080
    if rest.starts_with('[') {
        let end_bracket = rest.find(']')?;
        let host = &rest[..end_bracket + 1];
        let after = &rest[end_bracket + 1..];
        if after.is_empty() || after == "/" {
            Some((scheme, host, None))
        } else if let Some(port_part) = after.strip_prefix(':') {
            let port_str = port_part.trim_end_matches('/');
            Some((scheme, host, Some(port_str)))
        } else {
            None
        }
    } else {
        let (host, port) = match rest.find(':') {
            Some(i) => {
                let port_str = rest[i + 1..].trim_end_matches('/');
                (&rest[..i], Some(port_str))
            }
            None => {
                let host = rest.trim_end_matches('/');
                (host, None)
            }
        };
        if host.is_empty() {
            None
        } else {
            Some((scheme, host, port))
        }
    }
}

/// Validate the Origin header from a WebSocket upgrade request.
/// Returns true if the origin is acceptable (loopback or chrome-extension://).
fn is_origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(o) => {
            let lower = o.to_lowercase();
            match parse_origin(&lower) {
                None => false,
                Some((scheme, host, _port)) => {
                    if scheme == "chrome-extension" {
                        return true;
                    }
                    if scheme == "http" {
                        return matches!(host, "127.0.0.1" | "localhost" | "[::1]");
                    }
                    false
                }
            }
        }
    }
}

/// Handle a single incoming WebSocket connection.
/// Performs origin validation during the upgrade, then does the hello handshake.
async fn handle_connection(stream: TcpStream, state: Arc<Mutex<BridgeState>>) {
    // Use accept_hdr_async to inspect upgrade request headers for origin validation
    let ws = match tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::http::Request<()>,
         resp: tokio_tungstenite::tungstenite::http::Response<()>|
         -> std::result::Result<
            tokio_tungstenite::tungstenite::http::Response<()>,
            tokio_tungstenite::tungstenite::http::Response<Option<String>>,
        > {
            let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());

            if !is_origin_allowed(origin) {
                tracing::warn!("Rejected WebSocket connection with origin: {:?}", origin);
                let rejection = tokio_tungstenite::tungstenite::http::Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Some("Forbidden origin".to_string()))
                    .unwrap();
                return Err(rejection);
            }

            Ok(resp)
        },
    )
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    let (mut write, mut read) = ws.split();

    // Read first message - must be a hello handshake
    let first_msg = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        read.next(),
    )
    .await
    {
        Ok(Some(Ok(Message::Text(text)))) => text.to_string(),
        _ => {
            tracing::warn!("Client disconnected or timed out before sending hello");
            return;
        }
    };

    let parsed: serde_json::Value = match serde_json::from_str(&first_msg) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("Invalid JSON from client");
            return;
        }
    };

    // Validate hello handshake
    let msg_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if msg_type != "hello" {
        tracing::warn!("Expected hello message, got type={}", msg_type);
        return;
    }

    let client_token = parsed.get("token").and_then(|t| t.as_str()).unwrap_or("");
    let client_role = parsed.get("role").and_then(|r| r.as_str()).unwrap_or("");
    let client_version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");

    // Validate protocol version (require >= 0.2.0)
    let min_version = semver::Version::parse("0.2.0").unwrap();
    match semver::Version::parse(client_version) {
        Ok(v) if v >= min_version => {
            // Version OK
        }
        _ => {
            tracing::warn!(
                "Rejected {} client with version {} (minimum: {})",
                client_role,
                client_version,
                PROTOCOL_VERSION
            );
            let err_msg = serde_json::json!({
                "type": "hello_error",
                "error": "version_mismatch",
                "message": format!(
                    "Protocol version {} is not supported. Minimum required: {}",
                    client_version, PROTOCOL_VERSION
                ),
                "required_version": PROTOCOL_VERSION,
            });
            let _ = write
                .send(Message::Text(err_msg.to_string().into()))
                .await;
            return;
        }
    }

    // Validate token (constant-time to prevent timing side-channels)
    {
        let s = state.lock().await;
        let token_match = client_token.as_bytes().ct_eq(s.token.as_bytes());
        if token_match.unwrap_u8() != 1 {
            tracing::warn!("Invalid token from {} client", client_role);
            let err_msg = serde_json::json!({
                "type": "hello_error",
                "error": "invalid_token",
                "message": "Token mismatch. Reconnect via native messaging to obtain the current token.",
            });
            let _ = write
                .send(Message::Text(err_msg.to_string().into()))
                .await;
            return;
        }
    }

    // Send hello_ack to confirm successful authentication
    let ack = serde_json::json!({ "type": "hello_ack", "version": PROTOCOL_VERSION });
    if write
        .send(Message::Text(ack.to_string().into()))
        .await
        .is_err()
    {
        tracing::warn!("Failed to send hello_ack to {} client", client_role);
        return;
    }

    // Update activity timestamp
    {
        let mut s = state.lock().await;
        s.touch();
    }

    match client_role {
        "extension" => handle_extension_client(write, read, state).await,
        "cli" => handle_cli_client(write, read, state).await,
        other => {
            tracing::warn!("Unknown client role: {}", other);
        }
    }
}

/// Handle the extension client connection.
/// Stores the sender channel and routes responses back to pending CLI requests.
async fn handle_extension_client(
    mut write: futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    mut read: futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    state: Arc<Mutex<BridgeState>>,
) {
    println!("  {} Extension connected", colored::Colorize::green("✓"));

    // Create a channel for sending commands to the extension
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    {
        let mut s = state.lock().await;
        s.extension_tx = Some(tx);
    }

    // Spawn a task to forward commands from the channel to the WebSocket
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
        // Send close frame so the extension receives a clean disconnect
        let _ = write
            .send(Message::Close(Some(
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
                    reason: "Session ended".into(),
                },
            )))
            .await;
    });

    // Read responses from extension and route to pending CLI requests
    while let Some(frame) = read.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                // Update activity timestamp on every message
                {
                    let mut s = state.lock().await;
                    s.touch();
                }

                let text_str = text.to_string();
                match serde_json::from_str::<serde_json::Value>(&text_str) {
                    Ok(resp) => {
                        if let Some(id) = resp.get("id").and_then(|i| i.as_u64()) {
                            let mut s = state.lock().await;
                            if let Some(sender) = s.pending.remove(&id) {
                                let _ = sender.send(text_str);
                            } else {
                                tracing::warn!("Response for unknown request id: {}", id);
                            }
                        } else {
                            tracing::debug!("Extension message without id (event): {}", text_str);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Invalid JSON from extension: {}", e);
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::error!("Extension WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    println!(
        "  {} Extension disconnected",
        colored::Colorize::yellow("!")
    );

    // Clean up: notify all pending requests and clear extension channel
    {
        let mut s = state.lock().await;
        for (_id, sender) in s.pending.drain() {
            let err_msg = serde_json::json!({
                "id": 0,
                "error": { "code": -32000, "message": "Extension disconnected" }
            });
            let _ = sender.send(err_msg.to_string());
        }
        s.extension_tx = None;
    }

    write_handle.abort();
}

/// Handle a CLI client connection.
/// After the hello handshake, the CLI sends commands and receives responses.
async fn handle_cli_client(
    mut write: futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    mut read: futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    state: Arc<Mutex<BridgeState>>,
) {
    // Read the actual command message (second message after hello)
    let cmd_msg = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        read.next(),
    )
    .await
    {
        Ok(Some(Ok(Message::Text(text)))) => text.to_string(),
        _ => {
            tracing::warn!("CLI disconnected before sending command");
            return;
        }
    };

    let first_msg: serde_json::Value = match serde_json::from_str(&cmd_msg) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("Invalid JSON command from CLI");
            return;
        }
    };

    // Update activity
    {
        let mut s = state.lock().await;
        s.touch();
    }

    let method = first_msg
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let params = first_msg
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let cli_id = first_msg
        .get("id")
        .cloned()
        .unwrap_or(serde_json::json!(0));

    tracing::debug!("CLI command: {} {:?}", method, params);

    // Enforce CDP method allowlist
    let risk_level = match get_risk_level(method) {
        Some(level) => level,
        None => {
            tracing::warn!("Rejected unknown CDP method: {}", method);
            let err = serde_json::json!({
                "id": cli_id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not allowed: {}", method)
                }
            });
            let _ = write.send(Message::Text(err.to_string().into())).await;
            return;
        }
    };

    // Log L2+ operations
    match risk_level {
        RiskLevel::L2 => {
            tracing::info!("L2 operation: {} (page modification)", method);
        }
        RiskLevel::L3 => {
            tracing::warn!("L3 operation: {} (high risk)", method);
        }
        RiskLevel::L1 => {}
    }

    // Allocate a unique id and create a oneshot channel for the response
    let (response_tx, response_rx) = oneshot::channel::<String>();
    let request_id;

    {
        let mut s = state.lock().await;

        // Check extension is connected
        if s.extension_tx.is_none() {
            let err = serde_json::json!({
                "id": cli_id,
                "error": { "code": -32000, "message": "Extension not connected" }
            });
            let _ = write.send(Message::Text(err.to_string().into())).await;
            return;
        }

        request_id = s.next_id;
        s.next_id += 1;
        s.pending.insert(request_id, response_tx);

        // Forward command to extension with bridge-assigned id and risk level
        let cmd = serde_json::json!({
            "id": request_id,
            "method": method,
            "params": params,
            "risk_level": risk_level.as_str(),
        });

        if let Some(ext_tx) = &s.extension_tx {
            if ext_tx.send(cmd.to_string()).is_err() {
                s.pending.remove(&request_id);
                s.extension_tx = None;
                drop(s);
                let err = serde_json::json!({
                    "id": cli_id,
                    "error": { "code": -32000, "message": "Extension disconnected" }
                });
                let _ = write.send(Message::Text(err.to_string().into())).await;
                return;
            }
        }
    }

    // Wait for response from extension (with timeout)
    match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
        Ok(Ok(resp_str)) => {
            // Rewrite the id to match the CLI's original id
            if let Ok(mut resp) = serde_json::from_str::<serde_json::Value>(&resp_str) {
                resp["id"] = cli_id;
                let _ = write
                    .send(Message::Text(resp.to_string().into()))
                    .await;
            }
        }
        Ok(Err(_)) => {
            let err = serde_json::json!({
                "id": cli_id,
                "error": { "code": -32000, "message": "Extension connection lost" }
            });
            let _ = write.send(Message::Text(err.to_string().into())).await;
        }
        Err(_) => {
            // Timeout — clean up pending request
            let mut s = state.lock().await;
            s.pending.remove(&request_id);
            drop(s);

            let err = serde_json::json!({
                "id": cli_id,
                "error": { "code": -32000, "message": "Extension command timed out (30s)" }
            });
            let _ = write.send(Message::Text(err.to_string().into())).await;
        }
    }
}

/// Send a single command to the extension via the bridge and wait for the response.
/// Used by CLI commands when `--extension` mode is active.
/// If `token` is None, attempts to read from the token file.
pub async fn send_command(
    port: u16,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    // Auto-read token from file
    let token = read_token_file().await.ok_or_else(|| {
        ActionbookError::ExtensionError(
            "No bridge token found. Is `actionbook extension serve` running? \
             Token file not found at ~/.local/share/actionbook/bridge-token"
                .to_string(),
        )
    })?;

    send_command_with_token(port, method, params, &token).await
}

/// Send a single command with an explicit token.
pub async fn send_command_with_token(
    port: u16,
    method: &str,
    params: serde_json::Value,
    token: &str,
) -> Result<serde_json::Value> {
    use tokio_tungstenite::connect_async;

    let url = format!("ws://127.0.0.1:{}", port);
    let (mut ws, _) = connect_async(&url).await.map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "Cannot connect to bridge at {}. Is `actionbook extension serve` running? ({})",
            url, e
        ))
    })?;

    // Send hello handshake first
    let hello = serde_json::json!({
        "type": "hello",
        "role": "cli",
        "token": token,
        "version": PROTOCOL_VERSION,
    });

    ws.send(Message::Text(hello.to_string().into()))
        .await
        .map_err(|e| ActionbookError::ExtensionError(format!("Send hello failed: {}", e)))?;

    // Wait for hello_ack from server
    match tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            let ack: serde_json::Value =
                serde_json::from_str(text.as_str()).unwrap_or_default();
            if ack.get("type").and_then(|t| t.as_str()) != Some("hello_ack") {
                return Err(ActionbookError::ExtensionError(
                    "Authentication failed: invalid token".to_string(),
                ));
            }
        }
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
            return Err(ActionbookError::ExtensionError(
                "Authentication failed: connection closed (invalid token?)".to_string(),
            ));
        }
        Ok(Some(Err(e))) => {
            return Err(ActionbookError::ExtensionError(format!(
                "Authentication error: {}",
                e
            )));
        }
        Ok(Some(Ok(_))) => {
            // Binary, Ping, Pong, Frame - unexpected during handshake
            return Err(ActionbookError::ExtensionError(
                "Unexpected message type during handshake".to_string(),
            ));
        }
        Err(_) => {
            return Err(ActionbookError::ExtensionError(
                "Authentication timeout: server did not respond".to_string(),
            ));
        }
    }

    // Send the actual command
    let msg = serde_json::json!({
        "id": 1,
        "method": method,
        "params": params,
    });

    ws.send(Message::Text(msg.to_string().into()))
        .await
        .map_err(|e| ActionbookError::ExtensionError(format!("Send failed: {}", e)))?;

    // Wait for response
    while let Some(frame) = ws.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                let resp: serde_json::Value = serde_json::from_str(text.as_str())?;
                if let Some(error) = resp.get("error") {
                    return Err(ActionbookError::ExtensionError(
                        error
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown extension error")
                            .to_string(),
                    ));
                }
                return Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null));
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => {
                return Err(ActionbookError::ExtensionError(format!(
                    "WebSocket error: {}",
                    e
                )));
            }
        }
    }

    Err(ActionbookError::ExtensionError(
        "Connection closed without response".to_string(),
    ))
}

/// Check if the bridge server is running on the given port.
/// Uses a plain TCP connect to avoid leaving orphan WebSocket connections on the bridge.
pub async fn is_bridge_running(port: u16) -> bool {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_allowed() {
        // No origin is fine
        assert!(is_origin_allowed(None));

        // Allowed loopback origins
        assert!(is_origin_allowed(Some("http://127.0.0.1")));
        assert!(is_origin_allowed(Some("http://127.0.0.1:8080")));
        assert!(is_origin_allowed(Some("http://127.0.0.1/")));
        assert!(is_origin_allowed(Some("http://localhost")));
        assert!(is_origin_allowed(Some("http://localhost:3000")));
        assert!(is_origin_allowed(Some("http://localhost/")));
        assert!(is_origin_allowed(Some("http://[::1]")));
        assert!(is_origin_allowed(Some("http://[::1]:8080")));
        assert!(is_origin_allowed(Some("http://[::1]/")));

        // Chrome extension origins
        assert!(is_origin_allowed(Some("chrome-extension://abcdefghijklmnop")));
        assert!(is_origin_allowed(Some("chrome-extension://dpfioflkmnkklgjldmaggkodhlidkdcd")));

        // Case insensitive
        assert!(is_origin_allowed(Some("HTTP://LOCALHOST")));
        assert!(is_origin_allowed(Some("Chrome-Extension://abc")));
    }

    #[test]
    fn test_origin_rejected() {
        // Prefix-matching bypass attempts
        assert!(!is_origin_allowed(Some("http://127.0.0.1.evil.com")));
        assert!(!is_origin_allowed(Some("http://localhost.evil.com")));

        // HTTPS not allowed (only http for loopback)
        assert!(!is_origin_allowed(Some("https://127.0.0.1")));
        assert!(!is_origin_allowed(Some("https://localhost")));

        // External origins
        assert!(!is_origin_allowed(Some("http://evil.com")));
        assert!(!is_origin_allowed(Some("https://evil.com")));
        assert!(!is_origin_allowed(Some("http://example.com")));

        // Malformed origins
        assert!(!is_origin_allowed(Some("not-a-url")));
        assert!(!is_origin_allowed(Some("")));
        assert!(!is_origin_allowed(Some("http://")));
    }

    #[test]
    fn test_parse_origin() {
        assert_eq!(parse_origin("http://127.0.0.1"), Some(("http", "127.0.0.1", None)));
        assert_eq!(parse_origin("http://127.0.0.1:8080"), Some(("http", "127.0.0.1", Some("8080"))));
        assert_eq!(parse_origin("http://localhost/"), Some(("http", "localhost", None)));
        assert_eq!(parse_origin("http://[::1]"), Some(("http", "[::1]", None)));
        assert_eq!(parse_origin("http://[::1]:8080"), Some(("http", "[::1]", Some("8080"))));
        assert_eq!(parse_origin("chrome-extension://abcdef"), Some(("chrome-extension", "abcdef", None)));
        assert_eq!(parse_origin("http://"), None);
        assert_eq!(parse_origin("not-a-url"), None);
    }

    #[test]
    fn test_token_format() {
        let token = generate_token();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(token.len(), 4 + 32); // "abk_" + 32 hex chars
    }
}
