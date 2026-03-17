//! Electron desktop application automation commands.
//!
//! This module provides the `actionbook app` command for controlling Electron
//! desktop applications (VS Code, Slack, Discord, Notion, etc.) via Chrome DevTools Protocol.
//!
//! ## Command Delegation
//!
//! Most commands are delegated to browser.rs for maximum code reuse:
//! - App-specific: launch, attach, list, status, close, restart (implemented here)
//! - Shared commands: click, type, snapshot, etc. (delegated to browser module)

use colored::Colorize;

use crate::browser::{discover_electron_apps, SessionManager};
use crate::cli::{AppCommands, Cli};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Main entry point for app commands
pub async fn run(cli: &Cli, command: &AppCommands) -> Result<()> {
    let config = Config::load()?;

    match command {
        // App-specific lifecycle commands
        AppCommands::Launch { app_name } => launch(cli, &config, app_name).await,
        AppCommands::Attach { target } => attach(cli, &config, target).await,
        AppCommands::List => list(cli).await,
        AppCommands::Status => status(cli, &config).await,
        AppCommands::Close => close(cli, &config).await,
        AppCommands::Restart => restart(cli, &config).await,

        // Shared commands - delegate to browser module
        AppCommands::Goto { url, timeout } => {
            crate::commands::browser::goto(cli, &config, url, *timeout).await
        }
        AppCommands::Back => crate::commands::browser::back(cli, &config).await,
        AppCommands::Forward => crate::commands::browser::forward(cli, &config).await,
        AppCommands::Reload => crate::commands::browser::reload(cli, &config).await,
        AppCommands::Pages => crate::commands::browser::pages(cli, &config).await,
        AppCommands::Switch { page_id } => {
            crate::commands::browser::switch(cli, &config, page_id).await
        }
        AppCommands::Wait { selector, timeout } => {
            crate::commands::browser::wait(cli, &config, selector, *timeout).await
        }
        AppCommands::WaitNav { timeout } => {
            crate::commands::browser::wait_nav(cli, &config, *timeout).await
        }
        AppCommands::Click { selector, wait, ref_id, human } => {
            crate::commands::browser::click(
                cli,
                &config,
                selector.as_deref(),
                *wait,
                ref_id.as_deref(),
                *human,
            )
            .await
        }
        AppCommands::Type { selector, text, wait, ref_id, human } => {
            crate::commands::browser::type_text(
                cli,
                &config,
                selector.as_deref(),
                text,
                *wait,
                ref_id.as_deref(),
                *human,
            )
            .await
        }
        AppCommands::Fill { selector, text, wait, ref_id } => {
            crate::commands::browser::fill(
                cli,
                &config,
                selector.as_deref(),
                text,
                *wait,
                ref_id.as_deref(),
            )
            .await
        }
        AppCommands::Select { selector, value } => {
            crate::commands::browser::select(cli, &config, selector, value).await
        }
        AppCommands::Hover { selector } => {
            crate::commands::browser::hover(cli, &config, selector).await
        }
        AppCommands::Focus { selector } => {
            crate::commands::browser::focus(cli, &config, selector).await
        }
        AppCommands::Press { key } => {
            crate::commands::browser::press(cli, &config, key).await
        }
        AppCommands::Hotkey { keys } => {
            crate::commands::browser::hotkey(cli, &config, keys).await
        }
        AppCommands::Screenshot { path, full_page } => {
            crate::commands::browser::screenshot(cli, &config, path, *full_page).await
        }
        AppCommands::Pdf { path } => {
            crate::commands::browser::pdf(cli, &config, path).await
        }
        AppCommands::Eval { code } => {
            crate::commands::browser::eval(cli, &config, code).await
        }
        AppCommands::Html { selector } => {
            crate::commands::browser::html(cli, &config, selector.as_deref()).await
        }
        AppCommands::Text { selector, mode } => {
            crate::commands::browser::text(cli, &config, selector.as_deref(), mode).await
        }
        AppCommands::Snapshot {
            interactive,
            cursor,
            compact,
            depth,
            selector,
            format,
            diff,
            max_tokens,
        } => {
            crate::commands::browser::snapshot(
                cli,
                &config,
                *interactive,
                *cursor,
                *compact,
                format,
                *depth,
                selector.as_deref(),
                *diff,
                *max_tokens,
            )
            .await
        }
        AppCommands::Inspect { x, y, desc } => {
            crate::commands::browser::inspect(cli, &config, *x, *y, desc.as_deref()).await
        }
        AppCommands::Viewport => {
            crate::commands::browser::viewport(cli, &config).await
        }
        AppCommands::Cookies { command } => {
            crate::commands::browser::cookies(cli, &config, command).await
        }
        AppCommands::Scroll { direction, smooth, wait } => {
            crate::commands::browser::scroll(cli, &config, direction, *smooth, *wait).await
        }
        AppCommands::Batch { file, delay } => {
            crate::commands::batch::run(cli, &config, file.as_deref(), *delay).await
        }
        AppCommands::Fingerprint { command } => {
            crate::commands::browser::fingerprint(cli, &config, command).await
        }
        AppCommands::Console { duration, level } => {
            crate::commands::browser::console_log(cli, &config, *duration, level).await
        }
        AppCommands::WaitIdle { timeout, idle_time } => {
            crate::commands::browser::wait_idle(cli, &config, *timeout, *idle_time).await
        }
        AppCommands::Info { selector } => {
            crate::commands::browser::info(cli, &config, selector).await
        }
        AppCommands::Storage { command } => {
            crate::commands::browser::storage(cli, &config, command).await
        }
        AppCommands::Emulate { device } => {
            crate::commands::browser::emulate(cli, &config, device).await
        }
        AppCommands::WaitFn { expression, timeout, interval } => {
            crate::commands::browser::wait_fn(cli, &config, expression, *timeout, *interval).await
        }
        AppCommands::Upload { files, selector, ref_id, wait } => {
            crate::commands::browser::upload(cli, &config, files, selector.as_deref(), ref_id.as_deref(), *wait).await
        }
        AppCommands::Tab { command } => {
            crate::commands::browser::tab_command(cli, &config, command).await
        }
        AppCommands::SwitchFrame { target } => {
            crate::commands::browser::switch_frame(cli, &config, target).await
        }
    }
}

