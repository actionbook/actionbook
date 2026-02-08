//! Integration tests for the extension bridge WebSocket server.
//!
//! These tests spin up a real bridge server on a random port,
//! connect mock extension/CLI clients via WebSocket, and verify
//! end-to-end message routing.
//!
//! Run with: cargo test --test extension_bridge_test

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// Find a free port by binding to port 0 and reading the assigned port.
async fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Connect a WebSocket client to the given port.
async fn ws_connect(
    port: u16,
) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let url = format!("ws://127.0.0.1:{}", port);
    let (ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("Failed to connect to bridge");
    ws
}

/// Send a JSON message and return the stream for further use.
async fn send_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    value: serde_json::Value,
) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .expect("Failed to send message");
}

/// Read one text message and parse as JSON.
async fn recv_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(text.as_str())
                    .expect("Failed to parse JSON from bridge");
            }
            Some(Ok(Message::Close(_))) => panic!("WebSocket closed unexpectedly"),
            Some(Err(e)) => panic!("WebSocket error: {}", e),
            None => panic!("WebSocket stream ended"),
            _ => continue, // skip ping/pong
        }
    }
}

/// Read one text message with a timeout.
async fn recv_json_timeout(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    timeout_ms: u64,
) -> Option<serde_json::Value> {
    match tokio::time::timeout(Duration::from_millis(timeout_ms), recv_json(ws)).await {
        Ok(val) => Some(val),
        Err(_) => None,
    }
}

mod bridge_tests {
    use super::*;
    use assert_cmd::Command;

