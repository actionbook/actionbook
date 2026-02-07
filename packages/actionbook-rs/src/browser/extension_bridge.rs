use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

use crate::error::{ActionbookError, Result};

/// Shared state for the bridge server
struct BridgeState {
    /// Channel to send commands to the connected extension
    extension_tx: Option<mpsc::UnboundedSender<String>>,
    /// Pending CLI requests waiting for extension responses, keyed by request id
    pending: HashMap<u64, oneshot::Sender<String>>,
    /// Monotonically increasing request id counter
    next_id: u64,
}

impl BridgeState {
    fn new() -> Self {
        Self {
            extension_tx: None,
            pending: HashMap::new(),
            next_id: 1,
        }
    }
}

/// Start the bridge WebSocket server on the given port.
/// This function blocks until the server is shut down.
pub async fn serve(port: u16) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        ActionbookError::Other(format!("Failed to bind to {}: {}", addr, e))
    })?;

    let state = Arc::new(Mutex::new(BridgeState::new()));

    println!("Bridge server listening on ws://127.0.0.1:{}", port);
    println!("Waiting for extension connection...");

    loop {
        let (stream, peer) = listener.accept().await.map_err(|e| {
            ActionbookError::Other(format!("Accept failed: {}", e))
        })?;

        tracing::debug!("New connection from {}", peer);
        let state = Arc::clone(&state);
        tokio::spawn(handle_connection(stream, state));
    }
}

/// Send a single command to the extension via the bridge and wait for the response.
/// Used by CLI commands when `--extension` mode is active.
pub async fn send_command(
    port: u16,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    use tokio_tungstenite::connect_async;

    let url = format!("ws://127.0.0.1:{}", port);
    let (mut ws, _) = connect_async(&url).await.map_err(|e| {
        ActionbookError::ExtensionError(format!(
            "Cannot connect to bridge at {}. Is `actionbook extension serve` running? ({})",
            url, e
        ))
    })?;

    // Send a CLI hello + command in one message
    let msg = serde_json::json!({
        "type": "cli",
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

/// Handle a single incoming WebSocket connection.
/// Determines if the client is an extension or a CLI based on its first message.
async fn handle_connection(stream: TcpStream, state: Arc<Mutex<BridgeState>>) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    let (write, mut read) = ws.split();

    // Read first message to determine client type
    let first_msg = match read.next().await {
        Some(Ok(Message::Text(text))) => text.to_string(),
        _ => {
            tracing::warn!("Client disconnected before sending identification");
            return;
        }
    };

    let parsed: serde_json::Value = match serde_json::from_str(&first_msg) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("Invalid JSON from client: {}", first_msg);
            return;
        }
    };

    let client_type = parsed
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown");

    match client_type {
        "extension" => handle_extension_client(write, read, state).await,
        "cli" => handle_cli_client(parsed, write, read, state).await,
        other => {
            tracing::warn!("Unknown client type: {}", other);
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
        // Cleanup happens in the main read loop below
    });

    // Read responses from extension and route to pending CLI requests
    while let Some(frame) = read.next().await {
        match frame {
            Ok(Message::Text(text)) => {
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
/// The CLI sends one command and expects one response, then disconnects.
async fn handle_cli_client(
    first_msg: serde_json::Value,
    mut write: futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    _read: futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    state: Arc<Mutex<BridgeState>>,
) {
    let method = first_msg
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let params = first_msg
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    tracing::debug!("CLI command: {} {:?}", method, params);

    // Allocate a unique id and create a oneshot channel for the response
    let (response_tx, response_rx) = oneshot::channel::<String>();
    let request_id;

    {
        let mut s = state.lock().await;

        // Check extension is connected
        if s.extension_tx.is_none() {
            let err = serde_json::json!({
                "id": first_msg.get("id").cloned().unwrap_or(serde_json::json!(0)),
                "error": { "code": -32000, "message": "Extension not connected" }
            });
            let _ = write.send(Message::Text(err.to_string().into())).await;
            return;
        }

        request_id = s.next_id;
        s.next_id += 1;
        s.pending.insert(request_id, response_tx);

        // Forward command to extension with the bridge-assigned id
        let cmd = serde_json::json!({
            "id": request_id,
            "method": method,
            "params": params,
        });

        if let Some(ext_tx) = &s.extension_tx {
            let _ = ext_tx.send(cmd.to_string());
        }
    }

    // Wait for response from extension (with timeout)
    let cli_id = first_msg
        .get("id")
        .cloned()
        .unwrap_or(serde_json::json!(0));

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

/// Check if the bridge server is running on the given port
pub async fn is_bridge_running(port: u16) -> bool {
    use tokio_tungstenite::connect_async;
    let url = format!("ws://127.0.0.1:{}", port);
    connect_async(&url).await.is_ok()
}
