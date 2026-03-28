use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme_or_fatal;
use crate::daemon::registry::{SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::TabId;

/// Open a new tab
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
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

    let cdp = {
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
        match entry.cdp.clone() {
            Some(c) => c,
            None => {
                return ActionResult::fatal(
                    "INTERNAL_ERROR",
                    format!("no CDP connection for session '{}'", cmd.session),
                );
            }
        }
    };

    // Create tab via CDP Target.createTarget (unified for local + cloud)
    let mut params = json!({ "url": final_url });
    if cmd.new_window {
        params["newWindow"] = json!(true);
    }
    let resp = match cdp.execute_browser("Target.createTarget", params).await {
        Ok(r) => r,
        Err(e) => {
            return ActionResult::fatal(
                "CDP_ERROR",
                format!("Target.createTarget failed: {e}"),
            );
        }
    };

    let target_id = match resp.get("result").and_then(|r| r.get("targetId")).and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return ActionResult::fatal(
                "CDP_ERROR",
                format!("Target.createTarget did not return targetId: {resp}"),
            );
        }
    };

    // Attach the new tab to the persistent CDP session
    if let Err(e) = cdp.attach(&target_id).await {
        // Rollback: close the orphaned tab in the browser
        let _ = cdp
            .execute_browser("Target.closeTarget", json!({ "targetId": target_id }))
            .await;
        return ActionResult::fatal(
            "CDP_ERROR",
            format!("failed to attach tab to CDP session: {e}"),
        );
    }

    // Add to registry
    {
        let mut reg = registry.lock().await;
        if let Some(entry) = reg.get_mut(&cmd.session) {
            entry.tabs.push(TabEntry {
                id: TabId(target_id.clone()),
                url: final_url.clone(),
                title: String::new(),
            });
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
