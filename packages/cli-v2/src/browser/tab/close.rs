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
    let cdp_port;
    let cdp;

    {
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

        cdp_port = entry.cdp_port;
        cdp = entry.cdp.clone();
    }

    // Detach from the persistent CDP session before closing
    if let Some(ref cdp) = cdp {
        let _ = cdp.detach(&cmd.tab).await;
    }

    // Close the CDP target
    let close_url = format!(
        "http://127.0.0.1:{}/json/close/{}",
        cdp_port, cmd.tab
    );
    let _ = reqwest::Client::new().put(&close_url).send().await;

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
