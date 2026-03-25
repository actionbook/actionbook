//! RPC client for communicating with the actionbook daemon over UDS.
//!
//! The [`DaemonClient`] connects to the daemon's Unix Domain Socket,
//! sends [`Action`]s wrapped in the wire protocol, and returns
//! [`ActionResult`]s. This is the only transport the CLI thin client needs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use super::action::Action;
use super::action_result::ActionResult;
use super::wire::{self, Request, Response};

/// Default timeout for most commands (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default daemon socket path: `~/.actionbook/daemons/v2.sock`.
pub fn default_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".actionbook")
        .join("daemons")
        .join("v2.sock")
}

/// An RPC client that communicates with the daemon via UDS.
pub struct DaemonClient {
    stream: UnixStream,
    next_id: AtomicU64,
    timeout: Duration,
}

impl DaemonClient {
    /// Connect to the daemon at the given socket path.
    ///
    /// Returns a clear error with a hint if the socket is not available.
    pub async fn connect(socket_path: &Path) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            ClientError::ConnectionFailed {
                path: socket_path.to_path_buf(),
                source: e,
            }
        })?;
        Ok(DaemonClient {
            stream,
            next_id: AtomicU64::new(1),
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Override the default timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Send an action to the daemon and wait for the result.
    pub async fn send_action(&mut self, action: Action) -> Result<ActionResult, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, action);

        // Encode and send
        let frame = wire::encode_frame(&request).map_err(ClientError::Serialize)?;
        let send_recv = async {
            self.stream
                .write_all(&frame)
                .await
                .map_err(ClientError::Io)?;
            self.stream.flush().await.map_err(ClientError::Io)?;

            // Read 4-byte length prefix
            let mut len_buf = [0u8; 4];
            self.stream
                .read_exact(&mut len_buf)
                .await
                .map_err(ClientError::Io)?;
            let payload_len = u32::from_le_bytes(len_buf);

            wire::validate_frame_length(payload_len)
                .map_err(|msg| ClientError::Protocol(msg))?;

            // Read payload
            let mut payload = vec![0u8; payload_len as usize];
            self.stream
                .read_exact(&mut payload)
                .await
                .map_err(ClientError::Io)?;

            let response: Response =
                wire::decode_payload(&payload).map_err(ClientError::Deserialize)?;

            if response.id != id {
                return Err(ClientError::Protocol(format!(
                    "response id mismatch: expected {id}, got {}",
                    response.id
                )));
            }

            Ok(response.result)
        };

        tokio::time::timeout(self.timeout, send_recv)
            .await
            .map_err(|_| ClientError::Timeout(self.timeout))?
    }
}

/// Errors that can occur during daemon RPC communication.
#[derive(Debug)]
pub enum ClientError {
    /// Could not connect to the daemon socket.
    ConnectionFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    /// I/O error during send/receive.
    Io(std::io::Error),
    /// Failed to serialize the request.
    Serialize(serde_json::Error),
    /// Failed to deserialize the response.
    Deserialize(serde_json::Error),
    /// Wire protocol violation.
    Protocol(String),
    /// The request timed out.
    Timeout(Duration),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::ConnectionFailed { path, source } => {
                write!(
                    f,
                    "cannot connect to daemon at {}: {}\nhint: daemon not running, run `actionbook browser start`",
                    path.display(),
                    source,
                )
            }
            ClientError::Io(e) => write!(f, "daemon communication error: {e}"),
            ClientError::Serialize(e) => write!(f, "failed to serialize request: {e}"),
            ClientError::Deserialize(e) => write!(f, "failed to deserialize response: {e}"),
            ClientError::Protocol(msg) => write!(f, "protocol error: {msg}"),
            ClientError::Timeout(d) => {
                write!(f, "request timed out after {:.0}s", d.as_secs_f64())
            }
        }
    }
}

impl std::error::Error for ClientError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_socket_path_ends_with_v2_sock() {
        let p = default_socket_path();
        assert!(p.ends_with("v2.sock"));
        assert!(p.to_string_lossy().contains(".actionbook"));
    }

    #[test]
    fn client_error_display_connection_failed() {
        let err = ClientError::ConnectionFailed {
            path: PathBuf::from("/tmp/test.sock"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("daemon not running"));
        assert!(msg.contains("browser start"));
    }

    #[test]
    fn client_error_display_timeout() {
        let err = ClientError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("30s"));
    }
}
