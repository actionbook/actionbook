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
pub fn format_cli_result(action: &Action, result: &ActionResult) -> String {
    if result.is_ok() {
        if let Some(output) = format_lifecycle_text(action, result) {
            output
        } else if let Some(output) = format_tab_nav_text(action, result) {
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
        Action::StorageList { .. } => "browser.storage.list",
        Action::StorageGet { .. } => "browser.storage.get",
        Action::StorageSet { .. } => "browser.storage.set",
        Action::StorageDelete { .. } => "browser.storage.delete",
        Action::StorageClear { .. } => "browser.storage.clear",
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
                let session_id = data
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let mut out = prefixed_header(session_id, None, None);
                out.push_str(&format!("\nok {command}\n"));
                out.push_str(&format!("mode: {mode}\n"));
                out.push_str("status: running");
                out
            }
            Action::ListSessions => format_list_sessions_text(data),
            Action::SessionStatus { session } => {
                format_session_status_text(&session.to_string(), data)
            }
            Action::CloseSession { session } => {
                let mut out = prefixed_header(&session.to_string(), None, None);
                out.push_str(&format!("\nok {command}"));
                out
            }
            Action::RestartSession { session } => {
                let mut out = prefixed_header(&session.to_string(), None, None);
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
            let session_id = data.get("session_id")?.as_str()?;
            Some(serde_json::json!({
                "session_id": session_id,
                "tab_id": null,
                "url": null,
                "title": null
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
    let status = first_str(data, &["status", "state"]).unwrap_or("unknown");
    let tabs = first_u64(data, &["tabs_count", "tab_count"]).unwrap_or(0);
    let windows = first_u64(data, &["windows_count", "window_count"]).unwrap_or(0);
    format!(
        "[{session_id}]\nstatus: {}\ntabs: {tabs}\nwindows: {windows}",
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

fn display_lifecycle_status(status: &str) -> &str {
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
            let tab_id = data.get("tab").and_then(|v| v.as_str());
            // The daemon stores the URL in the tab entry; the wire data doesn't carry url/title
            // directly, but the action itself has the url parameter.
            let url = match action {
                Action::NewTab { url, .. } => Some(url.as_str()),
                _ => None,
            };
            Some(serde_json::json!({
                "session_id": session_id,
                "tab_id": tab_id,
                "url": url,
                "title": null
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
            // Daemon returns { total_tabs, tabs: [{ tab_id, url, title }] }
            // Already in PRD shape, pass through.
            data.clone()
        }
        Action::NewTab {
            new_window, url, ..
        } => {
            // Daemon returns { tab, target_id, window }
            let tab_id = data
                .get("tab")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            serde_json::json!({
                "tab": {
                    "tab_id": tab_id,
                    "url": url,
                    "title": ""
                },
                "created": true,
                "new_window": new_window
            })
        }
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
                for tab in &tabs {
                    let tab_id = tab.get("tab_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    out.push_str(&format!("\n[{session_id} {tab_id}] {url}"));
                }
                out
            }
            Action::NewTab { .. } => {
                let tab_id = data.get("tab").and_then(|v| v.as_str()).unwrap_or("?");
                let url = match action {
                    Action::NewTab { url, .. } => url.as_str(),
                    _ => "",
                };
                let title = data
                    .get("tab")
                    .and_then(|_| data.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mut out = prefixed_header(&session_id, Some(tab_id), Some(url));
                out.push_str(&format!("\nok {command}"));
                if !title.is_empty() {
                    out.push_str(&format!("\ntitle: {title}"));
                }
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
            "session_id": "local-1",
            "tab_ids": ["native-tab-1"]
        }));
        let out = format_cli_result_json(&action, &result, 42);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["ok"], true);
        assert_eq!(decoded["command"], "browser.start");
        assert_eq!(decoded["context"]["session_id"], "local-1");
        assert_eq!(decoded["context"]["tab_id"], Value::Null);
        assert_eq!(decoded["context"]["url"], Value::Null);
        assert_eq!(decoded["data"]["session_id"], "local-1");
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
        let result = ActionResult::ok(json!({
            "session": "local-1",
            "state": "running",
            "tab_count": 2,
            "window_count": 1
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1]"));
        assert!(out.contains("status: running"));
        assert!(out.contains("tabs: 2"));
        assert!(out.contains("windows: 1"));
    }

    #[test]
    fn lifecycle_text_start_uses_session_prefix_only() {
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
            "session_id": "local-1",
            "tab_ids": ["native-tab-1"]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[local-1]\n"));
        assert!(!out.contains("[s0 t0]"));
        assert!(out.contains("ok browser.start"));
        assert!(out.contains("mode: local"));
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
            "browser.storage.list"
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
                {"tab_id": "t0", "url": "https://actionbook.dev", "title": "Home"},
                {"tab_id": "t1", "url": "https://actionbook.dev/docs", "title": "Docs"}
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
                {"tab_id": "t0", "url": "https://actionbook.dev", "title": "Home"},
                {"tab_id": "t1", "url": "https://actionbook.dev/docs", "title": ""}
            ]
        }));
        let out = format_cli_result(&action, &result);
        assert!(
            out.starts_with("[local-1]"),
            "list-tabs text should start with session header, got: {out}"
        );
        // New format: session header + one line per tab, no "ok" line
        assert!(
            out.contains("[local-1 t0] https://actionbook.dev"),
            "should contain first tab line, got: {out}"
        );
        assert!(
            out.contains("[local-1 t1] https://actionbook.dev/docs"),
            "should contain second tab line, got: {out}"
        );
        // No "ok" status line in list-tabs format
        assert!(
            !out.contains("ok browser.list-tabs"),
            "list-tabs should not contain ok line, got: {out}"
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
            "tab": "t2",
            "target_id": "ABC123",
            "window": "w0"
        }));
        let out = format_cli_result_json(&action, &result, 10);
        let d: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(d["ok"], true);
        assert_eq!(d["command"], "browser.new-tab");
        assert_eq!(d["context"]["session_id"], "local-1");
        assert_eq!(d["context"]["tab_id"], "t2");
        assert_eq!(d["context"]["url"], "https://actionbook.dev");
        assert_eq!(d["data"]["tab"]["tab_id"], "t2");
        assert_eq!(d["data"]["tab"]["url"], "https://actionbook.dev");
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
            "tab": "t5",
            "target_id": "XYZ",
            "window": "w1"
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
            "tab": "t2",
            "target_id": "ABC",
            "window": "w0"
        }));
        let out = format_cli_result(&action, &result);
        assert!(
            out.starts_with("[local-1 t2] https://actionbook.dev"),
            "new-tab text should start with [session tab] url prefix, got: {out}"
        );
        assert!(
            out.contains("ok browser.new-tab"),
            "new-tab text should contain ok line, got: {out}"
        );
        // New format uses [sid tid] url header instead of separate tab:/url: lines
        assert!(
            !out.contains("tab: t2"),
            "new-tab should not use old 'tab:' format, got: {out}"
        );
        assert!(
            !out.contains("url: https://actionbook.dev"),
            "new-tab should not use old 'url:' format, got: {out}"
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
}
