use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Restart a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser restart --session my-session

Closes and reopens the session with the same profile and mode.
The session_id is preserved; tab IDs reset to t1.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser restart";

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
    let (mode, headless, stealth, profile, open_url, cdp_endpoint, headers, cdp, chrome_process);
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
        stealth = entry.stealth;
        profile = entry.profile.clone();
        open_url = entry.tabs.first().map(|t| t.url.clone());
        cdp_endpoint = entry.cdp_endpoint.clone();
        headers = entry
            .headers
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>();
        cdp = entry.cdp.take();
        chrome_process = entry.chrome_process.take();

        reg.clear_session_ref_caches(&cmd.session);
    }
    // Registry lock released — slow cleanup below won't block other sessions.

    if let Some(cdp) = cdp {
        cdp.clear_iframe_sessions().await;
        cdp.close().await;
    }
    if let Some(child) = chrome_process {
        crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
    }

    let start_cmd = super::start::Cmd {
        mode: Some(mode),
        // Restart preserves the session's effective runtime settings and
        // intentionally does not re-run config/env resolution.
        headless: Some(headless),
        profile: Some(profile),
        executable_path: None,
        open_url,
        cdp_endpoint,
        header: headers,
        session: None,
        set_session_id: Some(cmd.session.clone()),
        stealth,
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
