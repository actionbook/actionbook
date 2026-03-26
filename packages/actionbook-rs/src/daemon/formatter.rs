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
        // Arrays and objects get pretty-printed JSON
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
