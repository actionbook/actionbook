use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[cfg(windows)]
use tokio::net::TcpListener;
#[cfg(unix)]
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

/// Port file path (Windows IPC): stores the TCP port the daemon is listening on.
/// On Unix this file is never written; it is only used when `cfg(windows)`.
pub fn port_path() -> PathBuf {
    socket_path().with_extension("port")
}

/// Version file path (same directory as socket).
pub fn version_path() -> PathBuf {
    socket_path().with_extension("version")
}

/// Lock file path (Windows only).
///
/// On Windows the daemon uses a separate `daemon.lock` for the exclusive
/// singleton lock instead of locking `daemon.pid` itself.  Windows mandatory
/// byte-range locks prevent other processes from *reading* a locked file,
/// so keeping the PID in an unlocked file lets the CLI (and tests) read it
/// without ERROR_LOCK_VIOLATION (error code 33).
#[cfg(windows)]
fn lock_path() -> PathBuf {
    socket_path().with_extension("lock")
}

/// Check if a daemon is already running by probing the PID file lock.
///
/// Uses `flock(LOCK_EX | LOCK_NB)` — if the lock cannot be acquired, a daemon
/// holds it and is alive. This is more reliable than PID + `kill(pid, 0)` because
/// the kernel releases the lock automatically when the process exits (even on
/// SIGKILL or `panic = "abort"`), and it avoids cross-user EPERM issues.
#[cfg(unix)]
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

/// Check if a daemon is running on Windows by probing the TCP port stored in
/// `daemon.port`.  Returns `true` only if the file exists, contains a valid
/// port, and a TCP connection to `127.0.0.1:<port>` succeeds within 100 ms.
#[cfg(windows)]
pub fn is_daemon_running() -> bool {
    let Ok(port_str) = std::fs::read_to_string(port_path()) else {
        return false;
    };
    let Ok(port) = port_str.trim().parse::<u16>() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(100),
    )
    .is_ok()
}

#[cfg(not(any(unix, windows)))]
pub fn is_daemon_running() -> bool {
    false
}

/// Read daemon PID from file.
pub fn read_daemon_pid() -> Option<i32> {
    let pid_file = pid_path();
    std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Send SIGTERM to a process.
/// Returns `true` if signal was delivered, `false` if process doesn't exist (ESRCH).
/// Panics on EPERM (wrong user) — caller should validate PID ownership.
#[cfg(unix)]
pub fn send_sigterm(pid: i32) -> bool {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    if kill(pid, 15) == 0 {
        return true;
    }
    // ESRCH (3) = no such process — already dead
    std::io::Error::last_os_error().raw_os_error() != Some(3)
}

/// Terminate a daemon process on Windows using `taskkill /F /PID <pid>`.
/// Returns `true` if `taskkill` exits successfully (process was found and killed).
#[cfg(windows)]
pub fn send_sigterm(pid: i32) -> bool {
    std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(any(unix, windows)))]
pub fn send_sigterm(_pid: i32) -> bool {
    false
}

/// Check if a specific process is still alive (kill -0).
/// Returns `true` if the process exists (including EPERM — the process is
/// alive but we cannot signal it).  Only ESRCH means definitely dead.
#[cfg(unix)]
pub fn is_pid_alive(pid: i32) -> bool {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    if kill(pid, 0) == 0 {
        return true; // Signal succeeded → alive
    }
    // kill failed — check errno: EPERM means alive but no permission
    std::io::Error::last_os_error().raw_os_error() == Some(1) // EPERM = 1
}

/// On Windows, check liveness by probing the daemon TCP port rather than by PID.
/// (The PID argument is accepted for interface compatibility but is ignored.)
#[cfg(windows)]
pub fn is_pid_alive(_pid: i32) -> bool {
    is_daemon_running()
}

#[cfg(not(any(unix, windows)))]
pub fn is_pid_alive(_pid: i32) -> bool {
    false
}

