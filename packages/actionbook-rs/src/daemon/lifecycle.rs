use std::path::PathBuf;
use std::time::Duration;

use crate::error::{ActionbookError, Result};

/// Base directory for daemon state files.
fn daemons_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".actionbook")
        .join("daemons")
}

/// Sanitize a name for safe use in file paths.
/// Only allows alphanumeric characters, dashes, and underscores.
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if s.is_empty() {
        "default".to_string()
    } else {
        s
    }
}

/// Return the Unix Domain Socket path for a profile (default session).
pub fn socket_path(profile: &str) -> PathBuf {
    socket_path_for_session(profile, None)
}

/// Return the Unix Domain Socket path.
/// Daemon is always profile-scoped; session param is reserved for future use
/// but currently always resolves to the profile-level socket.
fn socket_path_for_session(profile: &str, session: Option<&str>) -> PathBuf {
    let safe_profile = sanitize(profile);
    match session.filter(|s| *s != "default") {
        Some(session) => daemons_dir().join(format!("{}@{}.sock", safe_profile, sanitize(session))),
        None => daemons_dir().join(format!("{}.sock", safe_profile)),
    }
}

/// Return the PID file path for a profile (default session).
#[allow(dead_code)]
pub fn pid_path(profile: &str) -> PathBuf {
    pid_path_for_session(profile, None)
}

/// Return the PID file path.
/// Daemon is always profile-scoped; session param reserved for future use.
fn pid_path_for_session(profile: &str, session: Option<&str>) -> PathBuf {
    let safe_profile = sanitize(profile);
    match session.filter(|s| *s != "default") {
        Some(session) => daemons_dir().join(format!("{}@{}.pid", safe_profile, sanitize(session))),
        None => daemons_dir().join(format!("{}.pid", safe_profile)),
    }
}

/// Check whether the daemon for the given profile (default session) is alive.
pub async fn is_daemon_alive(profile: &str) -> bool {
    is_daemon_alive_for_session(profile, None).await
}

/// Check whether the daemon is alive (internal, profile-scoped).
async fn is_daemon_alive_for_session(profile: &str, session: Option<&str>) -> bool {
    let sock = socket_path_for_session(profile, session);
    if !sock.exists() {
        return false;
    }

    // Try connecting to the socket
    match tokio::net::UnixStream::connect(&sock).await {
        Ok(_stream) => true,
        Err(_) => {
            // Socket file exists but no one is listening — check PID
            if let Some(pid) = read_pid_for_session(profile, session) {
                is_pid_alive(pid)
            } else {
                false
            }
        }
    }
}

/// Ensure the daemon for the given profile (default session) is running.
pub async fn ensure_daemon(profile: &str) -> Result<bool> {
    ensure_daemon_for_session(profile, None).await
}

/// Ensure the daemon for the given profile is running (internal).
/// Daemon is always profile-scoped; session routing happens inside the daemon.
async fn ensure_daemon_for_session(profile: &str, session: Option<&str>) -> Result<bool> {
    if is_daemon_alive_for_session(profile, session).await {
        tracing::debug!(
            "Daemon for profile '{}' session '{:?}' is already running",
            profile,
            session
        );
        return Ok(false);
    }

    // Clean up stale files
    cleanup_files_for_session(profile, session);

    // Spawn daemon
    let exe = std::env::current_exe().map_err(|e| {
        ActionbookError::DaemonError(format!("Cannot determine actionbook binary path: {}", e))
    })?;

    tracing::info!(
        "Auto-starting daemon for profile '{}' session '{:?}' ...",
        profile,
        session
    );
    spawn_detached(&exe, profile, session)?;

    // Poll UDS until daemon is reachable (up to 5 seconds)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if is_daemon_alive_for_session(profile, session).await {
            tracing::info!(
                "Daemon for profile '{}' session '{:?}' is now running",
                profile,
                session
            );
            return Ok(true);
        }
    }

    Err(ActionbookError::DaemonError(format!(
        "Daemon for profile '{}' session '{:?}' did not start within 5 seconds",
        profile, session
    )))
}

/// Stop the daemon for the given profile (default session).
pub async fn stop_daemon(profile: &str) -> Result<()> {
    stop_daemon_for_session(profile, None).await
}