// ============================================================================
// App-specific implementations
// ============================================================================

/// Launch an Electron application by name
async fn launch(cli: &Cli, config: &Config, app_name: &str) -> Result<()> {
    // Discover installed apps
    let apps = discover_electron_apps();

    // Find matching app (case-insensitive)
    let app_name_lower = app_name.to_lowercase();
    let app = apps
        .iter()
        .find(|a| {
            a.name.to_lowercase().contains(&app_name_lower)
                || a.path
                    .to_str()
                    .map(|p| p.to_lowercase().contains(&app_name_lower))
                    .unwrap_or(false)
        })
        .ok_or_else(|| {
            ActionbookError::ConfigError(format!(
                "Application '{}' not found. Run 'actionbook app list' to see available apps.",
                app_name
            ))
        })?;

    println!("{} {}", "Launching".green(), app.name);
    println!("  Path: {}", app.path.display());

    // Use the same profile resolution logic as other commands
    let profile_name = crate::commands::browser::effective_profile_name(cli, config);

    // Launch the app with CDP debugging
    let session_manager = SessionManager::new(config.clone());

    // Convert PathBuf to string
    let app_path = app
        .path
        .to_str()
        .ok_or_else(|| ActionbookError::ConfigError("Invalid app path".to_string()))?;

    // Parse CDP port from CLI if provided
    let port = if let Some(cdp) = &cli.cdp {
        // Try to parse as port number
        cdp.parse::<u16>().ok()
    } else {
        None
    };

    let (_browser, _handler) = session_manager
        .launch_custom_app(profile_name, app_path, vec![], port)
        .await?;

    println!("{} Connected to {}", "✓".green(), app.name);
    println!("  Profile: {}", profile_name);
    println!("\n{}", "App is ready for automation.".bright_green());
    println!("\nUse 'actionbook app status' to check connection info.");

    Ok(())
}

