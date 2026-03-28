use std::fs::OpenOptions;
use std::path::PathBuf;
use tokio::net::UnixListener;
use tracing::{info, warn};

use super::registry::{SharedRegistry, new_shared_registry};
use super::router;
use crate::config;
use crate::utils::wire;

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

/// Check if a daemon is already running by testing PID file liveness.
pub fn is_daemon_running() -> bool {
    let pid_file = pid_path();
    if !pid_file.exists() {
        return false;
    }
    if let Ok(pid_str) = std::fs::read_to_string(&pid_file)
        && let Ok(pid) = pid_str.trim().parse::<i32>()
        && process_alive(pid)
    {
        return true;
    }
    // Stale PID file — remove it
    std::fs::remove_file(&pid_file).ok();
    false
}

/// Read daemon PID from file.
pub fn read_daemon_pid() -> Option<i32> {
    let pid_file = pid_path();
    std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Check if a process is alive via kill(pid, 0).
fn process_alive(pid: i32) -> bool {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    // kill(pid, 0) returns 0 if process exists
    kill(pid, 0) == 0
}

/// Send SIGTERM to a process.
pub fn send_sigterm(pid: i32) {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, 15); // SIGTERM = 15
}

/// Run the daemon server (blocking).
pub async fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    let path = socket_path();
    let pid_file = pid_path();
    let ready_path = path.with_extension("ready");

    // Atomic PID file creation: O_CREAT | O_EXCL prevents race between two daemons
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pid_file)
    {
        Ok(f) => {
            use std::io::Write;
            let mut f = f;
            write!(f, "{}", std::process::id())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // PID file exists — check if the daemon is actually alive
            if is_daemon_running() {
                eprintln!("daemon already running");
                return Ok(());
            }
            // Stale PID — remove and retry
            std::fs::remove_file(&pid_file).ok();
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pid_file)?;
            use std::io::Write;
            write!(f, "{}", std::process::id())?;
        }
        Err(e) => return Err(e.into()),
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

    loop {
        tokio::select! {
            accept = listener.accept() => {
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
        }
    }

    // Graceful shutdown: kill all Chrome processes
    {
        let mut reg = registry.lock().await;
        let session_ids: Vec<String> = reg
            .list()
            .iter()
            .map(|s| s.id.as_str().to_string())
            .collect();
        for sid in session_ids {
            if let Some(mut entry) = reg.remove(&sid)
                && let Some(ref mut child) = entry.chrome_process
            {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    // Cleanup files
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&ready_path).ok();
    std::fs::remove_file(&pid_file).ok();
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
        let result = router::route(&request.action, registry).await;
        let response_payload = wire::serialize_response(request.id, &result)?;
        wire::write_frame(&mut writer, &response_payload).await?;
    }

    Ok(())
}
