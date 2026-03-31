use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::browser::observation::logs_console::ENSURE_LOG_CAPTURE_JS;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Get error logs (window error events + unhandled rejections).
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser logs errors --session s1 --tab t1
  actionbook browser logs errors --source app.js --session s1 --tab t1
  actionbook browser logs errors --tail 5 --session s1 --tab t1
  actionbook browser logs errors --since err-3 --session s1 --tab t1
  actionbook browser logs errors --clear --session s1 --tab t1

Captures uncaught exceptions and unhandled promise rejections.
Use --source to filter by originating file. Use --since to poll for new entries.
Use --clear to reset the error buffer after retrieval.")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Filter by error source file
    #[arg(long)]
    pub source: Option<String>,
    /// Return only the last n entries
    #[arg(long)]
    pub tail: Option<u64>,
    /// Return only logs after the specified ID
    #[arg(long)]
    pub since: Option<String>,
    /// Clear logs after retrieval
    #[arg(long)]
    pub clear: bool,
}

pub const COMMAND_NAME: &str = "browser logs errors";

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

    let url = navigation::get_tab_url(&cdp, &target_id).await;
    let title = navigation::get_tab_title(&cdp, &target_id).await;

    // Install log capture hook (idempotent)
    if let Err(e) = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": ENSURE_LOG_CAPTURE_JS, "returnByValue": true }),
        )
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Build source filter JS
    let source_filter = match &cmd.source {
        Some(src) => {
            let src_json = serde_json::to_string(src).unwrap_or_else(|_| format!("\"{}\"", src));
            format!(".filter(function(e) {{ return e.source === {src_json}; }})")
        }
        None => String::new(),
    };

    // Build since filter JS (filter items with id seq > since id seq)
    let since_filter = match &cmd.since {
        Some(id) => {
            let id_json = serde_json::to_string(id).unwrap_or_else(|_| format!("\"{}\"", id));
            format!(
                ".filter(function(e) {{ return parseInt((e.id||'').split('-')[1]||'0') > parseInt(({id_json}).split('-')[1]||'0'); }})"
            )
        }
        None => String::new(),
    };

    let limit = cmd.tail.unwrap_or(200);
    let clear_stmt = if cmd.clear {
        "window.__ab_error_logs = [];"
    } else {
        ""
    };

    let js = format!(
        "(function() {{ if (!window.__ab_error_logs) {{ return []; }} var errors = window.__ab_error_logs{source_filter}{since_filter}.slice(-{limit}); {clear_stmt} return errors; }})()"
    );

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return cdp_error_to_result(e, "CDP_ERROR"),
    };

    let items = resp
        .pointer("/result/result/value")
        .cloned()
        .unwrap_or(json!([]));

    ActionResult::ok(json!({
        "items": items,
        "cleared": cmd.clear,
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
