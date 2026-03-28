use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Restart a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.restart";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    let mut ctx = ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    };
    if let ActionResult::Ok { data } = result {
        if let Some(tab_id) = data
            .pointer("/session/tab_id")
            .or_else(|| data.pointer("/tab/tab_id"))
            .and_then(|v| v.as_str())
        {
            ctx.tab_id = Some(tab_id.to_string());
        } else {
            ctx.tab_id = Some("t1".to_string());
        }
    }
    Some(ctx)
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (mode, headless, profile, open_url);
    {
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
        mode = entry.mode;
        headless = entry.headless;
        profile = entry.profile.clone();
        open_url = entry.tabs.first().map(|t| t.url.clone());

        if let Some(ref mut child) = entry.chrome_process {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    let start_cmd = super::start::Cmd {
        mode: Some(mode),
        // Restart preserves the session's effective runtime settings and
        // intentionally does not re-run config/env resolution.
        headless: Some(headless),
        profile: Some(profile),
        executable: None,
        open_url,
        cdp_endpoint: None,
        header: None,
        set_session_id: Some(cmd.session.clone()),
    };

    let result = super::start::execute(&start_cmd, registry).await;

    match result {
        ActionResult::Ok { data } => {
            let mut session = data.get("session").cloned().unwrap_or(json!({}));
            if session.get("tabs_count").is_none() {
                session["tabs_count"] = json!(1);
            }
            ActionResult::ok(json!({
                "session": session,
                "reopened": true,
            }))
        }
        other => other,
    }
}
