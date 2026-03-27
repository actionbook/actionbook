//! Shared CDP (Chrome DevTools Protocol) helper functions.

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::CliError;
use crate::types::TabId;

use super::registry::SessionEntry;

fn ws_text(s: String) -> Message {
    Message::Text(s.into())
}

fn msg_to_string(msg: &Message) -> Option<String> {
    match msg {
        Message::Text(t) => Some(t.to_string()),
        _ => None,
    }
}

/// Resolve WebSocket URL for a tab from a session entry.
pub fn resolve_tab_ws_url(
    tab_id: &str,
    entry: &SessionEntry,
) -> Result<String, crate::action_result::ActionResult> {
    let parsed_tab: TabId = tab_id.parse().map_err(|e| {
        crate::action_result::ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid tab id: {e}"),
        )
    })?;
    let tab = entry
        .tabs
        .iter()
        .find(|t| t.id == parsed_tab)
        .ok_or_else(|| {
            crate::action_result::ActionResult::fatal(
                "TAB_NOT_FOUND",
                format!("tab '{tab_id}' not found"),
            )
        })?;
    Ok(if !tab.target_id.is_empty() {
        format!(
            "ws://127.0.0.1:{}/devtools/page/{}",
            entry.cdp_port, tab.target_id
        )
    } else {
        entry.ws_url.clone()
    })
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

/// Ensure a URL has a scheme prefix.
pub fn ensure_scheme(url: &str) -> String {
    if url.contains("://")
        || url.starts_with("about:")
        || url.starts_with("data:")
        || url.starts_with("chrome:")
        || url.starts_with("javascript:")
    {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}