/// Try to acquire an exclusive non-blocking file lock.
///
/// Uses `flock(fd, LOCK_EX | LOCK_NB)`. Returns `true` if the lock was acquired.
/// The lock is held as long as the file descriptor remains open; the kernel
/// releases it automatically when the fd is closed or the process exits.
#[cfg(unix)]
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
#[cfg(unix)]
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

    // Write version file for version mismatch detection
    let ver_path = version_path();
    std::fs::write(&ver_path, crate::BUILD_VERSION)?;

    // Write ready signal with build version for CLI version check
    std::fs::write(&ready_path, crate::BUILD_VERSION)?;

    let registry = new_shared_registry();

    // Bridge is no longer spawned at daemon boot — it lazy-binds on the first
    // `--mode extension` call via `bridge::ensure_bridge`. Non-extension users
    // never touch port 19222, removing the most common source of bind contention.

    // Handle SIGINT, SIGTERM, and SIGHUP (terminal close).
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

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
            _ = sighup.recv() => {
                info!("received SIGHUP, shutting down");
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
                // Windows: entry.job_object drops here (end of if-let block),
                // which calls ChromeJobObject::Drop → TerminateJobObject →
                // kills all Chrome processes (main + helpers) atomically.
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
        // Reap the main process exit status (Chrome is already dead on Windows
        // because the Job Object was terminated when entry dropped above).
        if let Some(child) = chrome {
            crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
        }
    }

    // Cleanup files
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&ready_path).ok();
    std::fs::remove_file(version_path()).ok();
    std::fs::remove_file(&pid_file).ok();

    info!("daemon shutdown complete (pid={})", std::process::id());

    // `pid_file_fd` is dropped here → kernel releases flock
    drop(pid_file_fd);

    Ok(())
}

/// Run the daemon server on Windows using TCP localhost transport.
///
/// Binds a `TcpListener` on `127.0.0.1:0` (OS assigns an ephemeral port) and
/// writes the actual port to `daemon.port` for the CLI to discover.
#[cfg(windows)]
pub async fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "daemon starting (pid={}, version={})",
        std::process::id(),
        crate::BUILD_VERSION
    );
    let pid_file = pid_path();
    let port_file = port_path();
    let lock_file = lock_path();
    let base_path = socket_path();
    let ready_path = base_path.with_extension("ready");

    // Acquire exclusive file lock to prevent two daemons from starting simultaneously.
    // We lock a separate `daemon.lock` file instead of `daemon.pid` because Windows
    // mandatory byte-range locks prevent other processes from reading the locked file
    // (error code 33 / ERROR_LOCK_VIOLATION).  Keeping the PID in an unlocked file
    // lets the CLI and tests read it freely.
    use fs2::FileExt;
    let lock_file_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_file)?;

    let mut locked = lock_file_fd.try_lock_exclusive().is_ok();
    if !locked {
        for attempt in 1..=3 {
            info!("daemon lock held by another process, retrying ({attempt}/3)");
            tokio::time::sleep(Duration::from_secs(1)).await;
            locked = lock_file_fd.try_lock_exclusive().is_ok();
            if locked {
                break;
            }
            if is_daemon_running() {
                info!("another daemon is ready, exiting to let CLI reuse it");
                return Ok(());
            }
        }
        if !locked {
            info!("daemon already running after retries, exiting");
            return Ok(());
        }
    }

    // Write our PID to a separate (unlocked) file so the client can target us
    // with taskkill on version mismatch.
    std::fs::write(&pid_file, std::process::id().to_string())?;

    // Bind TCP listener; OS assigns an ephemeral port in the dynamic range.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    info!(
        "daemon listening on 127.0.0.1:{port} (version {})",
        crate::BUILD_VERSION
    );

    // Write port file for client discovery
    std::fs::write(&port_file, port.to_string())?;

    // Write version and ready files
    std::fs::write(version_path(), crate::BUILD_VERSION)?;
    std::fs::write(&ready_path, crate::BUILD_VERSION)?;

    let registry = new_shared_registry();

    // Bridge is lazy: see `bridge::ensure_bridge`. No bind at daemon boot.

    let mut last_activity = Instant::now();
    let idle_timeout_duration = idle_timeout();
    let mut housekeeping = tokio::time::interval(housekeeping_interval());
    housekeeping.tick().await;

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
                info!("received Ctrl+C, shutting down");
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
                        last_activity = Instant::now();
                    }
            }
        }
    }

    info!(
        "daemon exiting main loop, starting graceful shutdown (pid={})",
        std::process::id()
    );

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
                // Windows: entry.job_object drops here → ChromeJobObject::Drop
                // → TerminateJobObject → kills all Chrome processes atomically.
                entries.push((cdp, chrome));
            }
        }
        entries
    };
    for (cdp, chrome) in entries_to_close {
        if let Some(cdp) = cdp {
            cdp.close().await;
        }
        // Reap the main process exit status (Chrome already dead on Windows).
        if let Some(child) = chrome {
            crate::daemon::chrome_reaper::kill_and_reap_async(child).await;
        }
    }

    // Cleanup files
    std::fs::remove_file(&port_file).ok();
    std::fs::remove_file(&ready_path).ok();
    std::fs::remove_file(version_path()).ok();
    std::fs::remove_file(&pid_file).ok();
    std::fs::remove_file(&lock_file).ok();

    info!("daemon shutdown complete (pid={})", std::process::id());
    // Drop the lock fd — Windows releases the byte-range lock when the fd closes.
    drop(lock_file_fd);
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub async fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    Err("daemon is not supported on this platform".into())
}