/// Stop the daemon (internal, profile-scoped).
async fn stop_daemon_for_session(profile: &str, session: Option<&str>) -> Result<()> {
    let pid = match read_pid_for_session(profile, session) {
        Some(pid) => pid,
        None => {
            // No PID file — check if socket exists and try to infer state
            if !socket_path_for_session(profile, session).exists() {
                return Ok(()); // Nothing to stop
            }
            cleanup_files_for_session(profile, session);
            return Ok(());
        }
    };

    // Guard: PID must be positive and fit in i32
    if pid == 0 || pid > i32::MAX as u32 {
        tracing::warn!("Invalid PID {} in daemon PID file, cleaning up", pid);
        cleanup_files_for_session(profile, session);
        return Ok(());
    }

    if !is_pid_alive(pid) {
        cleanup_files_for_session(profile, session);
        return Ok(());
    }

    // Send SIGTERM
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                cleanup_files_for_session(profile, session);
                return Ok(());
            }
            return Err(ActionbookError::DaemonError(format!(
                "Failed to send SIGTERM to daemon PID {}: {}",
                pid, err
            )));
        }
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            if !is_pid_alive(pid) {
                cleanup_files_for_session(profile, session);
                return Ok(());
            }
            return Err(ActionbookError::DaemonError(format!(
                "Failed to terminate daemon PID {}",
                pid
            )));
        }
    }

    // Wait for graceful exit
    tokio::time::sleep(Duration::from_millis(500)).await;

    if !is_pid_alive(pid) {
        cleanup_files_for_session(profile, session);
        tracing::info!(
            "Daemon for profile '{}' session '{:?}' stopped (PID {})",
            profile,
            session,
            pid
        );
        return Ok(());
    }

    // Escalate to SIGKILL if still alive
    #[cfg(unix)]
    {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if is_pid_alive(pid) {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // Only clean up files if the process is actually dead
    if is_pid_alive(pid) {
        return Err(ActionbookError::DaemonError(format!(
            "Daemon PID {} did not terminate after SIGKILL",
            pid
        )));
    }

    cleanup_files_for_session(profile, session);
    tracing::info!(
        "Daemon for profile '{}' session '{:?}' stopped (PID {})",
        profile,
        session,
        pid
    );
    Ok(())
}

/// Spawn `actionbook daemon serve --profile <profile>` as a fully detached background process.
/// One daemon per profile — session routing happens inside the daemon.
fn spawn_detached(exe: &std::path::Path, profile: &str, _session: Option<&str>) -> Result<()> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    let args = vec!["daemon", "serve", "--profile", profile];
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // On Unix, use setsid + pre_exec to fully detach from the parent process group
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid() is async-signal-safe and called between fork and exec
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let child = cmd.spawn().map_err(|e| {
        ActionbookError::DaemonError(format!(
            "Failed to spawn daemon process: {}. Binary: {}",
            e,
            exe.display()
        ))
    })?;

    tracing::debug!("Spawned daemon process PID={}", child.id());
    drop(child);
    Ok(())
}

/// Write PID file for the daemon (default session).
#[allow(dead_code)]
pub fn write_pid(profile: &str, pid: u32) -> Result<()> {
    write_pid_for_session(profile, None, pid)
}

/// Write PID file for the daemon for a specific session.
pub fn write_pid_for_session(profile: &str, session: Option<&str>, pid: u32) -> Result<()> {
    let dir = daemons_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(pid_path_for_session(profile, session), pid.to_string())?;
    Ok(())
}

/// Read PID from the PID file for the daemon (default session).
#[allow(dead_code)]
fn read_pid(profile: &str) -> Option<u32> {
    read_pid_for_session(profile, None)
}

/// Read PID from the PID file for a specific session.
fn read_pid_for_session(profile: &str, session: Option<&str>) -> Option<u32> {
    let path = pid_path_for_session(profile, session);
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
}

/// Check if a PID is alive.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // On Windows, use tasklist
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// Clean up socket and PID files for a profile (default session).
#[allow(dead_code)]
pub fn cleanup_files(profile: &str) {
    cleanup_files_for_session(profile, None);
}

/// Clean up socket and PID files for a specific session.
pub fn cleanup_files_for_session(profile: &str, session: Option<&str>) {
    let _ = std::fs::remove_file(socket_path_for_session(profile, session));
    let _ = std::fs::remove_file(pid_path_for_session(profile, session));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_format() {
        let path = socket_path("default");
        assert!(path.to_string_lossy().ends_with("daemons/default.sock"));
    }

    #[test]
    fn pid_path_format() {
        let path = pid_path("my-profile");
        assert!(path.to_string_lossy().ends_with("daemons/my-profile.pid"));
    }

    #[test]
    fn session_aware_socket_path() {
        // None session = default = legacy path
        let path = socket_path_for_session("default", None);
        assert!(path.to_string_lossy().ends_with("daemons/default.sock"));

        // "default" session = same as None
        let path = socket_path_for_session("default", Some("default"));
        assert!(path.to_string_lossy().ends_with("daemons/default.sock"));

        // Named session gets @session suffix
        let path = socket_path_for_session("work", Some("agent1"));
        assert!(path.to_string_lossy().ends_with("daemons/work@agent1.sock"));
    }

    #[test]
    fn session_aware_pid_path() {
        let path = pid_path_for_session("work", Some("agent1"));
        assert!(path.to_string_lossy().ends_with("daemons/work@agent1.pid"));

        let path = pid_path_for_session("work", None);
        assert!(path.to_string_lossy().ends_with("daemons/work.pid"));
    }

    #[tokio::test]
    async fn is_daemon_alive_returns_false_when_no_socket() {
        assert!(!is_daemon_alive("nonexistent-profile-12345").await);
    }
}
