use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Close a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.close";

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
    let mut reg = registry.lock().await;
    let mut entry = match reg.remove(&cmd.session) {
        Some(e) => e,
        None => {
            return ActionResult::fatal_with_hint(
                "SESSION_NOT_FOUND",
                format!("session '{}' not found", cmd.session),
                "run `actionbook browser list-sessions` to see available sessions",
            );
        }
    };
    let closed_tabs = entry.tabs_count();

    // Drop CDP session to close WebSocket connection (important for cloud
    // single-connection providers — frees the slot for reconnection)
    drop(entry.cdp.take());

    if let Some(mut child) = entry.chrome_process.take() {
        let _ = child.kill();
        tokio::task::spawn_blocking(move || {
            let _ = child.wait();
        });
    }

    // Clean up snapshot RefCaches for this session
    reg.clear_session_ref_caches(&cmd.session);

    ActionResult::ok(json!({
        "session_id": cmd.session,
        "status": "closed",
        "closed_tabs": closed_tabs,
    }))
}
