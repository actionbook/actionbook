use colored::Colorize;

use crate::browser::extension_installer;
use crate::browser::extension_bridge;
use crate::browser::native_messaging;
use crate::cli::{Cli, ExtensionCommands};
use crate::error::Result;

pub async fn run(cli: &Cli, command: &ExtensionCommands) -> Result<()> {
    match command {
        ExtensionCommands::Serve { port } => serve(cli, *port).await,
        ExtensionCommands::Status { port } => status(cli, *port).await,
        ExtensionCommands::Ping { port } => ping(cli, *port).await,
        ExtensionCommands::Stop { port } => stop(cli, *port).await,
        ExtensionCommands::Install { force } => install(cli, *force).await,
        ExtensionCommands::Path => path(cli).await,
        ExtensionCommands::Uninstall => uninstall(cli).await,
    }
}

async fn serve(_cli: &Cli, port: u16) -> Result<()> {
    // Clean up stale bridge files from previous ungraceful shutdowns.
    extension_bridge::delete_port_file().await;
    extension_bridge::delete_token_file().await;

    let extension_path = if extension_installer::is_installed() {
        let dir = extension_installer::extension_dir()?;
        let version = extension_installer::installed_version()
            .map(|v| format!(" (v{})", v))
            .unwrap_or_default();
        format!("{}{}", dir.display(), version)
    } else {
        "(not installed - run 'actionbook extension install')".dimmed().to_string()
    };

    // Generate session token
    let token = extension_bridge::generate_token();

    // Write token file for CLI auto-read
    if let Err(e) = extension_bridge::write_token_file(&token).await {
        eprintln!(
            "  {} Failed to write token file: {}",
            "!".yellow(),
            e
        );
    }

    println!();
    println!("  {}", "Actionbook Extension Bridge".bold());
    println!("  {}", "─".repeat(40).dimmed());
    println!();
    println!(
        "  {}  WebSocket server on ws://127.0.0.1:{}",
        "◆".cyan(),
        port
    );
    println!(
        "  {}  Extension: {}",
        "◆".cyan(),
        extension_path
    );
    println!();
    println!(
        "  {}  Session token: {}",
        "🔑".to_string().as_str(),
        token.bold()
    );
    println!(
        "  {}  Token file: {}",
        "◆".cyan(),
        extension_bridge::token_file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
            .dimmed()
    );
    println!();
    println!("  {}  Configure the extension with this token", "ℹ".dimmed());
    println!("  {}  Token expires after 30min of inactivity", "ℹ".dimmed());
    println!("  {}  Press Ctrl+C to stop", "ℹ".dimmed());
    println!();

    // Write PID file so `extension stop` can find this process
    if let Err(e) = extension_bridge::write_pid_file(port).await {
        eprintln!(
            "  {} Failed to write PID file: {}",
            "!".yellow(),
            e
        );
    }

    // Run the bridge server, cleaning up token file on shutdown
    let result = extension_bridge::serve(port, token).await;

    // Cleanup token + PID files on exit
    extension_bridge::delete_token_file().await;
    extension_bridge::delete_pid_file().await;

    result
}

async fn status(_cli: &Cli, port: u16) -> Result<()> {
    let running = extension_bridge::is_bridge_running(port).await;

    if running {
        println!(
            "  {} Bridge server is running on port {}",
            "✓".green(),
            port
        );
    } else {
        println!(
            "  {} Bridge server is not running on port {}",
            "✗".red(),
            port
        );
        println!(
            "  {}  Start with: {}",
            "ℹ".dimmed(),
            "actionbook extension serve".dimmed()
        );
    }

    Ok(())
}

async fn ping(_cli: &Cli, port: u16) -> Result<()> {
    let start = std::time::Instant::now();
    let result = extension_bridge::send_command(
        port,
        "Extension.ping",
        serde_json::json!({}),
    )
    .await;

    match result {
        Ok(resp) => {
            let elapsed = start.elapsed();
            println!(
                "  {} Extension responded: {} ({}ms)",
                "✓".green(),
                resp,
                elapsed.as_millis()
            );
        }
        Err(e) => {
            println!("  {} Ping failed: {}", "✗".red(), e);
        }
    }

    Ok(())
}

