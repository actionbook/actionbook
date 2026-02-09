use colored::Colorize;

use crate::browser::extension_bridge;
use crate::browser::extension_installer;
use crate::browser::launcher::BrowserLauncher;
use crate::config::{Config, ProfileConfig};
use crate::error::{ActionbookError, Result};

/// CDP port used internally for the isolated Chrome instance.
/// Distinct from the default 9222 to avoid conflicts.
const ISOLATED_CDP_PORT: u16 = 9333;

/// Start an isolated Chrome instance with the extension pre-loaded and run the bridge server.
///
/// This orchestrates:
/// 1. Extension installation check
/// 2. Chrome launch with isolated profile + extension loaded
/// 3. Bridge server lifecycle
/// 4. Cleanup on exit
pub async fn serve_isolated(config: &Config, bridge_port: u16) -> Result<()> {
    // 1. Pre-check: extension must be installed
    if !extension_installer::is_installed() {
        return Err(ActionbookError::ExtensionError(
            "Extension not installed. Run 'actionbook extension install' first.".to_string(),
        ));
    }
    let ext_dir = extension_installer::extension_dir()?;

    // 2. Build profile config for isolated mode
    let profile = ProfileConfig {
        cdp_port: ISOLATED_CDP_PORT,
        headless: false, // Extensions require visible browser
        browser_path: config.browser.executable.clone(),
        ..Default::default()
    };

    // 3. Create launcher with extension loaded
    let launcher =
        BrowserLauncher::from_profile("extension", &profile)?.with_load_extension(ext_dir.clone());

    // 4. Check if Chrome is already running on the isolated CDP port
    let already_running = is_cdp_running(ISOLATED_CDP_PORT).await;

    // 5. Launch Chrome if not already running
    let child = if already_running {
        println!(
            "  {}  Isolated Chrome already running on CDP port {}",
            "◆".cyan(),
            ISOLATED_CDP_PORT
        );
        None
    } else {
        println!(
            "  {}  Launching isolated Chrome (CDP port {})...",
            "◆".cyan(),
            ISOLATED_CDP_PORT
        );
        let (child, cdp_url) = launcher.launch_and_wait().await?;
        println!("  {}  Chrome ready: {}", "✓".green(), cdp_url.dimmed());
        Some(child)
    };

    // 6. Clean up stale files from previous runs
    extension_bridge::delete_port_file().await;
    extension_bridge::delete_token_file().await;

    // 7. Generate session token and write files
    let token = extension_bridge::generate_token();
    if let Err(e) = extension_bridge::write_token_file(&token).await {
        eprintln!("  {} Failed to write token file: {}", "!".yellow(), e);
    }

    // 8. Print bridge info
    let extension_path = format!(
        "{}{}",
        ext_dir.display(),
        extension_installer::installed_version()
            .map(|v| format!(" (v{})", v))
            .unwrap_or_default()
    );

    println!();
    println!("  {}", "Actionbook Extension Bridge (Isolated)".bold());
    println!("  {}", "─".repeat(45).dimmed());
    println!();
    println!(
        "  {}  WebSocket server on ws://127.0.0.1:{}",
        "◆".cyan(),
        bridge_port
    );
    println!("  {}  Extension: {}", "◆".cyan(), extension_path);
    println!(
        "  {}  Profile: {} (isolated)",
        "◆".cyan(),
        BrowserLauncher::default_user_data_dir("extension")
            .display()
            .to_string()
            .dimmed()
    );
    println!();
    println!("  \u{1f511}  Session token: {}", token.bold());
    println!(
        "  {}  Token file: {}",
        "◆".cyan(),
        extension_bridge::token_file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
            .dimmed()
    );
    println!();
    println!(
        "  {}  Extension auto-loaded in isolated Chrome",
        "ℹ".dimmed()
    );
    println!("  {}  Token expires after 30min of inactivity", "ℹ".dimmed());
    println!("  {}  Press Ctrl+C to stop", "ℹ".dimmed());
    println!();

    // 9. Create shutdown channel for the bridge
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // 10. Save Chrome PID before moving child into monitor task
    let chrome_pid = child.as_ref().map(|c| c.id());

    // 11. Monitor Chrome process exit in background
    let (chrome_exit_tx, chrome_exit_rx) = tokio::sync::oneshot::channel::<()>();

    if let Some(mut proc) = child {
        tokio::task::spawn_blocking(move || {
            let _ = proc.wait(); // blocks until Chrome exits
            let _ = chrome_exit_tx.send(());
        });
    }

    // 12. Set up signal handler
    let signal_handler = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
            tokio::select! {
                _ = sigint.recv() => tracing::info!("Received SIGINT"),
                _ = sigterm.recv() => tracing::info!("Received SIGTERM"),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
        }
    };

    // 13. Run bridge server with lifecycle management
    let bridge_handle = tokio::spawn(async move {
        extension_bridge::serve_with_shutdown(bridge_port, token, shutdown_rx).await
    });

    // 14. Select between bridge, Chrome exit, and signal
    tokio::select! {
        result = bridge_handle => {
            tracing::info!("Bridge server stopped");
            if let Ok(Err(e)) = result {
                tracing::error!("Bridge server error: {}", e);
            }
        }
        _ = async { chrome_exit_rx.await.ok(); } => {
            tracing::info!("Chrome exited, shutting down bridge...");
            println!("\n  {} Chrome exited", "!".yellow());
            let _ = shutdown_tx.send(());
        }
        _ = signal_handler => {
            tracing::info!("Signal received, shutting down...");
            let _ = shutdown_tx.send(());
        }
    }

    // 15. Cleanup
    println!("\n  {}  Cleaning up...", "◆".cyan());

    // Delete token and port files
    extension_bridge::delete_token_file().await;
    extension_bridge::delete_port_file().await;

    // Terminate Chrome if we launched it (use saved PID)
    if let Some(pid) = chrome_pid {
        #[cfg(unix)]
        {
            // Send SIGTERM for graceful shutdown via kill command
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
            // Give it a moment to shut down
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            // Force kill if still running
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
        }
        #[cfg(not(unix))]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .status();
        }
    }

    println!("  {}  Shutdown complete", "✓".green());

    Ok(())
}

/// Check if a CDP endpoint is responding on the given port.
async fn is_cdp_running(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    client
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