/// Attach to a running application
async fn attach(cli: &Cli, config: &Config, target: &str) -> Result<()> {
    // Parse target and try to infer app path for better restart support
    let (endpoint, inferred_app_path) = if let Ok(port) = target.parse::<u16>() {
        // It's a port number - try to infer app from CDP info
        let app_path = try_infer_app_from_port(port).await;
        (port.to_string(), app_path)
    } else if target.starts_with("ws://") || target.starts_with("wss://") {
        // WebSocket URL - cannot infer app path reliably
        (target.to_string(), None)
    } else if target.starts_with("http://") || target.starts_with("https://") {
        // HTTP URL - extract port and try to infer app
        let port = target
            .split("://")
            .nth(1)
            .and_then(|s| s.split(':').nth(1))
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or_else(|| {
                ActionbookError::ConfigError(format!(
                    "Cannot extract port from HTTP URL: {}. Use port number (e.g., 9222) or WebSocket URL instead.",
                    target
                ))
            })?;
        let app_path = try_infer_app_from_port(port).await;
        (port.to_string(), app_path)
    } else {
        // Try to find app by name
        let apps = discover_electron_apps();
        let app_name_lower = target.to_lowercase();
        let app = apps
            .iter()
            .find(|a| a.name.to_lowercase().contains(&app_name_lower))
            .ok_or_else(|| {
                ActionbookError::ConfigError(format!(
                    "Could not find app '{}'. Use port number or WebSocket URL instead.",
                    target
                ))
            })?;

        println!(
            "{} Found app: {} at {}",
            "ℹ".blue(),
            app.name,
            app.path.display()
        );

        // Try to auto-detect CDP port (common ports: 9222-9225)
        println!("Scanning for active CDP ports...");
        let mut found_ports = Vec::new();

        for port in [9222, 9223, 9224, 9225] {
            if let Some(cdp_info) = get_cdp_info(port).await {
                // Verify this port belongs to the target app
                if cdp_info_matches_app(&cdp_info, &app.name) {
                    println!(
                        "{} Detected CDP port {} for {}",
                        "✓".green(),
                        port,
                        app.name
                    );
                    found_ports.push((port, cdp_info));
                } else {
                    // Port is active but for different app
                    if let Some(browser) = cdp_info.get("Browser").and_then(|v| v.as_str()) {
                        println!(
                            "{} Port {} is active but belongs to: {}",
                            "ℹ".blue(),
                            port,
                            browser
                        );
                    }
                }
            }
        }

        if let Some((port, _cdp_info)) = found_ports.first() {
            // Connect and save session with app path
            let profile_name = crate::commands::browser::effective_profile_name(cli, config);
            let (cdp_port, cdp_url) =
                crate::commands::browser::resolve_cdp_endpoint(&port.to_string()).await?;

            let session_manager = SessionManager::new(config.clone());
            let app_path_str = app.path.to_str().map(|s| s.to_string());
            session_manager.save_external_session_with_app(
                profile_name,
                cdp_port,
                &cdp_url,
                app_path_str,
            )?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "app_name": app.name,
                        "app_path": app.path,
                        "profile": profile_name,
                        "cdp_port": cdp_port,
                        "cdp_url": cdp_url
                    })
                );
            } else {
                println!(
                    "{} Connected to {} at port {}",
                    "✓".green(),
                    app.name,
                    cdp_port
                );
                println!("  WebSocket URL: {}", cdp_url);
                println!("  Profile: {}", profile_name);
            }

            return Ok(());
        }

        return Err(ActionbookError::ConfigError(format!(
            "App '{}' found but no active CDP port detected (tried 9222-9225).\n\
             Please launch the app with --remote-debugging-port=<PORT> and use:\n  \
             actionbook app attach <PORT>",
            app.name
        )));
    };

    // Connect and save session with inferred app path (if any)
    let profile_name = crate::commands::browser::effective_profile_name(cli, config);
    let (cdp_port, cdp_url) = crate::commands::browser::resolve_cdp_endpoint(&endpoint).await?;

    // Verify reachability and resolve fresh WS URL — same logic as ensure_cdp_override().
    // For local ws:// endpoints, query /json/version to get the current webSocketDebuggerUrl
    // (browser IDs rotate on every launch). For remote wss://, do a full WS handshake.
    let resolved_url = crate::commands::browser::verify_and_resolve_cdp_url(
        cli, config, cdp_port, &cdp_url,
    )
    .await?;

    let session_manager = SessionManager::new(config.clone());
    session_manager.save_external_session_with_app(
        profile_name,
        cdp_port,
        &resolved_url,
        inferred_app_path.clone(),
    )?;

    // Stop any running daemon so it reconnects to the new endpoint on next command.
    // Without this, the daemon would keep its stale WS connection to the old browser.
    #[cfg(unix)]
    {
        if crate::daemon::lifecycle::is_daemon_alive(profile_name).await {
            tracing::info!("Stopping daemon for profile '{}' after attach (endpoint changed)", profile_name);
            let _ = crate::daemon::lifecycle::stop_daemon(profile_name).await;
        }
    }

    if cli.json {
        let mut json_output = serde_json::json!({
            "success": true,
            "profile": profile_name,
            "cdp_port": cdp_port,
            "cdp_url": resolved_url
        });
        if let Some(app_path) = inferred_app_path {
            json_output["app_path"] = serde_json::json!(app_path);
            json_output["note"] = serde_json::json!("App path inferred from CDP info");
        }
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!("{} Connected to CDP at port {}", "✓".green(), cdp_port);
        println!("  WebSocket URL: {}", resolved_url);
        println!("  Profile: {}", profile_name);
        if let Some(app_path) = inferred_app_path {
            println!("  {} Inferred app path: {}", "ℹ".blue(), app_path);
        } else {
            println!(
                "  {} Could not infer app path from CDP. Restart will use browser mode.",
                "⚠".yellow()
            );
        }
    }

    Ok(())
}

