use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::cdp_error_to_result;
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
    let cdp;

    {
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

        if !entry.tabs.iter().any(|t| t.id.0 == cmd.tab) {
            return ActionResult::fatal_with_hint(
                "TAB_NOT_FOUND",
                format!("tab '{}' not found in session '{}'", cmd.tab, cmd.session),
                "run `actionbook browser list-tabs` to see available tabs",
            );
        }

        cdp = match entry.cdp.clone() {
            Some(c) => c,
            None => {
                return ActionResult::fatal_with_hint(
                    "INTERNAL_ERROR",
                    format!("no CDP connection for session '{}'", cmd.session),
                    "try restarting the session",
                );
            }
        };
    }

    // Detach from the persistent CDP session before closing
    let _ = cdp.detach(&cmd.tab).await;

    // Close via CDP Target.closeTarget (works for both local and cloud)
    match cdp
        .execute_browser("Target.closeTarget", json!({ "targetId": cmd.tab }))
        .await
    {
        Ok(resp) => {
            // Check result.success boolean
            let success = resp
                .pointer("/result/success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !success {
                tracing::warn!("Target.closeTarget returned success=false for {}", cmd.tab);
            }
        }
        Err(e) => {
            // Idempotent: if target is already gone, treat as success
            let msg = e.to_string();
            if !msg.contains("not found") && !msg.contains("No target") {
                return cdp_error_to_result(e, "CDP_ERROR");
            }
        }
    }

    // Remove from registry + clean up snapshot RefCache
    {
        let mut reg = registry.lock().await;
        if let Some(entry) = reg.get_mut(&cmd.session) {
            entry.tabs.retain(|t| t.id.0 != cmd.tab);
        }
        reg.clear_ref_cache(&cmd.session, &cmd.tab);
    }

    ActionResult::ok(json!({
        "closed_tab_id": cmd.tab,
    }))
}
