use std::time::{Duration, Instant};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const POLL_INTERVAL_MS: u64 = 100;

/// Wait for a JavaScript expression to become truthy
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait condition 'window.__ready === true' --session s1 --tab t1 --timeout 5000")]
pub struct Cmd {
    /// JavaScript expression to evaluate (must become truthy)
    pub expression: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Timeout in milliseconds (default 30000)
    #[arg(long)]
    pub timeout: Option<u64>,
}

pub const COMMAND_NAME: &str = "browser wait condition";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let timeout_ms = cmd.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);
    let start = Instant::now();

    loop {
        let resp = cdp
            .execute_on_tab(
                &target_id,
                "Runtime.evaluate",
                json!({ "expression": &cmd.expression, "returnByValue": true }),
            )
            .await;

        if let Ok(v) = resp {
            let result_val = v.pointer("/result/result/value").cloned();
            let truthy = result_val
                .as_ref()
                .map(|rv| match rv {
                    serde_json::Value::Bool(b) => *b,
                    serde_json::Value::Null => false,
                    serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
                    serde_json::Value::String(s) => !s.is_empty(),
                    // Arrays and objects are always truthy in JS, even when empty.
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => true,
                })
                .unwrap_or(false);

            if truthy {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let url = navigation::get_tab_url(&cdp, &target_id).await;
                let title = navigation::get_tab_title(&cdp, &target_id).await;
                // observed_value is the raw JS result
                let observed = result_val.unwrap_or(serde_json::Value::Bool(true));
                return ActionResult::ok(json!({
                    "kind": "condition",
                    "satisfied": true,
                    "elapsed_ms": elapsed_ms,
                    "observed_value": observed,
                    "__ctx_url": url,
                    "__ctx_title": title,
                }));
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= timeout_ms {
            return ActionResult::fatal_with_hint(
                "TIMEOUT",
                format!(
                    "condition '{}' was not satisfied within {}ms",
                    cmd.expression, timeout_ms
                ),
                "check the expression or increase --timeout",
            );
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
