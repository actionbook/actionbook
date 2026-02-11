use std::time::Duration;

use crate::browser::extension_bridge;
use crate::error::{ActionbookError, Result};

/// Timeout for the bridge to become reachable after spawning.
const BRIDGE_START_TIMEOUT: Duration = Duration::from_secs(5);
/// Polling interval while waiting for the bridge to start.
const BRIDGE_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Ensure the extension bridge is running on the given port.
///
/// 1. TCP-probe the port — if connected, the bridge is already running.
/// 2. Check PID file — if exists but process is dead, clean up stale files.
/// 3. Fork `actionbook extension serve --port {port}` as a detached background process.
/// 4. Poll TCP probe every 100ms for up to 5 seconds.
/// 5. Return error if bridge doesn't start in time.
///
/// Returns `true` if this call auto-started the bridge, `false` if it was already running.
pub async fn ensure_bridge_running(port: u16) -> Result<bool> {
    // Fast path: port is listening — verify it's actually our bridge via PID file.
    // A bare TCP probe can be fooled by unrelated processes occupying the same port.
    if extension_bridge::is_bridge_running(port).await {
        if let Some((pid, recorded_port)) = extension_bridge::read_pid_file().await {
            if recorded_port == port && extension_bridge::is_pid_alive(pid) {
                tracing::debug!("Bridge already running on port {} (PID {})", port, pid);
                return Ok(false);
            }
        }
        // Port is occupied but no matching bridge PID — likely another application
        return Err(ActionbookError::ExtensionError(format!(
            "Port {} is already in use but does not appear to be an Actionbook bridge \
             (no matching PID file found). Another application may be using this port. \
             Try a different port in config.toml under [browser.extension].",
            port
        )));
    }

    // Clean up stale PID/token/port files if the recorded process is dead
    if let Some((pid, recorded_port)) = extension_bridge::read_pid_file().await {
        if recorded_port == port && !extension_bridge::is_pid_alive(pid) {
            tracing::debug!("Cleaning up stale bridge files (PID {} dead)", pid);
            cleanup_files().await;
        }
    }

    // Resolve the path to our own binary
    let exe = std::env::current_exe().map_err(|e| {
        ActionbookError::Other(format!("Cannot determine actionbook binary path: {}", e))
    })?;

    // Spawn `actionbook extension serve --port {port}` as a detached background process
    tracing::info!("Auto-starting bridge on port {} ...", port);
    eprintln!("Auto-starting extension bridge on port {} ...", port);

    spawn_detached(&exe, port)?;

    // Poll TCP until bridge is reachable
    let deadline = tokio::time::Instant::now() + BRIDGE_START_TIMEOUT;

    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(BRIDGE_POLL_INTERVAL).await;
        if extension_bridge::is_bridge_running(port).await {
            tracing::info!("Bridge is now running on port {}", port);
            return Ok(true);
        }
    }

    Err(ActionbookError::ExtensionError(format!(
        "Bridge did not start within 5 seconds on port {}. \
         Check logs or run `actionbook extension serve --port {}` manually.",
        port, port
    )))
}

/// Spawn the bridge as a fully detached background process.
fn spawn_detached(exe: &std::path::Path, port: u16) -> Result<()> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    cmd.args(["extension", "serve", "--port", &port.to_string()])
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
        ActionbookError::ExtensionError(format!(
            "Failed to spawn bridge process: {}. Binary: {}",
            e,
            exe.display()
        ))
    })?;

    // Log PID for debugging then drop the handle.
    // Rust's Child::drop() does NOT kill/wait the child — it only closes
    // internal handles. The child continues running independently via setsid().
    tracing::debug!("Spawned bridge process PID={}", child.id());
    drop(child);

    Ok(())
}

