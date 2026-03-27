//! Terminal output formatting for [`ActionResult`].
//!
//! Formats daemon responses for human-readable CLI output with colored
//! status indicators and contextual hints.

use colored::Colorize;
use serde_json::Value;

use super::action::Action;
use super::action_result::ActionResult;

/// Format an [`ActionResult`] for terminal display.
///
/// - `Ok` → format data based on content type (JSON object, string, or raw)
/// - `Retryable` → yellow warning with reason and hint
/// - `UserAction` → yellow prompt with required action and hint
/// - `Fatal` → red error with code, message, and hint
pub fn format_result(result: &ActionResult) -> String {
    match result {
        ActionResult::Ok { data } => format_ok(data),
        ActionResult::Retryable { reason, hint } => format_retryable(reason, hint),
        ActionResult::UserAction { action, hint } => format_user_action(action, hint),
        ActionResult::Fatal {
            code,
            message,
            hint,
        } => format_fatal(code, message, hint),
    }
}

/// Format an [`ActionResult`] for `--json` CLI output.
///
/// This preserves the full typed result envelope so machine consumers receive
/// `status` plus command-specific `data` or structured error metadata.
pub fn format_result_json(result: &ActionResult) -> String {
    serde_json::to_string(result).unwrap_or_else(|_| {
        r#"{"status":"Fatal","code":"serialization_failed","message":"failed to serialize result","hint":"retry the command"}"#.to_string()
    })
}

/// Format an [`ActionResult`] for CLI output, applying Phase A lifecycle
/// normalization where requested and falling back to the legacy formatter
/// for all other commands.
#[allow(dead_code)]
pub fn format_cli_result(action: &Action, result: &ActionResult) -> String {
    format_cli_result_with_duration(action, result, None)
}

/// Format an [`ActionResult`] for CLI output when the caller also knows the
/// measured command duration. This is used by the thin CLI so text output can
/// include PRD fields such as `elapsed_ms` for wait commands.
pub fn format_cli_result_with_duration(
    action: &Action,
    result: &ActionResult,
    duration_ms: Option<u128>,
) -> String {
    if result.is_ok() {
        if let Some(output) = format_lifecycle_text(action, result) {
            output
        } else if let Some(output) = format_tab_nav_text(action, result) {
            output
        } else if let Some(output) = format_observation_text(action, result) {
            output
        } else if let Some(output) = format_interaction_text(action, result, duration_ms) {
            output
        } else {
            format_result(result)
        }
    } else {
        format_error_text(action, result)
    }
}

/// Format a CLI-side error into the unified PRD envelope without requiring a
/// daemon-produced [`ActionResult`].
pub fn format_cli_side_error_json(
    action: &Action,
    code: &str,
    message: &str,
    details: Value,
    duration_ms: u128,
) -> String {
    let envelope = error_envelope(
        action,
        NormalizedError {
            code: code.to_string(),
            message: message.to_string(),
            retryable: false,
            details,
        },
        duration_ms,
    );
    serde_json::to_string(&envelope).unwrap_or_else(|_| {
        r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
    })
}

/// Format a CLI-side text error using the same prefix and error contract as
/// daemon-backed failures.
pub fn format_cli_side_error_text(action: &Action, code: &str, message: &str) -> String {
    let mut out = String::new();
    if let Some(prefix) = prefix_for_action(action) {
        out.push_str(&prefix);
        out.push('\n');
    }
    out.push_str(&format!("error {code}: {message}"));
    out
}

/// Format an [`ActionResult`] for `--json` CLI output, applying the
/// Phase A lifecycle envelope for the first 5 lifecycle commands only,
/// and Phase B1 tab/navigation envelope for 7 tab/nav commands.
pub fn format_cli_result_json(action: &Action, result: &ActionResult, duration_ms: u128) -> String {
    if result.is_ok() {
        if let Some(envelope) = normalize_lifecycle_json(action, result, duration_ms) {
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
            })
        } else if let Some(envelope) = normalize_tab_nav_json(action, result, duration_ms) {
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
            })
        } else if let Some(envelope) = normalize_observation_json(action, result, duration_ms) {
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
            })
        } else if let Some(envelope) = normalize_interaction_json(action, result, duration_ms) {
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
            })
        } else if let Some(envelope) = normalize_storage_json(action, result, duration_ms) {
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
            })
        } else {
            format_result_json(result)
        }
    } else {
        let envelope = error_envelope(action, normalize_error(result), duration_ms);
        serde_json::to_string(&envelope).unwrap_or_else(|_| {
            r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
        })
    }
}

/// Returns true if the result is an error (non-Ok), used for exit code.
pub fn is_error(result: &ActionResult) -> bool {
    !result.is_ok()
}

#[derive(Debug, Clone)]
struct NormalizedError {
    code: String,
    message: String,
    retryable: bool,
    details: Value,
}

fn error_envelope(action: &Action, error: NormalizedError, duration_ms: u128) -> Value {
    serde_json::json!({
        "ok": false,
        "command": command_name(action),
        "context": context_for_action(action).unwrap_or(Value::Null),
        "data": Value::Null,
        "error": {
            "code": error.code,
            "message": error.message,
            "retryable": error.retryable,
            "details": error.details
        },
        "meta": {
            "duration_ms": duration_ms,
            "warnings": [],
            "pagination": null,
            "truncated": false
        }
    })
}

fn format_error_text(action: &Action, result: &ActionResult) -> String {
    let error = normalize_error(result);
    let mut out = String::new();
    if let Some(prefix) = prefix_for_action(action) {
        out.push_str(&prefix);
        out.push('\n');
    }
    out.push_str(&format!("error {}: {}", error.code, error.message));
    out
}

fn prefix_for_action(action: &Action) -> Option<String> {
    let session_id = action.session_id()?;
    let tab_id = action_tab_id(action).map(|tab| tab.to_string());
    Some(prefixed_header(
        &session_id.to_string(),
        tab_id.as_deref(),
        None,
    ))
}

fn action_tab_id(action: &Action) -> Option<super::types::TabId> {
    match action {
        Action::CloseTab { tab, .. }
        | Action::Goto { tab, .. }
        | Action::Back { tab, .. }
        | Action::Forward { tab, .. }
        | Action::Reload { tab, .. }
        | Action::Open { tab, .. }
        | Action::Snapshot { tab, .. }
        | Action::Screenshot { tab, .. }
        | Action::Click { tab, .. }
        | Action::Type { tab, .. }
        | Action::Fill { tab, .. }
        | Action::Eval { tab, .. }
        | Action::WaitElement { tab, .. }
        | Action::Html { tab, .. }
        | Action::Text { tab, .. }
        | Action::Pdf { tab, .. }
        | Action::Title { tab, .. }
        | Action::Url { tab, .. }
        | Action::Value { tab, .. }
        | Action::Attr { tab, .. }
        | Action::Attrs { tab, .. }
        | Action::Describe { tab, .. }
        | Action::State { tab, .. }
        | Action::Box_ { tab, .. }
        | Action::Styles { tab, .. }
        | Action::Viewport { tab, .. }
        | Action::Query { tab, .. }
        | Action::InspectPoint { tab, .. }
        | Action::LogsConsole { tab, .. }
        | Action::LogsErrors { tab, .. }
        | Action::StorageList { tab, .. }
        | Action::StorageGet { tab, .. }
        | Action::StorageSet { tab, .. }
        | Action::StorageDelete { tab, .. }
        | Action::StorageClear { tab, .. }
        | Action::Select { tab, .. }
        | Action::Hover { tab, .. }
        | Action::Focus { tab, .. }
        | Action::Press { tab, .. }
        | Action::Drag { tab, .. }
        | Action::Upload { tab, .. }
        | Action::Scroll { tab, .. }
        | Action::MouseMove { tab, .. }
        | Action::CursorPosition { tab, .. }
        | Action::WaitNavigation { tab, .. }
        | Action::WaitNetworkIdle { tab, .. }
        | Action::WaitCondition { tab, .. } => Some(*tab),
        _ => None,
    }
}

fn command_name(action: &Action) -> &'static str {
    match action {
        Action::StartSession { .. } => "browser.start",
        Action::CloseSession { .. } | Action::Close { .. } => "browser.close",
        Action::ListSessions => "browser.list-sessions",
        Action::SessionStatus { .. } => "browser.status",
        Action::ListTabs { .. } => "browser.list-tabs",
        Action::ListWindows { .. } => "browser.list-windows",
        Action::NewTab { .. } => "browser.new-tab",
        Action::CloseTab { .. } => "browser.close-tab",
        Action::Goto { .. } => "browser.goto",
        Action::Back { .. } => "browser.back",
        Action::Forward { .. } => "browser.forward",
        Action::Reload { .. } => "browser.reload",
        Action::Open { .. } => "browser.open",
        Action::Snapshot { .. } => "browser.snapshot",
        Action::Screenshot { .. } => "browser.screenshot",
        Action::Click { .. } => "browser.click",
        Action::Type { .. } => "browser.type",
        Action::Fill { .. } => "browser.fill",
        Action::Eval { .. } => "browser.eval",
        Action::WaitElement { .. } => "browser.wait.element",
        Action::Html { .. } => "browser.html",
        Action::Text { .. } => "browser.text",
        Action::Pdf { .. } => "browser.pdf",
        Action::Title { .. } => "browser.title",
        Action::Url { .. } => "browser.url",
        Action::Value { .. } => "browser.value",
        Action::Attr { .. } => "browser.attr",
        Action::Attrs { .. } => "browser.attrs",
        Action::Describe { .. } => "browser.describe",
        Action::State { .. } => "browser.state",
        Action::Box_ { .. } => "browser.box",
        Action::Styles { .. } => "browser.styles",
        Action::Viewport { .. } => "browser.viewport",
        Action::Query { .. } => "browser.query",
        Action::InspectPoint { .. } => "browser.inspect-point",
        Action::LogsConsole { .. } => "browser.logs.console",
        Action::LogsErrors { .. } => "browser.logs.errors",
        Action::CookiesList { .. } => "browser.cookies.list",
        Action::CookiesGet { .. } => "browser.cookies.get",
        Action::CookiesSet { .. } => "browser.cookies.set",
        Action::CookiesDelete { .. } => "browser.cookies.delete",
        Action::CookiesClear { .. } => "browser.cookies.clear",
        Action::StorageList { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => "browser.local-storage.list",
            crate::daemon::types::StorageKind::Session => "browser.session-storage.list",
        },
        Action::StorageGet { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => "browser.local-storage.get",
            crate::daemon::types::StorageKind::Session => "browser.session-storage.get",
        },
        Action::StorageSet { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => "browser.local-storage.set",
            crate::daemon::types::StorageKind::Session => "browser.session-storage.set",
        },
        Action::StorageDelete { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => "browser.local-storage.delete",
            crate::daemon::types::StorageKind::Session => "browser.session-storage.delete",
        },
        Action::StorageClear { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => "browser.local-storage.clear",
            crate::daemon::types::StorageKind::Session => "browser.session-storage.clear",
        },
        Action::Select { .. } => "browser.select",
        Action::Hover { .. } => "browser.hover",
        Action::Focus { .. } => "browser.focus",
        Action::Press { .. } => "browser.press",
        Action::Drag { .. } => "browser.drag",
        Action::Upload { .. } => "browser.upload",
        Action::Scroll { .. } => "browser.scroll",
        Action::MouseMove { .. } => "browser.mouse-move",
        Action::CursorPosition { .. } => "browser.cursor-position",
        Action::WaitNavigation { .. } => "browser.wait.navigation",
        Action::WaitNetworkIdle { .. } => "browser.wait.network-idle",
        Action::WaitCondition { .. } => "browser.wait.condition",
        Action::RestartSession { .. } => "browser.restart",
    }
}

fn context_for_action(action: &Action) -> Option<Value> {
    match action {
        Action::ListSessions => None,
        _ => action.session_id().map(|session_id| {
            serde_json::json!({
                "session_id": session_id.to_string(),
                "tab_id": action_tab_id(action).map(|tab| tab.to_string()),
                "url": null,
                "title": null
            })
        }),
    }
}

// ---------------------------------------------------------------------------
// Internal formatters
// ---------------------------------------------------------------------------

fn format_ok(data: &Value) -> String {
    match data {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Object(map) => {
            // Query-specific text formatting (PRD §10.7)
            if let Some(mode) = map.get("mode").and_then(|v| v.as_str()) {
                return format_query_result(mode, data);
            }
            serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
        }
        _ => serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string()),
    }
}

/// Format query results as human-readable text per PRD §10.7.
fn format_query_result(mode: &str, data: &Value) -> String {
    let count = data.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    match mode {
        "count" => count.to_string(),
        "one" => {
            let mut out = String::from("1 match\n");
            if let Some(item) = data.get("item") {
                if let Some(sel) = item.get("selector").and_then(|v| v.as_str()) {
                    out.push_str(&format!("selector: {sel}\n"));
                }
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        out.push_str(&format!("text: {text}\n"));
                    }
                }
                if let Some(tag) = item.get("tag").and_then(|v| v.as_str()) {
                    out.push_str(&format!("tag: {tag}"));
                }
            }
            out.trim_end().to_string()
        }
        "all" => {
            let mut out = format!("{count} match{}\n", if count == 1 { "" } else { "es" });
            if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                for (i, item) in items.iter().enumerate() {
                    let sel = item.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                    let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    out.push_str(&format!("{}. {sel}\n", i + 1));
                    if !text.is_empty() {
                        out.push_str(&format!("   {text}\n"));
                    }
                }
            }
            out.trim_end().to_string()
        }
        "nth" => {
            let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let mut out = format!("match {index}/{count}\n");
            if let Some(item) = data.get("item") {
                if let Some(sel) = item.get("selector").and_then(|v| v.as_str()) {
                    out.push_str(&format!("selector: {sel}\n"));
                }
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        out.push_str(&format!("text: {text}"));
                    }
                }
            }
            out.trim_end().to_string()
        }
        _ => serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string()),
    }
}

fn format_retryable(reason: &str, hint: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} {}\n", "warning:".yellow().bold(), reason));
    out.push_str(&format!("{} {}", "hint:".dimmed(), hint));
    out
}