/// Generic connection handler — works with any `AsyncRead + AsyncWrite` stream
/// (UnixStream on Unix, TcpStream on Windows).
async fn handle_connection_inner<R, W>(
    mut reader: R,
    mut writer: W,
    registry: &SharedRegistry,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
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

#[cfg(unix)]
async fn handle_connection(
    stream: tokio::net::UnixStream,
    registry: &SharedRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, writer) = stream.into_split();
    handle_connection_inner(reader, writer, registry).await
}

#[cfg(windows)]
async fn handle_connection(
    stream: tokio::net::TcpStream,
    registry: &SharedRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, writer) = stream.into_split();
    handle_connection_inner(reader, writer, registry).await
}

// ─── Unit Tests ──────────────────────────────────────────────────────

/// Windows-specific unit tests.
///
/// These tests reference `port_path()` and Windows-only behaviour.
/// They fail to compile on Windows until the implementation commit adds those
/// functions — satisfying the TDD "red" gate on the Windows CI runner.
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn test_port_path_ends_with_daemon_port() {
        let path = port_path();
        assert!(
            path.to_string_lossy().ends_with("daemon.port"),
            "port_path() should end with 'daemon.port', got: {}",
            path.display()
        );
    }

    #[test]
    fn test_is_daemon_running_returns_false_when_no_port_file() {
        // Point ACTIONBOOK_HOME at an empty temp dir so there is no port file.
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test; no other thread reads ACTIONBOOK_HOME concurrently.
        unsafe {
            std::env::set_var("ACTIONBOOK_HOME", tmp.path().to_str().unwrap());
        }
        let result = is_daemon_running();
        unsafe {
            std::env::remove_var("ACTIONBOOK_HOME");
        }
        assert!(
            !result,
            "is_daemon_running() must return false when daemon.port is absent"
        );
    }

    #[test]
    fn test_send_sigterm_returns_false_for_nonexistent_pid() {
        // A very large PID that is almost certainly not a real process.
        // taskkill /F /PID <nonexistent> exits with non-zero → send_sigterm returns false.
        assert!(
            !send_sigterm(i32::MAX),
            "send_sigterm() on a nonexistent PID should return false"
        );
    }

    #[test]
    fn test_is_pid_alive_returns_false_when_daemon_not_running() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test; no other thread reads ACTIONBOOK_HOME concurrently.
        unsafe {
            std::env::set_var("ACTIONBOOK_HOME", tmp.path().to_str().unwrap());
        }
        let result = is_pid_alive(0);
        unsafe {
            std::env::remove_var("ACTIONBOOK_HOME");
        }
        assert!(
            !result,
            "is_pid_alive() should return false when no daemon TCP port is connectable"
        );
    }

    // parse_idle_timeout is cross-platform; run on Windows too.
    #[test]
    fn test_parse_idle_timeout_default_windows() {
        assert_eq!(
            parse_idle_timeout(None),
            Some(std::time::Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS))
        );
    }
}

#[cfg(all(test, unix))]
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
