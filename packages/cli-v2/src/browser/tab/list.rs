use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// List tabs in a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.list-tabs";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    match result {
        ActionResult::Ok { .. } => Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: None,
            window_id: None,
            url: None,
            title: None,
        }),
        _ => None,
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let cdp = {
        let reg = registry.lock().await;
        let entry = match reg.get(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };
        match entry.cdp.clone() {
            Some(c) => c,
            None => {
                return ActionResult::fatal(
                    "INTERNAL_ERROR",
                    format!("no CDP connection for session '{}'", cmd.session),
                );
            }
        }
    };

    // Real-time fetch via CDP Target.getTargets (unified for local + cloud)
    let targets = match cdp.execute_browser("Target.getTargets", json!({})).await {
        Ok(resp) => resp,
        Err(e) => {
            return crate::daemon::cdp_session::cdp_error_to_result(e, "CDP_ERROR");
        }
    };

    let target_infos = targets
        .get("result")
        .and_then(|r| r.get("targetInfos"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    // Filter to page targets only, merge with registry for consistency
    let tabs: Vec<serde_json::Value> = {
        let reg = registry.lock().await;
        let entry = match reg.get(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        };

        entry
            .tabs
            .iter()
            .map(|t| {
                let target_id = &t.id.0;
                // Find real-time url/title from CDP targetInfos
                let (url, title) = target_infos
                    .iter()
                    .find(|tgt| {
                        tgt.get("targetId").and_then(|v| v.as_str()) == Some(target_id)
                    })
                    .map(|tgt| {
                        let url = tgt.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        let title = tgt.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        (url.to_string(), title.to_string())
                    })
                    .unwrap_or_default();

                json!({
                    "tab_id": target_id,
                    "url": url,
                    "title": title,
                })
            })
            .collect()
    };

    ActionResult::ok(json!({
        "total_tabs": tabs.len(),
        "tabs": tabs,
    }))
}
