use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::map_cookie;

/// Get a single cookie by name
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser cookies get session_id --session s1")]
pub struct Cmd {
    /// Cookie name
    #[arg()]
    pub name: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.cookies.get";

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

    let item: Value = raw
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(&cmd.name))
        .map(map_cookie)
        .unwrap_or(Value::Null);

    ActionResult::ok(json!({ "item": item }))
}
