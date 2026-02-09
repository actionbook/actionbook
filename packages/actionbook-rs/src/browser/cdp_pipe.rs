use std::path::Path;

use crate::error::{ActionbookError, Result};

#[cfg(unix)]
use std::os::unix::io::FromRawFd;

/// Minimal CDP pipe client for communicating with Chrome over fd 3/4.
///
/// Chrome's pipe transport uses null-terminated (`\0`) JSON-RPC messages.
/// This is used exclusively to send `Extensions.loadUnpacked` after launch,
/// then converted to a [`PipeKeepAlive`] handle that must be held for the
/// duration of the Chrome process.
pub struct CdpPipe {
    reader: std::fs::File,
    writer: std::fs::File,
}

/// Opaque handle that keeps the CDP pipe's write end open.
///
/// Chrome exits when it detects EOF on its pipe input (fd 3).
/// This handle keeps the parent's write end open, preventing EOF.
/// Drop it when Chrome should exit.
pub struct PipeKeepAlive(#[allow(dead_code)] std::fs::File);

/// Result of creating pipe pairs for Chrome's `--remote-debugging-pipe`.
///
/// Chrome reads commands from its fd 3 and writes responses to its fd 4.
/// We need to provide both pipe ends to the child process via `dup2` in
/// `pre_exec`, while keeping the parent-side handles for communication.
#[cfg(unix)]
pub struct PipePair {
    /// Parent writes commands here -> Chrome reads from fd 3
    pub parent_writer: os_pipe::PipeWriter,
    /// Parent reads responses here <- Chrome writes to fd 4
    pub parent_reader: os_pipe::PipeReader,
    /// Child's read end (dup2'd to fd 3 in pre_exec) -- raw fd
    pub child_read_fd: i32,
    /// Child's write end (dup2'd to fd 4 in pre_exec) -- raw fd
    pub child_write_fd: i32,
}

/// Create the two pipe pairs needed for Chrome's pipe transport.
///
/// Returns a `PipePair` containing both parent and child handles.
/// The caller must `dup2` the child fds to 3 and 4 in `pre_exec`.
#[cfg(unix)]
pub fn create_pipe_pair() -> Result<PipePair> {
    use std::os::unix::io::IntoRawFd;

    // Pipe 1: parent -> Chrome (commands). Chrome reads from fd 3.
    let (cmd_reader, cmd_writer) = os_pipe::pipe()
        .map_err(|e| ActionbookError::Other(format!("Failed to create command pipe: {}", e)))?;

    // Pipe 2: Chrome -> parent (responses). Chrome writes to fd 4.
    let (resp_reader, resp_writer) = os_pipe::pipe()
        .map_err(|e| ActionbookError::Other(format!("Failed to create response pipe: {}", e)))?;

    let child_read_fd = cmd_reader.into_raw_fd();
    let child_write_fd = resp_writer.into_raw_fd();

    Ok(PipePair {
        parent_writer: cmd_writer,
        parent_reader: resp_reader,
        child_read_fd,
        child_write_fd,
    })
}

impl CdpPipe {
    /// Wrap parent-side file handles into a `CdpPipe`.
    ///
    /// # Safety
    /// The raw fds inside `PipePair` must be valid open file descriptors.
    #[cfg(unix)]
    pub fn from_pipe_pair(pair: PipePair) -> Self {
        use std::os::unix::io::IntoRawFd;

        // Convert os_pipe types to std::fs::File for uniform read/write
        let writer_fd = pair.parent_writer.into_raw_fd();
        let reader_fd = pair.parent_reader.into_raw_fd();

        // SAFETY: these fds are freshly created by os_pipe and are valid
        let writer = unsafe { std::fs::File::from_raw_fd(writer_fd) };
        let reader = unsafe { std::fs::File::from_raw_fd(reader_fd) };

        Self { reader, writer }
    }

    /// Send `Extensions.loadUnpacked` via the pipe and return the extension ID
    /// along with a [`PipeKeepAlive`] handle.
    ///
    /// **Blocking**: This performs synchronous pipe I/O. Callers in an async
    /// context must wrap this in `tokio::task::spawn_blocking` to avoid
    /// blocking the tokio runtime thread.
    ///
    /// The returned `PipeKeepAlive` **must** be held for the lifetime of the
    /// Chrome process. Chrome exits when the pipe's write end is closed.
    pub fn load_extension(self, path: &Path) -> Result<(String, PipeKeepAlive)> {
        let CdpPipe {
            mut reader,
            mut writer,
        } = self;

        use std::io::Write;

        let abs_path = path.canonicalize().map_err(|e| {
            ActionbookError::ExtensionError(format!(
                "Failed to canonicalize extension path {}: {}",
                path.display(),
                e
            ))
        })?;

        let request = serde_json::json!({
            "id": 1,
            "method": "Extensions.loadUnpacked",
            "params": {
                "path": abs_path.to_string_lossy()
            }
        });

        let mut msg = serde_json::to_string(&request).map_err(|e| {
            ActionbookError::Other(format!("Failed to serialize CDP request: {}", e))
        })?;
        msg.push('\0');

        tracing::debug!("CDP pipe -> {}", msg.trim_end_matches('\0'));

        writer.write_all(msg.as_bytes()).map_err(|e| {
            ActionbookError::ExtensionError(format!("Failed to write to CDP pipe: {}", e))
        })?;
        writer.flush().map_err(|e| {
            ActionbookError::ExtensionError(format!("Failed to flush CDP pipe: {}", e))
        })?;

        let ext_id = read_null_terminated_response(&mut reader)?;

        // Drop reader (no longer needed). Keep writer open so Chrome
        // doesn't see EOF on fd 3 and exit.
        drop(reader);

        Ok((ext_id, PipeKeepAlive(writer)))
    }

    /// Parse a CDP response JSON string and extract the extension ID or error.
    fn parse_load_extension_response(response_str: &str) -> Result<String> {
        let response: serde_json::Value =
            serde_json::from_str(response_str).map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to parse CDP pipe response: {} (raw: {})",
                    e, response_str
                ))
            })?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "CDP Extensions.loadUnpacked failed: {}",
                message
            )));
        }

        let ext_id = response
            .get("result")
            .and_then(|r| r.get("id"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                ActionbookError::ExtensionError(format!(
                    "CDP response missing result.id: {}",
                    response_str
                ))
            })?;

        Ok(ext_id.to_string())
    }
}

