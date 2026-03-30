use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Delete a cookie by name
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser cookies delete session_id --session s1")]
pub struct Cmd {
    /// Cookie name
    #[arg()]
    pub name: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.cookies.delete";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = {
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
        let cdp = match entry.cdp.clone() {
            Some(c) => c,
            None => {
                return ActionResult::fatal(
                    "INTERNAL_ERROR",
                    format!("no CDP connection for session '{}'", cmd.session),
                );
            }
        };
        let target_id = match entry.tabs.first() {
            Some(t) => t.native_id.clone(),
            None => {
                return ActionResult::fatal(
                    "NO_TAB",
                    format!("no active tab in session '{}'", cmd.session),
                );
            }
        };
        (cdp, target_id)
    };

    // Find all cookies matching the name so we can count deletions.
    let resp = match cdp
        .execute_on_tab(&target_id, "Network.getAllCookies", json!({}))
        .await
    {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("CDP_ERROR", e.to_string()),
    };

    let empty = vec![];
    let matching: Vec<_> = resp
        .pointer("/result/cookies")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|c| c.get("name").and_then(|v| v.as_str()) == Some(&cmd.name))
        .cloned()
        .collect();

    let mut deleted = 0u64;
    for cookie in &matching {
        let domain = cookie.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let path = cookie.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let params = json!({ "name": cmd.name, "domain": domain, "path": path });
        if cdp
            .execute_on_tab(&target_id, "Network.deleteCookies", params)
            .await
            .is_ok()
        {
            deleted += 1;
        }
    }

    ActionResult::ok(json!({
        "action": "delete",
        "affected": deleted,
    }))
}
