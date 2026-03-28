use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Go back in browser history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.back";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    // SESSION_NOT_FOUND: context must be null per §3.1
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("to_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("title").and_then(|v| v.as_str()).map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Get navigation history to check if going back is possible
    let history = match cdp
        .execute_on_tab(&target_id, "Page.getNavigationHistory", json!({}))
        .await
    {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("NAVIGATION_FAILED", e.to_string()),
    };

    let current_index = history["result"]["currentIndex"].as_i64().unwrap_or(0) as usize;
    let entries = history["result"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if current_index == 0 {
        return ActionResult::fatal("NAVIGATION_FAILED", "no previous page in history");
    }

    let from_url = entries[current_index]["url"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let target_entry = &entries[current_index - 1];
    let to_url = target_entry["url"].as_str().unwrap_or("").to_string();
    let title = target_entry["title"].as_str().unwrap_or("").to_string();
    let entry_id = target_entry["id"].as_i64().unwrap_or(0);

    if let Err(e) = cdp
        .execute_on_tab(
            &target_id,
            "Page.navigateToHistoryEntry",
            json!({ "entryId": entry_id }),
        )
        .await
    {
        return ActionResult::fatal("NAVIGATION_FAILED", e.to_string());
    }

    // Clear snapshot RefCache — page changed
    {
        let mut reg = registry.lock().await;
        reg.clear_ref_cache(&cmd.session, &cmd.tab);
    }

    ActionResult::ok(json!({
        "kind": "back",
        "requested_url": null,
        "from_url": from_url,
        "to_url": to_url,
        "title": title,
    }))
}
