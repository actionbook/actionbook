use colored::Colorize;

use crate::browser::cdp_http;
use crate::browser::cdp_pipe::PipeKeepAlive;
use crate::browser::extension_bridge;
use crate::browser::extension_installer;
use crate::browser::launcher::BrowserLauncher;
use crate::config::{Config, ProfileConfig};
use crate::error::{ActionbookError, Result};

/// CDP port used internally for the isolated Chrome instance.
/// Distinct from the default 9222 to avoid conflicts.
const ISOLATED_CDP_PORT: u16 = 9333;

/// Why the main event loop exited.
enum ShutdownReason {
    /// Bridge server exited on its own (includes result).
    BridgeExited(std::result::Result<Result<()>, tokio::task::JoinError>),
    /// The Chrome process we launched terminated.
    ChromeExited,
    /// User sent SIGINT / SIGTERM.
    Signal,
}

/// Start an isolated Chrome instance with the extension pre-loaded and run the bridge server.
///
/// This orchestrates:
/// 1. Extension installation check
/// 2. Chrome launch with isolated profile
/// 3. Bridge server startup (before extension loading so the extension can auto-connect)
/// 4. Extension loading via CDP pipe
/// 5. Bridge lifecycle management
/// 6. Cleanup on exit
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

    // 4. Check if *our* isolated Chrome is already running (profile lock + CDP)
    let profile_dir = BrowserLauncher::default_user_data_dir("extension");
    let already_running = is_isolated_chrome_running(ISOLATED_CDP_PORT, &profile_dir).await;

    // 5. Launch Chrome (but don't load extension yet — bridge must be ready first).
    //    _pipe_keepalive must live until shutdown — Chrome exits when the pipe closes.
    let mut _pipe_keepalive: Option<PipeKeepAlive> = None;
    let mut cdp_pipe_for_ext = None;

    let mut ext_id_for_injection: Option<String> = None;

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
        let (mut launch_result, cdp_url) = launcher.launch_and_wait().await?;
        println!("  {}  Chrome ready: {}", "✓".green(), cdp_url.dimmed());

        // Stash the CDP pipe for later — we'll load the extension after the bridge is up
        cdp_pipe_for_ext = launch_result.cdp_pipe.take();

        Some(launch_result.child)
    };

    // 6. Clean up stale isolated-mode bridge files from previous runs.
    extension_bridge::delete_isolated_port_file().await;
    extension_bridge::delete_isolated_token_file().await;

    // Clean up stale standard-mode files — but only if the standard bridge
    // process is confirmed dead. This prevents `send_command` from picking up
    // an outdated standard token while preserving files of a running bridge.
    let standard_alive = extension_bridge::read_pid_file()
        .await
        .is_some_and(|pid| extension_bridge::is_pid_alive(pid));
    if !standard_alive {
        extension_bridge::delete_port_file().await;
        extension_bridge::delete_token_file().await;
        extension_bridge::delete_pid_file().await;
    }

    let token = extension_bridge::generate_token();

    // 6b. Write isolated token file so CLI commands (ping, browser open, etc.) can discover it.
    //     This is safe because the file is at bridge-token.isolated, not the global bridge-token,
    //     so personal Chrome instances won't see it.
    extension_bridge::write_isolated_token_file(&token).await?;

    // 6c. Write isolated PID file so `extension stop` can find this process.
    if let Err(e) = extension_bridge::write_isolated_pid_file().await {
        eprintln!(
            "  {} Failed to write PID file: {}",
            "!".yellow(),
            e
        );
    }

    // 7. Create shutdown channel and start bridge server BEFORE loading extension.
    //    This ensures the bridge is listening when the extension's service worker
    //    fires its first native-messaging discovery request.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let token_for_bridge = token.clone();
    let bridge_handle = tokio::spawn(async move {
        extension_bridge::serve_with_shutdown(bridge_port, token_for_bridge, shutdown_rx, true).await
    });

    // 8. Wait for the bridge to be ready (accepting connections) before loading
    //    the extension, so the extension's first connect attempt succeeds.
    wait_for_bridge(bridge_port).await?;

    // 9. NOW load extension via CDP pipe — bridge + token are ready.
    if let Some(cdp_pipe) = cdp_pipe_for_ext {
        println!(
            "  {}  Loading extension via CDP pipe...",
            "◆".cyan(),
        );
        let ext_dir_owned = ext_dir.clone();
        let load_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || cdp_pipe.load_extension(&ext_dir_owned)),
        )
        .await;

        let (ext_id, keepalive) = match load_result {
            Ok(Ok(Ok(pair))) => pair,
            Ok(Ok(Err(e))) => {
                return Err(ActionbookError::ExtensionError(format!(
                    "Failed to load extension via CDP pipe: {}", e
                )));
            }
            Ok(Err(join_err)) => {
                return Err(ActionbookError::ExtensionError(format!(
                    "Extension loading task panicked: {}", join_err
                )));
            }
            Err(_) => {
                return Err(ActionbookError::ExtensionError(
                    "Timed out loading extension via CDP pipe (30s)".to_string(),
                ));
            }
        };
        _pipe_keepalive = Some(keepalive);
        println!(
            "  {}  Extension loaded (ID: {})",
            "✓".green(),
            ext_id.dimmed()
        );
        ext_id_for_injection = Some(ext_id);
    }

    // 10. Inject token directly into extension via CDP (isolated mode only).
    //     This bypasses global files entirely — only the isolated Chrome receives the token.
    if let Some(ref ext_id) = ext_id_for_injection {
        println!(
            "  {}  Injecting token via CDP...",
            "◆".cyan(),
        );
        if let Err(e) = cdp_http::inject_token_via_cdp(
            ISOLATED_CDP_PORT, ext_id, &token, bridge_port,
        ).await {
            eprintln!("  {} CDP token injection failed: {}", "!".yellow(), e);
            // Non-fatal: user can still enter token manually via popup
        } else {
            println!("  {}  Token injected via CDP", "✓".green());
        }
    } else if already_running {
        // Chrome is already running — find the extension's SW without knowing ext_id
        println!(
            "  {}  Injecting token into existing extension via CDP...",
            "◆".cyan(),
        );
        if let Err(e) = cdp_http::inject_token_existing(
            ISOLATED_CDP_PORT, &token, bridge_port,
        ).await {
            eprintln!("  {} CDP token injection failed: {}", "!".yellow(), e);
        } else {
            println!("  {}  Token injected via CDP", "✓".green());
        }
    }

    // 11. Print bridge info
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
        profile_dir.display().to_string().dimmed()
    );
    println!();
    println!("  \u{1f511}  Session token: {}", token.bold());
    println!(
        "  {}  Token delivery: {}",
        "◆".cyan(),
        "CDP injection (no global files)".dimmed()
    );
    println!();
    println!(
        "  {}  Extension auto-loaded in isolated Chrome",
        "ℹ".dimmed()
    );
    println!("  {}  Token expires after 30min of inactivity", "ℹ".dimmed());
    println!("  {}  Press Ctrl+C to stop", "ℹ".dimmed());
    println!();

    // 12. Save Chrome PID before moving child into monitor task
    let chrome_pid = child.as_ref().map(|c| c.id());

    // 13. Monitor Chrome process exit in background
    let (chrome_exit_tx, chrome_exit_rx) = tokio::sync::oneshot::channel::<()>();

    if let Some(mut proc) = child {
        tokio::task::spawn_blocking(move || {
            let _ = proc.wait(); // blocks until Chrome exits
            let _ = chrome_exit_tx.send(());
        });
    }

    // 14. Set up signal handler
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

    // 15. Select between bridge, Chrome exit, and signal — track reason
    let reason = tokio::select! {
        result = bridge_handle => {
            tracing::info!("Bridge server stopped");
            ShutdownReason::BridgeExited(result)
        }
        _ = async { chrome_exit_rx.await.ok(); } => {
            tracing::info!("Chrome exited, shutting down bridge...");
            println!("\n  {} Chrome exited", "!".yellow());
            let _ = shutdown_tx.send(());
            ShutdownReason::ChromeExited
        }
        _ = signal_handler => {
            tracing::info!("Signal received, shutting down...");
            let _ = shutdown_tx.send(());
            ShutdownReason::Signal
        }
    };

    // 16. Cleanup
    println!("\n  {}  Cleaning up...", "◆".cyan());

    // Delete only isolated token, port, and PID files — leave global files untouched
    // so a concurrently-running personal-Chrome bridge is not affected.
    extension_bridge::delete_isolated_token_file().await;
    extension_bridge::delete_isolated_port_file().await;
    extension_bridge::delete_isolated_pid_file().await;

    // Terminate Chrome only if we launched it AND it hasn't already exited.
    // Skipping when ChromeExited avoids sending signals to a potentially
    // recycled PID.
    if !matches!(reason, ShutdownReason::ChromeExited) {
        if let Some(pid) = chrome_pid {
            terminate_chrome(pid).await;
        }
    }

    println!("  {}  Shutdown complete", "✓".green());

    // Propagate bridge errors so callers see a non-zero exit code
    if let ShutdownReason::BridgeExited(result) = reason {
        return match result {
            Ok(inner) => inner,
            Err(join_err) => Err(ActionbookError::Other(format!(
                "Bridge task panicked: {}",
                join_err
            ))),
        };
    }

    Ok(())
}

