use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
#[cfg(windows)]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;

use crate::action::Action;
use crate::action_result::ActionResult;
use crate::daemon::server;
use crate::error::CliError;
use crate::utils::wire;

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(unix)]
pub struct DaemonClient {
    reader: tokio::io::ReadHalf<UnixStream>,
    writer: tokio::io::WriteHalf<UnixStream>,
}

#[cfg(windows)]
pub struct DaemonClient {
    reader: tokio::io::ReadHalf<TcpStream>,
    writer: tokio::io::WriteHalf<TcpStream>,
}

#[cfg(not(any(unix, windows)))]
pub struct DaemonClient {
    _private: (),
}

#[cfg(unix)]
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
            restart_daemon("daemon version mismatch", false).await?;
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
                restart_daemon("daemon version mismatch", false).await?;
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

/// Windows daemon client — communicates over TCP localhost.
/// The daemon writes its port to `daemon.port`; we read it on each connect.
#[cfg(windows)]
impl DaemonClient {
    /// Connect to the daemon, auto-starting it if needed.
    pub async fn connect() -> Result<Self, CliError> {
        let base = server::socket_path();
        let port_file = server::port_path();
        let ready_path = base.with_extension("ready");
        let version_path = base.with_extension("version");

        // Try connecting to an existing daemon
        if let Some(port) = read_daemon_port(&port_file) {
            if let Ok(stream) =
                TcpStream::connect(std::net::SocketAddr::from(([127, 0, 0, 1], port))).await
            {
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
                drop(stream);
                restart_daemon_windows("daemon version mismatch", false).await?;
                return wait_for_daemon_windows(&port_file, &ready_path, &version_path).await;
            }
        }

        // Daemon not connectable but process may be running
        if server::is_daemon_running() {
            let mut needs_restart = false;
            for _ in 0..10 {
                if versions_match(&version_path) {
                    break;
                }
                if version_path.exists() {
                    needs_restart = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if !needs_restart && !versions_match(&version_path) {
                needs_restart = true;
            }
            if needs_restart {
                restart_daemon_windows("daemon version mismatch", false).await?;
            }
        }

        if !server::is_daemon_running() {
            auto_start_daemon_windows()?;
        }

        wait_for_daemon_windows(&port_file, &ready_path, &version_path).await
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

#[cfg(not(any(unix, windows)))]
impl DaemonClient {
    pub async fn connect() -> Result<Self, CliError> {
        Err(CliError::Internal(
            "daemon is not supported on this platform".to_string(),
        ))
    }

    pub async fn send_action(&mut self, _action: &Action) -> Result<ActionResult, CliError> {
        Err(CliError::Internal(
            "daemon is not supported on this platform".to_string(),
        ))
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

/// Public wrapper for `actionbook daemon restart`. Stops the running daemon
/// (SIGTERM on Unix, taskkill on Windows), spawns a fresh one, and waits
/// until it is ready to accept connections. The user-facing contract is
/// "after this returns Ok, the next CLI call won't race against an unready
/// daemon".
pub async fn restart_daemon_now() -> Result<(), CliError> {
    #[cfg(unix)]
    {
        restart_daemon("user-requested daemon restart", true).await?;
        // Block until the new daemon writes its ready/version files and the
        // socket is connectable. Without this, the user sees "daemon
        // restarted" but the very next call may race and hit DaemonNotRunning.
        let path = server::socket_path();
        let ready = path.with_extension("ready");
        let version = path.with_extension("version");
        let _client = wait_for_daemon(&path, &ready, &version).await?;
        Ok(())
    }
    #[cfg(windows)]
    {
        restart_daemon_windows("user-requested daemon restart", true).await?;
        // Mirror the Unix readiness wait via the existing Windows path.
        let port_path = server::socket_path().with_extension("port");
        let ready = server::socket_path().with_extension("ready");
        let version = server::socket_path().with_extension("version");
        let _client = wait_for_daemon_windows(&port_path, &ready, &version).await?;
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(CliError::Internal(
            "daemon restart is not supported on this platform".to_string(),
        ))
    }
}

/// Stop the running daemon and start a fresh one with the current binary.
/// `reason` controls the user-facing log line so "version mismatch" doesn't
/// leak into a user-initiated `daemon restart`. When `force` is true the
/// "another CLI already restarted with matching version" short-circuit is
/// skipped — required for user-requested restarts because a crashed daemon
/// can leave stale same-version marker files behind that would otherwise
/// trick us into returning Ok without actually spawning a replacement.
#[cfg(unix)]
async fn restart_daemon(reason: &str, force: bool) -> Result<(), CliError> {
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

    eprintln!("{reason}, restarting daemon (pid={pid})...");

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

    // Concurrent-restart guard: if another CLI has already brought up a
    // fresh daemon (different pid, alive) while we were waiting, skip
    // cleanup so we don't unlink THEIR live socket/ready/version files.
    // This guard runs in BOTH force and non-force mode — it's about
    // process liveness, which a crashed daemon (the failure mode `force`
    // exists to escape) can't fake the way it can fake stale marker files.
    if let Some(current_pid) = server::read_daemon_pid().filter(|&p| p > 0)
        && current_pid != pid
        && server::is_pid_alive(current_pid)
    {
        return Ok(());
    }

    // Non-force path additionally short-circuits on matching version files
    // alone — preserves the original auto-restart optimization where two
    // CLIs racing on a version-mismatch restart don't double-spawn. force
    // mode skips this because a crashed daemon may have left stale
    // same-version markers behind, and `restart_daemon_now` callers need
    // the spawn to actually happen.
    let version_path = server::socket_path().with_extension("version");
    if !force && versions_match(&version_path) {
        return Ok(());
    }

    // No live successor daemon — safe to clean up stale files and start.
    cleanup_stale_files();

    auto_start_daemon()
}

/// Wait for daemon to be ready and connect (up to 10 seconds).
#[cfg(unix)]
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

#[cfg(unix)]
fn cleanup_stale_files() {
    let base = server::socket_path();
    std::fs::remove_file(&base).ok(); // daemon.sock
    std::fs::remove_file(base.with_extension("ready")).ok();
    std::fs::remove_file(base.with_extension("version")).ok();
}

#[cfg(unix)]
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

/// Read the daemon TCP port from the port file.
#[cfg(windows)]
fn read_daemon_port(port_path: &std::path::Path) -> Option<u16> {
    std::fs::read_to_string(port_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Stop the running daemon and start a fresh one on Windows.
///
/// `force`: see [`restart_daemon`] — user-requested restarts must bypass the
/// same-version short-circuit so a crashed-but-marker-left daemon is really
/// respawned.
#[cfg(windows)]
async fn restart_daemon_windows(reason: &str, force: bool) -> Result<(), CliError> {
    let Some(pid) = server::read_daemon_pid().filter(|&p| p > 0) else {
        if server::is_daemon_running() {
            return Err(CliError::Internal(
                "daemon PID file missing/corrupt but daemon is still running".to_string(),
            ));
        }
        cleanup_stale_files_windows();
        return auto_start_daemon_windows();
    };

    eprintln!("{reason}, restarting daemon (pid={pid})...");

    if server::send_sigterm(pid) {
        // On Windows, is_pid_alive() checks the TCP port via daemon.port
        // rather than actual PID liveness.  Remove daemon.port immediately
        // after the force-kill so the liveness check returns false without
        // waiting for the OS to release the socket.
        std::fs::remove_file(server::port_path()).ok();

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if !server::is_pid_alive(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if server::is_pid_alive(pid) {
            return Err(CliError::Internal(
                "old daemon did not exit after taskkill (5s timeout)".to_string(),
            ));
        }
    }

    // See restart_daemon (Unix) for the rationale of both guards.
    if let Some(current_pid) = server::read_daemon_pid().filter(|&p| p > 0)
        && current_pid != pid
        && server::is_pid_alive(current_pid)
    {
        return Ok(());
    }
    let version_path = server::socket_path().with_extension("version");
    if !force && versions_match(&version_path) {
        return Ok(());
    }

    cleanup_stale_files_windows();
    auto_start_daemon_windows()
}

/// Wait for the Windows daemon to be ready and connect (up to 10 seconds).
#[cfg(windows)]
async fn wait_for_daemon_windows(
    port_file: &std::path::Path,
    ready_path: &std::path::Path,
    version_path: &std::path::Path,
) -> Result<DaemonClient, CliError> {
    for _ in 0..100 {
        if ready_path.exists() {
            if let Some(port) = read_daemon_port(port_file) {
                if let Ok(stream) =
                    TcpStream::connect(std::net::SocketAddr::from(([127, 0, 0, 1], port))).await
                {
                    if versions_match(version_path) {
                        let (reader, writer) = tokio::io::split(stream);
                        return Ok(DaemonClient { reader, writer });
                    }
                    drop(stream);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(CliError::DaemonNotRunning)
}

#[cfg(windows)]
fn cleanup_stale_files_windows() {
    let base = server::socket_path();
    std::fs::remove_file(server::port_path()).ok(); // daemon.port
    std::fs::remove_file(base.with_extension("ready")).ok();
    std::fs::remove_file(base.with_extension("version")).ok();
    std::fs::remove_file(base.with_extension("lock")).ok(); // daemon.lock
}

/// Spawn the daemon as a detached process on Windows.
/// Uses `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` so the daemon survives
/// the parent CLI process exiting and has its own console signal group.
#[cfg(windows)]
fn auto_start_daemon_windows() -> Result<(), CliError> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let exe = std::env::current_exe().map_err(|e| CliError::Internal(e.to_string()))?;

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
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
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
