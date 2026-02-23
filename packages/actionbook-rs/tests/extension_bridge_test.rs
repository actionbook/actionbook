//! Integration tests for the extension bridge WebSocket server.
//!
//! These tests spin up a real bridge server on a random port,
//! connect mock extension/CLI clients via WebSocket, and verify
//! end-to-end message routing with the hello handshake protocol.
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

/// Try to read one text message. Returns None on close, error, or stream end.
/// Unlike recv_json, this does not panic on connection close/error.
async fn try_recv_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Option<serde_json::Value> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(text.as_str()).ok();
            }
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
            _ => return None, // Close, error, or stream end
        }
    }
}

/// Try to read with a timeout. Returns None on timeout, close, or error.
async fn try_recv_json_timeout(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    timeout_ms: u64,
) -> Option<serde_json::Value> {
    match tokio::time::timeout(Duration::from_millis(timeout_ms), try_recv_json(ws)).await {
        Ok(val) => val,
        Err(_) => None,
    }
}

/// Send the hello handshake as extension and wait for hello_ack.
async fn hello_extension(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    send_json(
        ws,
        serde_json::json!({
            "type": "hello",
            "role": "extension",
            "version": "0.2.0"
        }),
    )
    .await;

    // Wait for hello_ack from server
    let ack = recv_json_timeout(ws, 3000)
        .await
        .expect("Should receive hello_ack");
    assert_eq!(ack["type"].as_str(), Some("hello_ack"), "Expected hello_ack");
}

/// Send the hello handshake as CLI and wait for hello_ack.
async fn hello_cli(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    send_json(
        ws,
        serde_json::json!({
            "type": "hello",
            "role": "cli",
            "version": "0.2.0"
        }),
    )
    .await;

    // Wait for hello_ack from server
    let ack = recv_json_timeout(ws, 3000)
        .await
        .expect("Should receive hello_ack");
    assert_eq!(ack["type"].as_str(), Some("hello_ack"), "Expected hello_ack");
}

/// Start a bridge server on the given port.
fn start_bridge(port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = actionbook::browser::extension_bridge::serve(port).await;
    })
}

mod bridge_tests {
    use super::*;
    use assert_cmd::Command;
    use predicates::prelude::*;

    /// Test: Connection without hello message is closed.
    #[tokio::test]
    async fn no_hello_closes_connection() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut ws = ws_connect(port).await;

        // Send a non-hello message (old-style extension identification)
        send_json(
            &mut ws,
            serde_json::json!({ "type": "extension" }),
        )
        .await;

        // Should be closed
        let result = try_recv_json_timeout(&mut ws, 2000).await;
        assert!(result.is_none(), "Connection should be closed without hello");

