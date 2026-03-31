use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tracing::{error, info, warn};

use super::registry::{SharedRegistry, new_shared_registry};
use super::router;
use crate::action_result::ActionResult;
use crate::config;
use crate::utils::wire;

/// Default idle timeout: 30 minutes.
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30 * 60;

/// Housekeeping interval: check idle state every 60 seconds.
const HOUSEKEEPING_INTERVAL_SECS: u64 = 60;

/// Get daemon socket path.
pub fn socket_path() -> PathBuf {
    let dir = config::actionbook_home();
    // Create directory with restrictive permissions (0700)
    std::fs::create_dir_all(&dir).ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    dir.join("daemon.sock")
}

/// PID file path (same directory as socket).
pub fn pid_path() -> PathBuf {
    socket_path().with_extension("pid")
}

/// Check if a daemon is already running by probing the PID file lock.
///
/// Uses `flock(LOCK_EX | LOCK_NB)` — if the lock cannot be acquired, a daemon
/// holds it and is alive. This is more reliable than PID + `kill(pid, 0)` because
/// the kernel releases the lock automatically when the process exits (even on
/// SIGKILL or `panic = "abort"`), and it avoids cross-user EPERM issues.
pub fn is_daemon_running() -> bool {
    let pid_file = pid_path();
    let Ok(file) = OpenOptions::new().read(true).write(true).open(&pid_file) else {
        return false; // File doesn't exist → no daemon
    };
    if try_lock_exclusive(&file) {
        // We acquired the lock → no daemon is running.
        // Lock is released when `file` is dropped.
        false
    } else {
        // Cannot acquire lock → a daemon holds it → running.
        true
    }
}

/// Read daemon PID from file.
pub fn read_daemon_pid() -> Option<i32> {
    let pid_file = pid_path();
    std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Send SIGTERM to a process.
pub fn send_sigterm(pid: i32) {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, 15); // SIGTERM = 15
}

/// Try to acquire an exclusive non-blocking file lock.
///
/// Uses `flock(fd, LOCK_EX | LOCK_NB)`. Returns `true` if the lock was acquired.
/// The lock is held as long as the file descriptor remains open; the kernel
/// releases it automatically when the fd is closed or the process exits.
fn try_lock_exclusive(file: &std::fs::File) -> bool {
    unsafe extern "C" {
        safe fn flock(fd: i32, operation: i32) -> i32;
    }
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    // LOCK_EX (2) | LOCK_NB (4) = 6
    loop {
        if flock(fd, 6) == 0 {
            return true;
        }
        // Retry on EINTR (signal interrupted); give up on EWOULDBLOCK or other errors.
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return false;
        }
    }
}

/// Parse idle timeout from an optional string value.
///
/// - `None` → default (30 minutes)
/// - `Some("0")` → disabled (returns `None`)
/// - `Some(valid_int)` → that many seconds
/// - `Some(invalid)` → falls back to default (avoids silent misconfiguration)
fn parse_idle_timeout(val: Option<&str>) -> Option<Duration> {
    match val {
        Some("0") => None,
        Some(s) => Some(Duration::from_secs(
            s.parse::<u64>().unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS),
        )),
        None => Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)),
    }
}

/// Read idle timeout from environment variable.
fn idle_timeout() -> Option<Duration> {
    parse_idle_timeout(
        std::env::var("ACTIONBOOK_DAEMON_IDLE_TIMEOUT_SECS")
            .ok()
            .as_deref(),
    )
}

