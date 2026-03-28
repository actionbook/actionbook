//! CDP (Chrome DevTools Protocol) typed message structures
//!
//! Phase 2a Optimization: Replace dynamic Value access with typed deserialization
//! for ~10-15% performance improvement in CDP message parsing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// CDP Response message: { id, result?, error? }
///
/// Note: We use a struct instead of an enum to avoid `#[serde(untagged)]` overhead.
/// CDP Events are not parsed with this type (they're ignored in send_cdp_command).
#[derive(Deserialize, Debug)]
pub struct CdpResponse {
    pub id: i64,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<CdpError>,
}

/// CDP Error structure: { code, message, data? }
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CdpError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CDP Error {}: {}", self.code, self.message)
    }
}

/// CDP Event: Page.javascriptDialogOpening
///
/// Fired when a JavaScript dialog (alert, confirm, prompt, beforeunload) appears.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JavascriptDialogOpeningEvent {
    pub url: String,
    pub message: String,
    #[serde(rename = "type")]
    pub dialog_type: String,
    #[serde(default)]
    pub default_prompt: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub has_browser_handler: Option<bool>,
}

/// Tracks a currently open JavaScript dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDialog {
    pub dialog_type: String,
    pub message: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_prompt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cdp_response() {
        let json = r#"{"id":1,"result":{"value":"test"}}"#;
        let response: CdpResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, 1);
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_parse_cdp_error() {
        let json = r#"{"id":2,"error":{"code":-32000,"message":"Connection closed"}}"#;
        let response: CdpResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, 2);
        assert!(response.result.is_none());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32000);
        assert_eq!(error.message, "Connection closed");
    }

    #[test]
    fn test_parse_cdp_response_with_both_fields() {
        // Although rare, CDP allows both result and error
        let json = r#"{"id":3,"result":null,"error":null}"#;
        let response: CdpResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, 3);
    }

    #[test]
    fn test_parse_dialog_opening_event() {
        let json = r#"{
            "url": "https://example.com",
            "message": "Are you sure?",
            "type": "confirm",
            "defaultPrompt": null,
            "hasBrowserHandler": false
        }"#;
        let event: JavascriptDialogOpeningEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.dialog_type, "confirm");
        assert_eq!(event.message, "Are you sure?");
        assert_eq!(event.url, "https://example.com");
        assert!(event.default_prompt.is_none());
    }

    #[test]
    fn test_parse_dialog_opening_event_alert() {
        let json = r#"{
            "url": "https://example.com/page",
            "message": "Hello!",
            "type": "alert"
        }"#;
        let event: JavascriptDialogOpeningEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.dialog_type, "alert");
        assert_eq!(event.message, "Hello!");
        assert!(event.default_prompt.is_none());
        assert!(event.has_browser_handler.is_none());
    }

    #[test]
    fn test_parse_dialog_opening_event_prompt() {
        let json = r#"{
            "url": "https://example.com",
            "message": "Enter your name:",
            "type": "prompt",
            "defaultPrompt": "John"
        }"#;
        let event: JavascriptDialogOpeningEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.dialog_type, "prompt");
        assert_eq!(event.default_prompt.as_deref(), Some("John"));
    }

    #[test]
    fn test_pending_dialog_serialization() {
        let dialog = PendingDialog {
            dialog_type: "alert".to_string(),
            message: "Test alert".to_string(),
            url: "https://example.com".to_string(),
            default_prompt: None,
        };
        let json = serde_json::to_value(&dialog).unwrap();
        assert_eq!(json["dialogType"], "alert");
        assert_eq!(json["message"], "Test alert");
        assert!(json.get("defaultPrompt").is_none()); // skip_serializing_if
    }
}