async fn stop(cli: &Cli, port: u16) -> Result<()> {
    // Read PID file — contains PID:PORT for deterministic matching.
    // Also check legacy .isolated PID file from pre-0.7 versions.
    let std = extension_bridge::read_pid_file().await;
    let legacy = extension_bridge::read_legacy_isolated_pid_file().await;

    // PID selection: prefer standard file, fall back to legacy isolated file
    let resolved = match std {
        Some((p, pt)) if pt == port => Some(p),
        _ => match legacy {
            Some((p, pt)) if pt == port => Some(p),
            _ => None,
        },
    };

    let pid = match resolved {
        Some(p) => p,
        None => {
            // No PID file matches this port — fall back to port check
            let running = extension_bridge::is_bridge_running(port).await;
            if running {
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "status": "error", "error": "Bridge is running but no PID file found. Stop it manually with Ctrl+C." })
                    );
                } else {
                    println!(
                        "  {} Bridge is running on port {} but no PID file found",
                        "!".yellow(),
                        port
                    );
                    println!(
                        "  {}  Stop it manually with Ctrl+C in the terminal running 'actionbook extension serve'",
                        "ℹ".dimmed()
                    );
                }
            } else if cli.json {
                println!("{}", serde_json::json!({ "status": "not_running" }));
            } else {
                println!(
                    "  {} Bridge server is not running",
                    "ℹ".dimmed()
                );
            }
            return Ok(());
        }
    };

    // Guard against malformed PID files: PID must be positive
    if pid == 0 {
        extension_bridge::delete_pid_file().await;
        if cli.json {
            println!("{}", serde_json::json!({ "status": "not_running" }));
        } else {
            println!(
                "  {} Invalid PID file (cleaned up)",
                "ℹ".dimmed()
            );
        }
        return Ok(());
    }

    // Verify the bridge is actually listening on the expected port before
    // sending any signal. This prevents sending SIGTERM to an unrelated
    // process that happens to have the same PID (PID recycling).
    if !extension_bridge::is_bridge_running(port).await {
        let process_alive = extension_bridge::is_pid_alive(pid);

        if !process_alive {
            extension_bridge::delete_pid_file().await;
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "status": "not_running", "stale_pid": pid })
            );
        } else {
            println!(
                "  {} Bridge is not running on port {}{}",
                "ℹ".dimmed(),
                port,
                if process_alive {
                    format!(" (process {} may be on a different port)", pid)
                } else {
                    " (cleaned up stale PID file)".to_string()
                }
            );
        }
        return Ok(());
    }

    // Send SIGTERM for graceful shutdown.
    #[cfg(unix)]
    let kill_ok = {
        let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                extension_bridge::delete_pid_file().await;
                if cli.json {
                    println!("{}", serde_json::json!({ "status": "not_running" }));
                } else {
                    println!(
                        "  {} Bridge is not running (cleaned up stale PID file)",
                        "ℹ".dimmed()
                    );
                }
                return Ok(());
            }
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "error", "error": err.to_string(), "pid": pid })
                );
            } else {
                println!(
                    "  {} Failed to stop bridge (PID {}): {}",
                    "✗".red(),
                    pid,
                    err
                );
            }
            false
        } else {
            true
        }
    };

    #[cfg(not(unix))]
    let kill_ok = {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .status();
        match status {
            Ok(s) if s.success() => true,
            Ok(_) | Err(_) => {
                // Only delete PID file if process is confirmed dead
                if !extension_bridge::is_pid_alive(pid) {
                    extension_bridge::delete_pid_file().await;
                }
                if cli.json {
                    println!("{}", serde_json::json!({ "status": "error", "error": "Failed to stop bridge process" }));
                } else {
                    println!("  {} Failed to stop bridge (PID {})", "✗".red(), pid);
                }
                false
            }
        }
    };

    if !kill_ok {
        return Ok(());
    }

    // Wait for the process to exit, with SIGKILL escalation
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    #[cfg(unix)]
    {
        let still_running = unsafe { libc::kill(pid as i32, 0) } == 0;
        if still_running {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let still_running = unsafe { libc::kill(pid as i32, 0) } == 0;
            if still_running {
                unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }

    extension_bridge::delete_pid_file().await;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "status": "stopped", "pid": pid })
        );
    } else {
        println!(
            "  {} Bridge server stopped (PID {})",
            "✓".green(),
            pid
        );
    }

    Ok(())
}