fn format_user_action(action: &str, hint: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {}\n",
        "action required:".yellow().bold(),
        action
    ));
    out.push_str(&format!("{} {}", "hint:".dimmed(), hint));
    out
}

fn format_fatal(code: &str, message: &str, hint: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} ({})\n",
        "error:".red().bold(),
        message,
        code.dimmed()
    ));
    out.push_str(&format!("{} {}", "hint:".dimmed(), hint));
    out
}

fn lifecycle_command(action: &Action) -> Option<&'static str> {
    match action {
        Action::StartSession { .. } => Some("browser.start"),
        Action::ListSessions => Some("browser.list-sessions"),
        Action::SessionStatus { .. } => Some("browser.status"),
        Action::CloseSession { .. } => Some("browser.close"),
        Action::RestartSession { .. } => Some("browser.restart"),
        _ => None,
    }
}

fn normalize_lifecycle_json(
    action: &Action,
    result: &ActionResult,
    duration_ms: u128,
) -> Option<Value> {
    let command = lifecycle_command(action)?;
    let ok = result.is_ok();
    let context = lifecycle_context(action, result);
    let data = match result {
        ActionResult::Ok { data } => normalize_lifecycle_data(action, data),
        _ => Value::Null,
    };
    let error = match result {
        ActionResult::Ok { .. } => Value::Null,
        _ => normalized_error_value(&normalize_error(result)),
    };

    Some(serde_json::json!({
        "ok": ok,
        "command": command,
        "context": context.unwrap_or(Value::Null),
        "data": data,
        "error": error,
        "meta": {
            "duration_ms": duration_ms,
            "warnings": [],
            "pagination": null,
            "truncated": false
        }
    }))
}

fn normalize_lifecycle_data(action: &Action, data: &Value) -> Value {
    match action {
        Action::ListSessions => {
            let sessions = data
                .get("sessions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|session| {
                    let mut normalized = serde_json::Map::new();
                    normalized.insert(
                        "session_id".into(),
                        Value::String(
                            first_str(&session, &["session_id", "id"])
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                    );
                    normalized.insert(
                        "status".into(),
                        Value::String(
                            display_lifecycle_status(
                                first_str(&session, &["status", "state"]).unwrap_or("unknown"),
                            )
                            .to_string(),
                        ),
                    );
                    normalized.insert(
                        "tabs_count".into(),
                        Value::from(first_u64(&session, &["tabs_count", "tab_count"]).unwrap_or(0)),
                    );
                    for extra in ["mode", "headless", "profile", "uptime_secs"] {
                        if let Some(value) = session.get(extra) {
                            normalized.insert(extra.into(), value.clone());
                        }
                    }
                    Value::Object(normalized)
                })
                .collect::<Vec<_>>();

            serde_json::json!({
                "total_sessions": sessions.len(),
                "sessions": sessions
            })
        }
        _ => data.clone(),
    }
}

fn format_lifecycle_text(action: &Action, result: &ActionResult) -> Option<String> {
    let command = lifecycle_command(action)?;

    Some(match result {
        ActionResult::Ok { data } => match action {
            Action::StartSession { mode, .. } => {
                let session_obj = data.get("session");
                let tab_obj = data.get("tab");
                let session_id = session_obj
                    .and_then(|s| s.get("session_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let tab_id = tab_obj
                    .and_then(|t| t.get("tab_id"))
                    .and_then(|v| v.as_str());
                let url = tab_obj.and_then(|t| t.get("url")).and_then(|v| v.as_str());
                let title = tab_obj
                    .and_then(|t| t.get("title"))
                    .and_then(|v| v.as_str());
                let mut out = prefixed_header(session_id, tab_id, url);
                out.push_str(&format!("\nok {command}\n"));
                out.push_str(&format!("mode: {mode}\n"));
                out.push_str("status: running");
                if let Some(title) = title.filter(|t| !t.is_empty()) {
                    out.push_str(&format!("\ntitle: {title}"));
                }
                out
            }
            Action::ListSessions => format_list_sessions_text(data),
            Action::SessionStatus { session } => {
                format_session_status_text(&session.to_string(), data)
            }
            Action::CloseSession { session } => {
                let closed_tabs = first_u64(data, &["closed_tabs"]).unwrap_or(0);
                let mut out = prefixed_header(&session.to_string(), None, None);
                out.push_str(&format!("\nok {command}\n"));
                out.push_str(&format!("closed_tabs: {closed_tabs}"));
                out
            }
            Action::RestartSession { session } => {
                let tab_id = data
                    .get("session")
                    .and_then(|s| s.get("tabs_count"))
                    .and_then(|v| v.as_u64())
                    .filter(|&c| c > 0)
                    .map(|_| "t0");
                let mut out = prefixed_header(&session.to_string(), tab_id, None);
                out.push_str(&format!("\nok {command}\n"));
                out.push_str("status: running");
                out
            }
            _ => return None,
        },
        _ => {
            let err = normalize_error(result);
            format!("error {}: {}", err.code, err.message)
        }
    })
}

fn lifecycle_context(action: &Action, result: &ActionResult) -> Option<Value> {
    match action {
        Action::ListSessions => None,
        Action::StartSession { .. } => {
            let data = match result {
                ActionResult::Ok { data } => data,
                _ => return None,
            };
            let session_id = data
                .get("session")
                .and_then(|s| s.get("session_id"))
                .and_then(|v| v.as_str())?;
            let tab_id = data
                .get("tab")
                .and_then(|t| t.get("tab_id"))
                .and_then(|v| v.as_str());
            let url = data
                .get("tab")
                .and_then(|t| t.get("url"))
                .and_then(|v| v.as_str());
            let title = data
                .get("tab")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str());
            Some(serde_json::json!({
                "session_id": session_id,
                "tab_id": tab_id,
                "url": url,
                "title": title
            }))
        }
        Action::SessionStatus { session }
        | Action::CloseSession { session }
        | Action::RestartSession { session } => Some(serde_json::json!({
            "session_id": session.to_string(),
            "tab_id": null,
            "url": null,
            "title": null
        })),
        _ => None,
    }
}

fn normalize_error(result: &ActionResult) -> NormalizedError {
    match result {
        ActionResult::Fatal {
            code,
            message,
            hint,
        } => NormalizedError {
            code: normalize_error_code(code),
            message: message.clone(),
            retryable: false,
            details: error_details(&[
                ("hint", Some(Value::String(hint.clone()))),
                ("raw_code", Some(Value::String(code.clone()))),
            ]),
        },
        ActionResult::Retryable { reason, hint } => {
            let code = normalize_error_code(reason);
            NormalizedError {
                message: default_error_message(&code).to_string(),
                code,
                retryable: true,
                details: error_details(&[
                    ("hint", Some(Value::String(hint.clone()))),
                    ("reason", Some(Value::String(reason.clone()))),
                ]),
            }
        }
        ActionResult::UserAction { action, hint } => NormalizedError {
            code: "INTERNAL_ERROR".to_string(),
            message: action.clone(),
            retryable: false,
            details: error_details(&[
                ("hint", Some(Value::String(hint.clone()))),
                ("action", Some(Value::String(action.clone()))),
            ]),
        },
        ActionResult::Ok { .. } => NormalizedError {
            code: "INTERNAL_ERROR".to_string(),
            message: "command failed".to_string(),
            retryable: false,
            details: Value::Object(serde_json::Map::new()),
        },
    }
}

fn normalize_error_code(code: &str) -> String {
    let normalized = code.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "session_not_found" | "session_dead" => "SESSION_NOT_FOUND".to_string(),
        "tab_not_found" | "no_tabs" => "TAB_NOT_FOUND".to_string(),
        "frame_not_found" => "FRAME_NOT_FOUND".to_string(),
        "element_not_found" => "ELEMENT_NOT_FOUND".to_string(),
        "multiple_matches" => "MULTIPLE_MATCHES".to_string(),
        "index_out_of_range" => "INDEX_OUT_OF_RANGE".to_string(),
        "navigation_failed" => "NAVIGATION_FAILED".to_string(),
        "eval_error" => "EVAL_FAILED".to_string(),
        "pdf_write_error" | "pdf_decode_error" | "artifact_write_failed" => {
            "ARTIFACT_WRITE_FAILED".to_string()
        }
        "cdp_timeout" | "backend_disconnected" | "timeout" => "TIMEOUT".to_string(),
        "unsupported_operation" => "UNSUPPORTED_OPERATION".to_string(),
        code if code.starts_with("invalid_")
            || matches!(
                code,
                "missing_cdp_endpoint" | "session_exists" | "extension_session_exists"
            ) =>
        {
            "INVALID_ARGUMENT".to_string()
        }
        _ => "INTERNAL_ERROR".to_string(),
    }
}

fn normalized_error_value(error: &NormalizedError) -> Value {
    serde_json::json!({
        "code": error.code,
        "message": error.message,
        "retryable": error.retryable,
        "details": error.details
    })
}

fn default_error_message(code: &str) -> &'static str {
    match code {
        "SESSION_NOT_FOUND" => "Session not found",
        "TAB_NOT_FOUND" => "Tab not found",
        "FRAME_NOT_FOUND" => "Frame not found",
        "ELEMENT_NOT_FOUND" => "Element not found",
        "MULTIPLE_MATCHES" => "Multiple matches found",
        "INDEX_OUT_OF_RANGE" => "Index out of range",
        "NAVIGATION_FAILED" => "Navigation failed",
        "EVAL_FAILED" => "JavaScript evaluation failed",
        "ARTIFACT_WRITE_FAILED" => "Failed to write artifact",
        "INVALID_ARGUMENT" => "Invalid argument",
        "TIMEOUT" => "Operation timed out",
        "UNSUPPORTED_OPERATION" => "Unsupported operation",
        _ => "Internal error",
    }
}

fn error_details(fields: &[(&str, Option<Value>)]) -> Value {
    let mut details = serde_json::Map::new();
    for (key, value) in fields {
        if let Some(value) = value {
            details.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(details)
}

fn prefixed_header(session_id: &str, tab_id: Option<&str>, url: Option<&str>) -> String {
    match (tab_id, url.filter(|u| !u.is_empty())) {
        (Some(tab_id), Some(url)) => format!("[{session_id} {tab_id}] {url}"),
        (Some(tab_id), None) => format!("[{session_id} {tab_id}]"),
        (None, _) => format!("[{session_id}]"),
    }
}

fn format_list_sessions_text(data: &Value) -> String {
    let sessions = data
        .get("sessions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = format!(
        "{} session{}",
        sessions.len(),
        if sessions.len() == 1 { "" } else { "s" }
    );
    for session in sessions {
        let session_id = first_str(&session, &["session_id", "id"]).unwrap_or("unknown");
        let status = first_str(&session, &["status", "state"]).unwrap_or("unknown");
        let tabs = first_u64(&session, &["tabs_count", "tab_count"]).unwrap_or(0);
        out.push_str(&format!(
            "\n[{}]\nstatus: {}\ntabs: {}",
            session_id,
            display_lifecycle_status(status),
            tabs
        ));
    }
    out
}

fn format_session_status_text(session_id: &str, data: &Value) -> String {
    let session = data.get("session");
    let status = session
        .and_then(|s| s.get("status"))
        .and_then(|v| v.as_str())
        .or_else(|| first_str(data, &["status", "state"]))
        .unwrap_or("unknown");
    let mode = session
        .and_then(|s| s.get("mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let tabs = session
        .and_then(|s| s.get("tabs_count"))
        .and_then(|v| v.as_u64())
        .or_else(|| first_u64(data, &["tabs_count", "tab_count"]))
        .unwrap_or(0);
    format!(
        "[{session_id}]\nstatus: {}\nmode: {mode}\ntabs: {tabs}",
        display_lifecycle_status(status)
    )
}

fn first_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(|v| v.as_str()))
}

fn first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(|v| v.as_u64()))
}

