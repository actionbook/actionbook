use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme_or_fatal;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Navigate to URL
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Target URL
    pub url: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.goto";

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
    let final_url = match ensure_scheme_or_fatal(&cmd.url) {
        Ok(u) => u,
        Err(e) => return e,
    };

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Get from_url before navigation
    let from_url = super::get_tab_url(&cdp, &target_id).await;

    if !target_id.is_empty() {
        match cdp
            .execute_on_tab(&target_id, "Page.navigate", json!({ "url": final_url }))
            .await
        {
            Err(e) => return cdp_error_to_result(e, "NAVIGATION_FAILED"),
            Ok(v) => {
                // CDP may return a success response but with errorText indicating the
                // navigation failed (e.g. unrecognized scheme, ERR_ABORTED).
                if let Some(err_text) = v["result"]["errorText"].as_str()
                    && !err_text.is_empty()
                {
                    return ActionResult::fatal("NAVIGATION_FAILED", err_text.to_string());
                }
            }
        }
    }

    // Get to_url and title after navigation
    let to_url = super::get_tab_url(&cdp, &target_id).await;
    let title = super::get_tab_title(&cdp, &target_id).await;

    // Clear snapshot RefCache — page changed, old backendNodeIds are invalid
    {
        let mut reg = registry.lock().await;
        reg.clear_ref_cache(&cmd.session, &cmd.tab);
    }

    ActionResult::ok(json!({
        "kind": "goto",
        "requested_url": cmd.url,
        "from_url": from_url,
        "to_url": to_url,
        "title": title,
    }))
}
