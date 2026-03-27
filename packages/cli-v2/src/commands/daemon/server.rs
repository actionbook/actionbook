use std::path::PathBuf;
use tokio::net::UnixListener;
use tracing::{info, warn};

use crate::utils::wire;
use super::registry::{new_shared_registry, SharedRegistry};
use super::router;

/// Get daemon socket path.
pub fn socket_path() -> PathBuf {
    let data_dir = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{home}/.local/share")
    });
    let dir = PathBuf::from(&data_dir).join("actionbook");
    std::fs::create_dir_all(&dir).ok();
    dir.join("daemon.sock")
}

/// PID file path (same directory as socket).
pub fn pid_path() -> PathBuf {
    socket_path().with_extension("pid")
}

/// Check if a daemon is already running by testing PID file + socket connectivity.
pub fn is_daemon_running() -> bool {
    let pid_file = pid_path();
    if !pid_file.exists() {
        return false;
    }
    if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Check if process is alive
            unsafe {
                if libc_kill(pid, 0) == 0 {
                    return true;
                }
            }
        }
    }
    // Stale PID file
    std::fs::remove_file(&pid_file).ok();
    false
}

/// Signal check without libc dependency — use kill(2) via nix-style raw syscall.
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    unsafe extern "C" {
        safe fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, sig)
}

/// Run the daemon server (blocking).
pub async fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    let path = socket_path();
    let pid_file = pid_path();

    // Check if another daemon is running
    if is_daemon_running() {
        eprintln!("daemon already running");
        return Ok(());
    }

    // Write PID file
    std::fs::write(&pid_file, std::process::id().to_string())?;

    // Remove stale socket
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let listener = UnixListener::bind(&path)?;
    info!("daemon listening on {}", path.display());

    // Write ready signal
    let ready_path = path.with_extension("ready");
    std::fs::write(&ready_path, "ready")?;

    let registry = new_shared_registry();

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
        }
    }

    // Cleanup
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