/// Stop the extension bridge running on the given port.
///
/// Reads the PID file, verifies port match, sends SIGTERM, waits with
/// SIGKILL escalation, then cleans up files. Silent no-op if bridge is not running.
pub async fn stop_bridge(port: u16) -> Result<()> {
    let pid_info = extension_bridge::read_pid_file().await;

    let pid = match pid_info {
        Some((pid, recorded_port)) if recorded_port == port => pid,
        _ => {
            // No PID file or port mismatch — check if bridge is actually running
            if extension_bridge::is_bridge_running(port).await {
                tracing::warn!(
                    "Bridge running on port {} but no matching PID file; cannot auto-stop",
                    port
                );
            }
            return Ok(());
        }
    };

    // Guard: PID must be positive and fit in i32 to avoid signaling process groups
    // (pid=0 → caller's group, pid>i32::MAX → wraps to negative → named group)
    if pid == 0 || pid > i32::MAX as u32 {
        tracing::warn!("Invalid PID {} in bridge PID file, cleaning up", pid);
        cleanup_files().await;
        return Ok(());
    }

    // Verify bridge is actually listening before sending signals
    if !extension_bridge::is_bridge_running(port).await {
        if !extension_bridge::is_pid_alive(pid) {
            cleanup_files().await;
        }
        return Ok(());
    }

    // Send SIGTERM for graceful shutdown
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                // Process already gone
                cleanup_files().await;
                return Ok(());
            }
            tracing::warn!("Failed to send SIGTERM to bridge PID {}: {}", pid, err);
            return Ok(());
        }
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            if !extension_bridge::is_pid_alive(pid) {
                cleanup_files().await;
            }
            return Ok(());
        }
    }

    // Wait for graceful exit
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Escalate to SIGKILL if still alive AND the bridge port is still listening.
    // The port re-check mitigates PID reuse: if the port is down, the original
    // bridge is gone and the PID may now belong to an unrelated process.
    #[cfg(unix)]
    {
        if extension_bridge::is_pid_alive(pid) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if extension_bridge::is_pid_alive(pid)
                && extension_bridge::is_bridge_running(port).await
            {
                unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    cleanup_files().await;
    tracing::info!("Bridge stopped (PID {})", pid);

    Ok(())
}

/// Clean up all bridge state files.
async fn cleanup_files() {
    extension_bridge::delete_pid_file().await;
    extension_bridge::delete_port_file().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: bind a TCP listener on an OS-assigned port and return (listener, port).
    async fn bind_random_port() -> (tokio::net::TcpListener, u16) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind random port");
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[tokio::test]
    async fn stop_bridge_noop_when_not_running() {
        // Nothing is listening on this port, no PID file matches → silent no-op
        let (_listener, port) = bind_random_port().await;
        drop(_listener); // free the port immediately
        let result = stop_bridge(port).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn stop_bridge_noop_when_port_has_no_matching_pid() {
        // Something IS listening (our test TCP server), but PID file won't match
        // this random port → returns Ok with a tracing::warn
        let (_listener, port) = bind_random_port().await;
        let result = stop_bridge(port).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn ensure_bridge_detects_port_conflict() {
        // Bind a TCP listener to simulate a foreign process occupying the port.
        // No PID file will match this random port → should return port-conflict error.
        let (_listener, port) = bind_random_port().await;

        let result = ensure_bridge_running(port).await;
        assert!(result.is_err(), "Expected port conflict error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("already in use"),
            "Expected 'already in use' in error, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn ensure_bridge_attempts_spawn_when_port_free() {
        // Free port, nothing listening → ensure_bridge_running will try to spawn
        // the binary. In test env, the spawned process won't have `extension serve`
        // subcommand or will fail to start in time → expect timeout/spawn error.
        let (_listener, port) = bind_random_port().await;
        drop(_listener); // free the port

        let result = ensure_bridge_running(port).await;
        // Either spawned successfully (unlikely in test) or timed out
        match result {
            Ok(true) => {} // bridge somehow started
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("did not start") || msg.contains("spawn") || msg.contains("Bridge"),
                    "Expected bridge start/spawn error, got: {}",
                    msg
                );
            }
            Ok(false) => panic!("Should not return Ok(false) when bridge was not previously running"),
        }
    }
}
