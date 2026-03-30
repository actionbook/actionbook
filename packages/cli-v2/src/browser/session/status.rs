use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Show session status
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser status --session my-session
  actionbook browser status --session my-session --json

Returns mode, status, tab count, and lists all tabs with their URLs.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.status";

pub fn context(cmd: &Cmd, _result: &ActionResult) -> Option<ResponseContext> {
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
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
    let tabs: Vec<serde_json::Value> = entry
        .tabs
        .iter()
        .map(|t| {
            json!({
                "tab_id": t.id.to_string(),
                "native_tab_id": t.native_id,
                "url": t.url,
                "title": t.title,
            })
        })
        .collect();
    let mut session = json!({
        "session_id": entry.id.as_str(),
        "mode": entry.mode.to_string(),
        "status": entry.status.to_string(),
        "headless": entry.headless,
        "tabs_count": entry.tabs_count(),
    });
    // Include cdp_endpoint for cloud sessions (redacted), never expose headers
    if let Some(ref ep) = entry.cdp_endpoint {
        session["cdp_endpoint"] = json!(crate::browser::session::start::redact_endpoint(ep));
    }
    ActionResult::ok(json!({
        "session": session,
        "tabs": tabs,
        "capabilities": {
            "snapshot": true,
            "pdf": true,
            "upload": true,
        },
    }))
}
