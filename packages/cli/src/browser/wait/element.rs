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

/// Wait for a CSS selector to appear in the DOM
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait element '#loaded' --session s1 --tab t1 --timeout 5000
  actionbook browser wait element '.spinner[hidden]' --session s1 --tab t1")]
pub struct Cmd {
    /// Selector to wait for (CSS, XPath, or @ref)
    pub selector: String,
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

pub const COMMAND_NAME: &str = "browser wait element";

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
    let selector_json = serde_json::to_string(&cmd.selector).unwrap_or_default();
    let js = format!("!!document.querySelector({selector_json})");
    let start = Instant::now();

    loop {
        let resp = cdp
            .execute_on_tab(
                &target_id,
                "Runtime.evaluate",
                json!({ "expression": js, "returnByValue": true }),
            )
            .await;

        if let Ok(v) = resp {
            let found = v
                .pointer("/result/result/value")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if found {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let url = navigation::get_tab_url(&cdp, &target_id).await;
                let title = navigation::get_tab_title(&cdp, &target_id).await;
                return ActionResult::ok(json!({
                    "kind": "element",
                    "satisfied": true,
                    "elapsed_ms": elapsed_ms,
                    "observed_value": { "selector": cmd.selector },
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
                    "element '{}' not found within {}ms",
                    cmd.selector, timeout_ms
                ),
                "check selector or increase --timeout",
            );
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
