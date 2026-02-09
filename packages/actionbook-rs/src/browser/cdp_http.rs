use crate::error::{ActionbookError, Result};
use serde::Deserialize;

/// A target entry from Chrome's `/json/list` endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpTarget {
    /// e.g. "service_worker", "page", "background_page"
    #[serde(default)]
    r#type: String,
    /// Target URL — for service workers this is the extension's SW URL
    #[serde(default)]
    url: String,
    /// WebSocket debugger URL for attaching to this target
    #[serde(default)]
    web_socket_debugger_url: String,
}

/// Query Chrome's `/json/list` and find the service worker target for the given extension ID.
///
/// Returns the `webSocketDebuggerUrl` for the matching target.
async fn find_service_worker_target(cdp_port: u16, ext_id: &str) -> Result<String> {
    let url = format!("http://127.0.0.1:{}/json/list", cdp_port);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let targets: Vec<CdpTarget> = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to query CDP /json/list: {}", e)))?
        .json()
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to parse CDP /json/list: {}", e)))?;

    let pattern = format!("chrome-extension://{}/", ext_id);
    targets
        .into_iter()
        .find(|t| t.r#type == "service_worker" && t.url.starts_with(&pattern))
        .map(|t| t.web_socket_debugger_url)
        .filter(|ws| !ws.is_empty())
        .ok_or_else(|| {
            ActionbookError::ExtensionError(format!(
                "No service_worker target found for extension {}",
                ext_id
            ))
        })
}

/// The service worker filename used by the Actionbook extension.
/// Used to distinguish our extension from other extensions when the ext_id is unknown.
const ACTIONBOOK_SW_FILENAME: &str = "background.js";

/// Find the Actionbook extension's service worker target (when ext_id is unknown,
/// e.g. already-running case).
///
/// Matches `service_worker` targets whose URL matches `chrome-extension://<id>/background.js`
/// to avoid injecting the token into a different extension's storage.
async fn find_any_extension_service_worker(cdp_port: u16) -> Result<(String, String)> {
    let url = format!("http://127.0.0.1:{}/json/list", cdp_port);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let targets: Vec<CdpTarget> = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to query CDP /json/list: {}", e)))?
        .json()
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to parse CDP /json/list: {}", e)))?;

    // Match only service workers whose URL ends with our known SW filename.
    // This prevents injecting the bridge token into a random third-party
    // extension's chrome.storage.local.
    let sw_suffix = format!("/{}", ACTIONBOOK_SW_FILENAME);
    targets
        .into_iter()
        .find(|t| {
            t.r#type == "service_worker"
                && t.url.starts_with("chrome-extension://")
                && t.url.ends_with(&sw_suffix)
        })
        .map(|t| (t.web_socket_debugger_url.clone(), t.url.clone()))
        .filter(|(ws, _)| !ws.is_empty())
        .ok_or_else(|| {
            ActionbookError::ExtensionError(
                "No Actionbook extension service_worker target found via CDP. \
                 Looking for a service_worker with background.js"
                    .to_string(),
            )
        })
}

/// Connect to a target's WebSocket and evaluate a JS expression via `Runtime.evaluate`.
///
/// Returns the stringified result or an error.
async fn evaluate_in_target(ws_url: &str, expression: &str) -> Result<serde_json::Value> {
    use futures::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| {
            ActionbookError::Other(format!("Failed to connect to CDP WebSocket {}: {}", ws_url, e))
        })?;

    let request = serde_json::json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": expression,
            "awaitPromise": true,
            "returnByValue": true
        }
    });

    ws_stream
        .send(Message::Text(request.to_string().into()))
        .await
        .map_err(|e| ActionbookError::Other(format!("Failed to send CDP evaluate: {}", e)))?;

    // Read messages until we get our response (id: 1)
    use futures::StreamExt;
    let timeout = tokio::time::Duration::from_secs(10);
    let result = tokio::time::timeout(timeout, async {
        while let Some(msg) = ws_stream.next().await {
            let msg = msg.map_err(|e| {
                ActionbookError::Other(format!("CDP WebSocket read error: {}", e))
            })?;

            if let Message::Text(text) = msg {
                let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                    ActionbookError::Other(format!("Failed to parse CDP response: {}", e))
                })?;

                if parsed.get("id").and_then(|v| v.as_i64()) == Some(1) {
                    // Check for CDP-level error
                    if let Some(error) = parsed.get("error") {
                        let message = error
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown");
                        return Err(ActionbookError::Other(format!(
                            "CDP Runtime.evaluate error: {}",
                            message
                        )));
                    }

                    // Check for JS exception
                    if let Some(result) = parsed.get("result") {
                        if let Some(exception) = result.get("exceptionDetails") {
                            let desc = exception
                                .pointer("/exception/description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("unknown exception");
                            return Err(ActionbookError::Other(format!(
                                "JS exception during token injection: {}",
                                desc
                            )));
                        }
                        return Ok(result.clone());
                    }

                    return Ok(serde_json::Value::Null);
                }
            }
        }
        Err(ActionbookError::Other(
            "CDP WebSocket closed before receiving response".to_string(),
        ))
    })
    .await;

    // Close the WebSocket gracefully
    let _ = ws_stream.close(None).await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(ActionbookError::Other(
            "Timed out waiting for CDP Runtime.evaluate response (10s)".to_string(),
        )),
    }
}

