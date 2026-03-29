use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme_or_fatal;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::{SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::TabId;

/// Open a new tab
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser new-tab https://example.com --session my-session
  actionbook browser open https://github.com --session my-session

The new tab is assigned the next available ID (t2, t3, ...).
Use the returned tab_id to address this tab in subsequent commands.")]
pub struct Cmd {
    /// URL to open
    pub url: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Open in new window
    #[arg(long)]
    pub new_window: bool,
    /// Window ID
    #[arg(long)]
    pub window: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.new-tab";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Ok { data } = result {
        Some(ResponseContext {
            session_id: cmd.session.clone(),
            tab_id: data["tab"]["tab_id"].as_str().map(|s| s.to_string()),
            window_id: None,
            url: data["tab"]["url"].as_str().map(|s| s.to_string()),
            title: data["tab"]["title"].as_str().map(|s| s.to_string()),
        })
    } else {
        None
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let final_url = match ensure_scheme_or_fatal(&cmd.url) {
        Ok(u) => u,
        Err(e) => return e,
    };

    // Get CdpSession from registry
    let cdp = {
        let reg = registry.lock().await;
        match reg.get(&cmd.session) {
            Some(e) => match e.cdp.clone() {
                Some(c) => c,
                None => {
                    return ActionResult::fatal_with_hint(
                        "INTERNAL_ERROR",
                        format!("no CDP connection for session '{}'", cmd.session),
                        "try restarting the session",
                    );
                }
            },
            None => {
                return ActionResult::fatal_with_hint(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' not found", cmd.session),
                    "run `actionbook browser list-sessions` to see available sessions",
                );
            }
        }
    };

    // Create tab via CDP Target.createTarget (works for both local and cloud)
    let resp = match cdp
        .execute_browser("Target.createTarget", json!({ "url": final_url }))
        .await
    {
        Ok(r) => r,
        Err(e) => return cdp_error_to_result(e, "CDP_ERROR"),
    };
    let target_id = match resp.pointer("/result/targetId").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return ActionResult::fatal(
                "CDP_ERROR",
                format!("Target.createTarget did not return targetId: {}", resp),
            );
        }
    };

    // Attach before registering — rollback on failure
    if let Err(e) = cdp.attach(&target_id).await {
        // Rollback: close the target we just created
        let _ = cdp
            .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
            .await;
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Register the new tab
    {
        let mut reg = registry.lock().await;
        match reg.get_mut(&cmd.session) {
            Some(e) => {
                e.tabs.push(TabEntry {
                    id: TabId(target_id.clone()),
                    url: final_url.clone(),
                    title: String::new(),
                });
            }
            None => {
                // Session was closed concurrently — detach and close the target
                let _ = cdp.detach(&target_id).await;
                let _ = cdp
                    .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
                    .await;
                return ActionResult::fatal(
                    "SESSION_NOT_FOUND",
                    format!("session '{}' was closed during tab creation", cmd.session),
                );
            }
        }
    }

    ActionResult::ok(json!({
        "tab": {
            "tab_id": target_id,
            "url": final_url,
            "title": "",
        },
        "created": true,
        "new_window": cmd.new_window,
    }))
}