pub(super) fn display_lifecycle_status(status: &str) -> &str {
    match status {
        "ready" | "executing" | "recovering" => "running",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Phase B1: Tab / Navigation normalization
// ---------------------------------------------------------------------------

fn tab_nav_command(action: &Action) -> Option<&'static str> {
    match action {
        Action::ListTabs { .. } => Some("browser.list-tabs"),
        Action::NewTab { .. } => Some("browser.new-tab"),
        Action::CloseTab { .. } => Some("browser.close-tab"),
        Action::Goto { .. } => Some("browser.goto"),
        Action::Back { .. } => Some("browser.back"),
        Action::Forward { .. } => Some("browser.forward"),
        Action::Reload { .. } => Some("browser.reload"),
        _ => None,
    }
}

fn normalize_tab_nav_json(
    action: &Action,
    result: &ActionResult,
    duration_ms: u128,
) -> Option<Value> {
    let command = tab_nav_command(action)?;
    let ok = result.is_ok();
    let data = match result {
        ActionResult::Ok { data } => normalize_tab_nav_data(action, data),
        _ => Value::Null,
    };
    let context = tab_nav_context(action, result);
    let error = match result {
        ActionResult::Ok { .. } => Value::Null,
        _ => normalized_error_value(&normalize_error(result)),
    };

    Some(serde_json::json!({
        "ok": ok,
        "command": command,
        "context": context.unwrap_or(Value::Null),
        "data": data,
        "error": error,
        "meta": {
            "duration_ms": duration_ms,
            "warnings": [],
            "pagination": null,
            "truncated": false
        }
    }))
}

fn tab_nav_context(action: &Action, result: &ActionResult) -> Option<Value> {
    let session_id = action.session_id()?.to_string();

    match action {
        // Session-level: list-tabs has no tab context
        Action::ListTabs { .. } => Some(serde_json::json!({
            "session_id": session_id,
            "tab_id": null,
            "url": null,
            "title": null
        })),
        // new-tab: context points to the newly created tab
        Action::NewTab { .. } => {
            let data = match result {
                ActionResult::Ok { data } => data,
                _ => {
                    return Some(serde_json::json!({
                        "session_id": session_id,
                        "tab_id": null,
                        "url": null,
                        "title": null
                    }))
                }
            };
            let tab = data.get("tab").unwrap_or(data);
            let tab_id = tab.get("tab_id").and_then(|v| v.as_str());
            let url = tab.get("url").and_then(|v| v.as_str());
            let title = tab.get("title").and_then(|v| v.as_str());
            Some(serde_json::json!({
                "session_id": session_id,
                "tab_id": tab_id,
                "url": url,
                "title": title
            }))
        }
        // close-tab: context has the closed tab_id but no url
        Action::CloseTab { tab, .. } => Some(serde_json::json!({
            "session_id": session_id,
            "tab_id": tab.to_string(),
            "url": null,
            "title": null
        })),
        // Navigation commands: context.url = post-navigation URL
        Action::Goto { tab, .. }
        | Action::Back { tab, .. }
        | Action::Forward { tab, .. }
        | Action::Reload { tab, .. } => {
            let (to_url, title) = match result {
                ActionResult::Ok { data } => {
                    let url = data
                        .get("to_url")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let title = data.get("title").and_then(|v| v.as_str()).map(String::from);
                    (url, title)
                }
                _ => (None, None),
            };
            Some(serde_json::json!({
                "session_id": session_id,
                "tab_id": tab.to_string(),
                "url": to_url,
                "title": title
            }))
        }
        _ => None,
    }
}

fn normalize_tab_nav_data(action: &Action, data: &Value) -> Value {
    match action {
        Action::ListTabs { .. } => {
            // Daemon returns PRD shape directly.
            // Already in PRD shape, pass through.
            data.clone()
        }
        Action::NewTab { .. } => data.clone(),
        Action::CloseTab { .. } => {
            // Daemon returns { closed_tab_id }
            data.clone()
        }
        Action::Goto { .. } => {
            // Daemon returns { kind, requested_url, from_url, to_url }
            data.clone()
        }
        Action::Back { .. } | Action::Forward { .. } | Action::Reload { .. } => {
            // Daemon returns { kind, from_url, to_url }
            data.clone()
        }
        _ => data.clone(),
    }
}

fn format_tab_nav_text(action: &Action, result: &ActionResult) -> Option<String> {
    let command = tab_nav_command(action)?;
    let session_id = action.session_id()?.to_string();

    Some(match result {
        ActionResult::Ok { data } => match action {
            Action::ListTabs { .. } => {
                let tabs = data
                    .get("tabs")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut out = prefixed_header(&session_id, None, None);
                let tab_word = if tabs.len() == 1 { "tab" } else { "tabs" };
                out.push_str(&format!("\n{} {tab_word}", tabs.len()));
                for tab in &tabs {
                    let tab_id = tab.get("tab_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    out.push_str(&format!("\n[{tab_id}] {title}\n{url}"));
                }
                out
            }
            Action::NewTab { .. } => {
                let tab = data.get("tab").unwrap_or(data);
                let tab_id = tab.get("tab_id").and_then(|v| v.as_str()).unwrap_or("?");
                let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let mut out = prefixed_header(&session_id, Some(tab_id), Some(url));
                out.push_str(&format!("\nok {command}"));
                out.push_str(&format!("\ntitle: {title}"));
                out
            }
            Action::CloseTab { tab, .. } => {
                let mut out = prefixed_header(&session_id, Some(&tab.to_string()), None);
                out.push_str(&format!("\nok {command}"));
                out
            }
            Action::Goto { tab, .. }
            | Action::Back { tab, .. }
            | Action::Forward { tab, .. }
            | Action::Reload { tab, .. } => {
                let to_url = data.get("to_url").and_then(|v| v.as_str()).unwrap_or("");
                let from_url = data.get("from_url").and_then(|v| v.as_str()).unwrap_or("");
                let title = data.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let mut out = prefixed_header(&session_id, Some(&tab.to_string()), Some(to_url));
                out.push_str(&format!("\nok {command}"));
                if !title.is_empty() {
                    out.push_str(&format!("\ntitle: {title}"));
                }
                if from_url != to_url {
                    out.push_str(&format!("\n{from_url} \u{2192} {to_url}"));
                }
                out
            }
            _ => return None,
        },
        _ => {
            let err = normalize_error(result);
            let mut out = String::new();
            if let Some(prefix) = prefix_for_action(action) {
                out.push_str(&prefix);
                out.push('\n');
            }
            out.push_str(&format!("error {}: {}", err.code, err.message));
            out
        }
    })
}

// ---------------------------------------------------------------------------
// Phase B2a: Observation / Query / Logging normalization
// ---------------------------------------------------------------------------

fn observation_command(action: &Action) -> Option<&'static str> {
    match action {
        Action::Snapshot { .. } => Some("browser.snapshot"),
        Action::Title { .. } => Some("browser.title"),
        Action::Url { .. } => Some("browser.url"),
        Action::Viewport { .. } => Some("browser.viewport"),
        Action::Html { .. } => Some("browser.html"),
        Action::Text { .. } => Some("browser.text"),
        Action::Value { .. } => Some("browser.value"),
        Action::Attr { .. } => Some("browser.attr"),
        Action::Attrs { .. } => Some("browser.attrs"),
        Action::Box_ { .. } => Some("browser.box"),
        Action::Styles { .. } => Some("browser.styles"),
        Action::Query { .. } => Some("browser.query"),
        Action::Describe { .. } => Some("browser.describe"),
        Action::State { .. } => Some("browser.state"),
        Action::InspectPoint { .. } => Some("browser.inspect-point"),
        Action::LogsConsole { .. } => Some("browser.logs.console"),
        Action::LogsErrors { .. } => Some("browser.logs.errors"),
        _ => None,
    }
}

fn normalize_observation_json(
    action: &Action,
    result: &ActionResult,
    duration_ms: u128,
) -> Option<Value> {
    let command = observation_command(action)?;
    let ok = result.is_ok();
    let data = match result {
        ActionResult::Ok { data } => normalize_observation_data(action, data),
        _ => Value::Null,
    };
    let context = observation_context(action, result);
    let error = match result {
        ActionResult::Ok { .. } => Value::Null,
        _ => normalized_error_value(&normalize_error(result)),
    };
    Some(serde_json::json!({
        "ok": ok,
        "command": command,
        "context": context.unwrap_or(Value::Null),
        "data": data,
        "error": error,
        "meta": {
            "duration_ms": duration_ms,
            "warnings": [],
            "pagination": null,
            "truncated": false
        }
    }))
}

fn observation_context(action: &Action, result: &ActionResult) -> Option<Value> {
    let session_id = action.session_id()?.to_string();
    let tab_id = action_tab_id(action)?.to_string();
    // Extract url/title embedded by the snapshot handler via __ctx_url/__ctx_title.
    let (url, title) = if let ActionResult::Ok { data } = result {
        let url = data
            .get("__ctx_url")
            .and_then(|v| v.as_str())
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null);
        let title = data
            .get("__ctx_title")
            .and_then(|v| v.as_str())
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null);
        (url, title)
    } else {
        (Value::Null, Value::Null)
    };
    Some(serde_json::json!({
        "session_id": session_id,
        "tab_id": tab_id,
        "url": url,
        "title": title
    }))
}

fn normalize_observation_data(action: &Action, data: &Value) -> Value {
    match action {
        Action::Snapshot { .. } => {
            // Handler now returns PRD 10.1 shape with real parsed nodes/stats.
            // Extract fields, stripping internal __ctx_* keys.
            let content = data
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let nodes = data
                .get("nodes")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            let stats = data
                .get("stats")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            serde_json::json!({
                "format": "snapshot",
                "content": content,
                "nodes": nodes,
                "stats": stats
            })
        }
        Action::Title { .. } => {
            // Handler returns {"title": val}
            let value = data.get("title").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "value": value,
                "target": { "selector": Value::Null }
            })
        }
        Action::Url { .. } => {
            // Handler returns {"url": val}
            let value = data.get("url").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "value": value,
                "target": { "selector": Value::Null }
            })
        }
        Action::Html { selector, .. } => {
            // Handler returns {"html": val}
            let value = data.get("html").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "value": value,
                "target": { "selector": selector }
            })
        }
        Action::Text { selector, .. } => {
            // Handler returns {"text": val}
            let value = data.get("text").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "value": value,
                "target": { "selector": selector }
            })
        }
        Action::Value { selector, .. } => {
            // Handler returns {"value": val, "selector": selector}
            // "value" field name is already correct; just normalize shape
            let value = data.get("value").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "target": { "selector": selector },
                "value": value
            })
        }
        Action::Attr { selector, name, .. } => {
            // Handler returns {"attr": attr_name, "value": val, "selector": selector}
            let value = data.get("value").cloned().unwrap_or(Value::Null);
            let attribute = data
                .get("attr")
                .cloned()
                .unwrap_or_else(|| Value::String(name.clone()));
            serde_json::json!({
                "target": { "selector": selector },
                "attribute": attribute,
                "value": value
            })
        }
        Action::Viewport { .. } => {
            // Handler returns {"viewport": {width, height, scrollX, scrollY, devicePixelRatio}}
            let vp = data.get("viewport").unwrap_or(data);
            let width = vp.get("width").cloned().unwrap_or(Value::Null);
            let height = vp.get("height").cloned().unwrap_or(Value::Null);
            serde_json::json!({
                "width": width,
                "height": height
            })
        }
        Action::Attrs { selector, .. } => {
            // Handler returns {"attributes": val, "selector": selector}
            let value = data
                .get("attributes")
                .cloned()
                .unwrap_or_else(|| data.clone());
            serde_json::json!({
                "target": { "selector": selector },
                "value": value
            })
        }
        Action::Styles { selector, .. } => {
            // Handler returns {"styles": val, "selector": selector}
            let value = data.get("styles").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "target": { "selector": selector },
                "value": value
            })
        }
        Action::Box_ { selector, .. } => {
            // Handler returns {"box": val, "selector": selector}
            let box_val = data.get("box").unwrap_or(data);
            let x = box_val.get("x").cloned().unwrap_or(Value::Null);
            let y = box_val.get("y").cloned().unwrap_or(Value::Null);
            let width = box_val.get("width").cloned().unwrap_or(Value::Null);
            let height = box_val.get("height").cloned().unwrap_or(Value::Null);
            serde_json::json!({
                "target": { "selector": selector },
                "value": { "x": x, "y": y, "width": width, "height": height }
            })
        }
        Action::Query { .. } => {
            // Already has mode/count/item(s) structure from handler — pass through
            data.clone()
        }
        Action::Describe { selector, .. } => {
            // Handler returns {"description": val, "selector": selector}
            let summary = data
                .get("description")
                .cloned()
                .unwrap_or_else(|| data.clone());
            serde_json::json!({
                "target": { "selector": selector },
                "summary": summary
            })
        }
        Action::State { selector, .. } => {
            // Handler returns {"state": val, "selector": selector}
            let flags = data.get("state").cloned().unwrap_or_else(|| data.clone());
            serde_json::json!({
                "target": { "selector": selector },
                "flags": flags
            })
        }
        Action::InspectPoint { x, y, .. } => {
            // Handler returns {"element": val, "x": x, "y": y}
            let element = data.get("element").cloned().unwrap_or(Value::Null);
            serde_json::json!({
                "point": { "x": x, "y": y },
                "element": element
            })
        }
        Action::LogsConsole { clear, .. } => {
            // Handler returns {"logs": val (array)}
            let items = data
                .get("logs")
                .and_then(|v| v.as_array())
                .cloned()
                .map(Value::Array)
                .unwrap_or_else(|| {
                    data.as_array()
                        .cloned()
                        .map(Value::Array)
                        .unwrap_or(Value::Array(vec![]))
                });
            serde_json::json!({
                "items": items,
                "cleared": clear
            })
        }
        Action::LogsErrors { clear, .. } => {
            // Handler returns {"errors": val (array)}
            let items = data
                .get("errors")
                .and_then(|v| v.as_array())
                .cloned()
                .map(Value::Array)
                .unwrap_or_else(|| {
                    data.as_array()
                        .cloned()
                        .map(Value::Array)
                        .unwrap_or(Value::Array(vec![]))
                });
            serde_json::json!({
                "items": items,
                "cleared": clear
            })
        }
        _ => data.clone(),
    }
}

