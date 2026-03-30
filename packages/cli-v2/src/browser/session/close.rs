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
    let (closed_tabs, profile_name, chrome_process) = {
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

        // Drop CDP session to close WebSocket connection (important for cloud
        // single-connection providers — frees the slot for reconnection)
        drop(entry.cdp.take());

        // Only clean up profile directory for local sessions (those with a
        // Chrome child process). Cloud sessions don't own a local profile.
        let profile = if entry.chrome_process.is_some() {
            entry.profile.clone()
        } else {
            String::new()
        };

        reg.clear_session_ref_caches(&cmd.session);

        (tabs, profile, entry.chrome_process.take())
    };
    // Registry lock released here — slow I/O below won't block other sessions.

    if let Some(mut child) = chrome_process {
        let _ = child.kill();
        tokio::task::spawn_blocking(move || {
            let _ = child.wait();
        });
    }

    // Remove the Chrome profile directory to avoid disk accumulation.
    // Best-effort — Chrome may still hold locks briefly after kill.
    if !profile_name.is_empty() {
        let profile_dir = crate::config::profiles_dir().join(&profile_name);
        if profile_dir.exists() {
            let _ = std::fs::remove_dir_all(&profile_dir);
        }
    }

    ActionResult::ok(json!({
        "session_id": cmd.session,
        "status": "closed",
        "closed_tabs": closed_tabs,
    }))
}
