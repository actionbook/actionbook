//! Terminal output formatting for [`ActionResult`].
//!
//! Formats daemon responses for human-readable CLI output with colored
//! status indicators and contextual hints.

use colored::Colorize;
use serde_json::Value;

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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
}
