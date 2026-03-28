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

    let tabs: Vec<serde_json::Value> = entry
        .tabs
        .iter()
        .map(|t| {
            let native_tab_id: serde_json::Value = if t.target_id.is_empty() {
                serde_json::Value::Null
            } else {
                json!(t.target_id)
            };
            json!({
                "tab_id": t.id.to_string(),
                "url": t.url,
                "title": t.title,
                "native_tab_id": native_tab_id,
            })
        })
        .collect();

    ActionResult::ok(json!({
        "total_tabs": tabs.len(),
        "tabs": tabs,
    }))
}
