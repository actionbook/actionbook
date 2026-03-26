use colored::Colorize;
use tokio::net::UnixStream;

use crate::cli::{Cli, DaemonCommands};
use crate::daemon::client::default_socket_path;
use crate::daemon::server::default_pid_path;
use crate::error::Result;

pub async fn run(cli: &Cli, command: &DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Serve { .. } | DaemonCommands::ServeV2 => run_serve().await,
        DaemonCommands::Status => {
            let socket = default_socket_path();
            let alive = UnixStream::connect(&socket).await.is_ok();
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "running": alive,
                        "socket": socket.display().to_string(),
                    })
                );
            } else if alive {
                println!("{} Daemon is {}", "●".green(), "running".green());
                println!("  Socket: {}", socket.display());
            } else {
                println!("{} Daemon is {}", "○".dimmed(), "not running".dimmed());
            }
            Ok(())
        }
        DaemonCommands::Stop => {
            let socket = default_socket_path();
            let alive = UnixStream::connect(&socket).await.is_ok();
            if alive {
                // Send SIGTERM to the daemon process via PID file, then clean up.
                let pid_path = default_pid_path();
                if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        // SAFETY: sending SIGTERM to a known PID
                        unsafe { libc::kill(pid, libc::SIGTERM); }
                        // Brief wait for graceful shutdown
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
                // Clean up socket and PID files as fallback
                let _ = std::fs::remove_file(&socket);
                let _ = std::fs::remove_file(&pid_path);
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "stopped": true,
                        })
                    );
                } else {
                    println!("Daemon stopped");
                }
            } else if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "stopped": false,
                        "message": "daemon was not running",
                    })
                );
            } else {
                println!("Daemon is not running");
            }
            Ok(())
        }
    }
}

async fn run_serve() -> Result<()> {
    crate::daemon::cli_v2::run_daemon_foreground()
        .await
        .map_err(crate::error::ActionbookError::DaemonError)
}
