use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request sent from CLI client → daemon over UDS (JSON-line).
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonRequest {
    /// Unique request ID for multiplexing responses.
    pub id: u64,
    /// CDP method name (e.g. "Runtime.evaluate", "Page.navigate").
    pub method: String,
    /// CDP params (JSON object).
    #[serde(default)]
    pub params: Value,
    /// Session name for multi-session routing (None = "default").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

/// A response sent from daemon → CLI client over UDS (JSON-line).
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonResponse {
    /// Matches the request `id`.
    pub id: u64,
    /// CDP result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error message on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Optional warning (e.g. pending JavaScript dialog blocking the page).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl DaemonResponse {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
            warning: None,
        }
    }

    pub fn err(id: u64, error: String) -> Self {
        Self {
            id,
            result: None,
            error: Some(error),
            warning: None,
        }
    }

    /// Attach a dialog warning to this response.
    pub fn with_warning(mut self, warning: Option<String>) -> Self {
        self.warning = warning;
        self
    }
}

/// Encode a value as a JSON line (no embedded newlines) terminated by `\n`.
pub fn encode_line<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    Ok(line)
}

/// Decode a JSON line into a value, trimming trailing whitespace.
pub fn decode_line<'a, T: Deserialize<'a>>(line: &'a str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_request() {
        let req = DaemonRequest {
            id: 42,
            method: "Runtime.evaluate".to_string(),
            params: serde_json::json!({"expression": "1+1"}),
            session: None,
        };
        let line = encode_line(&req).unwrap();
        assert!(line.ends_with('\n'));
        assert!(!line[..line.len() - 1].contains('\n'));
        let decoded: DaemonRequest = decode_line(&line).unwrap();
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.method, "Runtime.evaluate");
        assert!(decoded.session.is_none());
    }

    #[test]
    fn round_trip_request_with_session() {
        let req = DaemonRequest {
            id: 10,
            method: "Page.navigate".to_string(),
            params: serde_json::json!({"url": "https://example.com"}),
            session: Some("work".to_string()),
        };
        let line = encode_line(&req).unwrap();
        let decoded: DaemonRequest = decode_line(&line).unwrap();
        assert_eq!(decoded.id, 10);
        assert_eq!(decoded.session.as_deref(), Some("work"));
    }

    #[test]
    fn backward_compat_request_without_session_field() {
        // Simulate an old client that doesn't send the session field
        let json = r#"{"id":1,"method":"Runtime.evaluate","params":{}}"#;
        let decoded: DaemonRequest = serde_json::from_str(json).unwrap();
        assert!(decoded.session.is_none());
    }

    #[test]
    fn round_trip_response_ok() {
        let resp = DaemonResponse::ok(7, serde_json::json!({"value": true}));
        let line = encode_line(&resp).unwrap();
        let decoded: DaemonResponse = decode_line(&line).unwrap();
        assert_eq!(decoded.id, 7);
        assert!(decoded.result.is_some());
        assert!(decoded.error.is_none());
    }

    #[test]
    fn round_trip_response_err() {
        let resp = DaemonResponse::err(3, "something went wrong".to_string());
        let line = encode_line(&resp).unwrap();
        let decoded: DaemonResponse = decode_line(&line).unwrap();
        assert_eq!(decoded.id, 3);
        assert!(decoded.result.is_none());
        assert_eq!(decoded.error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn round_trip_response_with_warning() {
        let resp = DaemonResponse::ok(5, serde_json::json!({"value": true}))
            .with_warning(Some("A JavaScript alert dialog is blocking the page".to_string()));
        let line = encode_line(&resp).unwrap();
        let decoded: DaemonResponse = decode_line(&line).unwrap();
        assert_eq!(decoded.id, 5);
        assert!(decoded.result.is_some());
        assert_eq!(
            decoded.warning.as_deref(),
            Some("A JavaScript alert dialog is blocking the page")
        );
    }

    #[test]
    fn response_without_warning_omits_field() {
        let resp = DaemonResponse::ok(6, serde_json::json!(null));
        let line = encode_line(&resp).unwrap();
        // The warning field should not appear in serialized JSON
        assert!(!line.contains("warning"));
        let decoded: DaemonResponse = decode_line(&line).unwrap();
        assert!(decoded.warning.is_none());
    }

    #[test]
    fn backward_compat_response_without_warning_field() {
        // Simulate a response from an older daemon that doesn't send warning
        let json = r#"{"id":1,"result":{"value":"ok"}}"#;
        let decoded: DaemonResponse = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.id, 1);
        assert!(decoded.warning.is_none());
    }
}