/// Read housekeeping interval from environment variable (for testing).
fn housekeeping_interval() -> Duration {
    std::env::var("ACTIONBOOK_DAEMON_HOUSEKEEPING_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(HOUSEKEEPING_INTERVAL_SECS))
}

/// Run the daemon server (blocking).
pub async fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "daemon starting (pid={}, version={})",
        std::process::id(),
        crate::BUILD_VERSION
    );
    let path = socket_path();
    let pid_file = pid_path();
    let ready_path = path.with_extension("ready");

    // Open or create PID file, then acquire an exclusive flock.
    // The flock is held for the entire daemon lifetime — the kernel releases it
    // automatically when the process exits (even on SIGKILL / panic = "abort").
    let pid_file_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pid_file)?;

    let mut locked = try_lock_exclusive(&pid_file_fd);
    if !locked {
        // Another daemon holds the lock. Retry up to 3 times, giving it time to
        // finish startup. If its socket becomes ready, exit and let the CLI reuse it.
        for attempt in 1..=3 {
            info!("daemon lock held by another process, retrying ({attempt}/3)");
            tokio::time::sleep(Duration::from_secs(1)).await;

            locked = try_lock_exclusive(&pid_file_fd);
            if locked {
                break;
            }

            // Check if the other daemon's socket is connectable.
            if path.exists() && std::os::unix::net::UnixStream::connect(&path).is_ok() {
                info!("another daemon is ready, exiting to let CLI reuse it");
                return Ok(());
            }
        }
        if !locked {
            info!("daemon already running after retries, exiting");
            return Ok(());
        }
    }

    // We hold the lock — write our PID.
    {
        use std::io::Write;
        pid_file_fd.set_len(0)?;
        write!(&pid_file_fd, "{}", std::process::id())?;
    }

    // Remove stale socket (verify it's actually a socket, not a symlink to something else)
    if path.exists() {
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err("daemon socket path is a symlink — refusing to start".into());
        }
        std::fs::remove_file(&path)?;
    }

    let listener = UnixListener::bind(&path)?;
    info!(
        "daemon listening on {} (version {})",
        path.display(),
        crate::BUILD_VERSION
    );

    // Write ready signal with build version for CLI version check
    std::fs::write(&ready_path, crate::BUILD_VERSION)?;

    let registry = new_shared_registry();

    // Handle both SIGINT and SIGTERM
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    // Idle timeout housekeeping
    let mut last_activity = Instant::now();
    let idle_timeout_duration = idle_timeout();
    let mut housekeeping = tokio::time::interval(housekeeping_interval());
    housekeeping.tick().await; // consume the immediate first tick

    loop {
        tokio::select! {
            accept = listener.accept() => {
                last_activity = Instant::now();
                match accept {
                    Ok((stream, _)) => {
                        let reg = registry.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, &reg).await {
                                warn!("connection error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        warn!("accept error: {e}");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down");
                break;
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, shutting down");
                break;
            }
            _ = housekeeping.tick() => {
                if let Some(timeout) = idle_timeout_duration
                    && last_activity.elapsed() > timeout {
                        let has_active = registry.lock().await.has_active_sessions();
                        if !has_active {
                            info!(
                                "idle for {:?} with no active sessions, shutting down",
                                last_activity.elapsed()
                            );
                            break;
                        }
                        // Sessions still active — reset so we don't re-check every tick
                        last_activity = Instant::now();
                    }
            }
        }
    }

    info!(
        "daemon exiting main loop, starting graceful shutdown (pid={})",
        std::process::id()
    );

    // Graceful shutdown: collect all sessions, then release registry lock
    // before slow I/O (CDP close + Chrome kill).
    let entries_to_close = {
        let mut reg = registry.lock().await;
        let session_ids: Vec<String> = reg
            .list()
            .iter()
            .map(|s| s.id.as_str().to_string())
            .collect();
        let mut entries = Vec::new();
        for sid in session_ids {
            if let Some(mut entry) = reg.remove(&sid) {
                let cdp = entry.cdp.take();
                let chrome = entry.chrome_process.take();
                entries.push((cdp, chrome));
            }
        }
        entries
    };
    // Registry lock released — cleanup below runs without blocking.
    for (cdp, chrome) in entries_to_close {
        if let Some(cdp) = cdp {
            cdp.close().await;
        }
        if let Some(child) = chrome {
            crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
        }
    }

    // Cleanup files
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&ready_path).ok();
    std::fs::remove_file(&pid_file).ok();

    info!("daemon shutdown complete (pid={})", std::process::id());

    // `pid_file_fd` is dropped here → kernel releases flock
    drop(pid_file_fd);

    Ok(())
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    registry: &SharedRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut reader, mut writer) = stream.into_split();

    loop {
        let payload = match wire::read_frame(&mut reader).await {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };

        let request: wire::Request = serde_json::from_slice(&payload)?;
        let cmd_name = request.action.command_name().to_owned();
        let addr = request.action.session_tab_label();
        let start = std::time::Instant::now();

        let result = router::route(&request.action, registry).await;
        let elapsed = start.elapsed();

        match &result {
            ActionResult::Ok { .. } => {
                info!("{cmd_name} [{addr}] ok ({elapsed:.0?})");
            }
            ActionResult::Retryable { reason, .. } => {
                warn!("{cmd_name} [{addr}] retryable: {reason} ({elapsed:.0?})");
            }
            ActionResult::UserAction { action, .. } => {
                warn!("{cmd_name} [{addr}] user_action: {action} ({elapsed:.0?})");
            }
            ActionResult::Fatal { code, message, .. } => {
                error!("{cmd_name} [{addr}] fatal({code}): {message} ({elapsed:.0?})");
            }
        }

        let response_payload = wire::serialize_response(request.id, &result)?;
        wire::write_frame(&mut writer, &response_payload).await?;
    }

    Ok(())
}

// ─── Unit Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_lock_exclusive_basic() {
        let tmp = tempfile::NamedTempFile::new().unwrap();

        // First lock should succeed
        let f1 = OpenOptions::new()
            .read(true)
            .write(true)
            .open(tmp.path())
            .unwrap();
        assert!(try_lock_exclusive(&f1), "first lock should succeed");

        // Second fd on the same file should fail (lock is held by f1)
        let f2 = OpenOptions::new()
            .read(true)
            .write(true)
            .open(tmp.path())
            .unwrap();
        assert!(
            !try_lock_exclusive(&f2),
            "second lock should fail while first is held"
        );

        // Drop f1 → releases lock. f2 should now succeed.
        drop(f1);
        assert!(
            try_lock_exclusive(&f2),
            "lock should succeed after first fd is dropped"
        );
    }

    // Test parse_idle_timeout directly (pure function, no env var mutation).

    #[test]
    fn test_parse_idle_timeout_default() {
        assert_eq!(
            parse_idle_timeout(None),
            Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS))
        );
    }

    #[test]
    fn test_parse_idle_timeout_custom() {
        assert_eq!(
            parse_idle_timeout(Some("120")),
            Some(Duration::from_secs(120))
        );
    }

    #[test]
    fn test_parse_idle_timeout_disabled() {
        assert_eq!(parse_idle_timeout(Some("0")), None);
    }

    #[test]
    fn test_parse_idle_timeout_invalid_falls_back_to_default() {
        // Invalid value should NOT silently disable timeout — fall back to default
        assert_eq!(
            parse_idle_timeout(Some("abc")),
            Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS))
        );
        assert_eq!(
            parse_idle_timeout(Some("")),
            Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS))
        );
    }
}
