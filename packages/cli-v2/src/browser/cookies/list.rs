use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::{map_cookie, normalize_domain};

/// List all cookies for a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser cookies list --session s1
  actionbook browser cookies list --session s1 --domain example.com")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Filter cookies by domain
    #[arg(long)]
    pub domain: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.cookies.list";

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

    let resp = match cdp
        .execute_on_tab(&target_id, "Network.getAllCookies", json!({}))
        .await
    {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("CDP_ERROR", e.to_string()),
    };

    let empty = vec![];
    let raw = resp
        .pointer("/result/cookies")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let items: Vec<_> = raw
        .iter()
        .map(map_cookie)
        .filter(|c| {
            if let Some(ref filter_domain) = cmd.domain {
                let cookie_domain = c.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                normalize_domain(cookie_domain) == normalize_domain(filter_domain)
            } else {
                true
            }
        })
        .collect();

    ActionResult::ok(json!({ "items": items }))
}
