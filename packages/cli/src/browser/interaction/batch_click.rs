use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::click;

/// Click multiple elements in sequence (batch)
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser batch-click @e5 @e6 @e7 @e8 --session s1 --tab t1
  actionbook browser batch-click \"#card-1\" \"#card-2\" \"#card-3\" --session s1 --tab t1

Clicks each element sequentially. Stops on first failure.
Use this for expanding cards, toggling checkboxes, or any bulk DOM action.
Refs come from snapshot output (e.g. [ref=e5]).

Unlike 'click', batch-click skips per-click state detection (URL/focus
changes) for maximum throughput. Use 'click' when you need to know
whether a click triggered navigation.")]
pub struct Cmd {
    /// Snapshot refs or CSS selectors to click (2 or more)
    #[arg(num_args(2..))]
    pub selectors: Vec<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser batch-click";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    if cmd.selectors.len() < 2 {
        return ActionResult::fatal("INVALID_ARGUMENT", "batch-click requires at least 2 selectors");
    }

    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let mut results = Vec::new();
    for (i, selector) in cmd.selectors.iter().enumerate() {
        match click::execute_fast_click(selector, &mut ctx).await {
            Ok(()) => {
                results.push(json!({ "index": i, "selector": selector }));
            }
            Err(_) => {
                return ActionResult::fatal_with_details(
                    "BATCH_CLICK_ERROR",
                    format!("click failed at index {i} (selector: {selector})"),
                    format!(
                        "completed {}/{}, retry from index {i}",
                        results.len(),
                        cmd.selectors.len()
                    ),
                    json!({
                        "failed_index": i,
                        "failed_selector": selector,
                        "completed": results.len()
                    }),
                );
            }
        }
    }

    ActionResult::ok(json!({
        "action": "batch-click",
        "clicks": results.len(),
        "results": results,
    }))
}
