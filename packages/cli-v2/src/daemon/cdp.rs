//! Shared CDP (Chrome DevTools Protocol) helper functions.

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::CliError;

fn ws_text(s: String) -> Message {
    Message::Text(s.into())
}

fn msg_to_string(msg: &Message) -> Option<String> {
    match msg {
        Message::Text(t) => Some(t.to_string()),
        _ => None,
    }
}

/// CDP Runtime.evaluate via WebSocket.
pub async fn cdp_runtime_evaluate(ws_url: &str, expression: &str) -> Result<String, CliError> {
    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

    let msg = json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": { "expression": expression, "returnByValue": true }
    });
    ws.send(ws_text(msg.to_string()))
        .await
        .map_err(|e| CliError::CdpError(e.to_string()))?;

    while let Some(raw) = ws.next().await {
        let raw = raw.map_err(|e| CliError::CdpError(e.to_string()))?;
        if let Some(text) = msg_to_string(&raw) {
            let resp: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| CliError::CdpError(e.to_string()))?;
            if resp.get("id").and_then(|v| v.as_u64()) == Some(1) {
                if let Some(result) = resp.get("result").and_then(|r| r.get("result")) {
                    let value = result
                        .get("value")
                        .map(|v| {
                            if v.is_string() {
                                v.as_str().unwrap().to_string()
                            } else {
                                v.to_string()
                            }
                        })
                        .unwrap_or_default();
                    let _ = ws.close(None).await;
                    return Ok(value);
                }
                if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
                    let emsg = exc
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("expression error");
                    let _ = ws.close(None).await;
                    return Err(CliError::EvalFailed(emsg.to_string()));
                }
            }
        }
    }
    Err(CliError::CdpError("no response from CDP".to_string()))
}

/// CDP Page.navigate via WebSocket.
pub async fn cdp_navigate(ws_url: &str, url: &str) -> Result<(), CliError> {
    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

    let msg = json!({ "id": 1, "method": "Page.navigate", "params": { "url": url } });
    ws.send(ws_text(msg.to_string()))
        .await
        .map_err(|e| CliError::CdpError(e.to_string()))?;

    while let Some(raw) = ws.next().await {
        let raw = raw.map_err(|e| CliError::CdpError(e.to_string()))?;
        if let Some(text) = msg_to_string(&raw) {
            let resp: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if resp.get("id").and_then(|v| v.as_u64()) == Some(1) {
                let _ = ws.close(None).await;
                // CDP protocol-level error (e.g. invalid method, internal error)
                if let Some(err) = resp.get("error") {
                    let msg = err["message"].as_str().unwrap_or("CDP error");
                    return Err(CliError::NavigationFailed(msg.to_string()));
                }
                // Page.navigate can succeed at CDP level but report a navigation
                // error via result.errorText (e.g. net::ERR_ABORTED, invalid scheme).
                if let Some(error_text) = resp
                    .get("result")
                    .and_then(|r| r.get("errorText"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    return Err(CliError::NavigationFailed(error_text.to_string()));
                }
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Get accessibility tree via CDP.
pub async fn cdp_get_ax_tree(ws_url: &str) -> Result<String, CliError> {
    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| CliError::CdpConnectionFailed(e.to_string()))?;

    let msg = json!({ "id": 1, "method": "Accessibility.getFullAXTree", "params": {} });
    ws.send(ws_text(msg.to_string()))
        .await
        .map_err(|e| CliError::CdpError(e.to_string()))?;

    while let Some(raw) = ws.next().await {
        let raw = raw.map_err(|e| CliError::CdpError(e.to_string()))?;
        if let Some(text) = msg_to_string(&raw) {
            let resp: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if resp.get("id").and_then(|v| v.as_u64()) == Some(1) {
                let _ = ws.close(None).await;
                return Ok(text);
            }
        }
    }
    Err(CliError::CdpError("no response".to_string()))
}

/// Ensure a URL has a scheme prefix. Rejects dangerous protocols.
pub fn ensure_scheme(url: &str) -> Result<String, crate::error::CliError> {
    // Block dangerous protocols (L3 CDP security level, case-insensitive)
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:text/html") {
        return Err(crate::error::CliError::InvalidArgument(format!(
            "dangerous URL protocol blocked: {}",
            &url[..url.len().min(30)]
        )));
    }
    if url.contains("://")
        || lower.starts_with("about:")
        || lower.starts_with("chrome:")
        || lower.starts_with("data:")
    {
        Ok(url.to_string())
    } else {
        Ok(format!("https://{url}"))
    }
}

/// Ensure scheme, returning the URL or a fatal ActionResult.
pub fn ensure_scheme_or_fatal(url: &str) -> Result<String, crate::action_result::ActionResult> {
    ensure_scheme(url)
        .map_err(|e| crate::action_result::ActionResult::fatal("INVALID_ARGUMENT", e.to_string()))
}