/// Maximum size for a CDP pipe response (1 MB). A legitimate
/// `Extensions.loadUnpacked` response is well under 1 KB; this limit
/// prevents unbounded memory growth from malformed or unexpected data.
const MAX_PIPE_RESPONSE_SIZE: usize = 1_048_576;

/// Read a single null-terminated response from the pipe and parse the extension ID.
fn read_null_terminated_response(reader: &mut std::fs::File) -> Result<String> {
    use std::io::Read;

    let mut buf = Vec::with_capacity(4096);
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                return Err(ActionbookError::ExtensionError(
                    "CDP pipe closed before receiving response".to_string(),
                ));
            }
            Ok(_) => {
                if byte[0] == 0 {
                    break;
                }
                if buf.len() >= MAX_PIPE_RESPONSE_SIZE {
                    return Err(ActionbookError::ExtensionError(
                        "CDP pipe response exceeded maximum size (1 MB)".to_string(),
                    ));
                }
                buf.push(byte[0]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(ActionbookError::ExtensionError(format!(
                    "Failed to read from CDP pipe: {}",
                    e
                )));
            }
        }
    }

    let response_str = String::from_utf8(buf).map_err(|e| {
        ActionbookError::ExtensionError(format!("CDP pipe response is not valid UTF-8: {}", e))
    })?;

    tracing::debug!("CDP pipe <- {}", response_str);

    CdpPipe::parse_load_extension_response(&response_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Verify null-terminated framing: messages end with \0
    #[test]
    fn pipe_framing_round_trip() {
        let (mut mock_chrome_reader, mut parent_writer) = os_pipe::pipe().unwrap();
        let (mut parent_reader, mut mock_chrome_writer) = os_pipe::pipe().unwrap();

        let request =
            r#"{"id":1,"method":"Extensions.loadUnpacked","params":{"path":"/tmp/ext"}}"#;
        let mut msg = request.to_string();
        msg.push('\0');
        parent_writer.write_all(msg.as_bytes()).unwrap();

        // Chrome side reads until null
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            mock_chrome_reader.read_exact(&mut byte).unwrap();
            if byte[0] == 0 {
                break;
            }
            buf.push(byte[0]);
        }
        assert_eq!(String::from_utf8(buf).unwrap(), request);

        // Chrome side sends response
        let response = r#"{"id":1,"result":{"id":"abcdef123456"}}"#;
        let mut resp_msg = response.to_string();
        resp_msg.push('\0');
        mock_chrome_writer.write_all(resp_msg.as_bytes()).unwrap();

        // Parent reads response
        let mut resp_buf = Vec::new();
        loop {
            parent_reader.read_exact(&mut byte).unwrap();
            if byte[0] == 0 {
                break;
            }
            resp_buf.push(byte[0]);
        }
        let received_resp = String::from_utf8(resp_buf).unwrap();
        assert_eq!(received_resp, response);

        let parsed: serde_json::Value = serde_json::from_str(&received_resp).unwrap();
        assert_eq!(
            parsed
                .get("result")
                .unwrap()
                .get("id")
                .unwrap()
                .as_str()
                .unwrap(),
            "abcdef123456"
        );
    }

    /// Test parse_load_extension_response with a success response
    #[test]
    fn parse_success_response() {
        let resp = r#"{"id":1,"result":{"id":"mock-ext-id-12345"}}"#;
        let ext_id = CdpPipe::parse_load_extension_response(resp).unwrap();
        assert_eq!(ext_id, "mock-ext-id-12345");
    }

    /// Test parse_load_extension_response with a CDP error response
    #[test]
    fn parse_error_response() {
        let resp = r#"{"id":1,"error":{"code":-32000,"message":"Extension not found"}}"#;
        let err = CdpPipe::parse_load_extension_response(resp).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Extension not found"),
            "Error should contain the CDP error message, got: {}",
            msg
        );
    }

    /// Test parse_load_extension_response with missing result.id
    #[test]
    fn parse_response_missing_id() {
        let resp = r#"{"id":1,"result":{}}"#;
        let err = CdpPipe::parse_load_extension_response(resp).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing result.id"),
            "Error should mention missing result.id, got: {}",
            msg
        );
    }

    /// Test parse_load_extension_response with invalid JSON
    #[test]
    fn parse_invalid_json() {
        let resp = "not json";
        let err = CdpPipe::parse_load_extension_response(resp).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Failed to parse CDP pipe response"),
            "Error should mention parse failure, got: {}",
            msg
        );
    }

    /// Integration test: CdpPipe.load_extension with a mock Chrome (pipe pair)
    #[test]
    fn load_extension_end_to_end() {
        use std::os::unix::io::IntoRawFd;

        // Create pipes manually (not using create_pipe_pair, to control both ends)
        let (cmd_reader, cmd_writer) = os_pipe::pipe().unwrap();
        let (resp_reader, resp_writer) = os_pipe::pipe().unwrap();

        // Save child-side raw fds before moving parent-side into CdpPipe
        let child_read_fd = cmd_reader.into_raw_fd();
        let child_write_fd = resp_writer.into_raw_fd();

        let parent_writer_fd = cmd_writer.into_raw_fd();
        let parent_reader_fd = resp_reader.into_raw_fd();

        let cdp = CdpPipe {
            writer: unsafe { std::fs::File::from_raw_fd(parent_writer_fd) },
            reader: unsafe { std::fs::File::from_raw_fd(parent_reader_fd) },
        };

        // Spawn mock Chrome thread
        let handle = std::thread::spawn(move || {
            let mut reader = unsafe { std::fs::File::from_raw_fd(child_read_fd) };
            let mut writer = unsafe { std::fs::File::from_raw_fd(child_write_fd) };

            // Read request until null
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                reader.read_exact(&mut byte).unwrap();
                if byte[0] == 0 {
                    break;
                }
                buf.push(byte[0]);
            }

            // Verify it's a valid Extensions.loadUnpacked request
            let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
            assert_eq!(req["method"], "Extensions.loadUnpacked");

            // Send success response
            let resp = r#"{"id":1,"result":{"id":"test-extension-id"}}"#;
            writer.write_all(resp.as_bytes()).unwrap();
            writer.write_all(&[0]).unwrap();
            writer.flush().unwrap();
        });

        // /tmp always exists on unix, so canonicalize will work
        let (ext_id, _keepalive) = cdp.load_extension(Path::new("/tmp")).unwrap();
        assert_eq!(ext_id, "test-extension-id");

        handle.join().unwrap();
    }

    /// Verify PipeKeepAlive keeps the write end open
    #[test]
    fn keepalive_prevents_eof() {
        use std::os::unix::io::IntoRawFd;

        let (cmd_reader, cmd_writer) = os_pipe::pipe().unwrap();
        let (resp_reader, resp_writer) = os_pipe::pipe().unwrap();

        let child_read_fd = cmd_reader.into_raw_fd();
        let child_write_fd = resp_writer.into_raw_fd();

        let parent_writer_fd = cmd_writer.into_raw_fd();
        let parent_reader_fd = resp_reader.into_raw_fd();

        let cdp = CdpPipe {
            writer: unsafe { std::fs::File::from_raw_fd(parent_writer_fd) },
            reader: unsafe { std::fs::File::from_raw_fd(parent_reader_fd) },
        };

        // Mock Chrome: respond, then check if pipe is still open
        let handle = std::thread::spawn(move || {
            let mut reader = unsafe { std::fs::File::from_raw_fd(child_read_fd) };
            let mut writer = unsafe { std::fs::File::from_raw_fd(child_write_fd) };

            // Drain request
            let mut byte = [0u8; 1];
            loop {
                reader.read_exact(&mut byte).unwrap();
                if byte[0] == 0 {
                    break;
                }
            }

            // Send response
            let resp = r#"{"id":1,"result":{"id":"keep-alive-test"}}"#;
            writer.write_all(resp.as_bytes()).unwrap();
            writer.write_all(&[0]).unwrap();
            writer.flush().unwrap();

            // Try to read more — with keepalive held, this should block
            // (not return EOF). We use a non-blocking check via poll.
            use std::os::unix::io::AsRawFd;
            let fd = reader.as_raw_fd();
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLIN | libc::POLLHUP,
                revents: 0,
            };
            // Poll with 100ms timeout — should return 0 (no events = pipe still open)
            let ret = unsafe { libc::poll(&mut pollfd, 1, 100) };
            assert_eq!(ret, 0, "Pipe should still be open while PipeKeepAlive is held");
        });

        let (_ext_id, keepalive) = cdp.load_extension(Path::new("/tmp")).unwrap();

        // keepalive is alive here — Chrome's reader should NOT see EOF
        handle.join().unwrap();

        // Now drop keepalive — Chrome would see EOF and exit
        drop(keepalive);
    }
}
