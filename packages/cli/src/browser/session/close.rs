use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;
use crate::types::Mode;

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

pub const COMMAND_NAME: &str = "browser close";

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
    // Extract everything from registry then release the lock before slow I/O.
    let (closed_tabs, cdp, chrome_process, profile_to_clean, mode) = {
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
        let entry_mode = entry.mode;

        // Only delete non-default profile directories for local sessions.
        // The default profile ("actionbook") is long-lived and preserves
        // user state (cookies, localStorage) across sessions.
        let profile =
            if entry.chrome_process.is_some() && entry.profile != crate::config::DEFAULT_PROFILE {
                Some(entry.profile.clone())
            } else {
                None
            };

        reg.clear_session_ref_caches(&cmd.session);
        (
            tabs,
            entry.cdp.take(),
            entry.chrome_process.take(),
            profile,
            entry_mode,
        )
    };
    // Registry lock released here — slow I/O below won't block other sessions.

    // Extension mode: detach debugger before tearing down the CDP connection.
    // Extension mode doesn't own the browser — we only release the debugger,
    // leaving tabs open for the user.
    if mode == Mode::Extension
        && let Some(ref cdp) = cdp
        && let Err(e) = cdp
            .execute_browser("Extension.detachTab", serde_json::json!({}))
            .await
    {
        tracing::warn!("extension: failed to detach: {e}");
    }

    // Close CDP session AFTER extension cleanup is complete.
    if let Some(cdp) = cdp {
        cdp.clear_iframe_sessions().await;
        cdp.close().await;
    }

    if let Some(child) = chrome_process {
        crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
    }

    // Remove non-default profile directory after Chrome has fully exited.
    if let Some(profile) = profile_to_clean {
        let profile_dir = crate::config::profiles_dir().join(&profile);
        // Remove chrome.pid so a future browser start does not mistake the
        // now-dead PID for an orphan.
        let _ = std::fs::remove_file(profile_dir.join("chrome.pid"));
        if profile_dir.exists() {
            let _ = std::fs::remove_dir_all(&profile_dir);
        }
    }

    // Remove per-session data directory (snapshots, etc.).
    // Safety: only delete if the path is an absolute path under sessions_dir().
    let sessions_base = crate::config::sessions_dir();
    let session_data_dir = sessions_base.join(&cmd.session);
    if session_data_dir.is_absolute()
        && session_data_dir.starts_with(&sessions_base)
        && session_data_dir.exists()
    {
        let _ = std::fs::remove_dir_all(&session_data_dir);
    }

    ActionResult::ok(json!({
        "session_id": cmd.session,
        "status": "closed",
        "closed_tabs": closed_tabs,
    }))
}