/// Try to infer app path from CDP port by matching against known apps
async fn try_infer_app_from_port(port: u16) -> Option<String> {
    let cdp_info = get_cdp_info(port).await?;
    let apps = discover_electron_apps();

    // Try to match CDP info against known apps
    for app in &apps {
        if cdp_info_matches_app(&cdp_info, &app.name) {
            return app.path.to_str().map(|s| s.to_string());
        }
    }

    // If we found Electron but can't match to specific app, return None
    // (User can still use it, but restart will be in browser mode)
    None
}

/// Get CDP info from a port, returns JSON response if valid
async fn get_cdp_info(port: u16) -> Option<serde_json::Value> {
    use std::time::Duration;

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(1))
        .build()
        .ok()?;

    // Check /json/version endpoint for CDP protocol
    let response = client
        .get(format!("http://127.0.0.1:{}/json/version", port))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    // Parse and verify response is valid CDP JSON
    let json = response.json::<serde_json::Value>().await.ok()?;

    // Verify it has CDP-specific fields
    if json.get("webSocketDebuggerUrl").is_some()
        || json.get("Browser").is_some()
        || json.get("Protocol-Version").is_some()
    {
        Some(json)
    } else {
        None
    }
}

/// Check if CDP info matches expected app name
fn cdp_info_matches_app(cdp_info: &serde_json::Value, app_name: &str) -> bool {
    let app_name_lower = app_name.to_lowercase();

    // Check Browser field (e.g., "Chrome/91.0.4472.124", "Electron/13.1.7")
    if let Some(browser) = cdp_info.get("Browser").and_then(|v| v.as_str()) {
        if browser.to_lowercase().contains(&app_name_lower) {
            return true;
        }
    }

    // Check User-Agent field
    if let Some(user_agent) = cdp_info.get("User-Agent").and_then(|v| v.as_str()) {
        if user_agent.to_lowercase().contains(&app_name_lower) {
            return true;
        }
    }

    // Check Android-App-Info field (some Electron apps expose this)
    if let Some(android_info) = cdp_info.get("Android-App-Info").and_then(|v| v.as_str()) {
        if android_info.to_lowercase().contains(&app_name_lower) {
            return true;
        }
    }

    // For Electron apps, check if app name appears in any part of the version info
    // This helps match when CDP doesn't expose the app name directly
    // But we require at least partial name match - not just "Electron"
    if let Some(browser) = cdp_info.get("Browser").and_then(|v| v.as_str()) {
        let browser_lower = browser.to_lowercase();
        // Only accept if it's Electron AND app name has some match in the CDP response
        if browser_lower.contains("electron") {
            // Check if any field contains the app name
            let json_str = serde_json::to_string(cdp_info).unwrap_or_default().to_lowercase();
            if json_str.contains(&app_name_lower) {
                return true;
            }
        }
    }

    false
}

