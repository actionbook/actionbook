use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::browser;
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
    let cdp_port = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => e.cdp_port,
            None => {
                return ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                );
            }
        }
    };

    // Real-time fetch from Chrome
    let targets = browser::list_targets(cdp_port).await.unwrap_or_default();

    let tabs: Vec<serde_json::Value> = {
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

        entry
            .tabs
            .iter()
            .map(|t| {
                let target_id = &t.id.0;
                // Find real-time url/title from Chrome targets
                let (url, title) = targets
                    .iter()
                    .find(|tgt| {
                        tgt.get("id").and_then(|v| v.as_str()) == Some(target_id)
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