fn format_observation_text(action: &Action, result: &ActionResult) -> Option<String> {
    let _command = observation_command(action)?;
    let session_id = action.session_id()?.to_string();
    let tab_id = action_tab_id(action)?.to_string();

    Some(match result {
        ActionResult::Ok { data } => {
            let prefix = prefixed_header(&session_id, Some(&tab_id), None);
            match action {
                Action::Snapshot { .. } => {
                    // PRD 10.1: text output = "[session tab] url\n<tree content>"
                    let url = data.get("__ctx_url").and_then(|v| v.as_str());
                    let header = prefixed_header(&session_id, Some(&tab_id), url);
                    let content = data
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            if data.is_string() {
                                data.as_str().unwrap_or("").to_string()
                            } else {
                                serde_json::to_string_pretty(data)
                                    .unwrap_or_else(|_| data.to_string())
                            }
                        });
                    format!("{header}\n{content}")
                }
                Action::Title { .. } => {
                    // Handler returns {"title": val}
                    data.get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| data.as_str().unwrap_or(""))
                        .to_string()
                }
                Action::Url { .. } => {
                    // Handler returns {"url": val}
                    data.get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| data.as_str().unwrap_or(""))
                        .to_string()
                }
                Action::Html { .. } => {
                    // Handler returns {"html": val}
                    data.get("html")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| data.as_str().unwrap_or(""))
                        .to_string()
                }
                Action::Value { .. } => {
                    // Handler returns {"value": val, "selector": selector}
                    data.get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| data.as_str().unwrap_or(""))
                        .to_string()
                }
                Action::Attr { .. } => {
                    // Handler returns {"attr": attr_name, "value": val, "selector": selector}
                    data.get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| data.as_str().unwrap_or(""))
                        .to_string()
                }
                Action::Text { .. } => {
                    // Handler returns {"text": val}
                    data.get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| data.as_str().unwrap_or(""))
                        .to_string()
                }
                Action::Viewport { .. } => {
                    // Handler returns {"viewport": {width, height, ...}}
                    let vp = data.get("viewport").unwrap_or(data);
                    let width = vp.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
                    let height = vp.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                    format!("{width}x{height}")
                }
                Action::Attrs { .. } => {
                    // Handler returns {"attributes": val, "selector": selector}
                    let obj_val = data.get("attributes").unwrap_or(data);
                    let mut out = prefix;
                    if let Some(obj) = obj_val.as_object() {
                        for (k, v) in obj {
                            let val_str = v
                                .as_str()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| v.to_string());
                            out.push_str(&format!("\n{k}: {val_str}"));
                        }
                    } else {
                        out.push('\n');
                        out.push_str(
                            &serde_json::to_string_pretty(obj_val)
                                .unwrap_or_else(|_| obj_val.to_string()),
                        );
                    }
                    out
                }
                Action::Styles { .. } => {
                    // Handler returns {"styles": val, "selector": selector}
                    let obj_val = data.get("styles").unwrap_or(data);
                    let mut out = prefix;
                    if let Some(obj) = obj_val.as_object() {
                        for (k, v) in obj {
                            let val_str = v
                                .as_str()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| v.to_string());
                            out.push_str(&format!("\n{k}: {val_str}"));
                        }
                    } else {
                        out.push('\n');
                        out.push_str(
                            &serde_json::to_string_pretty(obj_val)
                                .unwrap_or_else(|_| obj_val.to_string()),
                        );
                    }
                    out
                }
                Action::State { .. } => {
                    // Handler returns {"state": val, "selector": selector}
                    let obj_val = data.get("state").unwrap_or(data);
                    let mut out = prefix;
                    if let Some(obj) = obj_val.as_object() {
                        for (k, v) in obj {
                            let val_str = v
                                .as_str()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| v.to_string());
                            out.push_str(&format!("\n{k}: {val_str}"));
                        }
                    } else {
                        out.push('\n');
                        out.push_str(
                            &serde_json::to_string_pretty(obj_val)
                                .unwrap_or_else(|_| obj_val.to_string()),
                        );
                    }
                    out
                }
                Action::Box_ { .. } => {
                    // Handler returns {"box": val, "selector": selector}
                    let box_val = data.get("box").unwrap_or(data);
                    let mut out = prefix;
                    for key in &["x", "y", "width", "height"] {
                        if let Some(v) = box_val.get(key) {
                            out.push_str(&format!("\n{key}: {v}"));
                        }
                    }
                    out
                }
                Action::Query { .. } => {
                    // Use existing format_query_result
                    if let Some(mode) = data.get("mode").and_then(|v| v.as_str()) {
                        format_query_result(mode, data)
                    } else {
                        serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
                    }
                }
                Action::Describe { .. } => {
                    // Handler returns {"description": val, "selector": selector}
                    let desc = data.get("description").unwrap_or(data);
                    let mut out = prefix;
                    // Output summary line if available
                    if let Some(tag) = desc.get("tag").and_then(|v| v.as_str()) {
                        out.push_str(&format!("\ntag: {tag}"));
                        if let Some(role) = desc.get("role").and_then(|v| v.as_str()) {
                            if !role.is_empty() {
                                out.push_str(&format!("  role: {role}"));
                            }
                        }
                        if let Some(text) = desc.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                out.push_str(&format!("\ntext: {text}"));
                            }
                        }
                    } else {
                        out.push('\n');
                        out.push_str(
                            &serde_json::to_string_pretty(desc)
                                .unwrap_or_else(|_| desc.to_string()),
                        );
                    }
                    out
                }
                Action::InspectPoint { x, y, .. } => {
                    // Handler returns {"element": val, "x": x, "y": y}
                    let element = data.get("element").unwrap_or(data);
                    let mut out = prefix;
                    out.push_str(&format!("\npoint: {x},{y}"));
                    if let Some(tag) = element.get("tag").and_then(|v| v.as_str()) {
                        out.push_str(&format!("\ntag: {tag}"));
                    }
                    if let Some(sel) = element.get("selector").and_then(|v| v.as_str()) {
                        out.push_str(&format!("\nselector: {sel}"));
                    }
                    out
                }
                Action::LogsConsole { .. } => {
                    // Handler returns {"logs": val (array)}
                    let items = data
                        .get("logs")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_else(|| data.as_array().cloned().unwrap_or_default());
                    let mut out = prefix;
                    for item in &items {
                        let line = item.as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                            item.get("message")
                                .or_else(|| item.get("text"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| item.to_string())
                        });
                        out.push_str(&format!("\n{line}"));
                    }
                    out
                }
                Action::LogsErrors { .. } => {
                    // Handler returns {"errors": val (array)}
                    let items = data
                        .get("errors")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_else(|| data.as_array().cloned().unwrap_or_default());
                    let mut out = prefix;
                    for item in &items {
                        let line = item.as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                            item.get("message")
                                .or_else(|| item.get("text"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| item.to_string())
                        });
                        out.push_str(&format!("\n{line}"));
                    }
                    out
                }
                _ => return None,
            }
        }
        _ => {
            let err = normalize_error(result);
            let mut out = String::new();
            if let Some(prefix) = prefix_for_action(action) {
                out.push_str(&prefix);
                out.push('\n');
            }
            out.push_str(&format!("error {}: {}", err.code, err.message));
            out
        }
    })
}

// ---------------------------------------------------------------------------
// Phase B2b: Interaction / Wait / Eval normalization
// ---------------------------------------------------------------------------

fn interaction_command(action: &Action) -> Option<&'static str> {
    match action {
        Action::Click { .. } => Some("browser.click"),
        Action::Type { .. } => Some("browser.type"),
        Action::Fill { .. } => Some("browser.fill"),
        Action::Select { .. } => Some("browser.select"),
        Action::Hover { .. } => Some("browser.hover"),
        Action::Focus { .. } => Some("browser.focus"),
        Action::Press { .. } => Some("browser.press"),
        Action::Drag { .. } => Some("browser.drag"),
        Action::Upload { .. } => Some("browser.upload"),
        Action::Scroll { .. } => Some("browser.scroll"),
        Action::MouseMove { .. } => Some("browser.mouse-move"),
        Action::CursorPosition { .. } => Some("browser.cursor-position"),
        Action::Eval { .. } => Some("browser.eval"),
        Action::WaitElement { .. } => Some("browser.wait.element"),
        Action::WaitNavigation { .. } => Some("browser.wait.navigation"),
        Action::WaitNetworkIdle { .. } => Some("browser.wait.network-idle"),
        Action::WaitCondition { .. } => Some("browser.wait.condition"),
        _ => None,
    }
}

fn normalize_interaction_json(
    action: &Action,
    result: &ActionResult,
    duration_ms: u128,
) -> Option<Value> {
    let command = interaction_command(action)?;
    let ok = result.is_ok();
    let data = match result {
        ActionResult::Ok { data } => normalize_interaction_data(action, data, duration_ms),
        _ => Value::Null,
    };
    let context = match result {
        ActionResult::Ok { data } => interaction_context(action, data),
        _ => interaction_context(action, &Value::Null),
    };
    let error = match result {
        ActionResult::Ok { .. } => Value::Null,
        _ => normalized_error_value(&normalize_error(result)),
    };
    Some(serde_json::json!({
        "ok": ok,
        "command": command,
        "context": context.unwrap_or(Value::Null),
        "data": data,
        "error": error,
        "meta": {
            "duration_ms": duration_ms,
            "warnings": [],
            "pagination": null,
            "truncated": false
        }
    }))
}

fn interaction_context(action: &Action, data: &Value) -> Option<Value> {
    let session_id = action.session_id()?.to_string();
    let tab_id = action_tab_id(action).map(|tab| tab.to_string());
    let (url, title) = match action {
        Action::WaitNavigation { .. } => (
            data.get("url").and_then(|v| v.as_str()),
            data.get("title").and_then(|v| v.as_str()),
        ),
        Action::Click { .. } => (
            data.get("url").and_then(|v| v.as_str()),
            data.get("title").and_then(|v| v.as_str()),
        ),
        _ => (None, None),
    };
    Some(serde_json::json!({
        "session_id": session_id,
        "tab_id": tab_id,
        "url": url,
        "title": title
    }))
}

fn normalize_interaction_data(action: &Action, data: &Value, duration_ms: u128) -> Value {
    match action {
        Action::Click { selector, .. } => {
            serde_json::json!({
                "action": "click",
                "target": { "selector": selector },
                "changed": {
                    "url_changed": data.get("url").and_then(|v| v.as_str()).is_some(),
                    "focus_changed": false
                }
            })
        }
        Action::Type { selector, text, .. } => {
            serde_json::json!({
                "action": "type",
                "target": { "selector": selector },
                "value_summary": {
                    "text_length": text.chars().count()
                }
            })
        }
        Action::Fill {
            selector, value, ..
        } => {
            serde_json::json!({
                "action": "fill",
                "target": { "selector": selector },
                "value_summary": {
                    "text_length": value.chars().count()
                }
            })
        }
        Action::Select {
            selector,
            value,
            by_text,
            ..
        } => {
            serde_json::json!({
                "action": "select",
                "target": { "selector": selector },
                "value_summary": {
                    "value": data.get("selected").cloned().unwrap_or_else(|| Value::String(value.clone())),
                    "by_text": by_text
                }
            })
        }
        Action::Hover { selector, .. } => {
            serde_json::json!({
                "action": "hover",
                "target": { "selector": selector },
                "changed": {
                    "url_changed": false,
                    "focus_changed": false
                }
            })
        }
        Action::Focus { selector, .. } => {
            serde_json::json!({
                "action": "focus",
                "target": { "selector": selector }
            })
        }
        Action::Press { key_or_chord, .. } => {
            serde_json::json!({
                "action": "press",
                "keys": key_or_chord
            })
        }
        Action::Drag {
            from_selector,
            to_selector,
            ..
        } => {
            serde_json::json!({
                "action": "drag",
                "target": {
                    "from": { "selector": from_selector },
                    "to": { "selector": to_selector }
                },
                "changed": {
                    "dragged": true
                }
            })
        }
        Action::Upload {
            selector, files, ..
        } => {
            let count = data
                .get("uploaded")
                .cloned()
                .unwrap_or_else(|| Value::Number(serde_json::Number::from(files.len())));
            serde_json::json!({
                "action": "upload",
                "target": { "selector": selector },
                "value_summary": {
                    "files": files,
                    "count": count
                }
            })
        }
        Action::Scroll {
            direction,
            amount,
            selector,
            ..
        } => {
            serde_json::json!({
                "action": "scroll",
                "target": { "selector": selector },
                "changed": {
                    "scroll_changed": true
                },
                "direction": direction,
                "amount": amount
            })
        }
        Action::MouseMove { x, y, .. } => {
            serde_json::json!({
                "action": "mouse-move",
                "target": {
                    "coordinates": format!("{x},{y}")
                },
                "point": {
                    "x": x,
                    "y": y
                }
            })
        }
        Action::CursorPosition { .. } => {
            let cursor = data.get("cursor").unwrap_or(data);
            serde_json::json!({
                "x": cursor.get("x").cloned().unwrap_or(Value::Null),
                "y": cursor.get("y").cloned().unwrap_or(Value::Null)
            })
        }
        Action::Eval { .. } => {
            serde_json::json!({
                "value": data,
                "type": json_value_type(data),
                "preview": json_value_preview(data)
            })
        }
        Action::WaitElement { selector, .. } => {
            serde_json::json!({
                "kind": "element",
                "satisfied": true,
                "elapsed_ms": duration_ms,
                "observed_value": {
                    "selector": data
                        .get("found")
                        .cloned()
                        .unwrap_or_else(|| Value::String(selector.clone()))
                }
            })
        }
        Action::WaitNavigation { .. } => {
            serde_json::json!({
                "kind": "navigation",
                "satisfied": true,
                "elapsed_ms": duration_ms,
                "observed_value": {
                    "url": data.get("url").cloned().unwrap_or(Value::Null),
                    "ready_state": data.get("readyState").cloned().unwrap_or(Value::Null)
                }
            })
        }
        Action::WaitNetworkIdle { .. } => {
            serde_json::json!({
                "kind": "network-idle",
                "satisfied": true,
                "elapsed_ms": duration_ms,
                "observed_value": {
                    "idle": data
                        .get("network_idle")
                        .cloned()
                        .unwrap_or(Value::Bool(true))
                }
            })
        }
        Action::WaitCondition { .. } => {
            serde_json::json!({
                "kind": "condition",
                "satisfied": true,
                "elapsed_ms": duration_ms,
                "observed_value": data.get("value").cloned().unwrap_or(Value::Null)
            })
        }
        _ => data.clone(),
    }
}

// ---------------------------------------------------------------------------
// Phase B3: Storage normalization
// ---------------------------------------------------------------------------

fn storage_command(action: &Action) -> Option<&'static str> {
    match action {
        Action::StorageList { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => Some("browser.local-storage.list"),
            crate::daemon::types::StorageKind::Session => Some("browser.session-storage.list"),
        },
        Action::StorageGet { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => Some("browser.local-storage.get"),
            crate::daemon::types::StorageKind::Session => Some("browser.session-storage.get"),
        },
        Action::StorageSet { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => Some("browser.local-storage.set"),
            crate::daemon::types::StorageKind::Session => Some("browser.session-storage.set"),
        },
        Action::StorageDelete { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => Some("browser.local-storage.delete"),
            crate::daemon::types::StorageKind::Session => Some("browser.session-storage.delete"),
        },
        Action::StorageClear { kind, .. } => match kind {
            crate::daemon::types::StorageKind::Local => Some("browser.local-storage.clear"),
            crate::daemon::types::StorageKind::Session => Some("browser.session-storage.clear"),
        },
        _ => None,
    }
}