/// List all discoverable Electron applications
async fn list(_cli: &Cli) -> Result<()> {
    let apps = discover_electron_apps();

    if apps.is_empty() {
        println!("{}", "No Electron applications detected.".yellow());
        println!("\nTo control an app, it must be launched with:");
        println!("  --remote-debugging-port=9222");
        return Ok(());
    }

    println!("{}", "Detected Electron applications:".bright_green());
    println!();

    for (idx, app) in apps.iter().enumerate() {
        println!("{}. {}", idx + 1, app.name.bright_cyan());
        println!("   Path: {}", app.path.display().to_string().dimmed());
        if let Some(version) = &app.version {
            println!("   Version: {}", version.dimmed());
        }
        println!();
    }

    println!("{}", "To launch an app:".bright_white());
    println!("  actionbook app launch \"App Name\"");
    println!();
    println!("{}", "To attach to a running app:".bright_white());
    println!("  actionbook app attach <port>");

    Ok(())
}

/// Show application status
async fn status(cli: &Cli, config: &Config) -> Result<()> {
    // Delegate to browser status
    crate::commands::browser::status(cli, config).await
}

/// Close the connected application
async fn close(cli: &Cli, config: &Config) -> Result<()> {
    // Delegate to browser close
    crate::commands::browser::close(cli, config).await
}

/// Restart the connected application
async fn restart(cli: &Cli, config: &Config) -> Result<()> {
    use crate::browser::SessionManager;
    use std::fs;
    use std::path::PathBuf;

    let profile_name = crate::commands::browser::effective_profile_name(cli, config);

    // Load session state to check if it's a custom app
    // Use same path as SessionManager: ~/.actionbook/sessions
    let sessions_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".actionbook")
        .join("sessions");
    let session_file = sessions_dir.join(format!("{}.json", profile_name));

    let session_state_content = fs::read_to_string(&session_file).map_err(|_| {
        ActionbookError::BrowserNotRunning
    })?;

    let session_state: serde_json::Value = serde_json::from_str(&session_state_content)
        .map_err(|e| ActionbookError::ConfigError(format!("Failed to parse session state: {}", e)))?;

    // Check if this is a custom app session
    if let Some(app_path) = session_state.get("custom_app_path").and_then(|v| v.as_str()) {
        // This is a custom app - restart it properly
        println!("{} Restarting application: {}", "ℹ".blue(), app_path);

        // Close current session
        crate::commands::browser::close(cli, config).await?;

        // Get CDP port from old session
        let port = session_state.get("cdp_port").and_then(|v| v.as_u64()).map(|p| p as u16);

        // Relaunch the custom app
        let session_manager = SessionManager::new(config.clone());
        let (_browser, _handler) = session_manager
            .launch_custom_app(profile_name, app_path, vec![], port)
            .await?;

        println!("{} Application restarted", "✓".green());
        Ok(())
    } else {
        // This is a regular browser session - use browser restart
        crate::commands::browser::restart(cli, config).await
    }
}