        server_handle.abort();
    }

    /// Test: CLI command sent without extension connected gets an error response.
    #[tokio::test]
    async fn cli_without_extension_gets_error() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect as CLI and handshake
        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;

        // Send a CLI command
        send_json(
            &mut cli_ws,
            serde_json::json!({
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
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 1. Connect as extension with hello handshake
        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;

        // Give bridge time to register extension
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 2. Connect as CLI with hello handshake and send command
        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "id": 42,
                "method": "Runtime.evaluate",
                "params": { "expression": "1+1" }
            }),
        )
        .await;

        // 3. Extension should receive the forwarded command with risk_level
        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive command");

        assert_eq!(
            ext_msg["method"].as_str().unwrap(),
            "Runtime.evaluate"
        );
        assert!(ext_msg["id"].is_number(), "Should have a bridge-assigned id");
        assert_eq!(
            ext_msg["risk_level"].as_str(),
            Some("L2"),
            "Runtime.evaluate should have L2 risk level"
        );
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
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect extension with hello
        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect CLI with hello and send command
        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
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
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect extension with hello
        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send first CLI command with hello
        let mut cli1 = ws_connect(port).await;
        hello_cli(&mut cli1).await;
        send_json(
            &mut cli1,
            serde_json::json!({
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

        // Send second CLI command (different connection) with hello
        let mut cli2 = ws_connect(port).await;
        hello_cli(&mut cli2).await;
        send_json(
            &mut cli2,
            serde_json::json!({
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

    /// Test: Unknown CDP method is rejected at the bridge level.
    #[tokio::test]
    async fn unknown_method_rejected() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect extension
        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect CLI and send unknown method
        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "id": 1,
                "method": "Debugger.enable",
                "params": {}
            }),
        )
        .await;

        // Should get error response immediately (not forwarded to extension)
        let response = recv_json_timeout(&mut cli_ws, 3000)
            .await
            .expect("Should receive error response");

        assert!(response.get("error").is_some(), "Should have error field");
        let error_msg = response["error"]["message"].as_str().unwrap_or("");
        assert!(
            error_msg.contains("not allowed"),
            "Error should mention method not allowed: {}",
            error_msg
        );
        assert_eq!(
            response["error"]["code"].as_i64(),
            Some(-32601),
            "Should use JSON-RPC method not found code"
        );

        // Extension should NOT have received anything
        let ext_msg = try_recv_json_timeout(&mut ext_ws, 500).await;
        assert!(ext_msg.is_none(), "Extension should not receive rejected commands");

        server_handle.abort();
    }

    /// Test: L2 methods include risk_level in forwarded message.
    #[tokio::test]
    async fn l2_method_includes_risk_level() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "id": 1,
                "method": "Page.navigate",
                "params": { "url": "https://example.com" }
            }),
        )
        .await;

        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive command");

        assert_eq!(ext_msg["method"].as_str(), Some("Page.navigate"));
        assert_eq!(
            ext_msg["risk_level"].as_str(),
            Some("L2"),
            "Page.navigate should have L2 risk level"
        );

        server_handle.abort();
    }

    /// Test: L3 methods include risk_level in forwarded message.
    #[tokio::test]
    async fn l3_method_includes_risk_level() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "id": 1,
                "method": "Network.setCookie",
                "params": { "name": "test", "value": "val" }
            }),
        )
        .await;

        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive command");

        assert_eq!(ext_msg["method"].as_str(), Some("Network.setCookie"));
        assert_eq!(
            ext_msg["risk_level"].as_str(),
            Some("L3"),
            "Network.setCookie should have L3 risk level"
        );

        server_handle.abort();
    }

    /// Test: Extension.* methods are allowed (L1).
    #[tokio::test]
    async fn extension_internal_methods_allowed() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut cli_ws = ws_connect(port).await;
        hello_cli(&mut cli_ws).await;
        send_json(
            &mut cli_ws,
            serde_json::json!({
                "id": 1,
                "method": "Extension.ping",
                "params": {}
            }),
        )
        .await;

        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive Extension.ping");

        assert_eq!(ext_msg["method"].as_str(), Some("Extension.ping"));
        assert_eq!(
            ext_msg["risk_level"].as_str(),
            Some("L1"),
            "Extension.* methods should have L1 risk level"
        );

        server_handle.abort();
    }

    /// Test: get_risk_level returns correct levels for all categories.
    #[test]
    fn risk_level_categorization() {
        use actionbook::browser::extension_bridge::{get_risk_level, RiskLevel};

        // L1 - Read only
        assert_eq!(get_risk_level("Page.captureScreenshot"), Some(RiskLevel::L1));
        assert_eq!(get_risk_level("DOM.getDocument"), Some(RiskLevel::L1));
        assert_eq!(get_risk_level("DOM.querySelector"), Some(RiskLevel::L1));
        assert_eq!(get_risk_level("DOM.querySelectorAll"), Some(RiskLevel::L1));
        assert_eq!(get_risk_level("DOM.getOuterHTML"), Some(RiskLevel::L1));

        // L2 - Page modification (includes Runtime.evaluate)
        assert_eq!(get_risk_level("Runtime.evaluate"), Some(RiskLevel::L2));
        assert_eq!(get_risk_level("Page.navigate"), Some(RiskLevel::L2));
        assert_eq!(get_risk_level("Page.reload"), Some(RiskLevel::L2));
        assert_eq!(get_risk_level("Input.dispatchMouseEvent"), Some(RiskLevel::L2));
        assert_eq!(get_risk_level("Input.dispatchKeyEvent"), Some(RiskLevel::L2));
        assert_eq!(get_risk_level("Emulation.setDeviceMetricsOverride"), Some(RiskLevel::L2));
        assert_eq!(get_risk_level("Page.printToPDF"), Some(RiskLevel::L2));

        // L3 - High risk
        assert_eq!(get_risk_level("Network.setCookie"), Some(RiskLevel::L3));
        assert_eq!(get_risk_level("Network.deleteCookies"), Some(RiskLevel::L3));
        assert_eq!(get_risk_level("Network.clearBrowserCookies"), Some(RiskLevel::L3));
        assert_eq!(get_risk_level("Page.setDownloadBehavior"), Some(RiskLevel::L3));
        assert_eq!(get_risk_level("Storage.clearDataForOrigin"), Some(RiskLevel::L3));

        // Extension.* methods - L1
        assert_eq!(get_risk_level("Extension.ping"), Some(RiskLevel::L1));
        assert_eq!(get_risk_level("Extension.createTab"), Some(RiskLevel::L1));

        // Unknown - None
        assert_eq!(get_risk_level("Debugger.enable"), None);
        assert_eq!(get_risk_level("Target.createTarget"), None);
        assert_eq!(get_risk_level("Browser.close"), None);
    }

    /// Test: is_bridge_running returns true when server is up.
    #[tokio::test]
    async fn is_bridge_running_returns_true() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
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
            stdout.contains("failed") || stdout.contains("Cannot connect")
                || !output.status.success(),
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

    // --- Issue 1: createTab/activateTab auto-attach tests ---
    // Note: The bridge handles one command per CLI connection. Each command
    // must use a separate WebSocket connection (which is how the real CLI works).

    /// Test: Extension.createTab is forwarded and response includes attached: true.
    /// Then a subsequent CDP command (new CLI connection) is also forwarded successfully.
    #[tokio::test]
    async fn create_tab_then_cdp_succeeds() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect extension
        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // CLI connection 1: send Extension.createTab
        let mut cli1 = ws_connect(port).await;
        hello_cli(&mut cli1).await;
        send_json(
            &mut cli1,
            serde_json::json!({
                "id": 1,
                "method": "Extension.createTab",
                "params": { "url": "https://example.com" }
            }),
        )
        .await;

        // Extension receives createTab
        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive createTab");
        assert_eq!(ext_msg["method"].as_str(), Some("Extension.createTab"));
        let bridge_id = ext_msg["id"].as_u64().unwrap();

        // Extension responds with attached: true (simulating the fixed behavior)
        send_json(
            &mut ext_ws,
            serde_json::json!({
                "id": bridge_id,
                "result": { "tabId": 123, "title": "Example", "url": "https://example.com", "attached": true }
            }),
        )
        .await;

        // CLI receives response
        let cli_resp = recv_json_timeout(&mut cli1, 3000)
            .await
            .expect("CLI should receive createTab response");
        assert_eq!(cli_resp["id"].as_u64(), Some(1));
        assert_eq!(cli_resp["result"]["attached"].as_bool(), Some(true));
        assert_eq!(cli_resp["result"]["tabId"].as_u64(), Some(123));

        // CLI connection 2: send a CDP command — should be forwarded to extension
        let mut cli2 = ws_connect(port).await;
        hello_cli(&mut cli2).await;
        send_json(
            &mut cli2,
            serde_json::json!({
                "id": 2,
                "method": "Runtime.evaluate",
                "params": { "expression": "document.title" }
            }),
        )
        .await;

        let ext_cdp = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive CDP command after createTab");
        assert_eq!(ext_cdp["method"].as_str(), Some("Runtime.evaluate"));

        server_handle.abort();
    }

    /// Test: Extension.activateTab is forwarded and subsequent CDP works.
    #[tokio::test]
    async fn activate_tab_then_cdp_succeeds() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // CLI connection 1: send Extension.activateTab
        let mut cli1 = ws_connect(port).await;
        hello_cli(&mut cli1).await;
        send_json(
            &mut cli1,
            serde_json::json!({
                "id": 10,
                "method": "Extension.activateTab",
                "params": { "tabId": 456 }
            }),
        )
        .await;

        let ext_msg = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive activateTab");
        assert_eq!(ext_msg["method"].as_str(), Some("Extension.activateTab"));
        let bridge_id = ext_msg["id"].as_u64().unwrap();

        // Extension responds with attached: true
        send_json(
            &mut ext_ws,
            serde_json::json!({
                "id": bridge_id,
                "result": { "success": true, "tabId": 456, "title": "Tab B", "url": "https://b.com", "attached": true }
            }),
        )
        .await;

        let cli_resp = recv_json_timeout(&mut cli1, 3000)
            .await
            .expect("CLI should receive activateTab response");
        assert_eq!(cli_resp["id"].as_u64(), Some(10));
        assert_eq!(cli_resp["result"]["attached"].as_bool(), Some(true));

        // CLI connection 2: send CDP command — should be forwarded
        let mut cli2 = ws_connect(port).await;
        hello_cli(&mut cli2).await;
        send_json(
            &mut cli2,
            serde_json::json!({
                "id": 11,
                "method": "Runtime.evaluate",
                "params": { "expression": "location.href" }
            }),
        )
        .await;

        let ext_cdp = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("Extension should receive CDP command after activateTab");
        assert_eq!(ext_cdp["method"].as_str(), Some("Runtime.evaluate"));

        server_handle.abort();
    }

    /// Test: open tab A (createTab), switch to tab B (activateTab) — subsequent CDP goes to extension.
    /// Each command uses a separate CLI connection (bridge handles one command per connection).
    #[tokio::test]
    async fn switch_tab_changes_cdp_target() {
        let port = free_port().await;
        let server_handle = start_bridge(port);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut ext_ws = ws_connect(port).await;
        hello_extension(&mut ext_ws).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // CLI connection 1: Create tab A
        let mut cli1 = ws_connect(port).await;
        hello_cli(&mut cli1).await;
        send_json(
            &mut cli1,
            serde_json::json!({ "id": 1, "method": "Extension.createTab", "params": { "url": "https://a.com" } }),
        )
        .await;
        let ext_msg = recv_json_timeout(&mut ext_ws, 3000).await.unwrap();
        let bid = ext_msg["id"].as_u64().unwrap();
        send_json(&mut ext_ws, serde_json::json!({ "id": bid, "result": { "tabId": 100, "attached": true } })).await;
        let _ = recv_json_timeout(&mut cli1, 3000).await;

        // CLI connection 2: Switch to tab B (activateTab)
        let mut cli2 = ws_connect(port).await;
        hello_cli(&mut cli2).await;
        send_json(
            &mut cli2,
            serde_json::json!({ "id": 2, "method": "Extension.activateTab", "params": { "tabId": 200 } }),
        )
        .await;
        let ext_msg2 = recv_json_timeout(&mut ext_ws, 3000).await.unwrap();
        let bid2 = ext_msg2["id"].as_u64().unwrap();
        send_json(&mut ext_ws, serde_json::json!({ "id": bid2, "result": { "success": true, "tabId": 200, "attached": true } })).await;
        let _ = recv_json_timeout(&mut cli2, 3000).await;

        // CLI connection 3: CDP command — should be forwarded to extension (which targets tab B now)
        let mut cli3 = ws_connect(port).await;
        hello_cli(&mut cli3).await;
        send_json(
            &mut cli3,
            serde_json::json!({ "id": 3, "method": "Runtime.evaluate", "params": { "expression": "1+1" } }),
        )
        .await;

        let ext_cdp = recv_json_timeout(&mut ext_ws, 3000)
            .await
            .expect("CDP should be forwarded after tab switch");
        assert_eq!(ext_cdp["method"].as_str(), Some("Runtime.evaluate"));

        server_handle.abort();
    }

    // --- Issue 2: --profile + --extension rejected ---

    /// Test: --profile flag combined with --extension produces an error.
    #[test]
    fn profile_flag_rejected_in_extension_mode() {
        let mut cmd = Command::cargo_bin("actionbook").unwrap();
        cmd.args(["--profile", "myprofile", "--extension", "browser", "status"])
            .timeout(Duration::from_secs(5))
            .assert()
            .failure()
            .stderr(
                predicates::str::contains("--profile is not supported in extension mode")
                    .or(predicates::str::contains("not supported in extension"))
            );
    }

    // --- Issue 3: cookies clear with --domain flag ---

    /// Test: cookies clear --dry-run flag is accepted by CLI parser.
    #[test]
    fn cookies_clear_dry_run_accepted() {
        let mut cmd = Command::cargo_bin("actionbook").unwrap();
        cmd.args(["browser", "cookies", "clear", "--dry-run", "--help"])
            .assert()
            .success()
            .stdout(predicates::str::contains("dry-run"));
    }

    /// Test: cookies clear --domain flag is accepted by CLI parser.
    #[test]
    fn cookies_clear_domain_accepted() {
        let mut cmd = Command::cargo_bin("actionbook").unwrap();
        cmd.args(["browser", "cookies", "clear", "--help"])
            .assert()
            .success()
            .stdout(predicates::str::contains("--domain"))
            .stdout(predicates::str::contains("--yes"));
    }
}
