use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Daemon → CLI response, classified by recovery strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ActionResult {
    Ok {
        data: Value,
    },
    Retryable {
        reason: String,
        hint: String,
    },
    UserAction {
        action: String,
        hint: String,
    },
    Fatal {
        code: String,
        message: String,
        hint: String,
        #[serde(default)]
        details: Option<Value>,
    },
}

impl ActionResult {
    pub fn ok(data: Value) -> Self {
        ActionResult::Ok { data }
    }

    pub fn fatal(code: impl Into<String>, message: impl Into<String>) -> Self {
        ActionResult::Fatal {
            code: code.into(),
            message: message.into(),
            hint: String::new(),
            details: None,
        }
    }

    pub fn fatal_with_hint(
        code: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        ActionResult::Fatal {
            code: code.into(),
            message: message.into(),
            hint: hint.into(),
            details: None,
        }
    }

    pub fn fatal_with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
        details: Value,
    ) -> Self {
        ActionResult::Fatal {
            code: code.into(),
            message: message.into(),
            hint: hint.into(),
            details: Some(details),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ActionResult::Ok { .. })
    }
}
