use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::net::UnixStream;

use crate::action::Action;
use crate::action_result::ActionResult;
use crate::daemon::server;
use crate::error::CliError;
use crate::utils::wire;

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub struct DaemonClient {
    reader: tokio::io::ReadHalf<UnixStream>,
    writer: tokio::io::WriteHalf<UnixStream>,
}

impl DaemonClient {
    /// Connect to the daemon, auto-starting it if needed.
    pub async fn connect() -> Result<Self, CliError> {
        let path = server::socket_path();

        // Try connecting first
        if let Ok(stream) = UnixStream::connect(&path).await {
            let (reader, writer) = tokio::io::split(stream);
            return Ok(DaemonClient { reader, writer });
        }

        // Only auto-start if no daemon is running
        if !server::is_daemon_running() {
            auto_start_daemon()?;
        }

        // Wait for daemon to be ready (up to 10 seconds)
        let ready_path = path.with_extension("ready");
        for _ in 0..100 {
            if ready_path.exists()
                && let Ok(stream) = UnixStream::connect(&path).await
            {
                let (reader, writer) = tokio::io::split(stream);
                return Ok(DaemonClient { reader, writer });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(CliError::DaemonNotRunning)
    }

    /// Send an action and receive the result.
    pub async fn send_action(&mut self, action: &Action) -> Result<ActionResult, CliError> {
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let payload = wire::serialize_request(id, action)?;
        wire::write_frame(&mut self.writer, &payload).await?;

        let response_payload = wire::read_frame(&mut self.reader).await?;
        let response: wire::Response = serde_json::from_slice(&response_payload)?;
        Ok(response.result)
    }
}

fn auto_start_daemon() -> Result<(), CliError> {
    let exe = std::env::current_exe().map_err(|e| CliError::Internal(e.to_string()))?;

    std::process::Command::new(&exe)
        .arg("__daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| CliError::Internal(format!("failed to start daemon: {e}")))?;

    Ok(())
}