async fn install(cli: &Cli, force: bool) -> Result<()> {
    let dir = extension_installer::extension_dir()?;

    // Download from GitHub (handles version comparison internally —
    // returns AlreadyUpToDate when installed version >= latest)
    if !cli.json {
        println!(
            "  {} Checking for latest extension release...",
            "◆".cyan()
        );
    }

    let result = extension_installer::download_and_install(force).await;

    // Handle "already up to date" as a success case, not an error
    match &result {
        Err(crate::error::ActionbookError::ExtensionAlreadyUpToDate {
            current,
            latest: _,
        }) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "already_installed",
                        "version": current,
                        "path": dir.display().to_string()
                    })
                );
            } else {
                println!(
                    "  {} Extension v{} is already up to date",
                    "✓".green(),
                    current,
                );
                println!(
                    "  {}  Use {} to force reinstall",
                    "ℹ".dimmed(),
                    "--force".dimmed()
                );
            }
            return Ok(());
        }
        _ => {}
    }

    let version = result?;

    // Register native messaging host for automatic token exchange
    let native_host_result = native_messaging::install_manifest();

    if cli.json {
        let mut result = serde_json::json!({
            "status": "installed",
            "version": version,
            "path": dir.display().to_string()
        });
        match &native_host_result {
            Ok(p) => {
                result["native_messaging_host"] = serde_json::json!(p.display().to_string());
            }
            Err(e) => {
                result["native_messaging_host_error"] = serde_json::json!(e.to_string());
            }
        }
        println!("{}", result);
    } else {
        println!();
        println!(
            "  {} Extension v{} installed successfully",
            "✓".green(),
            version
        );
        println!("  {}  Path: {}", "◆".cyan(), dir.display());

        match &native_host_result {
            Ok(p) => {
                println!(
                    "  {} Native messaging host registered",
                    "✓".green()
                );
                println!("  {}  Manifest: {}", "◆".cyan(), p.display().to_string().dimmed());
            }
            Err(e) => {
                println!(
                    "  {} Failed to register native messaging host: {}",
                    "!".yellow(),
                    e
                );
                println!(
                    "  {}  Token auto-pairing will not work; manual token entry required",
                    "ℹ".dimmed()
                );
            }
        }

        println!();
        println!("  {}", "Next steps:".bold());
        println!("  1. Open {} in Chrome", "chrome://extensions".cyan());
        println!("  2. Enable {}", "Developer mode".bold());
        println!(
            "  3. Click {} and select:",
            "Load unpacked".bold()
        );
        println!("     {}", dir.display().to_string().dimmed());
        println!(
            "  4. Run {}",
            "actionbook extension serve".cyan()
        );
        println!(
            "  5. Extension {} via native messaging",
            "auto-connects".green().bold()
        );
        println!();
    }

    Ok(())
}

async fn path(cli: &Cli) -> Result<()> {
    let dir = extension_installer::extension_dir()?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "path": dir.display().to_string(),
                "installed": extension_installer::is_installed(),
                "version": extension_installer::installed_version(),
            })
        );
    } else {
        println!("{}", dir.display());
    }

    Ok(())
}

async fn uninstall(cli: &Cli) -> Result<()> {
    if !extension_installer::is_installed() {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "status": "not_installed" })
            );
        } else {
            println!(
                "  {} Extension is not installed",
                "ℹ".dimmed()
            );
        }
        return Ok(());
    }

    let dir = extension_installer::extension_dir()?;
    extension_installer::uninstall()?;

    // Also remove native messaging host manifest
    let _ = native_messaging::uninstall_manifest();

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "status": "uninstalled",
                "path": dir.display().to_string()
            })
        );
    } else {
        println!(
            "  {} Extension removed from {}",
            "✓".green(),
            dir.display()
        );
        println!(
            "  {} Native messaging host unregistered",
            "✓".green()
        );
    }

    Ok(())
}
