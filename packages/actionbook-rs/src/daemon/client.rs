use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::lifecycle;
use super::protocol::{self, DaemonRequest, DaemonResponse};
use crate::error::{ActionbookError, Result};

/// Client for communicating with a per-profile daemon over Unix Domain Socket.
pub struct DaemonClient {
    profile: String,
    next_id: AtomicU64,
}

impl DaemonClient {
    pub fn new(profile: String) -> Self {
        Self {
            profile,
            next_id: AtomicU64::new(1),
        }
    }

    /// Ensure the daemon is running and send a CDP command through it.
    ///
    /// Returns `Ok(Value)` on success, or an error if the daemon is unreachable
    /// or the CDP command fails.
    pub async fn send_cdp(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let sock_path = lifecycle::socket_path(&self.profile);

        let mut stream = UnixStream::connect(&sock_path).await.map_err(|e| {
            ActionbookError::DaemonNotRunning(format!(
                "Cannot connect to daemon for profile '{}': {}",
                self.profile, e
            ))
        })?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = DaemonRequest {
            id,
            method: method.to_string(),
            params,
        };

        let encoded = protocol::encode_line(&request)
            .map_err(|e| ActionbookError::DaemonError(format!("Encode error: {}", e)))?;

        // Pre-send failure: daemon hasn't seen the command — safe to fall back.
        stream.write_all(encoded.as_bytes()).await.map_err(|e| {
            ActionbookError::DaemonNotRunning(format!("Failed to write to daemon socket: {}", e))
        })?;

        // ---- POINT OF NO RETURN ----
        // After write_all succeeds, the daemon may have already forwarded the CDP
        // command to the browser. All errors below use DaemonError (not DaemonNotRunning)
        // so the caller can distinguish "never sent" from "maybe sent".

        // Read response line
        let (reader, _writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let line = lines
            .next_line()
            .await
            .map_err(|e| ActionbookError::DaemonError(format!("Read error (command may have been executed): {}", e)))?
            .ok_or_else(|| {
                ActionbookError::DaemonError("Daemon closed connection without response (command may have been executed)".to_string())
            })?;

        let response: DaemonResponse = protocol::decode_line(&line)
            .map_err(|e| ActionbookError::DaemonError(format!("Invalid response: {}", e)))?;

        if response.id != id {
            return Err(ActionbookError::DaemonError(format!(
                "Response ID mismatch: expected {}, got {}",
                id, response.id
            )));
        }

        if let Some(error) = response.error {
            return Err(ActionbookError::DaemonError(error));
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    /// Profile name this client is configured for.
    #[allow(dead_code)]
    pub fn profile(&self) -> &str {
        &self.profile
    }
}

/// Convenience function: ensure daemon is running and send a single CDP command.
///
/// Returns `None` if daemon cannot be started, `Some(Err)` on CDP failure,
/// `Some(Ok(Value))` on success.
#[allow(dead_code)]
pub async fn try_send(
    profile: &str,
    method: &str,
    params: Value,
) -> Option<Result<Value>> {
    // Ensure daemon is running
    if let Err(e) = lifecycle::ensure_daemon(profile).await {
        tracing::warn!("Failed to ensure daemon for profile '{}': {}", profile, e);
        return None;
    }

    let client = DaemonClient::new(profile.to_string());
    Some(client.send_cdp(method, params).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_send_returns_none_for_nonexistent_profile() {
        // No daemon running for this profile, ensure_daemon will fail
        // because there's no session state to connect to
        let result = try_send("nonexistent-test-profile-xyz", "Runtime.evaluate", serde_json::json!({})).await;
        // Either None (daemon couldn't start) or Some(Err) (daemon started but no WS)
        match result {
            None => {} // Expected
            Some(Err(_)) => {} // Also acceptable
            Some(Ok(_)) => panic!("Should not succeed with nonexistent profile"),
        }
    }
}
