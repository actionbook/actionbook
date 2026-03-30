use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// JS hook that monkey-patches console.* methods and listens for error/unhandledrejection events.
/// Idempotent: checks window.__ab_console_logs / window.__ab_error_logs before installing.
/// console.* calls → __ab_console_logs only.
/// Uncaught exceptions + unhandled rejections → __ab_error_logs only.
pub const ENSURE_LOG_CAPTURE_JS: &str = r#"(function() {
    if (typeof window.__ab_log_seq === 'undefined') { window.__ab_log_seq = 0; }
    if (typeof window.__ab_err_seq === 'undefined') { window.__ab_err_seq = 0; }
    if (!window.__ab_console_logs) {
        window.__ab_console_logs = [];
        var orig = {
            log: console.log,
            warn: console.warn,
            info: console.info,
            debug: console.debug,
            error: console.error
        };
        for (var level in orig) {
            (function(lvl, fn) {
                console[lvl] = function() {
                    var args = Array.prototype.slice.call(arguments);
                    window.__ab_console_logs.push({
                        id: 'log-' + (++window.__ab_log_seq),
                        level: lvl,
                        text: args.map(function(a) { return typeof a === 'object' ? JSON.stringify(a) : String(a); }).join(' '),
                        source: location.href || 'javascript',
                        timestamp_ms: Date.now()
                    });
                    fn.apply(console, args);
                };
            })(level, orig[level]);
        }
    }
    if (!window.__ab_error_logs) {
        window.__ab_error_logs = [];
        window.addEventListener('error', function(e) {
            window.__ab_error_logs.push({
                id: 'err-' + (++window.__ab_err_seq),
                level: 'error',
                text: e.message || '',
                source: e.filename || '',
                timestamp_ms: Date.now()
            });
        });
        window.addEventListener('unhandledrejection', function(e) {
            window.__ab_error_logs.push({
                id: 'err-' + (++window.__ab_err_seq),
                level: 'error',
                text: 'Unhandled rejection: ' + String(e.reason),
                source: location.href || '',
                timestamp_ms: Date.now()
            });
        });
    }
    return true;
})()"#;

/// Get console logs.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Filter by level (comma-separated, e.g. warn,error)
    #[arg(long)]
    pub level: Option<String>,
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

pub const COMMAND_NAME: &str = "browser.logs.console";

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

    // Build level filter JS
    let level_filter = match &cmd.level {
        Some(lvl) => {
            let levels: Vec<serde_json::Value> = lvl
                .split(',')
                .map(|s| serde_json::Value::String(s.trim().to_string()))
                .collect();
            let levels_json = serde_json::to_string(&levels).unwrap_or("[]".to_string());
            format!(".filter(function(l) {{ return {levels_json}.indexOf(l.level) !== -1; }})")
        }
        None => String::new(),
    };

    // Build since filter JS (filter items with id seq > since id seq)
    let since_filter = match &cmd.since {
        Some(id) => {
            let id_json = serde_json::to_string(id).unwrap_or_else(|_| format!("\"{}\"", id));
            format!(
                ".filter(function(l) {{ return parseInt((l.id||'').split('-')[1]||'0') > parseInt(({id_json}).split('-')[1]||'0'); }})"
            )
        }
        None => String::new(),
    };

    let limit = cmd.tail.unwrap_or(200);
    let clear_stmt = if cmd.clear {
        "window.__ab_console_logs = [];"
    } else {
        ""
    };

    let js = format!(
        "(function() {{ if (!window.__ab_console_logs) {{ return []; }} var logs = window.__ab_console_logs{level_filter}{since_filter}.slice(-{limit}); {clear_stmt} return logs; }})()"
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
