use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Reload the current page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.reload";

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

    // Get current URL and title before reload (from_url == to_url for reload)
    let url = super::get_tab_url(&cdp, &target_id).await;
    let title = super::get_tab_title(&cdp, &target_id).await;

    if let Err(e) = cdp
        .execute_on_tab(&target_id, "Page.reload", json!({}))
        .await
    {
        return ActionResult::fatal("NAVIGATION_FAILED", e.to_string());
    }

    // Clear snapshot RefCache — page reloaded, old backendNodeIds may be invalid
    {
        let mut reg = registry.lock().await;
        reg.clear_ref_cache(&cmd.session, &cmd.tab);
    }

    ActionResult::ok(json!({
        "kind": "reload",
        "requested_url": null,
        "from_url": url,
        "to_url": url,
        "title": title,
    }))
}
