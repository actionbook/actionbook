use std::time::{Duration, Instant};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation as nav_helpers;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const POLL_INTERVAL_MS: u64 = 100;

/// Wait for a navigation to complete
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait navigation --session s1 --tab t1 --timeout 10000")]
pub struct Cmd {
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

pub const COMMAND_NAME: &str = "browser wait navigation";

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

    // Record the URL at the moment the wait command starts so we can detect a change.
    let initial_url = nav_helpers::get_tab_url(&cdp, &target_id).await;

    // Poll until URL changes and the new page reaches readyState=complete.
    //
    // Two-phase detection:
    //   Phase 1 — wait for URL to differ from initial_url (navigation started).
    //   Phase 2 — once URL changed, wait for readyState=complete (page loaded).
    //
    // This avoids a false-positive on the first poll when the original page
    // already has readyState=complete but navigation hasn't fired yet.
    let js = r#"(function(){
        return { url: location.href, ready_state: document.readyState };
    })()"#;

    let mut url_changed_seen = false;

    loop {
        let resp = cdp
            .execute_on_tab(
                &target_id,
                "Runtime.evaluate",
                json!({ "expression": js, "returnByValue": true }),
            )
            .await;

        if let Ok(v) = resp {
            let result_val = v.pointer("/result/result/value");
            if let Some(rv) = result_val {
                let current_url = rv.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let ready_state = rv.get("ready_state").and_then(|v| v.as_str()).unwrap_or("");

                if current_url != initial_url.as_str() {
                    url_changed_seen = true;
                }

                // Return once URL has changed and new page is fully loaded.
                if url_changed_seen && ready_state == "complete" {
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    let title = nav_helpers::get_tab_title(&cdp, &target_id).await;
                    return ActionResult::ok(json!({
                        "kind": "navigation",
                        "satisfied": true,
                        "elapsed_ms": elapsed_ms,
                        "observed_value": {
                            "url": current_url,
                            "ready_state": ready_state,
                        },
                        "__ctx_url": current_url,
                        "__ctx_title": title,
                    }));
                }
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= timeout_ms {
            return ActionResult::fatal_with_hint(
                "TIMEOUT",
                format!("navigation not detected within {}ms", timeout_ms),
                "check that navigation is triggered or increase --timeout",
            );
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
