use colored::Colorize;
use tokio::net::UnixStream;

use crate::cli::{Cli, DaemonCommands};
use crate::daemon::client::default_socket_path;
use crate::error::Result;

pub async fn run(cli: &Cli, command: &DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Serve { .. } | DaemonCommands::ServeV2 => {
            run_serve().await
        }
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
                println!(
                    "{} Daemon is {}",
                    "●".green(),
                    "running".green()
                );
                println!("  Socket: {}", socket.display());
            } else {
                println!(
                    "{} Daemon is {}",
                    "○".dimmed(),
                    "not running".dimmed()
                );
            }
            Ok(())
        }
        DaemonCommands::Stop => {
            let socket = default_socket_path();
            let alive = UnixStream::connect(&socket).await.is_ok();
            if alive {
                // Remove the socket file to signal shutdown.
                let _ = std::fs::remove_file(&socket);
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
