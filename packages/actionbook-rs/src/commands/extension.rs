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
        ExtensionCommands::Install { force } => install(cli, *force).await,
        ExtensionCommands::Path => path(cli).await,
        ExtensionCommands::Uninstall => uninstall(cli).await,
    }
}

async fn serve(_cli: &Cli, port: u16) -> Result<()> {
    // Clean up stale files from previous ungraceful shutdowns to prevent
    // native messaging from returning outdated port/token to the extension.
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

    // Run the bridge server, cleaning up token file on shutdown
    let result = extension_bridge::serve(port, token).await;

    // Cleanup token file on exit
    extension_bridge::delete_token_file().await;

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
