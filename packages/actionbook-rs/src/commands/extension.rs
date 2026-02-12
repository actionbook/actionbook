use colored::Colorize;

use crate::browser::extension_installer;
use crate::browser::extension_bridge;
use crate::browser::native_messaging;
use crate::cli::{Cli, ExtensionCommands};
use crate::error::{ActionbookError, Result};

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

    let extension_path = if extension_installer::is_installed() {
        let dir = extension_installer::extension_dir()?;
        let version = extension_installer::installed_version()
            .map(|v| format!(" (v{})", v))
            .unwrap_or_default();
        format!("{}{}", dir.display(), version)
    } else {
        "(not installed - run 'actionbook extension install')".dimmed().to_string()
    };

    // Show deprecation notice (only in non-JSON mode)
    if !_cli.json {
        println!();
        println!(
            "  {}  {}",
            "ℹ".dimmed(),
            "Note: The bridge now auto-starts with browser commands".dimmed()
        );
        println!(
            "  {}  {}",
            "ℹ".dimmed(),
            "This manual start is only needed for debugging".dimmed()
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
    println!("  {}  Press Ctrl+C to stop", "ℹ".dimmed());
    println!();

    // Write PID file so `extension stop` can find this process
    // Mandatory: fail-fast if PID file cannot be written
    extension_bridge::write_pid_file(port).await
        .map_err(|e| {
            ActionbookError::Other(format!(
                "Failed to write PID file - check directory permissions: {}",
                e
            ))
        })?;

    // Run the bridge server
    let result = extension_bridge::serve(port).await;

    // Cleanup PID file on exit
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
            "  {}  It will auto-start when you run browser commands",
            "ℹ".dimmed()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PidResolution {
    Selected(u32),
    Ambiguous,
    BothMatchedButDead,
    NoMatch,
}

fn resolve_pid_for_port<F>(
    std: Option<(u32, u16)>,
    legacy: Option<(u32, u16)>,
    port: u16,
    mut is_pid_alive: F,
) -> PidResolution
where
    F: FnMut(u32) -> bool,
{
    let std_pid = std.and_then(|(pid, p)| (p == port).then_some(pid));
    let legacy_pid = legacy.and_then(|(pid, p)| (p == port).then_some(pid));

    match (std_pid, legacy_pid) {
        (Some(a), Some(b)) if a == b => PidResolution::Selected(a),
        (Some(a), Some(b)) => match (is_pid_alive(a), is_pid_alive(b)) {
            (true, false) => PidResolution::Selected(a),
            (false, true) => PidResolution::Selected(b),
            (true, true) => PidResolution::Ambiguous,
            (false, false) => PidResolution::BothMatchedButDead,
        },
        (Some(pid), None) | (None, Some(pid)) => PidResolution::Selected(pid),
        (None, None) => PidResolution::NoMatch,
    }
}

async fn stop(cli: &Cli, port: u16) -> Result<()> {
    // Read PID file — contains PID:PORT for deterministic matching.
    // Also check legacy .isolated PID file from pre-0.7 versions.
    let std = extension_bridge::read_pid_file().await;
    let legacy = extension_bridge::read_legacy_isolated_pid_file().await;

    let pid = match resolve_pid_for_port(std, legacy, port, extension_bridge::is_pid_alive) {
        PidResolution::Selected(p) => p,
        PidResolution::Ambiguous => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "error": "Multiple bridges detected on same port. Stop manually with Ctrl+C."
                    })
                );
            } else {
                println!(
                    "  {} Multiple bridges detected on port {}",
                    "!".yellow(),
                    port
                );
                println!(
                    "  {}  Stop the bridge manually with Ctrl+C in its terminal",
                    "ℹ".dimmed()
                );
            }
            return Ok(());
        }
        PidResolution::BothMatchedButDead => {
            extension_bridge::delete_pid_file().await;
            if cli.json {
                println!("{}", serde_json::json!({ "status": "not_running" }));
            } else {
                println!(
                    "  {} Bridge is not running (cleaned up stale PID files)",
                    "ℹ".dimmed()
                );
            }
            return Ok(());
        }
        PidResolution::NoMatch => {
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

#[cfg(test)]
mod tests {
    use super::{resolve_pid_for_port, PidResolution};

    #[test]
    fn resolve_pid_prefers_alive_when_both_match_port() {
        let result = resolve_pid_for_port(
            Some((1001, 19222)),
            Some((2002, 19222)),
            19222,
            |pid| pid == 2002,
        );
        assert_eq!(result, PidResolution::Selected(2002));
    }

    #[test]
    fn resolve_pid_marks_ambiguous_when_both_alive() {
        let result = resolve_pid_for_port(
            Some((1001, 19222)),
            Some((2002, 19222)),
            19222,
            |_pid| true,
        );
        assert_eq!(result, PidResolution::Ambiguous);
    }

    #[test]
    fn resolve_pid_marks_both_dead_when_both_dead() {
        let result = resolve_pid_for_port(
            Some((1001, 19222)),
            Some((2002, 19222)),
            19222,
            |_pid| false,
        );
        assert_eq!(result, PidResolution::BothMatchedButDead);
    }

    #[test]
    fn resolve_pid_falls_back_to_legacy_when_standard_missing() {
        let result = resolve_pid_for_port(None, Some((2002, 19222)), 19222, |_pid| true);
        assert_eq!(result, PidResolution::Selected(2002));
    }

    #[test]
    fn resolve_pid_returns_no_match_for_other_port() {
        let result = resolve_pid_for_port(
            Some((1001, 18080)),
            Some((2002, 19090)),
            19222,
            |_pid| true,
        );
        assert_eq!(result, PidResolution::NoMatch);
    }
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

    // Register native messaging host for automatic bridge connection
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
                    "  {}  Auto-connect via native messaging will not work",
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
            "  4. Run any browser command (bridge auto-starts):"
        );
        println!(
            "     {}",
            "actionbook browser open https://example.com".cyan()
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
