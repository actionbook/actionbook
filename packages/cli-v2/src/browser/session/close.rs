use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Close a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser close --session my-session

Closes the browser and all tabs in the session. The session ID cannot be reused.")]
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
    // Extract everything we need from the registry, then release the lock
    // before any slow I/O (Chrome kill, profile deletion).
    // Extract everything from registry then release the lock before slow I/O.
    let (closed_tabs, cdp, chrome_process) = {
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
        let tabs = entry.tabs_count();
        reg.clear_session_ref_caches(&cmd.session);
        (tabs, entry.cdp.take(), entry.chrome_process.take())
    };
    // Registry lock released here — slow I/O below won't block other sessions.

    // Close CDP session (shuts down background tasks, frees cloud connection slot).
    if let Some(cdp) = cdp {
        cdp.close().await;
    }

    if let Some(child) = chrome_process {
        crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
    }

    ActionResult::ok(json!({
        "session_id": cmd.session,
        "status": "closed",
        "closed_tabs": closed_tabs,
    }))
}
