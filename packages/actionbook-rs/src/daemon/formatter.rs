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
    if let Some(output) = format_lifecycle_text(action, result) {
        output
    } else {
        format_result(result)
    }
}

/// Format an [`ActionResult`] for `--json` CLI output, applying the
/// Phase A lifecycle envelope for the first 5 lifecycle commands only.
pub fn format_cli_result_json(action: &Action, result: &ActionResult, duration_ms: u128) -> String {
    if let Some(envelope) = normalize_lifecycle_json(action, result, duration_ms) {
        serde_json::to_string(&envelope).unwrap_or_else(|_| {
            r#"{"ok":false,"command":"internal.serialization","context":null,"data":null,"error":{"code":"INTERNAL_ERROR","message":"failed to serialize result","retryable":false,"details":{"hint":"retry the command"}},"meta":{"duration_ms":0,"warnings":[],"pagination":null,"truncated":false}}"#.to_string()
        })
    } else {
        format_result_json(result)
    }
}

/// Returns true if the result is an error (non-Ok), used for exit code.
pub fn is_error(result: &ActionResult) -> bool {
    !result.is_ok()
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
        _ => normalize_error(result),
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
            let code = err
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("INTERNAL_ERROR");
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("command failed");
            format!("error {code}: {message}")
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

fn normalize_error(result: &ActionResult) -> Value {
    match result {
        ActionResult::Fatal {
            code,
            message,
            hint,
        } => serde_json::json!({
            "code": normalize_error_code(code),
            "message": message,
            "retryable": false,
            "details": { "hint": hint }
        }),
        ActionResult::Retryable { reason, hint } => serde_json::json!({
            "code": normalize_error_code(reason),
            "message": reason,
            "retryable": true,
            "details": { "hint": hint }
        }),
        ActionResult::UserAction { action, hint } => serde_json::json!({
            "code": "USER_ACTION_REQUIRED",
            "message": action,
            "retryable": false,
            "details": { "hint": hint }
        }),
        ActionResult::Ok { .. } => Value::Null,
    }
}

fn normalize_error_code(code: &str) -> String {
    let normalized = code
        .chars()
        .map(|c| match c {
            'a'..='z' => c.to_ascii_uppercase(),
            'A'..='Z' | '0'..='9' => c,
            _ => '_',
        })
        .collect::<String>();
    normalized.trim_matches('_').to_string()
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::action::Action;
    use crate::daemon::types::{Mode, SessionId};
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
    fn ok_object_pretty_printed() {
        let r = ActionResult::ok(json!({"title": "Example", "url": "https://example.com"}));
        let out = format_result(&r);
        assert!(out.contains("title"));
        assert!(out.contains("Example"));
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
        };
        let result = ActionResult::ok(json!({
            "session_id": "s0",
            "tab_ids": ["native-tab-1"]
        }));
        let out = format_cli_result_json(&action, &result, 42);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["ok"], true);
        assert_eq!(decoded["command"], "browser.start");
        assert_eq!(decoded["context"]["session_id"], "s0");
        assert_eq!(decoded["context"]["tab_id"], Value::Null);
        assert_eq!(decoded["context"]["url"], Value::Null);
        assert_eq!(decoded["data"]["session_id"], "s0");
        assert_eq!(decoded["meta"]["duration_ms"], 42);
    }

    #[test]
    fn lifecycle_text_formats_list_sessions() {
        let action = Action::ListSessions;
        let result = ActionResult::ok(json!({
            "sessions": [
                {"id": "s0", "state": "ready", "tab_count": 2}
            ]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.contains("1 session"));
        assert!(out.contains("[s0]"));
        assert!(out.contains("status: running"));
        assert!(out.contains("tabs: 2"));
    }

    #[test]
    fn lifecycle_text_formats_status_with_prefix() {
        let action = Action::SessionStatus {
            session: SessionId(0),
        };
        let result = ActionResult::ok(json!({
            "session": "s0",
            "state": "running",
            "tab_count": 2,
            "window_count": 1
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[s0]"));
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
        };
        let result = ActionResult::ok(json!({
            "session_id": "s0",
            "tab_ids": ["native-tab-1"]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[s0]\n"));
        assert!(!out.contains("[s0 t0]"));
        assert!(out.contains("ok browser.start"));
        assert!(out.contains("mode: local"));
    }

    #[test]
    fn lifecycle_text_restart_uses_session_prefix_only() {
        let action = Action::RestartSession {
            session: SessionId(0),
        };
        let result = ActionResult::ok(json!({
            "session_id": "s0",
            "tab_ids": ["native-tab-1"]
        }));
        let out = format_cli_result(&action, &result);
        assert!(out.starts_with("[s0]\n"));
        assert!(!out.contains("[s0 t0]"));
        assert!(out.contains("ok browser.restart"));
    }

    #[test]
    fn lifecycle_json_envelope_normalizes_list_sessions_fields() {
        let action = Action::ListSessions;
        let result = ActionResult::ok(json!({
            "sessions": [
                {
                    "id": "s0",
                    "mode": "local",
                    "state": "ready",
                    "tab_count": 2
                }
            ]
        }));
        let out = format_cli_result_json(&action, &result, 7);
        let decoded: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(decoded["data"]["total_sessions"], 1);
        assert_eq!(decoded["data"]["sessions"][0]["session_id"], "s0");
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
            session: SessionId(5),
        };
        let result = ActionResult::fatal(
            "session_not_found",
            "session s5 does not exist",
            "run list-sessions",
        );
        let out = format_cli_result(&action, &result);
        assert_eq!(out, "error SESSION_NOT_FOUND: session s5 does not exist");
    }
}
