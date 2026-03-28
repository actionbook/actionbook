use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Close a tab
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.close-tab";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    match result {
        ActionResult::Ok { .. } => Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: Some(cmd.tab.clone()),
            window_id: None,
            url: None,
            title: None,
        }),
        ActionResult::Fatal { code, .. } => {
            // §4: return context.session_id as long as the session has been located
            if code == "TAB_NOT_FOUND" {
                Some(ResponseContext {
                    session_id: cmd.session.clone(),
                    tab_id: None,
                    window_id: None,
                    url: None,
                    title: None,
                })
            } else {
                // SESSION_NOT_FOUND: session not located, no context
                None
            }
        }
        _ => None,
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let cdp = {
        let reg = registry.lock().await;
        let entry = match reg.get(&cmd.session) {
            Some(e) => e,
            None => {
                return ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                );
            }
        };

        if !entry.tabs.iter().any(|t| t.id.0 == cmd.tab) {
            return ActionResult::fatal(
                "TAB_NOT_FOUND",
                format!("tab '{}' not found in session '{}'", cmd.tab, cmd.session),
            );
        }

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

    // Close the target first via CDP (unified for local + cloud)
    // Treat "target not found" as success (idempotent — target already gone)
    match cdp
        .execute_browser("Target.closeTarget", json!({ "targetId": cmd.tab }))
        .await
    {
        Ok(resp) => {
            // Check result.success boolean — some targets may decline closure
            if resp.get("result").and_then(|r| r.get("success")).and_then(|v| v.as_bool()) == Some(false) {
                return ActionResult::fatal(
                    "CDP_ERROR",
                    format!("Target.closeTarget returned success=false for tab '{}'", cmd.tab),
                );
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("No target with given id") && !msg.contains("Target closed") {
                return ActionResult::fatal(
                    "CDP_ERROR",
                    format!("Target.closeTarget failed: {e}"),
                );
            }
            // Target already gone — proceed with cleanup
        }
    }

    // Then detach (cleanup session mapping) — ignore errors since target is already gone
    let _ = cdp.detach(&cmd.tab).await;

    // Remove from registry
    {
        let mut reg = registry.lock().await;
        if let Some(entry) = reg.get_mut(&cmd.session) {
            entry.tabs.retain(|t| t.id.0 != cmd.tab);
        }
    }

    ActionResult::ok(json!({
        "closed_tab_id": cmd.tab,
    }))
}
