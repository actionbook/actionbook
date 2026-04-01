use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::action::Action;
use crate::action_result::ActionResult;

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub id: u64,
    pub action: Action,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub result: ActionResult,
}

/// Encode a frame: 4-byte LE length prefix + JSON payload.
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Write a frame to an async writer.
pub async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> std::io::Result<()> {
    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a frame from an async reader.
pub async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_PAYLOAD_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {len} > {MAX_PAYLOAD_SIZE}"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

pub fn serialize_request(id: u64, action: &Action) -> serde_json::Result<Vec<u8>> {
    let req = Request {
        v: PROTOCOL_VERSION,
        id,
        action: action.clone(),
    };
    serde_json::to_vec(&req)
}

pub fn deserialize_response(payload: &[u8]) -> serde_json::Result<Response> {
    serde_json::from_slice(payload)
}

pub fn deserialize_request(payload: &[u8]) -> serde_json::Result<Request> {
    serde_json::from_slice(payload)
}

pub fn serialize_response(id: u64, result: &ActionResult) -> serde_json::Result<Vec<u8>> {
    let resp = Response {
        id,
        result: result.clone(),
    };
    serde_json::to_vec(&resp)
}
