use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::browser::observation::logs_console::ENSURE_LOG_CAPTURE_JS;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Evaluate JavaScript
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser eval \"document.title\" --session s1 --tab t1
  actionbook browser eval \"window.scrollY\" --session s1 --tab t1
  actionbook browser eval \"document.querySelectorAll('a').length\" --session s1 --tab t1

Evaluates a JavaScript expression in the page context and returns the result.
The expression is evaluated via Runtime.evaluate with returnByValue.")]
pub struct Cmd {
    /// JavaScript expression
    pub expression: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser eval";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("post_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            data.get("post_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
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

    // Install log capture hook before eval so console.* calls in the expression are captured.
    let _ = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": ENSURE_LOG_CAPTURE_JS, "returnByValue": true }),
        )
        .await;

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": cmd.expression, "returnByValue": true, "awaitPromise": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return crate::daemon::cdp_session::cdp_error_to_result(e, "EVAL_FAILED"),
    };

    // Extract value from CDP response
    if let Some(result) = resp.get("result").and_then(|r| r.get("result")) {
        if let Some(exc) = resp.get("result").and_then(|r| r.get("exceptionDetails")) {
            // Prefer exception.description (e.g. "Error: boom-eval"), fall back to text
            let emsg = exc
                .pointer("/exception/description")
                .and_then(|v| v.as_str())
                .or_else(|| exc.get("text").and_then(|v| v.as_str()))
                .unwrap_or("expression error");
            return ActionResult::fatal("EVAL_FAILED", emsg.to_string());
        }

        let js_type = result
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("undefined")
            .to_string();

        // Return the typed value as-is from CDP (number, bool, string, etc.)
        let value = result.get("value").cloned().unwrap_or(json!(null));

        let preview = result
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                if value.is_string() {
                    value.as_str().unwrap().to_string()
                } else {
                    value.to_string()
                }
            });

        let url = navigation::get_tab_url(&cdp, &target_id).await;
        let title = navigation::get_tab_title(&cdp, &target_id).await;

        ActionResult::ok(json!({
            "value": value,
            "type": js_type,
            "preview": preview,
            "post_url": url,
            "post_title": title,
        }))
    } else {
        ActionResult::fatal("EVAL_FAILED", "no result in CDP response")
    }
}
