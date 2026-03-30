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
    fn cdp_error_display_format() {
        let err = CdpError {
            code: -32001,
            message: "Session not found".to_string(),
            data: None,
        };
        assert_eq!(format!("{err}"), "CDP Error -32001: Session not found");
    }

    #[test]
    fn cdp_error_with_data_field() {
        let json = r#"{"code":-32000,"message":"Oops","data":{"detail":"extra info"}}"#;
        let err: CdpError = serde_json::from_str(json).unwrap();
        assert_eq!(err.code, -32000);
        assert!(err.data.is_some());
    }

    #[test]
    fn cdp_error_serialize_round_trip() {
        let err = CdpError {
            code: 1,
            message: "test".to_string(),
            data: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: CdpError = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, err.code);
        assert_eq!(back.message, err.message);
    }
}
