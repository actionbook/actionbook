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
        let version_path = path.with_extension("version");

        // Try connecting to an existing daemon
        if let Ok(stream) = UnixStream::connect(&path).await {
            // Wait briefly for version file — daemon may still be writing it
            let mut matched = versions_match(&version_path);
            if !matched {
                for _ in 0..10 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    if versions_match(&version_path) {
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                let (reader, writer) = tokio::io::split(stream);
                return Ok(DaemonClient { reader, writer });
            }
            // Version mismatch confirmed — drop connection, restart daemon
            drop(stream);
            restart_daemon().await?;
            return wait_for_daemon(&path, &ready_path, &version_path).await;
        }

        // Daemon not connectable but process may be running.
        // Wait briefly for version file — daemon may still be starting up.
        if server::is_daemon_running() {
            let mut needs_restart = false;
            for _ in 0..10 {
                if versions_match(&version_path) {
                    break; // Same version, just wait for it to become connectable
                }
                if version_path.exists() {
                    needs_restart = true; // Version file present but mismatched
                    break;
                }
                // No version file yet — daemon may still be writing it
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            // If version file never appeared after 1s, treat as old daemon
            if !needs_restart && !versions_match(&version_path) {
                needs_restart = true;
            }
            if needs_restart {
                restart_daemon().await?;
            }
        }

        // No daemon running — start one
        if !server::is_daemon_running() {
            auto_start_daemon()?;
        }

        wait_for_daemon(&path, &ready_path, &version_path).await
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

/// Check if the running daemon's version matches the CLI binary exactly.
/// Missing or empty version file → `false` (old daemon without version support).
fn versions_match(version_path: &std::path::Path) -> bool {
    let Ok(daemon_version) = std::fs::read_to_string(version_path) else {
        return false;
    };
    let daemon_version = daemon_version.trim();
    !daemon_version.is_empty() && daemon_version == crate::BUILD_VERSION
}

/// Stop the running daemon and start a fresh one with the current binary.
async fn restart_daemon() -> Result<(), CliError> {
    let Some(pid) = server::read_daemon_pid().filter(|&p| p > 0) else {
        // No valid PID — cannot signal old daemon. If flock is still held,
        // don't blindly clean up files (would break the live daemon).
        if server::is_daemon_running() {
            return Err(CliError::Internal(
                "daemon PID file missing/corrupt but daemon is still running".to_string(),
            ));
        }
        // Daemon is truly gone — clean up and start fresh
        cleanup_stale_files();
        return auto_start_daemon();
    };

    eprintln!("daemon version mismatch, restarting (pid={pid})...",);

    // send_sigterm returns false if process is already dead (ESRCH)
    if server::send_sigterm(pid) {
        // Wait for the specific PID to exit (up to 5 seconds).
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if !server::is_pid_alive(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if server::is_pid_alive(pid) {
            return Err(CliError::Internal(
                "old daemon did not exit after SIGTERM (5s timeout)".to_string(),
            ));
        }
    }

    // Before cleaning up, check if a concurrent CLI already started a new
    // daemon with the correct version — if so, skip cleanup and let the
    // caller connect to it via wait_for_daemon.
    let version_path = server::socket_path().with_extension("version");
    if versions_match(&version_path) {
        return Ok(());
    }

    // No matching daemon running — safe to clean up stale files and start
    cleanup_stale_files();

    auto_start_daemon()
}

/// Wait for daemon to be ready and connect (up to 10 seconds).
async fn wait_for_daemon(
    path: &std::path::Path,
    ready_path: &std::path::Path,
    version_path: &std::path::Path,
) -> Result<DaemonClient, CliError> {
    for _ in 0..100 {
        if ready_path.exists()
            && let Ok(stream) = UnixStream::connect(path).await
        {
            if versions_match(version_path) {
                let (reader, writer) = tokio::io::split(stream);
                return Ok(DaemonClient { reader, writer });
            }
            drop(stream); // Old daemon still responding during restart window
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(CliError::DaemonNotRunning)
}

fn cleanup_stale_files() {
    let base = server::socket_path();
    std::fs::remove_file(&base).ok(); // daemon.sock
    std::fs::remove_file(base.with_extension("ready")).ok();
    std::fs::remove_file(base.with_extension("version")).ok();
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

    fn write_version_file(version: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let version_path = dir.path().join("daemon.version");
        std::fs::write(&version_path, version).unwrap();
        (dir, version_path)
    }

    #[test]
    fn versions_match_exact() {
        let (_dir, path) = write_version_file(crate::BUILD_VERSION);
        assert!(versions_match(&path), "exact version must match");
    }

    #[test]
    fn versions_mismatch_empty_file() {
        let (_dir, path) = write_version_file("");
        assert!(
            !versions_match(&path),
            "empty version file must be treated as mismatch (old daemon)"
        );
    }

    #[test]
    fn versions_mismatch_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.version");
        assert!(
            !versions_match(&path),
            "missing version file must be treated as mismatch (old daemon)"
        );
    }

    #[test]
    fn versions_mismatch_different_patch() {
        let (major, minor, patch) = parsed_build_version();
        let daemon_version = format!("{major}.{minor}.{}", patch + 1);
        let (_dir, path) = write_version_file(&daemon_version);
        assert!(
            !versions_match(&path),
            "different patch version must NOT match (full version compare)"
        );
    }

    #[test]
    fn versions_mismatch_different_minor() {
        let (major, minor, _) = parsed_build_version();
        let daemon_version = format!("{major}.{}.0", minor + 1);
        let (_dir, path) = write_version_file(&daemon_version);
        assert!(
            !versions_match(&path),
            "different minor version must NOT match"
        );
    }
}
