//! Wire protocol types and length-prefix framing helpers (Layer 3).
//!
//! Transport: UDS (Unix) / Named Pipe (Windows)
//! Framing:   4-byte little-endian length prefix + JSON payload
//! Encoding:  serde_json
//!
//! Request:  `{ "v": 2, "id": 42, "action": { "type": "Goto", ... } }`
//! Response: `{ "id": 42, "result": { "status": "Ok", ... } }`

use serde::{Deserialize, Serialize};

use super::action::Action;
use super::action_result::ActionResult;

/// Current protocol version.
pub const PROTOCOL_VERSION: u32 = 2;

/// Maximum payload size (16 MiB) to prevent unbounded allocations.
pub const MAX_PAYLOAD_SIZE: u32 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

/// A request sent from a client (CLI/MCP/AI SDK) to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Protocol version — must be [`PROTOCOL_VERSION`].
    pub v: u32,
    /// Unique request ID for correlating responses.
    pub id: u64,
    /// The action to execute.
    pub action: Action,
}

/// A response sent from the daemon back to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Matches the request [`id`](Request::id).
    pub id: u64,
    /// The result of executing the action.
    pub result: ActionResult,
}

impl Request {
    /// Create a new request with the current protocol version.
    pub fn new(id: u64, action: Action) -> Self {
        Request {
            v: PROTOCOL_VERSION,
            id,
            action,
        }
    }
}

impl Response {
    /// Create a new response pairing a request ID with a result.
    pub fn new(id: u64, result: ActionResult) -> Self {
        Response { id, result }
    }
}

// ---------------------------------------------------------------------------
// Length-prefix framing helpers
// ---------------------------------------------------------------------------

/// Encode a value as a length-prefixed frame: `[4-byte LE length][JSON payload]`.
///
/// Returns the complete frame as a byte vector.
pub fn encode_frame<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    let payload = serde_json::to_vec(value)?;
    let len = payload.len() as u32;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Read the 4-byte length prefix from a buffer, returning the payload length.
///
/// Returns `None` if the buffer has fewer than 4 bytes.
pub fn read_frame_length(buf: &[u8]) -> Option<u32> {
    if buf.len() < 4 {
        return None;
    }
    let len_bytes: [u8; 4] = buf[..4].try_into().ok()?;
    Some(u32::from_le_bytes(len_bytes))
}

/// Decode a JSON payload from bytes into the target type.
pub fn decode_payload<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> serde_json::Result<T> {
    serde_json::from_slice(payload)
}

/// Validate a frame length against [`MAX_PAYLOAD_SIZE`].
///
/// Returns `Err` with a descriptive message if the length exceeds the limit.
pub fn validate_frame_length(len: u32) -> Result<(), String> {
    if len > MAX_PAYLOAD_SIZE {
        Err(format!(
            "frame payload too large: {len} bytes (max {MAX_PAYLOAD_SIZE})"
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_v2::action_result::ActionResult;
    use crate::daemon_v2::types::{Mode, SessionId, TabId};

    #[test]
    fn request_round_trip() {
        let req = Request::new(
            1,
            Action::Goto {
                session: SessionId(0),
                tab: TabId(1),
                url: "https://example.com".into(),
            },
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""v":2"#));
        assert!(json.contains(r#""id":1"#));
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.v, PROTOCOL_VERSION);
        assert_eq!(decoded.id, 1);
    }

    #[test]
    fn response_round_trip() {
        let resp = Response::new(42, ActionResult::ok(serde_json::json!({"clicked": true})));
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, 42);
        assert!(decoded.result.is_ok());
    }

    #[test]
    fn frame_encode_decode_round_trip() {
        let req = Request::new(
            7,
            Action::StartSession {
                mode: Mode::Local,
                profile: None,
                headless: false,
                open_url: None,
                cdp_endpoint: None,
            },
        );
        let frame = encode_frame(&req).unwrap();

        // Verify length prefix
        let len = read_frame_length(&frame).unwrap();
        assert_eq!(len as usize, frame.len() - 4);

        // Decode payload
        let payload = &frame[4..];
        let decoded: Request = decode_payload(payload).unwrap();
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.v, 2);
    }

    #[test]
    fn read_frame_length_too_short() {
        assert!(read_frame_length(&[0, 1]).is_none());
        assert!(read_frame_length(&[]).is_none());
    }

    #[test]
    fn read_frame_length_exact_four_bytes() {
        let bytes = 256u32.to_le_bytes();
        assert_eq!(read_frame_length(&bytes), Some(256));
    }

    #[test]
    fn validate_frame_length_within_limit() {
        assert!(validate_frame_length(1024).is_ok());
        assert!(validate_frame_length(MAX_PAYLOAD_SIZE).is_ok());
    }

    #[test]
    fn validate_frame_length_exceeds_limit() {
        assert!(validate_frame_length(MAX_PAYLOAD_SIZE + 1).is_err());
    }

    #[test]
    fn response_fatal_frame_round_trip() {
        let resp = Response::new(
            99,
            ActionResult::fatal(
                "session_not_found",
                "session s9 does not exist",
                "run `actionbook browser list-sessions`",
            ),
        );
        let frame = encode_frame(&resp).unwrap();
        let len = read_frame_length(&frame).unwrap();
        let payload = &frame[4..4 + len as usize];
        let decoded: Response = decode_payload(payload).unwrap();
        assert_eq!(decoded.id, 99);
        assert!(!decoded.result.is_ok());
    }

    #[test]
    fn request_list_sessions_minimal() {
        let req = Request::new(0, Action::ListSessions);
        let json = serde_json::to_string(&req).unwrap();
        // ListSessions has no fields, should be compact
        assert!(json.contains(r#""type":"ListSessions""#));
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, 0);
    }
}
