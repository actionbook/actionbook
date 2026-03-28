use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// List all active sessions
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {}

pub const COMMAND_NAME: &str = "browser.list-sessions";

pub fn context(_cmd: &Cmd, _result: &ActionResult) -> Option<ResponseContext> {
    None
}

pub async fn execute(_cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let reg = registry.lock().await;
    let sessions: Vec<serde_json::Value> = reg
        .list()
        .iter()
        .map(|s| {
            json!({
                "session_id": s.id.as_str(),
                "mode": s.mode.to_string(),
                "status": s.status.to_string(),
                "headless": s.headless,
                "tabs_count": s.tabs_count(),
            })
        })
        .collect();
    ActionResult::ok(json!({
        "total_sessions": sessions.len(),
        "sessions": sessions,
    }))
}