    /// Test: CLI command sent without extension connected gets an error response.
    #[tokio::test]
    async fn cli_without_extension_gets_error() {
        let port = free_port().await;

        // Start bridge server in background
        let server_handle = tokio::spawn(async move {
            // We use the library function directly
            // The serve function blocks, so we run it as a background task
            let _ = actionbook::browser::extension_bridge::serve(port).await;
        });

        // Give server time to bind
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect as CLI (no extension connected)
        let mut cli_ws = ws_connect(port).await;

        // Send a CLI command
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "type": "cli",
                "id": 1,
                "method": "Page.navigate",
                "params": { "url": "https://example.com" }
            }),
        )
        .await;

        // Should get error response about extension not connected
        let response = recv_json_timeout(&mut cli_ws, 3000)
            .await
            .expect("Should receive response");

        assert!(response.get("error").is_some(), "Should have error field");
        let error_msg = response["error"]["message"]
            .as_str()
            .unwrap_or("");
        assert!(
            error_msg.contains("not connected"),
            "Error should mention extension not connected: {}",
            error_msg
        );

        server_handle.abort();
    }

    /// Test: Full round-trip - extension connects, CLI sends command, extension responds.
    #[tokio::test]
    async fn full_roundtrip_extension_to_cli() {
        let port = free_port().await;

        // Start bridge server
        let server_handle = tokio::spawn(async move {
            let _ = actionbook::browser::extension_bridge::serve(port).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // 1. Connect as extension
        let mut ext_ws = ws_connect(port).await;
        send_json(
            &mut ext_ws,
            serde_json::json!({ "type": "extension" }),
        )
        .await;

        // Give bridge time to register extension
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 2. Connect as CLI and send command
        let mut cli_ws = ws_connect(port).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "type": "cli",
                "id": 42,
                "method": "Runtime.evaluate",
                "params": { "expression": "1+1" }
            }),
        )
        .await;

        // 3. Extension should receive the forwarded command
        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive command");

        assert_eq!(
            ext_msg["method"].as_str().unwrap(),
            "Runtime.evaluate"
        );
        assert!(ext_msg["id"].is_number(), "Should have a bridge-assigned id");
        let bridge_id = ext_msg["id"].as_u64().unwrap();

        // 4. Extension sends back a response with the bridge-assigned id
        send_json(
            &mut ext_ws,
            serde_json::json!({
                "id": bridge_id,
                "result": {
                    "result": {
                        "type": "number",
                        "value": 2
                    }
                }
            }),
        )
        .await;

        // 5. CLI should receive the response with its original id (42)
        let cli_response = recv_json_timeout(&mut cli_ws, 3000)
            .await
            .expect("CLI should receive response");

        assert_eq!(cli_response["id"].as_u64(), Some(42));
        assert!(cli_response.get("result").is_some());
        assert_eq!(
            cli_response["result"]["result"]["value"].as_u64(),
            Some(2)
        );

        server_handle.abort();
    }

    /// Test: Extension error response is forwarded to CLI.
    #[tokio::test]
    async fn extension_error_forwarded_to_cli() {
        let port = free_port().await;

        let server_handle = tokio::spawn(async move {
            let _ = actionbook::browser::extension_bridge::serve(port).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect extension
        let mut ext_ws = ws_connect(port).await;
        send_json(
            &mut ext_ws,
            serde_json::json!({ "type": "extension" }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect CLI and send command
        let mut cli_ws = ws_connect(port).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "type": "cli",
                "id": 7,
                "method": "Page.navigate",
                "params": { "url": "chrome://invalid" }
            }),
        )
        .await;

        // Extension receives command
        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive command");
        let bridge_id = ext_msg["id"].as_u64().unwrap();

        // Extension responds with error
        send_json(
            &mut ext_ws,
            serde_json::json!({
                "id": bridge_id,
                "error": {
                    "code": -32000,
                    "message": "Cannot navigate to chrome:// URLs"
                }
            }),
        )
        .await;

        // CLI should receive the error with its original id
        let cli_response = recv_json_timeout(&mut cli_ws, 3000)
            .await
            .expect("CLI should receive error response");

        assert_eq!(cli_response["id"].as_u64(), Some(7));
        assert!(cli_response.get("error").is_some());
        assert!(
            cli_response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("chrome://")
        );

        server_handle.abort();
    }

    /// Test: Multiple CLI commands are routed with unique bridge ids.
    #[tokio::test]
    async fn multiple_cli_commands_get_unique_ids() {
        let port = free_port().await;

        let server_handle = tokio::spawn(async move {
            let _ = actionbook::browser::extension_bridge::serve(port).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect extension
        let mut ext_ws = ws_connect(port).await;
        send_json(
            &mut ext_ws,
            serde_json::json!({ "type": "extension" }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send first CLI command
        let mut cli1 = ws_connect(port).await;
        send_json(
            &mut cli1,
            serde_json::json!({
                "type": "cli",
                "id": 1,
                "method": "Page.navigate",
                "params": { "url": "https://a.com" }
            }),
        )
        .await;

        let msg1 = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Should get first command");
        let id1 = msg1["id"].as_u64().unwrap();

        // Send second CLI command (different connection)
        let mut cli2 = ws_connect(port).await;
        send_json(
            &mut cli2,
            serde_json::json!({
                "type": "cli",
                "id": 1,
                "method": "Page.navigate",
                "params": { "url": "https://b.com" }
            }),
        )
        .await;

        let msg2 = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Should get second command");
        let id2 = msg2["id"].as_u64().unwrap();

        // Bridge should assign different ids even though CLI ids are the same
        assert_ne!(id1, id2, "Bridge should assign unique ids");

        // Respond to both in reverse order
        send_json(
            &mut ext_ws,
            serde_json::json!({
                "id": id2,
                "result": { "url": "https://b.com" }
            }),
        )
        .await;
        send_json(
            &mut ext_ws,
            serde_json::json!({
                "id": id1,
                "result": { "url": "https://a.com" }
            }),
        )
        .await;

        // Each CLI should get the correct response
        let resp2 = recv_json_timeout(&mut cli2, 3000)
            .await
            .expect("CLI 2 should get response");
        assert_eq!(resp2["result"]["url"].as_str(), Some("https://b.com"));

        let resp1 = recv_json_timeout(&mut cli1, 3000)
            .await
            .expect("CLI 1 should get response");
        assert_eq!(resp1["result"]["url"].as_str(), Some("https://a.com"));

        server_handle.abort();
    }

    /// Test: is_bridge_running returns true when server is up.
    #[tokio::test]
    async fn is_bridge_running_returns_true() {
        let port = free_port().await;

        let server_handle = tokio::spawn(async move {
            let _ = actionbook::browser::extension_bridge::serve(port).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let running = actionbook::browser::extension_bridge::is_bridge_running(port).await;
        assert!(running, "Bridge should be detected as running");

        server_handle.abort();
    }

    /// Test: is_bridge_running returns false when no server is running.
    #[tokio::test]
    async fn is_bridge_running_returns_false_when_not_running() {
        let port = free_port().await;
        // Don't start any server
        let running = actionbook::browser::extension_bridge::is_bridge_running(port).await;
        assert!(!running, "Bridge should not be detected as running");
    }

    /// Test: send_command returns error when bridge is not running.
    #[tokio::test]
    async fn send_command_fails_when_bridge_not_running() {
        let port = free_port().await;
        // Don't start any server

        let result = actionbook::browser::extension_bridge::send_command(
            port,
            "Runtime.evaluate",
            serde_json::json!({ "expression": "1" }),
        )
        .await;

        assert!(result.is_err(), "Should fail when bridge not running");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Cannot connect") || err.contains("bridge"),
            "Error should mention connection failure: {}",
            err
        );
    }

    /// Test: CLI extension ping command via assert_cmd.
    #[test]
    fn cli_extension_ping_without_bridge_shows_error() {
        // Extension ping should show error when bridge is not running
        let mut cmd = Command::cargo_bin("actionbook").unwrap();
        let output = cmd
            .args(["extension", "ping", "--port", "19999"])
            .timeout(Duration::from_secs(5))
            .output()
            .expect("Should execute");

        let stdout = String::from_utf8_lossy(&output.stdout);
        // The command may succeed (exit 0) but print a failure message
        assert!(
            stdout.contains("failed") || stdout.contains("Cannot connect") || !output.status.success(),
            "Should indicate ping failed: {}",
            stdout
        );
    }

    /// Test: CLI extension status command via assert_cmd.
    #[test]
    fn cli_extension_status_runs() {
        let mut cmd = Command::cargo_bin("actionbook").unwrap();
        let result = cmd
            .args(["extension", "status", "--port", "19999"])
            .timeout(Duration::from_secs(5))
            .assert();
        // Should complete without panic (may report not running)
        let _ = result;
    }
}
