use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::normalize_domain;

/// Clear cookies (optionally filtered by domain)
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser cookies clear --session s1
  actionbook browser cookies clear --session s1 --domain example.com")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Clear only cookies matching this domain
    #[arg(long)]
    pub domain: Option<String>,
}

pub const COMMAND_NAME: &str = "browser cookies clear";

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

    // Get all cookies to determine what to delete (needed for count + domain filter).
    let resp = match cdp
        .execute_on_tab(&target_id, "Network.getAllCookies", json!({}))
        .await
    {
        Ok(v) => v,
        Err(e) => return ActionResult::fatal("CDP_ERROR", e.to_string()),
    };

    let empty = vec![];
    let candidates: Vec<_> = resp
        .pointer("/result/cookies")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|c| {
            if let Some(ref filter_domain) = cmd.domain {
                let d = c.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                normalize_domain(d) == normalize_domain(filter_domain)
            } else {
                true
            }
        })
        .cloned()
        .collect();

    let mut cleared = 0u64;
    for cookie in &candidates {
        let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let domain = cookie.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let path = cookie.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let params = json!({ "name": name, "domain": domain, "path": path });
        if cdp
            .execute_on_tab(&target_id, "Network.deleteCookies", params)
            .await
            .is_ok()
        {
            cleared += 1;
        }
    }

    let domain_val: Value = cmd
        .domain
        .as_deref()
        .map(|d| Value::String(d.to_string()))
        .unwrap_or(Value::Null);

    ActionResult::ok(json!({
        "action": "clear",
        "affected": cleared,
        "domain": domain_val,
    }))
}