fn normalize_storage_json(
    action: &Action,
    result: &ActionResult,
    duration_ms: u128,
) -> Option<Value> {
    let command = storage_command(action)?;
    let ok = result.is_ok();
    let session_id = action.session_id()?.to_string();
    let tab_id = action_tab_id(action).map(|t| t.to_string());
    let context = serde_json::json!({
        "session_id": session_id,
        "tab_id": tab_id,
        "url": null,
        "title": null
    });
    let data = match result {
        ActionResult::Ok { data } => data.clone(),
        _ => Value::Null,
    };
    let error = match result {
        ActionResult::Ok { .. } => Value::Null,
        _ => normalized_error_value(&normalize_error(result)),
    };
    Some(serde_json::json!({
        "ok": ok,
        "command": command,
        "context": context,
        "data": data,
        "error": error,
        "meta": {
            "duration_ms": duration_ms,
            "warnings": [],
            "pagination": null,
            "truncated": false
        }
    }))
}

fn json_value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn json_value_preview(value: &Value) -> String {
    let raw = if let Some(s) = value.as_str() {
        s.to_string()
    } else if value.is_null() {
        "null".to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_string())
    };

    if raw.chars().count() > 120 {
        raw.chars().take(117).collect::<String>() + "..."
    } else {
        raw
    }
}

