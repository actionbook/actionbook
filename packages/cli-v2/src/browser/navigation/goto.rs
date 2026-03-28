use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme;
use crate::daemon::cdp_session::get_cdp_and_target;
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

pub fn context(cmd: &Cmd, _result: &ActionResult) -> Option<ResponseContext> {
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let final_url = ensure_scheme(&cmd.url);

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    if !target_id.is_empty() {
        if let Err(e) = cdp
            .execute_on_tab(
                &target_id,
                "Page.navigate",
                json!({ "url": final_url }),
            )
            .await
        {
            return ActionResult::fatal("NAVIGATION_FAILED", e.to_string());
        }
    }

    ActionResult::ok(json!({
        "kind": "goto",
        "to_url": final_url,
    }))
}