/// Wait for the bridge server to start accepting connections.
/// Polls with short intervals, fails after a timeout.
async fn wait_for_bridge(port: u16) -> Result<()> {
    for _ in 0..20 {
        if extension_bridge::is_bridge_running(port).await {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(ActionbookError::Other(
        "Timeout waiting for bridge server to start".to_string(),
    ))
}

/// Terminate a Chrome process by PID using direct syscalls (unix) or taskkill (windows).
///
/// Uses `libc::kill` instead of shelling out to `/bin/kill` to avoid PATH-hijacking
/// risks. Sends SIGTERM first, then SIGKILL only if the process is still alive.
async fn terminate_chrome(pid: u32) {
    #[cfg(unix)]
    {
        let pid = pid as libc::pid_t;
        // SAFETY: Sending signals to a PID we obtained from our own Child.
        // The caller already verified Chrome hasn't exited (ShutdownReason check),
        // so PID reuse risk is minimal.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        // Give Chrome time to shut down gracefully
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // Force kill only if still running (kill(pid, 0) probes without sending a signal)
        unsafe {
            if libc::kill(pid, 0) == 0 {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
}

/// Check if an isolated Chrome instance is likely running.
///
/// Verifies both the Chrome profile lock file (proving a Chrome instance
/// is using *our* isolated profile directory) and the CDP endpoint (proving
/// it is accepting debugging connections). This avoids mistakenly reusing
/// a different Chrome instance that happens to listen on the same port.
async fn is_isolated_chrome_running(port: u16, profile_dir: &std::path::Path) -> bool {
    // Check profile lock file first (cheap filesystem check).
    // Chrome creates SingletonLock in the user-data-dir while running.
    // On macOS this is a dangling symlink (target = "hostname-PID"), so
    // Path::exists() returns false.  Use symlink_metadata() instead.
    let lock_file = profile_dir.join("SingletonLock");
    if lock_file.symlink_metadata().is_err() {
        return false;
    }

    // Then verify CDP endpoint responds
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
