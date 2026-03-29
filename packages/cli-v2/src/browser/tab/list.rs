use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// List tabs in a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser list-tabs --session my-session
  actionbook browser list-tabs --session my-session --json

Returns each tab's ID (t1, t2, ...), URL, and title.")]
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
    // Get CdpSession from registry
    let cdp = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => match e.cdp.clone() {
                Some(c) => c,
                None => {
                    return ActionResult::fatal_with_hint(
                        "INTERNAL_ERROR",
                        format!("no CDP connection for session '{}'", cmd.session),
                        "try restarting the session",
                    );
                }
            },
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        }
    };

    // Real-time fetch via CDP Target.getTargets (works for both local and cloud)
    let resp = match cdp.execute_browser("Target.getTargets", json!({})).await {
        Ok(r) => r,
        Err(e) => return cdp_error_to_result(e, "CDP_CONNECTION_FAILED"),
    };

    let target_infos = resp
        .pointer("/result/targetInfos")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Filter by type=="page" and cross-reference with registry
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
            .filter_map(|t| {
                let target_id = &t.id.0;
                target_infos
                    .iter()
                    .find(|tgt| {
                        tgt.get("targetId").and_then(|v| v.as_str()) == Some(target_id)
                            && tgt.get("type").and_then(|v| v.as_str()) == Some("page")
                    })
                    .map(|tgt| {
                        let url = tgt.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        let title = tgt.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        json!({
                            "tab_id": target_id,
                            "url": url,
                            "title": title,
                        })
                    })
            })
            .collect()
    };

    ActionResult::ok(json!({
        "total_tabs": tabs.len(),
        "tabs": tabs,
    }))
}
