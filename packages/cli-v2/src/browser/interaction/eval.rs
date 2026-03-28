use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Evaluate JavaScript
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// JavaScript expression
    pub expression: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.eval";

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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": cmd.expression, "returnByValue": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("EVAL_FAILED", e.to_string()),
    };

    // Extract value from CDP response
    if let Some(result) = resp.get("result").and_then(|r| r.get("result")) {
        if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
            let emsg = exc
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("expression error");
            return ActionResult::fatal("EVAL_FAILED", emsg.to_string());
        }
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
        ActionResult::ok(json!({ "value": value }))
    } else {
        ActionResult::fatal("EVAL_FAILED", "no result in CDP response")
    }
}
