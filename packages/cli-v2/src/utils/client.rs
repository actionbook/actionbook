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
        let ready_path = path.with_extension("ready");

        // Try connecting first
        if let Ok(stream) = UnixStream::connect(&path).await {
            check_version(&ready_path)?;
            let (reader, writer) = tokio::io::split(stream);
            return Ok(DaemonClient { reader, writer });
        }

        // Only auto-start if no daemon is running
        if !server::is_daemon_running() {
            auto_start_daemon()?;
        }

        // Wait for daemon to be ready (up to 10 seconds)
        for _ in 0..100 {
            if ready_path.exists()
                && let Ok(stream) = UnixStream::connect(&path).await
            {
                check_version(&ready_path)?;
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

fn major_minor(version: &str) -> (&str, &str) {
    let core = version.split('-').next().unwrap_or(version);
    let mut parts = core.splitn(3, '.');
    let major = parts.next().unwrap_or("0");
    let minor = parts.next().unwrap_or("0");
    (major, minor)
}

fn check_version(ready_path: &std::path::Path) -> Result<(), CliError> {
    let daemon_version = std::fs::read_to_string(ready_path).unwrap_or_default();
    if major_minor(&daemon_version) != major_minor(crate::BUILD_VERSION) {
        return Err(CliError::VersionMismatch {
            cli: crate::BUILD_VERSION.to_string(),
            daemon: daemon_version,
        });
    }
    Ok(())
}

fn auto_start_daemon() -> Result<(), CliError> {
    let exe = std::env::current_exe().map_err(|e| CliError::Internal(e.to_string()))?;

    // Redirect daemon stderr to a log file for diagnostics.
    // Without this, all tracing output (including exit reasons) is lost.
    let log_path = server::socket_path().with_extension("log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map(std::process::Stdio::from)
        .unwrap_or_else(|_| std::process::Stdio::null());

    std::process::Command::new(&exe)
        .arg("__daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log_file)
        .env(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        )
        .spawn()
        .map_err(|e| CliError::Internal(format!("failed to start daemon: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_build_version() -> (u64, u64, u64) {
        let core = crate::BUILD_VERSION
            .split('-')
            .next()
            .unwrap_or(crate::BUILD_VERSION);
        let mut parts = core.split('.');
        let major = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let minor = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let patch = parts.next().unwrap_or("0").parse().unwrap_or(0);
        (major, minor, patch)
    }

    fn write_ready_file(version: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let ready_path = dir.path().join("daemon.ready");
        std::fs::write(&ready_path, version).unwrap();
        (dir, ready_path)
    }

    #[test]
    fn check_version_accepts_same_major_minor_with_patch_hash_delta() {
        let (major, minor, patch) = parsed_build_version();
        let daemon_version = format!("{major}.{minor}.{}-hash2", patch + 1);
        let (_dir, ready_path) = write_ready_file(&daemon_version);

        let result = check_version(&ready_path);
        assert!(
            result.is_ok(),
            "same major.minor should be compatible: cli={}, daemon={daemon_version}",
            crate::BUILD_VERSION
        );
    }

    #[test]
    fn check_version_accepts_exact_match() {
        let (_dir, ready_path) = write_ready_file(crate::BUILD_VERSION);

        let result = check_version(&ready_path);
        assert!(result.is_ok(), "exact version match should stay compatible");
    }

    #[test]
    fn check_version_accepts_missing_hash_on_daemon_side() {
        let (major, minor, patch) = parsed_build_version();
        let daemon_version = format!("{major}.{minor}.{patch}");
        let (_dir, ready_path) = write_ready_file(&daemon_version);

        let result = check_version(&ready_path);
        assert!(
            result.is_ok(),
            "same major.minor.patch should stay compatible when daemon omits hash: cli={}, daemon={daemon_version}",
            crate::BUILD_VERSION
        );
    }

    #[test]
    fn check_version_rejects_different_minor() {
        let (major, minor, _) = parsed_build_version();
        let daemon_version = format!("{major}.{}.0", minor + 1);
        let (_dir, ready_path) = write_ready_file(&daemon_version);

        let err = check_version(&ready_path).expect_err("different minor must be incompatible");
        match err {
            CliError::VersionMismatch { cli, daemon } => {
                assert_eq!(cli, crate::BUILD_VERSION);
                assert_eq!(daemon, daemon_version);
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }
    }
}