/// Inject bridge token and port into the extension's `chrome.storage.local` via CDP.
///
/// This polls for the extension's service worker target (it may not appear immediately
/// after `Extensions.loadUnpacked`), then evaluates `chrome.storage.local.set(...)`.
pub async fn inject_token_via_cdp(
    cdp_port: u16,
    ext_id: &str,
    token: &str,
    bridge_port: u16,
) -> Result<()> {
    // Poll for the service worker target with exponential backoff
    let mut ws_url = None;
    let mut delay_ms = 200u64;
    for attempt in 1..=15 {
        match find_service_worker_target(cdp_port, ext_id).await {
            Ok(url) => {
                ws_url = Some(url);
                break;
            }
            Err(e) => {
                tracing::debug!(
                    "CDP SW target not found (attempt {}/15): {}",
                    attempt,
                    e
                );
                if attempt < 15 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(2000);
                }
            }
        }
    }

    let ws_url = ws_url.ok_or_else(|| {
        ActionbookError::ExtensionError(
            "Timed out waiting for extension service worker to appear in CDP targets".to_string(),
        )
    })?;

    // JSON-escape the token to prevent injection
    let token_json = serde_json::to_string(token).map_err(|e| {
        ActionbookError::Other(format!("Failed to JSON-encode token: {}", e))
    })?;

    let expression = format!(
        "chrome.storage.local.set({{ bridgeToken: {}, bridgePort: {} }})",
        token_json, bridge_port
    );

    evaluate_in_target(&ws_url, &expression).await?;

    Ok(())
}

/// Inject token into an already-running extension (ext_id unknown).
///
/// Used when Chrome is already running and we need to find the extension's
/// service worker without knowing the extension ID upfront.
pub async fn inject_token_existing(
    cdp_port: u16,
    token: &str,
    bridge_port: u16,
) -> Result<()> {
    let (ws_url, _sw_url) = find_any_extension_service_worker(cdp_port).await?;

    let token_json = serde_json::to_string(token).map_err(|e| {
        ActionbookError::Other(format!("Failed to JSON-encode token: {}", e))
    })?;

    let expression = format!(
        "chrome.storage.local.set({{ bridgeToken: {}, bridgePort: {} }})",
        token_json, bridge_port
    );

    evaluate_in_target(&ws_url, &expression).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdp_target_deserialize() {
        let json = r#"[
            {
                "type": "service_worker",
                "url": "chrome-extension://abcdef123456/background.js",
                "webSocketDebuggerUrl": "ws://127.0.0.1:9333/devtools/page/ABC"
            },
            {
                "type": "page",
                "url": "chrome://extensions/",
                "webSocketDebuggerUrl": "ws://127.0.0.1:9333/devtools/page/DEF"
            }
        ]"#;

        let targets: Vec<CdpTarget> = serde_json::from_str(json).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].r#type, "service_worker");
        assert!(targets[0].url.contains("abcdef123456"));
        assert!(targets[0].web_socket_debugger_url.contains("ABC"));
    }

    #[test]
    fn token_json_escaping() {
        // Verify that serde_json::to_string properly escapes tokens
        let token = r#"abk_deadbeef"with"quotes"#;
        let escaped = serde_json::to_string(token).unwrap();
        // Should be wrapped in quotes with internal quotes escaped
        assert!(escaped.starts_with('"'));
        assert!(escaped.ends_with('"'));
        assert!(escaped.contains(r#"\""#));
    }

    #[test]
    fn expression_format() {
        let token = "abk_0123456789abcdef0123456789abcdef";
        let port = 19222u16;
        let token_json = serde_json::to_string(token).unwrap();
        let expr = format!(
            "chrome.storage.local.set({{ bridgeToken: {}, bridgePort: {} }})",
            token_json, port
        );
        assert!(expr.contains("bridgeToken: \"abk_0123456789abcdef0123456789abcdef\""));
        assert!(expr.contains("bridgePort: 19222"));
    }

    #[test]
    fn sw_filename_filter_matches_actionbook_only() {
        // Actionbook extension: background.js → should match
        let actionbook_url = "chrome-extension://abcdef123456/background.js";
        let sw_suffix = format!("/{}", ACTIONBOOK_SW_FILENAME);
        assert!(
            actionbook_url.starts_with("chrome-extension://") && actionbook_url.ends_with(&sw_suffix),
            "Actionbook SW URL should match the filter"
        );

        // Third-party extension with different SW filename → should NOT match
        let other_url = "chrome-extension://xyz789/service-worker.js";
        assert!(
            !(other_url.starts_with("chrome-extension://") && other_url.ends_with(&sw_suffix)),
            "Non-Actionbook SW URL should not match the filter"
        );

        // Another third-party with no path suffix → should NOT match
        let bare_url = "chrome-extension://xyz789/sw.js";
        assert!(
            !(bare_url.starts_with("chrome-extension://") && bare_url.ends_with(&sw_suffix)),
            "Non-Actionbook bare SW URL should not match the filter"
        );
    }
}
