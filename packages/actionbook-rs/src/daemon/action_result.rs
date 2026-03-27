//! Structured result type for daemon responses to client Actions.
//!
//! Results are classified by recovery strategy so that AI agents and CLI
//! consumers can programmatically decide what to do next.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Result of executing an [`Action`](super::action::Action).
///
/// Every error variant carries a `hint` field that tells the agent or user
/// what to do next (e.g. "run `actionbook browser list-sessions`").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ActionResult {
    /// The action succeeded. `data` contains command-specific structured output.
    Ok { data: Value },

    /// Transient failure — safe to retry (e.g. network blip, CDP timeout).
    Retryable {
        /// Machine-readable reason (e.g. "cdp_timeout").
        reason: String,
        /// Human/agent-readable guidance on what to do next.
        hint: String,
    },

    /// Requires user intervention (e.g. extension not connected, browser closed).
    UserAction {
        /// Description of the action the user needs to take.
        action: String,
        /// Human/agent-readable guidance.
        hint: String,
    },

    /// Unrecoverable error (e.g. session not found, invalid parameters).
    Fatal {
        /// Machine-readable error code (e.g. "session_not_found").
        code: String,
        /// Human-readable error message.
        message: String,
        /// Human/agent-readable guidance on what to do next.
        hint: String,
    },
}

impl ActionResult {
    /// Create a successful result.
    pub fn ok(data: Value) -> Self {
        ActionResult::Ok { data }
    }

    /// Create a retryable error result.
    pub fn retryable(reason: impl Into<String>, hint: impl Into<String>) -> Self {
        ActionResult::Retryable {
            reason: reason.into(),
            hint: hint.into(),
        }
    }

    /// Create a user-action-required result.
    #[allow(dead_code)]
    pub fn user_action(action: impl Into<String>, hint: impl Into<String>) -> Self {
        ActionResult::UserAction {
            action: action.into(),
            hint: hint.into(),
        }
    }

    /// Create a fatal error result.
    pub fn fatal(
        code: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        ActionResult::Fatal {
            code: code.into(),
            message: message.into(),
            hint: hint.into(),
        }
    }

    /// Returns true if this is a successful result.
    pub fn is_ok(&self) -> bool {
        matches!(self, ActionResult::Ok { .. })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_round_trip() {
        let result = ActionResult::ok(serde_json::json!({"title": "Example"}));
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""status":"Ok""#));
        let decoded: ActionResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.is_ok());
        match decoded {
            ActionResult::Ok { data } => {
                assert_eq!(data["title"], "Example");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn retryable_round_trip() {
        let result = ActionResult::retryable("cdp_timeout", "try again in a few seconds");
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""status":"Retryable""#));
        let decoded: ActionResult = serde_json::from_str(&json).unwrap();
        match decoded {
            ActionResult::Retryable { reason, hint } => {
                assert_eq!(reason, "cdp_timeout");
                assert_eq!(hint, "try again in a few seconds");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn user_action_round_trip() {
        let result = ActionResult::user_action(
            "reconnect extension",
            "open Chrome and click the Actionbook extension icon",
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""status":"UserAction""#));
        let decoded: ActionResult = serde_json::from_str(&json).unwrap();
        match decoded {
            ActionResult::UserAction { action, hint } => {
                assert_eq!(action, "reconnect extension");
                assert!(hint.contains("extension icon"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn fatal_round_trip() {
        let result = ActionResult::fatal(
            "session_not_found",
            "session s5 does not exist",
            "run `actionbook browser list-sessions` to see available sessions",
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""status":"Fatal""#));
        let decoded: ActionResult = serde_json::from_str(&json).unwrap();
        assert!(!decoded.is_ok());
        match decoded {
            ActionResult::Fatal {
                code,
                message,
                hint,
            } => {
                assert_eq!(code, "session_not_found");
                assert!(message.contains("s5"));
                assert!(hint.contains("list-sessions"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn all_variants_deserialize_from_status_tag() {
        let cases = vec![
            r#"{"status":"Ok","data":null}"#,
            r#"{"status":"Retryable","reason":"timeout","hint":"retry"}"#,
            r#"{"status":"UserAction","action":"do something","hint":"please"}"#,
            r#"{"status":"Fatal","code":"bad","message":"oops","hint":"fix it"}"#,
        ];
        for json in cases {
            let _: ActionResult = serde_json::from_str(json).unwrap();
        }
    }
}