fn format_interaction_text(
    action: &Action,
    result: &ActionResult,
    duration_ms: Option<u128>,
) -> Option<String> {
    let command = interaction_command(action)?;
    let session_id = action.session_id()?.to_string();
    let tab_id = action_tab_id(action).map(|tab| tab.to_string());

    Some(match result {
        ActionResult::Ok { data } => {
            let context = interaction_context(action, data);
            let prefix = prefixed_header(
                &session_id,
                tab_id.as_deref(),
                context
                    .as_ref()
                    .and_then(|ctx| ctx.get("url"))
                    .and_then(|v| v.as_str()),
            );
            match action {
                Action::Click { selector, .. } => {
                    format!("{prefix}\nok {command}\ntarget: {selector}")
                }
                Action::Type { selector, text, .. } => {
                    format!(
                        "{prefix}\nok {command}\ntarget: {selector}\ntext_length: {}",
                        text.chars().count()
                    )
                }
                Action::Fill {
                    selector, value, ..
                } => {
                    format!(
                        "{prefix}\nok {command}\ntarget: {selector}\ntext_length: {}",
                        value.chars().count()
                    )
                }
                Action::Select {
                    selector, by_text, ..
                } => {
                    let value = data.get("selected").and_then(|v| v.as_str()).unwrap_or("");
                    let mut out =
                        format!("{prefix}\nok {command}\ntarget: {selector}\nvalue: {value}");
                    if *by_text {
                        out.push_str("\nby_text: true");
                    }
                    out
                }
                Action::Hover { selector, .. } => {
                    format!("{prefix}\nok {command}\ntarget: {selector}")
                }
                Action::Focus { selector, .. } => {
                    format!("{prefix}\nok {command}\ntarget: {selector}")
                }
                Action::Press { key_or_chord, .. } => {
                    format!("{prefix}\nok {command}\nkeys: {key_or_chord}")
                }
                Action::Drag {
                    from_selector,
                    to_selector,
                    ..
                } => {
                    format!("{prefix}\nok {command}\nfrom: {from_selector}\nto: {to_selector}")
                }
                Action::Upload {
                    selector, files, ..
                } => {
                    format!(
                        "{prefix}\nok {command}\ntarget: {selector}\ncount: {}",
                        files.len()
                    )
                }
                Action::Scroll {
                    direction,
                    amount,
                    selector,
                    ..
                } => {
                    let mut out = format!("{prefix}\nok {command}\ndirection: {direction}");
                    if let Some(px) = amount {
                        out.push_str(&format!("\namount: {px}"));
                    }
                    if let Some(sel) = selector {
                        out.push_str(&format!("\ntarget: {sel}"));
                    }
                    out
                }
                Action::MouseMove { x, y, .. } => {
                    format!("{prefix}\nok {command}\nx: {x}\ny: {y}")
                }
                Action::CursorPosition { .. } => {
                    let cursor = data.get("cursor").unwrap_or(data);
                    let x = cursor.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = cursor.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    format!("{prefix}\nok {command}\nx: {x}\ny: {y}")
                }
                Action::Eval { .. } => {
                    // Eval: output the raw value directly (no ok prefix)
                    if data.is_string() {
                        data.as_str().unwrap_or("").to_string()
                    } else if data.is_null() {
                        "null".to_string()
                    } else {
                        serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
                    }
                }
                Action::WaitElement { selector, .. } => {
                    let mut out = format!("{prefix}\nok {command}");
                    if let Some(duration_ms) = duration_ms {
                        out.push_str(&format!("\nelapsed_ms: {duration_ms}"));
                    }
                    out.push_str(&format!("\ntarget: {selector}"));
                    out
                }
                Action::WaitNavigation { .. } => {
                    let url = data.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let mut out = format!("{prefix}\nok {command}");
                    if let Some(duration_ms) = duration_ms {
                        out.push_str(&format!("\nelapsed_ms: {duration_ms}"));
                    }
                    if !url.is_empty() {
                        out.push_str(&format!("\nurl: {url}"));
                    }
                    out
                }
                Action::WaitNetworkIdle { .. } => {
                    let mut out = format!("{prefix}\nok {command}");
                    if let Some(duration_ms) = duration_ms {
                        out.push_str(&format!("\nelapsed_ms: {duration_ms}"));
                    }
                    out
                }
                Action::WaitCondition { .. } => {
                    let value = data.get("value").cloned().unwrap_or(Value::Null);
                    let mut out = format!("{prefix}\nok {command}");
                    if let Some(duration_ms) = duration_ms {
                        out.push_str(&format!("\nelapsed_ms: {duration_ms}"));
                    }
                    out.push_str(&format!("\nobserved_value: {value}"));
                    out
                }
                _ => return None,
            }
        }
        _ => {
            let err = normalize_error(result);
            let mut out = String::new();
            if let Some(prefix) = prefix_for_action(action) {
                out.push_str(&prefix);
                out.push('\n');
            }
            out.push_str(&format!("error {}: {}", err.code, err.message));
            out
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::action::Action;
    use crate::daemon::types::{Mode, SessionId, TabId};
    use serde_json::json;

    #[test]
    fn ok_string_passthrough() {
        let r = ActionResult::ok(json!("hello world"));
        let out = format_result(&r);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn ok_null_empty() {
        let r = ActionResult::ok(json!(null));
        let out = format_result(&r);
        assert_eq!(out, "");
    }

    #[test]
    fn ok_scalar_values_use_plain_text() {
        assert_eq!(format_result(&ActionResult::ok(json!(true))), "true");
        assert_eq!(format_result(&ActionResult::ok(json!(42))), "42");
    }

    #[test]
    fn ok_object_pretty_printed() {
        let r = ActionResult::ok(json!({"title": "Example", "url": "https://example.com"}));
        let out = format_result(&r);
        assert!(out.contains("title"));
        assert!(out.contains("Example"));
    }

    #[test]
    fn query_results_follow_text_contract() {
        let count = format_result(&ActionResult::ok(json!({
            "mode": "count",
            "count": 3
        })));
        assert_eq!(count, "3");

        let one = format_result(&ActionResult::ok(json!({
            "mode": "one",
            "count": 1,
            "item": {
                "selector": "#ready",
                "text": "Ready",
                "tag": "button"
            }
        })));
        assert_eq!(one, "1 match\nselector: #ready\ntext: Ready\ntag: button");

        let all = format_result(&ActionResult::ok(json!({
            "mode": "all",
            "count": 2,
            "items": [
                {"selector": "#first", "text": "First"},
                {"selector": "#second", "text": ""}
            ]
        })));
        assert_eq!(all, "2 matches\n1. #first\n   First\n2. #second");

        let nth = format_result(&ActionResult::ok(json!({
            "mode": "nth",
            "count": 4,
            "index": 2,
            "item": {
                "selector": "#picked",
                "text": "Picked"
            }
        })));
        assert_eq!(nth, "match 2/4\nselector: #picked\ntext: Picked");
    }

    #[test]
    fn json_output_preserves_ok_envelope() {
        let r = ActionResult::ok(json!({"title": "Example"}));
        let out = format_result_json(&r);
        let decoded: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["status"], "Ok");
        assert_eq!(decoded["data"]["title"], "Example");
    }

    #[test]
    fn json_output_preserves_fatal_envelope() {
        let r = ActionResult::fatal("session_not_found", "missing session", "list sessions");
        let out = format_result_json(&r);
        let decoded: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["status"], "Fatal");
        assert_eq!(decoded["code"], "session_not_found");
        assert_eq!(decoded["message"], "missing session");
    }

    #[test]
    fn fatal_contains_code_and_hint() {
        let r = ActionResult::fatal(
            "session_not_found",
            "session s5 does not exist",
            "run `actionbook browser list-sessions`",
        );
        let out = format_result(&r);
        assert!(out.contains("session s5 does not exist"));
        assert!(out.contains("session_not_found"));
        assert!(out.contains("list-sessions"));
    }

    #[test]
    fn retryable_contains_warning() {
        let r = ActionResult::retryable("cdp_timeout", "try again in a few seconds");
        let out = format_result(&r);
        assert!(out.contains("cdp_timeout"));
        assert!(out.contains("try again"));
    }

    #[test]
    fn user_action_contains_action() {
        let r = ActionResult::user_action("reconnect extension", "click the extension icon");
        let out = format_result(&r);
        assert!(out.contains("reconnect extension"));
        assert!(out.contains("extension icon"));
    }

    #[test]
    fn is_error_detects_non_ok() {
        assert!(!is_error(&ActionResult::ok(json!(null))));
        assert!(is_error(&ActionResult::fatal("x", "y", "z")));
        assert!(is_error(&ActionResult::retryable("x", "y")));
        assert!(is_error(&ActionResult::user_action("x", "y")));
    }

    #[test]
    fn lifecycle_json_envelope_wraps_start_result() {
        let action = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: true,
            open_url: Some("https://example.com".into()),
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        let result = ActionResult::ok(json!({
            "session": {
                "session_id": "local-1",
                "mode": "local",
                "status": "running",
                "headless": true,
                "cdp_endpoint": null
            },
            "tab": {
                "tab_id": "t0",
                "url": "https://example.com",
                "title": "Example"
            },
            "reused": false
        }));
        let out = format_cli_result_json(&action, &result, 42);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["ok"], true);
        assert_eq!(decoded["command"], "browser.start");
        assert_eq!(decoded["context"]["session_id"], "local-1");
        assert_eq!(decoded["context"]["tab_id"], "t0");
        assert_eq!(decoded["context"]["url"], "https://example.com");
        assert_eq!(decoded["data"]["session"]["session_id"], "local-1");
        assert_eq!(decoded["data"]["tab"]["tab_id"], "t0");
        assert_eq!(decoded["data"]["reused"], false);
        assert_eq!(decoded["meta"]["duration_ms"], 42);
    }

    #[test]
    fn lifecycle_text_formats_list_sessions() {
        let action = Action::ListSessions;
        let result = ActionResult::ok(json!({
            "sessions": [
                {"id": "local-1", "state": "ready", "tab_count": 2}
            ]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("1 session"));
        assert!(out.contains("[local-1]"));
        assert!(out.contains("status: running"));
        assert!(out.contains("tabs: 2"));
    }

    #[test]
    fn lifecycle_text_formats_status_with_prefix() {
        let action = Action::SessionStatus {
            session: SessionId::new_unchecked("local-1"),
        };
        // Data now comes from the router in PRD 7.3 shape.
        let result = ActionResult::ok(json!({
            "session": {
                "session_id": "local-1",
                "mode": "local",
                "status": "running",
                "headless": false,
                "tabs_count": 2
            },
            "tabs": [],
            "capabilities": { "snapshot": true, "pdf": true, "upload": true }
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1]"));
        assert!(out.contains("status: running"));
        assert!(out.contains("mode: local"));
        assert!(out.contains("tabs: 2"));
        assert!(!out.contains("windows:"));
    }

    #[test]
    fn lifecycle_text_start_includes_tab_and_url() {
        let action = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: true,
            open_url: Some("https://example.com".into()),
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        let result = ActionResult::ok(json!({
            "session": {
                "session_id": "local-1",
                "mode": "local",
                "status": "running",
                "headless": true,
                "cdp_endpoint": null
            },
            "tab": {
                "tab_id": "t0",
                "url": "https://example.com",
                "title": "Example"
            },
            "reused": false
        }));
        let out = format_cli_result(&action, &result);
        assert!(
            out.starts_with("[local-1 t0] https://example.com"),
            "expected [session tab] url prefix, got: {out}"
        );
        assert!(out.contains("ok browser.start"));
        assert!(out.contains("mode: local"));
        assert!(out.contains("title: Example"));
    }

    #[test]
    fn lifecycle_text_restart_uses_session_prefix_only() {
        let action = Action::RestartSession {
            session: SessionId::new_unchecked("local-1"),
        };
        let result = ActionResult::ok(json!({
            "session_id": "local-1",
            "tab_ids": ["native-tab-1"]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1]\n"));
        assert!(!out.contains("[s0 t0]"));
        assert!(out.contains("ok browser.restart"));
    }

    #[test]
    fn lifecycle_json_envelope_normalizes_list_sessions_fields() {
        let action = Action::ListSessions;
        let result = ActionResult::ok(json!({
            "sessions": [
                {
                    "id": "local-1",
                    "mode": "local",
                    "state": "ready",
                    "tab_count": 2
                }
            ]
        }));
        let out = format_cli_result_json(&action, &result, 7);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["data"]["total_sessions"], 1);
        assert_eq!(decoded["data"]["sessions"][0]["session_id"], "local-1");
        assert_eq!(decoded["data"]["sessions"][0]["status"], "running");
        assert_eq!(decoded["data"]["sessions"][0]["tabs_count"], 2);
        assert_eq!(decoded["data"]["sessions"][0]["mode"], "local");
    }

    #[test]
    fn lifecycle_text_accepts_normalized_session_keys() {
        let action = Action::ListSessions;
        let result = ActionResult::ok(json!({
            "sessions": [
                {"session_id": "local-1", "status": "running", "tabs_count": 3}
            ]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("[local-1]"));
        assert!(out.contains("status: running"));
        assert!(out.contains("tabs: 3"));
    }

    #[test]
    fn lifecycle_text_formats_failures_with_error_code() {
        let action = Action::CloseSession {
            session: SessionId::new_unchecked("local-5"),
        };
        let result = ActionResult::fatal(
            "session_not_found",
            "session local-5 does not exist",
            "run list-sessions",
        );
        let out = format_cli_result(&action, &result);
        assert_eq!(
            out,
            "[local-5]\nerror SESSION_NOT_FOUND: session local-5 does not exist"
        );
    }

    #[test]
    fn non_lifecycle_json_errors_use_prd_envelope() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-1"),
            tab: crate::daemon::types::TabId(0),
            selector: "#missing".into(),
            button: None,
            count: None,
            new_tab: false,
            coordinates: None,
        };
        let result = ActionResult::fatal(
            "element_not_found",
            "element '#missing' not found",
            "check selector",
        );
        let out = format_cli_result_json(&action, &result, 12);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["ok"], false);
        assert_eq!(decoded["command"], "browser.click");
        assert_eq!(decoded["context"]["session_id"], "local-1");
        assert_eq!(decoded["context"]["tab_id"], "t0");
        assert_eq!(decoded["data"], Value::Null);
        assert_eq!(decoded["error"]["code"], "ELEMENT_NOT_FOUND");
        assert_eq!(decoded["error"]["message"], "element '#missing' not found");
        assert_eq!(decoded["error"]["details"]["hint"], "check selector");
    }

    #[test]
    fn non_lifecycle_retryable_errors_map_to_timeout() {
        let action = Action::WaitCondition {
            session: SessionId::new_unchecked("local-1"),
            tab: crate::daemon::types::TabId(0),
            expression: "window.ready".into(),
            timeout_ms: Some(5000),
        };
        let result = ActionResult::retryable("cdp_timeout", "retry in a moment");
        let out = format_cli_result_json(&action, &result, 33);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["error"]["code"], "TIMEOUT");
        assert_eq!(decoded["error"]["message"], "Operation timed out");
        assert_eq!(decoded["error"]["retryable"], true);
        assert_eq!(decoded["error"]["details"]["reason"], "cdp_timeout");
        assert_eq!(decoded["error"]["details"]["hint"], "retry in a moment");
    }

    #[test]
    fn cli_side_artifact_errors_use_prd_envelope() {
        let action = Action::Screenshot {
            session: SessionId::new_unchecked("local-1"),
            tab: crate::daemon::types::TabId(0),
            full_page: false,
        };
        let out = format_cli_side_error_json(
            &action,
            "ARTIFACT_WRITE_FAILED",
            "failed to write screenshot to /tmp/out.png: permission denied",
            json!({"path": "/tmp/out.png"}),
            9,
        );
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["command"], "browser.screenshot");
        assert_eq!(decoded["context"]["session_id"], "local-1");
        assert_eq!(decoded["context"]["tab_id"], "t0");
        assert_eq!(decoded["error"]["code"], "ARTIFACT_WRITE_FAILED");
        assert_eq!(decoded["error"]["details"]["path"], "/tmp/out.png");
    }

    #[test]
    fn cli_side_artifact_errors_use_text_contract() {
        let action = Action::Screenshot {
            session: SessionId::new_unchecked("local-1"),
            tab: crate::daemon::types::TabId(0),
            full_page: false,
        };
        let out = format_cli_side_error_text(
            &action,
            "ARTIFACT_WRITE_FAILED",
            "failed to write screenshot to /tmp/out.png: permission denied",
        );
        assert_eq!(
            out,
            "[local-1 t0]\nerror ARTIFACT_WRITE_FAILED: failed to write screenshot to /tmp/out.png: permission denied"
        );
    }

    #[test]
    fn normalize_error_code_maps_prd_table() {
        assert_eq!(
            normalize_error_code("session_not_found"),
            "SESSION_NOT_FOUND"
        );
        assert_eq!(normalize_error_code("tab_not_found"), "TAB_NOT_FOUND");
        assert_eq!(normalize_error_code("frame_not_found"), "FRAME_NOT_FOUND");
        assert_eq!(
            normalize_error_code("element_not_found"),
            "ELEMENT_NOT_FOUND"
        );
        assert_eq!(normalize_error_code("multiple_matches"), "MULTIPLE_MATCHES");
        assert_eq!(
            normalize_error_code("index_out_of_range"),
            "INDEX_OUT_OF_RANGE"
        );
        assert_eq!(
            normalize_error_code("navigation_failed"),
            "NAVIGATION_FAILED"
        );
        assert_eq!(normalize_error_code("eval_error"), "EVAL_FAILED");
        assert_eq!(
            normalize_error_code("pdf_write_error"),
            "ARTIFACT_WRITE_FAILED"
        );
        assert_eq!(normalize_error_code("cdp_timeout"), "TIMEOUT");
        assert_eq!(
            normalize_error_code("unsupported_operation"),
            "UNSUPPORTED_OPERATION"
        );
        assert_eq!(
            normalize_error_code("invalid_selector_mode"),
            "INVALID_ARGUMENT"
        );
        assert_eq!(normalize_error_code("something_else"), "INTERNAL_ERROR");
    }

    #[test]
    fn command_names_follow_prd_namespace() {
        assert_eq!(
            command_name(&Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: true,
                open_url: None,
                cdp_endpoint: None,
                ws_headers: None,
                set_session_id: None,
            }),
            "browser.start"
        );
        assert_eq!(
            command_name(&Action::ListTabs {
                session: SessionId::new_unchecked("local-1"),
            }),
            "browser.list-tabs"
        );
        assert_eq!(
            command_name(&Action::CloseTab {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
            }),
            "browser.close-tab"
        );
        assert_eq!(
            command_name(&Action::Open {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                url: "https://example.com".into(),
            }),
            "browser.open"
        );
        assert_eq!(
            command_name(&Action::Snapshot {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                interactive: true,
                compact: false,
                cursor: false,
                depth: None,
                selector: None,
            }),
            "browser.snapshot"
        );
        assert_eq!(
            command_name(&Action::WaitElement {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                selector: "#ready".into(),
                timeout_ms: Some(1000),
            }),
            "browser.wait.element"
        );
        assert_eq!(
            command_name(&Action::LogsConsole {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                level: None,
                tail: None,
                since: None,
                clear: false,
            }),
            "browser.logs.console"
        );
        assert_eq!(
            command_name(&Action::CookiesList {
                session: SessionId::new_unchecked("local-1"),
                domain: None,
            }),
            "browser.cookies.list"
        );
        assert_eq!(
            command_name(&Action::StorageList {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                kind: crate::daemon::types::StorageKind::Local,
            }),
            "browser.local-storage.list"
        );
        assert_eq!(
            command_name(&Action::StorageList {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                kind: crate::daemon::types::StorageKind::Session,
            }),
            "browser.session-storage.list"
        );
        assert_eq!(
            command_name(&Action::WaitNetworkIdle {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                idle_time_ms: Some(500),
                timeout_ms: Some(5000),
            }),
            "browser.wait.network-idle"
        );
        assert_eq!(
            command_name(&Action::MouseMove {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                x: 10.0,
                y: 20.0,
            }),
            "browser.mouse-move"
        );
        assert_eq!(
            command_name(&Action::CursorPosition {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
            }),
            "browser.cursor-position"
        );
        assert_eq!(
            command_name(&Action::WaitNavigation {
                session: SessionId::new_unchecked("local-1"),
                tab: crate::daemon::types::TabId(0),
                timeout_ms: Some(1000),
            }),
            "browser.wait.navigation"
        );
    }

    #[test]
    fn prefix_for_action_uses_session_and_tab_when_available() {
        let session_action = Action::ListTabs {
            session: SessionId::new_unchecked("local-1"),
        };
        assert_eq!(
            prefix_for_action(&session_action).as_deref(),
            Some("[local-1]")
        );

        let tab_action = Action::MouseMove {
            session: SessionId::new_unchecked("local-1"),
            tab: crate::daemon::types::TabId(2),
            x: 1.5,
            y: 3.5,
        };
        assert_eq!(
            prefix_for_action(&tab_action).as_deref(),
            Some("[local-1 t2]")
        );

        assert_eq!(prefix_for_action(&Action::ListSessions), None);
    }

    // -----------------------------------------------------------------------
    // Phase B1: Tab / Navigation tests
    // -----------------------------------------------------------------------

    #[test]
    fn tab_nav_command_returns_correct_names() {
        assert_eq!(
            tab_nav_command(&Action::ListTabs {
                session: SessionId::new_unchecked("s0"),
            }),
            Some("browser.list-tabs")
        );
        assert_eq!(
            tab_nav_command(&Action::NewTab {
                session: SessionId::new_unchecked("s0"),
                url: "https://actionbook.dev".into(),
                new_window: false,
                window: None,
            }),
            Some("browser.new-tab")
        );
        assert_eq!(
            tab_nav_command(&Action::CloseTab {
                session: SessionId::new_unchecked("s0"),
                tab: TabId(1),
            }),
            Some("browser.close-tab")
        );
        assert_eq!(
            tab_nav_command(&Action::Goto {
                session: SessionId::new_unchecked("s0"),
                tab: TabId(0),
                url: "https://actionbook.dev".into(),
            }),
            Some("browser.goto")
        );
        assert_eq!(
            tab_nav_command(&Action::Back {
                session: SessionId::new_unchecked("s0"),
                tab: TabId(0),
            }),
            Some("browser.back")
        );
        assert_eq!(
            tab_nav_command(&Action::Forward {
                session: SessionId::new_unchecked("s0"),
                tab: TabId(0),
            }),
            Some("browser.forward")
        );
        assert_eq!(
            tab_nav_command(&Action::Reload {
                session: SessionId::new_unchecked("s0"),
                tab: TabId(0),
            }),
            Some("browser.reload")
        );
    }

    #[test]
    fn tab_nav_json_list_tabs() {
        let action = Action::ListTabs {
            session: SessionId::new_unchecked("local-1"),
        };
        let result = ActionResult::ok(json!({
            "total_tabs": 2,
            "tabs": [
                {"tab_id": "t0", "url": "https://actionbook.dev", "title": "Home", "native_tab_id": "TARGET_0"},
                {"tab_id": "t1", "url": "https://actionbook.dev/docs", "title": "Docs", "native_tab_id": "TARGET_1"}
            ]
        }));
        let out = format_cli_result_json(&action, &result, 5);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.list-tabs");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], Value::Null);
        assert_eq!(d["data"]["total_tabs"], 2);
        assert_eq!(d["data"]["tabs"][0]["tab_id"], "t0");
        assert_eq!(d["data"]["tabs"][1]["url"], "https://actionbook.dev/docs");
        assert_eq!(d["data"]["tabs"][0]["native_tab_id"], "TARGET_0");
        assert_eq!(d["meta"]["duration_ms"], 5);
    }

    #[test]
    fn tab_nav_text_list_tabs() {
        let action = Action::ListTabs {
            session: SessionId::new_unchecked("local-1"),
        };
        let result = ActionResult::ok(json!({
            "total_tabs": 2,
            "tabs": [
                {"tab_id": "t0", "url": "https://actionbook.dev", "title": "Home", "native_tab_id": "TARGET_0"},
                {"tab_id": "t1", "url": "https://actionbook.dev/docs", "title": "Docs", "native_tab_id": "TARGET_1"}
            ]
        }));
        let out = format_cli_result(&action, &result);
        assert_eq!(
            out,
            "[local-1]\n2 tabs\n[t0] Home\nhttps://actionbook.dev\n[t1] Docs\nhttps://actionbook.dev/docs"
        );
    }

    #[test]
    fn tab_nav_json_new_tab() {
        let action = Action::NewTab {
            session: SessionId::new_unchecked("local-1"),
            url: "https://actionbook.dev".into(),
            new_window: false,
            window: None,
        };
        let result = ActionResult::ok(json!({
            "tab": {
                "tab_id": "t2",
                "url": "https://actionbook.dev",
                "title": "Actionbook",
                "native_tab_id": "ABC123"
            },
            "created": true,
            "new_window": false
        }));
        let out = format_cli_result_json(&action, &result, 10);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.new-tab");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], "t2");
        assert_eq!(d["context"]["url"], "https://actionbook.dev");
        assert_eq!(d["context"]["title"], "Actionbook");
        assert_eq!(d["data"]["tab"]["tab_id"], "t2");
        assert_eq!(d["data"]["tab"]["url"], "https://actionbook.dev");
        assert_eq!(d["data"]["tab"]["title"], "Actionbook");
        assert_eq!(d["data"]["tab"]["native_tab_id"], "ABC123");
        assert_eq!(d["data"]["created"], true);
        assert_eq!(d["data"]["new_window"], false);
    }

    #[test]
    fn tab_nav_json_new_tab_new_window() {
        let action = Action::NewTab {
            session: SessionId::new_unchecked("local-1"),
            url: "about:blank".into(),
            new_window: true,
            window: None,
        };
        let result = ActionResult::ok(json!({
            "tab": {
                "tab_id": "t5",
                "url": "about:blank",
                "title": "New Tab",
                "native_tab_id": "XYZ"
            },
            "created": true,
            "new_window": true
        }));
        let out = format_cli_result_json(&action, &result, 3);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["data"]["new_window"], true);
    }

    #[test]
    fn tab_nav_text_new_tab() {
        let action = Action::NewTab {
            session: SessionId::new_unchecked("local-1"),
            url: "https://actionbook.dev".into(),
            new_window: false,
            window: None,
        };
        let result = ActionResult::ok(json!({
            "tab": {
                "tab_id": "t2",
                "url": "https://actionbook.dev",
                "title": "Actionbook",
                "native_tab_id": "ABC"
            },
            "created": true,
            "new_window": false
        }));
        let out = format_cli_result(&action, &result);
        assert_eq!(
            out,
            "[local-1 t2] https://actionbook.dev\nok browser.new-tab\ntitle: Actionbook"
        );
    }

    #[test]
    fn tab_nav_json_close_tab() {
        let action = Action::CloseTab {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(3),
        };
        let result = ActionResult::ok(json!({"closed_tab_id": "t3"}));
        let out = format_cli_result_json(&action, &result, 8);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.close-tab");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], "t3");
        assert_eq!(d["data"]["closed_tab_id"], "t3");
    }

    #[test]
    fn tab_nav_text_close_tab() {
        let action = Action::CloseTab {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(3),
        };
        let result = ActionResult::ok(json!({"closed_tab_id": "t3"}));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1 t3]"));
        assert!(out.contains("ok browser.close-tab"));
    }

    #[test]
    fn tab_nav_json_goto() {
        let action = Action::Goto {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            url: "https://actionbook.dev/new".into(),
        };
        let result = ActionResult::ok(json!({
            "kind": "goto",
            "requested_url": "https://actionbook.dev/new",
            "from_url": "https://actionbook.dev",
            "to_url": "https://actionbook.dev/new"
        }));
        let out = format_cli_result_json(&action, &result, 15);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.goto");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], "t0");
        assert_eq!(d["context"]["url"], "https://actionbook.dev/new");
        assert_eq!(d["data"]["kind"], "goto");
        assert_eq!(d["data"]["requested_url"], "https://actionbook.dev/new");
        assert_eq!(d["data"]["from_url"], "https://actionbook.dev");
        assert_eq!(d["data"]["to_url"], "https://actionbook.dev/new");
    }

    #[test]
    fn tab_nav_text_goto() {
        let action = Action::Goto {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            url: "https://actionbook.dev/new".into(),
        };
        let result = ActionResult::ok(json!({
            "kind": "goto",
            "requested_url": "https://actionbook.dev/new",
            "from_url": "https://actionbook.dev",
            "to_url": "https://actionbook.dev/new"
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1 t0] https://actionbook.dev/new"));
        assert!(out.contains("ok browser.goto"));
        assert!(out.contains("https://actionbook.dev \u{2192} https://actionbook.dev/new"));
    }

    #[test]
    fn tab_nav_text_goto_same_url_no_arrow() {
        let action = Action::Goto {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            url: "https://actionbook.dev".into(),
        };
        let result = ActionResult::ok(json!({
            "kind": "goto",
            "requested_url": "https://actionbook.dev",
            "from_url": "https://actionbook.dev",
            "to_url": "https://actionbook.dev"
        }));
        let out = format_cli_result(&action, &result);
        assert!(!out.contains("\u{2192}"));
    }

    #[test]
    fn tab_nav_json_back() {
        let action = Action::Back {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({
            "kind": "back",
            "from_url": "https://actionbook.dev/page2",
            "to_url": "https://actionbook.dev/page1"
        }));
        let out = format_cli_result_json(&action, &result, 6);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.back");
        assert_eq!(d["context"]["url"], "https://actionbook.dev/page1");
        assert_eq!(d["data"]["kind"], "back");
        assert_eq!(d["data"]["from_url"], "https://actionbook.dev/page2");
        assert_eq!(d["data"]["to_url"], "https://actionbook.dev/page1");
    }

    #[test]
    fn tab_nav_text_back() {
        let action = Action::Back {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({
            "kind": "back",
            "from_url": "https://actionbook.dev/page2",
            "to_url": "https://actionbook.dev/page1"
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1 t0] https://actionbook.dev/page1"));
        assert!(out.contains("ok browser.back"));
        assert!(out.contains("https://actionbook.dev/page2 \u{2192} https://actionbook.dev/page1"));
    }

    #[test]
    fn tab_nav_json_forward() {
        let action = Action::Forward {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
        };
        let result = ActionResult::ok(json!({
            "kind": "forward",
            "from_url": "https://actionbook.dev/a",
            "to_url": "https://actionbook.dev/b"
        }));
        let out = format_cli_result_json(&action, &result, 4);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["command"], "browser.forward");
        assert_eq!(d["context"]["tab_id"], "t1");
        assert_eq!(d["context"]["url"], "https://actionbook.dev/b");
        assert_eq!(d["data"]["kind"], "forward");
    }

    #[test]
    fn tab_nav_text_forward() {
        let action = Action::Forward {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(1),
        };
        let result = ActionResult::ok(json!({
            "kind": "forward",
            "from_url": "https://actionbook.dev/a",
            "to_url": "https://actionbook.dev/b"
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("[local-1 t1] https://actionbook.dev/b"));
        assert!(out.contains("ok browser.forward"));
    }

    #[test]
    fn tab_nav_json_reload() {
        let action = Action::Reload {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({
            "kind": "reload",
            "from_url": "https://actionbook.dev",
            "to_url": "https://actionbook.dev"
        }));
        let out = format_cli_result_json(&action, &result, 20);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.reload");
        assert_eq!(d["context"]["url"], "https://actionbook.dev");
        assert_eq!(d["data"]["kind"], "reload");
        assert_eq!(d["data"]["from_url"], "https://actionbook.dev");
        assert_eq!(d["data"]["to_url"], "https://actionbook.dev");
    }

    #[test]
    fn tab_nav_text_reload_same_url() {
        let action = Action::Reload {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({
            "kind": "reload",
            "from_url": "https://actionbook.dev",
            "to_url": "https://actionbook.dev"
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("[local-1 t0] https://actionbook.dev"));
        assert!(out.contains("ok browser.reload"));
        // Same URL, no arrow
        assert!(!out.contains("\u{2192}"));
    }

    #[test]
    fn tab_nav_text_reload_redirect() {
        let action = Action::Reload {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({
            "kind": "reload",
            "from_url": "https://actionbook.dev/old",
            "to_url": "https://actionbook.dev/new"
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("https://actionbook.dev/old \u{2192} https://actionbook.dev/new"));
    }

    #[test]
    fn tab_nav_error_uses_prd_envelope_json() {
        let action = Action::Goto {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            url: "https://actionbook.dev".into(),
        };
        let result = ActionResult::fatal(
            "navigation_failed",
            "net::ERR_NAME_NOT_RESOLVED",
            "check the URL",
        );
        let out = format_cli_result_json(&action, &result, 50);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], false);
        assert_eq!(d["command"], "browser.goto");
        assert_eq!(d["error"]["code"], "NAVIGATION_FAILED");
    }

    #[test]
    fn tab_nav_error_uses_text_contract() {
        let action = Action::CloseTab {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(5),
        };
        let result = ActionResult::fatal("tab_not_found", "tab t5 does not exist", "run list-tabs");
        let out = format_cli_result(&action, &result);
        assert_eq!(
            out,
            "[local-1 t5]\nerror TAB_NOT_FOUND: tab t5 does not exist"
        );
    }

    // -----------------------------------------------------------------------
    // Phase B2a: Observation / Query / Logging tests
    // -----------------------------------------------------------------------

    #[test]
    fn observation_json_envelope_wraps_title_result() {
        let action = Action::Title {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!("My Page Title"));
        let out = format_cli_result_json(&action, &result, 8);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.title");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], "t0");
        assert_eq!(d["data"]["value"], "My Page Title");
        assert_eq!(d["meta"]["duration_ms"], 8);
        assert!(d["error"].is_null());
    }

    #[test]
    fn observation_json_envelope_wraps_viewport_result() {
        let action = Action::Viewport {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({"width": 1280, "height": 720}));
        let out = format_cli_result_json(&action, &result, 3);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.viewport");
        assert_eq!(d["data"]["width"], 1280);
        assert_eq!(d["data"]["height"], 720);
        assert_eq!(d["meta"]["duration_ms"], 3);
    }

    #[test]
    fn observation_json_envelope_wraps_query_result() {
        let action = Action::Query {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: ".item".into(),
            mode: crate::daemon::types::QueryMode::Css,
            cardinality: crate::daemon::types::QueryCardinality::All,
            nth_index: None,
        };
        let result = ActionResult::ok(json!({
            "mode": "all",
            "count": 2,
            "items": [{"selector": ".item:nth-child(1)"}, {"selector": ".item:nth-child(2)"}]
        }));
        let out = format_cli_result_json(&action, &result, 12);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.query");
        assert_eq!(d["data"]["mode"], "all");
        assert_eq!(d["data"]["count"], 2);
    }

    #[test]
    fn observation_text_formats_title() {
        let action = Action::Title {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!("My Page Title"));
        let out = format_cli_result(&action, &result);
        assert_eq!(out, "My Page Title");
    }

    #[test]
    fn observation_text_formats_viewport() {
        let action = Action::Viewport {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({"width": 1920, "height": 1080}));
        let out = format_cli_result(&action, &result);
        assert_eq!(out, "1920x1080");
    }

    #[test]
    fn observation_json_logs_console_wraps_array() {
        let action = Action::LogsConsole {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            level: None,
            tail: None,
            since: None,
            clear: false,
        };
        let result = ActionResult::ok(json!([
            {"level": "log", "text": "hello"},
            {"level": "warn", "text": "world"}
        ]));
        let out = format_cli_result_json(&action, &result, 5);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.logs.console");
        assert!(d["data"]["items"].is_array());
        assert_eq!(d["data"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(d["data"]["cleared"], false);
    }

    #[test]
    fn observation_json_logs_console_cleared_true_when_flag_set() {
        let action = Action::LogsConsole {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            level: None,
            tail: None,
            since: None,
            clear: true,
        };
        let result = ActionResult::ok(json!([]));
        let out = format_cli_result_json(&action, &result, 1);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            d["data"]["cleared"], true,
            "cleared must reflect --clear flag"
        );
    }

    #[test]
    fn observation_json_value_uses_selector() {
        let action = Action::Value {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#my-input".into(),
        };
        let result = ActionResult::ok(json!("hello world"));
        let out = format_cli_result_json(&action, &result, 4);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.value");
        assert_eq!(d["data"]["value"], "hello world");
        assert_eq!(d["data"]["target"]["selector"], "#my-input");
    }

    // -----------------------------------------------------------------------
    // Phase B2b: Interaction / Wait / Eval JSON envelope tests
    // -----------------------------------------------------------------------

    #[test]
    fn interaction_json_click() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#btn".into(),
            button: None,
            count: None,
            new_tab: false,
            coordinates: None,
        };
        let result = ActionResult::ok(json!({"clicked": "#btn", "x": 100, "y": 200}));
        let out = format_cli_result_json(&action, &result, 5);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.click");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], "t0");
        assert_eq!(d["data"]["action"], "click");
        assert_eq!(d["data"]["target"]["selector"], "#btn");
        assert_eq!(d["data"]["changed"]["url_changed"], false);
        assert_eq!(d["meta"]["duration_ms"], 5);
    }

    #[test]
    fn interaction_text_click() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#btn".into(),
            button: None,
            count: None,
            new_tab: false,
            coordinates: None,
        };
        let result = ActionResult::ok(json!({"clicked": "#btn", "x": 100, "y": 200}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("[local-1 t0]"));
        assert!(out.contains("ok browser.click"));
        assert!(out.contains("target: #btn"));
    }

    #[test]
    fn interaction_text_type() {
        let action = Action::Type {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#input".into(),
            text: "hello".into(),
        };
        let result = ActionResult::ok(json!({"typed": "hello", "selector": "#input"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.type"));
        assert!(out.contains("target: #input"));
        assert!(out.contains("text_length: 5"));
    }

    #[test]
    fn interaction_text_fill() {
        let action = Action::Fill {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#email".into(),
            value: "test@example.com".into(),
        };
        let result = ActionResult::ok(json!({"filled": "#email", "value": "test@example.com"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.fill"));
        assert!(out.contains("target: #email"));
        assert!(out.contains("text_length: 16"));
    }

    #[test]
    fn interaction_text_select() {
        let action = Action::Select {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#dropdown".into(),
            value: "option-2".into(),
            by_text: false,
        };
        let result = ActionResult::ok(json!({"selected": "option-2", "selector": "#dropdown"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.select"));
        assert!(out.contains("target: #dropdown"));
        assert!(out.contains("value: option-2"));
    }

    #[test]
    fn interaction_text_hover() {
        let action = Action::Hover {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#menu".into(),
        };
        let result = ActionResult::ok(json!({"hovered": "#menu", "x": 50, "y": 60}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.hover"));
        assert!(out.contains("target: #menu"));
    }

    #[test]
    fn interaction_text_focus() {
        let action = Action::Focus {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#search".into(),
        };
        let result = ActionResult::ok(json!({"focused": "#search"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.focus"));
        assert!(out.contains("target: #search"));
    }

    #[test]
    fn interaction_text_drag() {
        let action = Action::Drag {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            from_selector: "#source".into(),
            to_selector: "#target".into(),
            button: None,
            to_coordinates: None,
        };
        let result = ActionResult::ok(json!({
            "dragged": {"from": "#source", "to": "#target"},
            "from": {"x": 10, "y": 20},
            "to": {"x": 100, "y": 200}
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.drag"));
        assert!(out.contains("from: #source"));
        assert!(out.contains("to: #target"));
    }

    #[test]
    fn interaction_text_upload() {
        let action = Action::Upload {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#file-input".into(),
            files: vec!["a.txt".into(), "b.txt".into()],
        };
        let result = ActionResult::ok(json!({"uploaded": 2, "selector": "#file-input"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.upload"));
        assert!(out.contains("target: #file-input"));
        assert!(out.contains("count: 2"));
    }

    #[test]
    fn interaction_text_mouse_move() {
        let action = Action::MouseMove {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            x: 150.5,
            y: 250.0,
        };
        let result = ActionResult::ok(json!({"moved": {"x": 150.5, "y": 250.0}}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.mouse-move"));
        assert!(out.contains("x: 150.5"));
        assert!(out.contains("y: 250"));
    }

    #[test]
    fn interaction_json_type() {
        let action = Action::Type {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#input".into(),
            text: "hello".into(),
        };
        let result = ActionResult::ok(json!({"typed": "hello", "selector": "#input"}));
        let out = format_cli_result_json(&action, &result, 3);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.type");
        assert_eq!(d["data"]["action"], "type");
        assert_eq!(d["data"]["target"]["selector"], "#input");
        assert_eq!(d["data"]["value_summary"]["text_length"], 5);
    }

    #[test]
    fn interaction_json_fill() {
        let action = Action::Fill {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#email".into(),
            value: "test@example.com".into(),
        };
        let result = ActionResult::ok(json!({"filled": "#email", "value": "test@example.com"}));
        let out = format_cli_result_json(&action, &result, 4);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.fill");
        assert_eq!(d["data"]["action"], "fill");
        assert_eq!(d["data"]["target"]["selector"], "#email");
        assert_eq!(d["data"]["value_summary"]["text_length"], 16);
    }

    #[test]
    fn interaction_json_select() {
        let action = Action::Select {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#dropdown".into(),
            value: "option-2".into(),
            by_text: false,
        };
        let result = ActionResult::ok(json!({"selected": "option-2", "selector": "#dropdown"}));
        let out = format_cli_result_json(&action, &result, 2);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.select");
        assert_eq!(d["data"]["action"], "select");
        assert_eq!(d["data"]["target"]["selector"], "#dropdown");
        assert_eq!(d["data"]["value_summary"]["value"], "option-2");
        assert_eq!(d["data"]["value_summary"]["by_text"], false);
    }

    #[test]
    fn interaction_json_hover() {
        let action = Action::Hover {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#menu".into(),
        };
        let result = ActionResult::ok(json!({"hovered": "#menu", "x": 50, "y": 60}));
        let out = format_cli_result_json(&action, &result, 1);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.hover");
        assert_eq!(d["data"]["action"], "hover");
        assert_eq!(d["data"]["target"]["selector"], "#menu");
    }

    #[test]
    fn interaction_json_focus() {
        let action = Action::Focus {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#search".into(),
        };
        let result = ActionResult::ok(json!({"focused": "#search"}));
        let out = format_cli_result_json(&action, &result, 1);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.focus");
        assert_eq!(d["data"]["action"], "focus");
        assert_eq!(d["data"]["target"]["selector"], "#search");
    }

    #[test]
    fn interaction_json_press() {
        let action = Action::Press {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            key_or_chord: "Enter".into(),
        };
        let result = ActionResult::ok(json!({"pressed": "Enter"}));
        let out = format_cli_result_json(&action, &result, 1);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.press");
        assert_eq!(d["data"]["action"], "press");
        assert_eq!(d["data"]["keys"], "Enter");
    }

    #[test]
    fn interaction_text_press() {
        let action = Action::Press {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            key_or_chord: "Control+c".into(),
        };
        let result = ActionResult::ok(json!({"pressed": "Control+c"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.press"));
        assert!(out.contains("keys: Control+c"));
    }

    #[test]
    fn interaction_json_drag() {
        let action = Action::Drag {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            from_selector: "#source".into(),
            to_selector: "#target".into(),
            button: None,
            to_coordinates: None,
        };
        let result = ActionResult::ok(json!({
            "dragged": {"from": "#source", "to": "#target"},
            "from": {"x": 10, "y": 20},
            "to": {"x": 100, "y": 200}
        }));
        let out = format_cli_result_json(&action, &result, 10);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.drag");
        assert_eq!(d["data"]["action"], "drag");
        assert_eq!(d["data"]["target"]["from"]["selector"], "#source");
        assert_eq!(d["data"]["target"]["to"]["selector"], "#target");
    }

    #[test]
    fn interaction_json_upload() {
        let action = Action::Upload {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#file-input".into(),
            files: vec!["a.txt".into(), "b.txt".into()],
        };
        let result = ActionResult::ok(json!({"uploaded": 2, "selector": "#file-input"}));
        let out = format_cli_result_json(&action, &result, 5);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.upload");
        assert_eq!(d["data"]["action"], "upload");
        assert_eq!(d["data"]["target"]["selector"], "#file-input");
        assert_eq!(d["data"]["value_summary"]["count"], 2);
    }

    #[test]
    fn interaction_json_scroll() {
        let action = Action::Scroll {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            direction: "down".into(),
            amount: Some(300),
            selector: None,
            container: None,
            align: None,
        };
        let result = ActionResult::ok(json!({"scrolled": "down", "amount": 300}));
        let out = format_cli_result_json(&action, &result, 2);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.scroll");
        assert_eq!(d["data"]["action"], "scroll");
        assert_eq!(d["data"]["direction"], "down");
        assert_eq!(d["data"]["amount"], 300);
        assert_eq!(d["data"]["changed"]["scroll_changed"], true);
    }

    #[test]
    fn interaction_text_scroll_into_view() {
        let action = Action::Scroll {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            direction: "into-view".into(),
            amount: None,
            selector: Some("#footer".into()),
            container: None,
            align: None,
        };
        let result = ActionResult::ok(json!({"scrolled": "into-view", "selector": "#footer"}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.scroll"));
        assert!(out.contains("direction: into-view"));
        assert!(out.contains("target: #footer"));
    }

    #[test]
    fn interaction_json_mouse_move() {
        let action = Action::MouseMove {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            x: 150.5,
            y: 250.0,
        };
        let result = ActionResult::ok(json!({"moved": {"x": 150.5, "y": 250.0}}));
        let out = format_cli_result_json(&action, &result, 1);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.mouse-move");
        assert_eq!(d["data"]["action"], "mouse-move");
        assert_eq!(d["data"]["point"]["x"], 150.5);
        assert_eq!(d["data"]["point"]["y"], 250.0);
    }

    #[test]
    fn interaction_json_cursor_position() {
        let action = Action::CursorPosition {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({"cursor": {"x": 42.0, "y": 99.0}}));
        let out = format_cli_result_json(&action, &result, 1);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.cursor-position");
        assert_eq!(d["data"]["x"], 42.0);
        assert_eq!(d["data"]["y"], 99.0);
    }

    #[test]
    fn interaction_text_cursor_position() {
        let action = Action::CursorPosition {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
        };
        let result = ActionResult::ok(json!({"cursor": {"x": 42.0, "y": 99.0}}));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ok browser.cursor-position"));
        assert!(out.contains("x: 42"));
        assert!(out.contains("y: 99"));
    }

    #[test]
    fn interaction_json_eval() {
        let action = Action::Eval {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            expression: "1 + 1".into(),
        };
        let result = ActionResult::ok(json!(2));
        let out = format_cli_result_json(&action, &result, 3);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.eval");
        assert_eq!(d["data"]["value"], 2);
        assert_eq!(d["data"]["type"], "number");
        assert_eq!(d["data"]["preview"], "2");
    }

    #[test]
    fn interaction_text_eval() {
        let action = Action::Eval {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            expression: "document.title".into(),
        };
        let result = ActionResult::ok(json!("My Page"));
        let out = format_cli_result(&action, &result);
        assert_eq!(out, "My Page");
    }

    // -----------------------------------------------------------------------
    // Phase B2b: Wait command JSON envelope tests
    // -----------------------------------------------------------------------

    #[test]
    fn wait_json_element() {
        let action = Action::WaitElement {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#ready".into(),
            timeout_ms: Some(5000),
        };
        let result = ActionResult::ok(json!({"found": "#ready"}));
        let out = format_cli_result_json(&action, &result, 120);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.wait.element");
        assert_eq!(d["data"]["kind"], "element");
        assert_eq!(d["data"]["satisfied"], true);
        assert_eq!(d["data"]["elapsed_ms"], 120);
        assert_eq!(d["data"]["observed_value"]["selector"], "#ready");
        assert_eq!(d["meta"]["duration_ms"], 120);
    }

    #[test]
    fn wait_text_element() {
        let action = Action::WaitElement {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#ready".into(),
            timeout_ms: Some(5000),
        };
        let result = ActionResult::ok(json!({"found": "#ready"}));
        let out = format_cli_result_with_duration(&action, &result, Some(120));
        assert!(out.contains("ok browser.wait.element"));
        assert!(out.contains("elapsed_ms: 120"));
        assert!(out.contains("target: #ready"));
    }

    #[test]
    fn wait_json_navigation() {
        let action = Action::WaitNavigation {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            timeout_ms: Some(10000),
        };
        let result = ActionResult::ok(json!({
            "navigated": true,
            "url": "https://actionbook.dev/page2",
            "readyState": "complete"
        }));
        let out = format_cli_result_json(&action, &result, 250);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.wait.navigation");
        assert_eq!(d["context"]["url"], "https://actionbook.dev/page2");
        assert_eq!(d["data"]["kind"], "navigation");
        assert_eq!(d["data"]["satisfied"], true);
        assert_eq!(d["data"]["elapsed_ms"], 250);
        assert_eq!(
            d["data"]["observed_value"]["url"],
            "https://actionbook.dev/page2"
        );
        assert_eq!(d["data"]["observed_value"]["ready_state"], "complete");
    }

    #[test]
    fn wait_text_navigation() {
        let action = Action::WaitNavigation {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            timeout_ms: None,
        };
        let result = ActionResult::ok(json!({
            "navigated": true,
            "url": "https://actionbook.dev/page2",
            "readyState": "complete"
        }));
        let out = format_cli_result_with_duration(&action, &result, Some(250));
        assert!(out.contains("ok browser.wait.navigation"));
        assert!(out.contains("elapsed_ms: 250"));
        assert!(out.contains("url: https://actionbook.dev/page2"));
    }

    #[test]
    fn wait_json_network_idle() {
        let action = Action::WaitNetworkIdle {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            timeout_ms: Some(30000),
            idle_time_ms: Some(500),
        };
        let result = ActionResult::ok(json!({"network_idle": true}));
        let out = format_cli_result_json(&action, &result, 600);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.wait.network-idle");
        assert_eq!(d["data"]["kind"], "network-idle");
        assert_eq!(d["data"]["satisfied"], true);
        assert_eq!(d["data"]["elapsed_ms"], 600);
        assert_eq!(d["data"]["observed_value"]["idle"], true);
    }

    #[test]
    fn wait_text_network_idle() {
        let action = Action::WaitNetworkIdle {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            timeout_ms: None,
            idle_time_ms: None,
        };
        let result = ActionResult::ok(json!({"network_idle": true}));
        let out = format_cli_result_with_duration(&action, &result, Some(600));
        assert!(out.contains("ok browser.wait.network-idle"));
        assert!(out.contains("elapsed_ms: 600"));
    }

    #[test]
    fn wait_json_condition() {
        let action = Action::WaitCondition {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            expression: "document.readyState === 'complete'".into(),
            timeout_ms: Some(5000),
        };
        let result = ActionResult::ok(json!({"condition_met": true, "value": true}));
        let out = format_cli_result_json(&action, &result, 80);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.wait.condition");
        assert_eq!(d["data"]["kind"], "condition");
        assert_eq!(d["data"]["satisfied"], true);
        assert_eq!(d["data"]["elapsed_ms"], 80);
        assert_eq!(d["data"]["observed_value"], true);
    }

    #[test]
    fn wait_text_condition() {
        let action = Action::WaitCondition {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            expression: "window.loaded".into(),
            timeout_ms: None,
        };
        let result = ActionResult::ok(json!({"condition_met": true, "value": true}));
        let out = format_cli_result_with_duration(&action, &result, Some(80));
        assert!(out.contains("ok browser.wait.condition"));
        assert!(out.contains("elapsed_ms: 80"));
        assert!(out.contains("observed_value: true"));
    }

    #[test]
    fn interaction_json_error_envelope() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#missing".into(),
            button: None,
            count: None,
            new_tab: false,
            coordinates: None,
        };
        let result = ActionResult::fatal(
            "element_not_found",
            "no element matches #missing",
            "check the selector",
        );
        let out = format_cli_result_json(&action, &result, 3);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], false);
        assert_eq!(d["command"], "browser.click");
        assert!(d["error"]["code"]
            .as_str()
            .unwrap()
            .contains("ELEMENT_NOT_FOUND"));
    }

    #[test]
    fn interaction_text_error() {
        let action = Action::Click {
            session: SessionId::new_unchecked("local-1"),
            tab: TabId(0),
            selector: "#missing".into(),
            button: None,
            count: None,
            new_tab: false,
            coordinates: None,
        };
        let result = ActionResult::fatal(
            "element_not_found",
            "no element matches #missing",
            "check the selector",
        );
        let out = format_cli_result(&action, &result);
        assert!(out.contains("ELEMENT_NOT_FOUND"));
        assert!(out.contains("#missing"));
    }

    #[test]
    fn lifecycle_json_start_context_extracts_tab_info() {
        let action = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        let result = ActionResult::ok(json!({
            "session": {
                "session_id": "research",
                "mode": "local",
                "status": "running",
                "headless": false,
                "cdp_endpoint": null
            },
            "tab": {
                "tab_id": "t0",
                "url": "https://actionbook.dev",
                "title": "Actionbook"
            },
            "reused": false
        }));
        let out = format_cli_result_json(&action, &result, 10);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["context"]["session_id"], "research");
        assert_eq!(decoded["context"]["tab_id"], "t0");
        assert_eq!(decoded["context"]["url"], "https://actionbook.dev");
        assert_eq!(decoded["context"]["title"], "Actionbook");
    }

    #[test]
    fn lifecycle_text_start_no_tab_falls_back_to_session_only() {
        let action = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: true,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        let result = ActionResult::ok(json!({
            "session": {
                "session_id": "local-1",
                "mode": "local",
                "status": "running",
                "headless": true,
                "cdp_endpoint": null
            },
            "tab": null,
            "reused": false
        }));
        let out = format_cli_result(&action, &result);
        assert!(
            out.starts_with("[local-1]\n"),
            "with null tab, should show session-only prefix, got: {out}"
        );
        assert!(out.contains("ok browser.start"));
        assert!(!out.contains("title:"));
    }

    #[test]
    fn lifecycle_json_start_data_preserves_prd_structure() {
        let action = Action::StartSession {
            mode: Mode::Local,
            profile: None,
            headless: false,
            open_url: None,
            cdp_endpoint: None,
            ws_headers: None,
            set_session_id: None,
        };
        let result = ActionResult::ok(json!({
            "session": {
                "session_id": "local-1",
                "mode": "local",
                "status": "running",
                "headless": false,
                "cdp_endpoint": null
            },
            "tab": {
                "tab_id": "t0",
                "url": "about:blank",
                "title": ""
            },
            "reused": false
        }));
        let out = format_cli_result_json(&action, &result, 5);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        // data must preserve session/tab/reused top-level keys
        assert!(decoded["data"]["session"].is_object());
        assert!(decoded["data"]["tab"].is_object());
        assert_eq!(decoded["data"]["reused"], false);
    }
}
