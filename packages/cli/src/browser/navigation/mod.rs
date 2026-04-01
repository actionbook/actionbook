pub mod back;
pub mod forward;
pub mod goto;
pub mod reload;

use crate::daemon::cdp_session::CdpSession;
use serde_json::json;

/// Get the current URL of a tab via Runtime.evaluate.
pub async fn get_tab_url(cdp: &CdpSession, target_id: &str) -> String {
    cdp.execute_on_tab(
        target_id,
        "Runtime.evaluate",
        json!({"expression": "document.URL", "returnByValue": true}),
    )
    .await
    .ok()
    .and_then(|v| v["result"]["result"]["value"].as_str().map(String::from))
    .unwrap_or_default()
}

/// Get the current title of a tab via Runtime.evaluate.
pub async fn get_tab_title(cdp: &CdpSession, target_id: &str) -> String {
    cdp.execute_on_tab(
        target_id,
        "Runtime.evaluate",
        json!({"expression": "document.title", "returnByValue": true}),
    )
    .await
    .ok()
    .and_then(|v| v["result"]["result"]["value"].as_str().map(String::from))
    .unwrap_or_default()
}
