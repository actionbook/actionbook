use std::fs;
use std::path::Path;
use std::time::Duration;

use colored::Colorize;

use crate::browser::backend::BrowserBackend;
use crate::browser::bridge_lifecycle;
use crate::browser::extension_backend::ExtensionBackend;
use crate::browser::extension_bridge;
use crate::browser::isolated_backend::IsolatedBackend;
use crate::browser::{
    build_stealth_profile, discover_all_browsers, stealth_status, SessionManager, SessionStatus,
    StealthConfig,
};
use crate::cli::{BrowserCommands, BrowserMode, Cli, CookiesCommands};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

// ---------------------------------------------------------------------------
// Mode resolution & backend factory
// ---------------------------------------------------------------------------

/// Resolve the effective browser mode from CLI flags and config.
/// Priority: --extension flag (deprecated) > --browser-mode flag > config.browser.mode
fn resolve_mode(cli: &Cli, config: &Config) -> BrowserMode {
    if cli.extension {
        return BrowserMode::Extension;
    }
    match cli.browser_mode {
        Some(mode) => mode,
        None => config.browser.mode,
    }
}

/// Create a SessionManager with appropriate stealth configuration from CLI flags
fn create_session_manager(cli: &Cli, config: &Config) -> SessionManager {
    if cli.stealth {
        let stealth_profile =
            build_stealth_profile(cli.stealth_os.as_deref(), cli.stealth_gpu.as_deref());

        let stealth_config = StealthConfig {
            enabled: true,
            headless: cli.headless,
            profile: stealth_profile,
        };

        SessionManager::with_stealth(config.clone(), stealth_config)
    } else {
        SessionManager::new(config.clone())
    }
}

fn effective_profile_name<'a>(cli: &'a Cli, config: &'a Config) -> &'a str {
    cli.profile
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let default_profile = config.browser.default_profile.trim();
            if default_profile.is_empty() {
                None
            } else {
                Some(default_profile)
            }
        })
        .unwrap_or("actionbook")
}

fn effective_profile_arg<'a>(cli: &'a Cli, config: &'a Config) -> Option<&'a str> {
    Some(effective_profile_name(cli, config))
}

/// Create the appropriate backend based on resolved mode.
/// In extension mode, auto-starts the bridge daemon if not already running.
///
/// Returns `(backend, bridge_auto_started)` where `bridge_auto_started` is true
/// only if this invocation spawned the bridge daemon (so `close` knows whether
/// to stop it — the bridge is shared, so we only stop what we started).
async fn create_backend(
    cli: &Cli,
    config: &Config,
) -> Result<(Box<dyn BrowserBackend>, bool)> {
    match resolve_mode(cli, config) {
        BrowserMode::Isolated => {
            let sm = create_session_manager(cli, config);
            let profile = effective_profile_name(cli, config).to_string();
            Ok((Box::new(IsolatedBackend::new(sm, profile)), false))
        }
        BrowserMode::Extension => {
            let auto_started =
                bridge_lifecycle::ensure_bridge_running(cli.extension_port).await?;
            Ok((Box::new(ExtensionBackend::new(cli.extension_port)), auto_started))
        }
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

fn normalize_navigation_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err(ActionbookError::Other(
            "Invalid URL: empty input".to_string(),
        ));
    }

    if let Some(rest) = trimmed.strip_prefix("//") {
        return Ok(format!("https://{}", rest));
    }

    if trimmed.contains("://") {
        return Ok(trimmed.to_string());
    }

    if is_host_port_with_optional_path(trimmed) {
        return Ok(format!("https://{}", trimmed));
    }

    if has_explicit_scheme(trimmed) {
        return Ok(trimmed.to_string());
    }

    Ok(format!("https://{}", trimmed))
}

fn is_host_port_with_optional_path(input: &str) -> bool {
    let boundary = input.find(['/', '?', '#']).unwrap_or(input.len());
    let authority = &input[..boundary];

    if authority.is_empty() {
        return false;
    }

    match authority.rsplit_once(':') {
        Some((host, port)) => {
            !host.is_empty() && !port.is_empty() && port.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn has_explicit_scheme(input: &str) -> bool {
    let mut chars = input.chars();

    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }

    for c in chars {
        if c == ':' {
            return true;
        }

        if c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.' {
            continue;
        }

        return false;
    }

    false
}

// ---------------------------------------------------------------------------
// CDP helpers (isolated-mode-only utilities)
// ---------------------------------------------------------------------------

/// Resolve a CDP endpoint string (port number or ws:// URL) into a (port, ws_url) pair.
async fn resolve_cdp_endpoint(endpoint: &str) -> Result<(u16, String)> {
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        let port = endpoint
            .split("://")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .and_then(|host_port| host_port.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(9222);
        Ok((port, endpoint.to_string()))
    } else if let Ok(port) = endpoint.parse::<u16>() {
        let version_url = format!("http://127.0.0.1:{}/json/version", port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let resp = client.get(&version_url).send().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!(
                "Cannot reach CDP at port {}. Is the browser running with --remote-debugging-port={}? Error: {}",
                port, port, e
            ))
        })?;

        let version_info: serde_json::Value = resp.json().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!(
                "Invalid response from CDP endpoint: {}",
                e
            ))
        })?;

        let ws_url = version_info
            .get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ws://127.0.0.1:{}", port));

        Ok((port, ws_url))
    } else {
        Err(ActionbookError::CdpConnectionFailed(
            "Invalid endpoint. Use a port number or WebSocket URL (ws://...).".to_string(),
        ))
    }
}

/// If the user passed `--cdp <port_or_url>`, resolve it to a fresh WebSocket URL
/// and persist it as the active session so that `get_or_create_session` picks it up.
async fn ensure_cdp_override(cli: &Cli, config: &Config) -> Result<()> {
    let cdp = match &cli.cdp {
        Some(c) => c.as_str(),
        None => return Ok(()),
    };

    let profile_name = effective_profile_name(cli, config);
    let (cdp_port, cdp_url) = resolve_cdp_endpoint(cdp).await?;

    let session_manager = create_session_manager(cli, config);
    session_manager.save_external_session(profile_name, cdp_port, &cdp_url)?;
    tracing::debug!(
        "CDP override applied: port={}, url={}, profile={}",
        cdp_port,
        cdp_url,
        profile_name
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run(cli: &Cli, command: &BrowserCommands) -> Result<()> {
    let config = Config::load()?;

    // --profile is not supported in extension mode
    if resolve_mode(cli, &config) == BrowserMode::Extension && cli.profile.is_some() {
        return Err(ActionbookError::Other(
            "--profile is not supported in extension mode. Extension operates on your live Chrome profile. \
             Remove --profile to use the default profile, or switch to isolated mode.".to_string()
        ));
    }

    // CDP override (isolated mode only, skip for connect)
    if resolve_mode(cli, &config) == BrowserMode::Isolated
        && !matches!(command, BrowserCommands::Connect { .. })
    {
        ensure_cdp_override(cli, &config).await?;
    }

    // Commands that don't use backend (isolated-mode-only utilities)
    match command {
        BrowserCommands::Status => return status(cli, &config).await,
        BrowserCommands::Connect { endpoint } => return connect(cli, &config, endpoint).await,
        _ => {}
    }

    // Close in extension mode: don't auto-start bridge, best-effort detach.
    // This avoids the pathological case where `close` starts a bridge just to
    // send detachTab, then waits 30s for an extension that will never connect,
    // and potentially leaks the auto-started bridge process.
    if matches!(command, BrowserCommands::Close)
        && resolve_mode(cli, &config) == BrowserMode::Extension
    {
        return close_extension(cli).await;
    }

    // Create backend for all other commands
    let (backend, bridge_auto_started) = create_backend(cli, &config).await?;

    match command {
        BrowserCommands::Open { url } => open(cli, &*backend, url).await,
        BrowserCommands::Goto { url, timeout: t } => goto(cli, &*backend, url, *t).await,
        BrowserCommands::Back => back(cli, &*backend).await,
        BrowserCommands::Forward => forward(cli, &*backend).await,
        BrowserCommands::Reload => reload(cli, &*backend).await,
        BrowserCommands::Pages => pages(cli, &*backend).await,
        BrowserCommands::Switch { page_id } => switch(cli, &*backend, page_id).await,
        BrowserCommands::Wait {
            selector,
            timeout: t,
        } => wait(cli, &*backend, selector, *t).await,
        BrowserCommands::WaitNav { timeout: t } => wait_nav(cli, &*backend, *t).await,
        BrowserCommands::Click { selector, wait: w } => {
            click(cli, &*backend, selector, *w).await
        }
        BrowserCommands::Type {
            selector,
            text,
            wait: w,
        } => type_text(cli, &*backend, selector, text, *w).await,
        BrowserCommands::Fill {
            selector,
            text,
            wait: w,
        } => fill(cli, &*backend, selector, text, *w).await,
        BrowserCommands::Select { selector, value } => {
            select(cli, &*backend, selector, value).await
        }
        BrowserCommands::Hover { selector } => hover(cli, &*backend, selector).await,
        BrowserCommands::Focus { selector } => focus(cli, &*backend, selector).await,
        BrowserCommands::Press { key } => press(cli, &*backend, key).await,
        BrowserCommands::Screenshot { path, full_page } => {
            screenshot(cli, &*backend, path, *full_page).await
        }
        BrowserCommands::Pdf { path } => pdf(cli, &*backend, path).await,
        BrowserCommands::Eval { code } => eval(cli, &*backend, code).await,
        BrowserCommands::Html { selector } => html(cli, &*backend, selector.as_deref()).await,
        BrowserCommands::Text { selector } => text(cli, &*backend, selector.as_deref()).await,
        BrowserCommands::Snapshot => snapshot(cli, &*backend).await,
        BrowserCommands::Inspect { x, y, desc } => {
            inspect(cli, &*backend, *x, *y, desc.as_deref()).await
        }
        BrowserCommands::Viewport => viewport(cli, &*backend).await,
        BrowserCommands::Cookies { command: cmd } => cookies(cli, &*backend, cmd).await,
        BrowserCommands::Close => close(cli, &config, &*backend, bridge_auto_started).await,
        BrowserCommands::Restart => restart(cli, &*backend).await,
        // Status and Connect are handled above
        BrowserCommands::Status | BrowserCommands::Connect { .. } => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// status & connect — isolated-mode-only, no backend needed
// ---------------------------------------------------------------------------

async fn status(cli: &Cli, config: &Config) -> Result<()> {
    println!("{}", "API Key:".bold());
    let api_key = cli.api_key.as_deref().or(config.api.api_key.as_deref());
    match api_key {
        Some(key) if key.len() > 8 => {
            let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
            println!("  {} Configured ({})", "✓".green(), masked.dimmed());
        }
        Some(_) => {
            println!("  {} Configured", "✓".green());
        }
        None => {
            println!(
                "  {} Not configured (set via --api-key or ACTIONBOOK_API_KEY)",
                "○".dimmed()
            );
        }
    }
    println!();

    println!("{}", "Stealth Mode:".bold());
    let stealth = stealth_status();
    if stealth.starts_with("enabled") {
        println!("  {} {}", "✓".green(), stealth);
        if cli.stealth {
            let profile =
                build_stealth_profile(cli.stealth_os.as_deref(), cli.stealth_gpu.as_deref());
            println!("  {} OS: {:?}", "  ".dimmed(), profile.os);
            println!("  {} GPU: {:?}", "  ".dimmed(), profile.gpu);
            println!("  {} Chrome: v{}", "  ".dimmed(), profile.chrome_version);
            println!("  {} Locale: {}", "  ".dimmed(), profile.locale);
        }
    } else {
        println!("  {} {}", "○".dimmed(), stealth);
    }
    println!();

    println!("{}", "Detected Browsers:".bold());
    let browsers = discover_all_browsers();
    if browsers.is_empty() {
        println!("  {} No browsers found", "!".yellow());
    } else {
        for browser in browsers {
            println!(
                "  {} {} {}",
                "✓".green(),
                browser.browser_type.name(),
                browser
                    .version
                    .map(|v| format!("(v{})", v))
                    .unwrap_or_default()
                    .dimmed()
            );
            println!("    {}", browser.path.display().to_string().dimmed());
        }
    }

    println!();

    let session_manager = create_session_manager(cli, config);
    let profile_name = effective_profile_arg(cli, config);
    let status = session_manager.get_status(profile_name).await;

    println!("{}", "Session Status:".bold());
    match status {
        SessionStatus::Running {
            profile,
            cdp_port,
            cdp_url,
        } => {
            println!("  {} Profile: {}", "✓".green(), profile.cyan());
            println!("  {} CDP Port: {}", "✓".green(), cdp_port);
            println!("  {} CDP URL: {}", "✓".green(), cdp_url.dimmed());

            if let Ok(pages) = session_manager.get_pages(Some(&profile)).await {
                println!();
                println!("{}", "Open Pages:".bold());
                for (i, page) in pages.iter().enumerate() {
                    println!(
                        "  {}. {} {}",
                        (i + 1).to_string().cyan(),
                        page.title.bold(),
                        format!("({})", page.id).dimmed()
                    );
                    println!("     {}", page.url.dimmed());
                }
            }
        }
        SessionStatus::Stale { profile } => {
            println!(
                "  {} Profile: {} (stale session)",
                "!".yellow(),
                profile.cyan()
            );
        }
        SessionStatus::NotRunning { profile } => {
            println!(
                "  {} Profile: {} (not running)",
                "○".dimmed(),
                profile.cyan()
            );
        }
    }

    Ok(())
}

async fn connect(cli: &Cli, config: &Config, endpoint: &str) -> Result<()> {
    let profile_name = effective_profile_name(cli, config);
    let (cdp_port, cdp_url) = resolve_cdp_endpoint(endpoint).await?;

    let session_manager = create_session_manager(cli, config);
    session_manager.save_external_session(profile_name, cdp_port, &cdp_url)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "profile": profile_name,
                "cdp_port": cdp_port,
                "cdp_url": cdp_url
            })
        );
    } else {
        println!("{} Connected to CDP at port {}", "✓".green(), cdp_port);
        println!("  WebSocket URL: {}", cdp_url);
        println!("  Profile: {}", profile_name);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Command functions — all use `backend: &dyn BrowserBackend`
// ---------------------------------------------------------------------------

async fn open(cli: &Cli, backend: &dyn BrowserBackend, url: &str) -> Result<()> {
    let normalized_url = normalize_navigation_url(url)?;
    let result = backend.open(&normalized_url).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "url": normalized_url,
                "title": result.title
            })
        );
    } else {
        println!("{} {}", "✓".green(), result.title.bold());
        println!("  {}", normalized_url.dimmed());
    }

    Ok(())
}

async fn goto(cli: &Cli, backend: &dyn BrowserBackend, url: &str, _timeout_ms: u64) -> Result<()> {
    let normalized_url = normalize_navigation_url(url)?;
    backend.goto(&normalized_url).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "url": normalized_url })
        );
    } else {
        println!("{} Navigated to: {}", "✓".green(), normalized_url);
    }

    Ok(())
}

async fn back(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    backend.back().await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Went back", "✓".green());
    }

    Ok(())
}

async fn forward(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    backend.forward().await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Went forward", "✓".green());
    }

    Ok(())
}

async fn reload(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    backend.reload().await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Page reloaded", "✓".green());
    }

    Ok(())
}

async fn pages(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    let entries = backend.pages().await?;

    if cli.json {
        let pages_json: Vec<_> = entries
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "title": p.title,
                    "url": p.url
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&pages_json)?);
    } else if entries.is_empty() {
        println!("{} No pages open", "!".yellow());
    } else {
        println!("{} {} pages open\n", "✓".green(), entries.len());
        for (i, page) in entries.iter().enumerate() {
            let id_display = format_page_id(&page.id);
            println!(
                "{}. {} {}",
                (i + 1).to_string().cyan(),
                page.title.bold(),
                format!("({})", id_display).dimmed()
            );
            println!("   {}", page.url.dimmed());
        }
    }

    Ok(())
}

/// Shorten a page ID for display: tab IDs are shown as-is, UUIDs are truncated.
fn format_page_id(id: &str) -> &str {
    if id.starts_with("tab:") {
        id
    } else if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

async fn switch(cli: &Cli, backend: &dyn BrowserBackend, page_id: &str) -> Result<()> {
    backend.switch(page_id).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "pageId": page_id })
        );
    } else {
        println!("{} Switched to page {}", "✓".green(), page_id);
    }

    Ok(())
}

async fn wait(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    selector: &str,
    timeout_ms: u64,
) -> Result<()> {
    backend.wait_for(selector, timeout_ms).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector })
        );
    } else {
        println!("{} Element found: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn wait_nav(cli: &Cli, backend: &dyn BrowserBackend, timeout_ms: u64) -> Result<()> {
    let new_url = backend.wait_nav(timeout_ms).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "url": new_url })
        );
    } else {
        println!("{} Navigation complete: {}", "✓".green(), new_url);
    }

    Ok(())
}

async fn click(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    selector: &str,
    wait_ms: u64,
) -> Result<()> {
    backend.click(selector, wait_ms).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector })
        );
    } else {
        println!("{} Clicked: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn type_text(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    selector: &str,
    text: &str,
    wait_ms: u64,
) -> Result<()> {
    backend.type_text(selector, text, wait_ms).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector, "text": text })
        );
    } else {
        println!("{} Typed into: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn fill(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    selector: &str,
    text: &str,
    wait_ms: u64,
) -> Result<()> {
    backend.fill(selector, text, wait_ms).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector, "text": text })
        );
    } else {
        println!("{} Filled: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn select(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    selector: &str,
    value: &str,
) -> Result<()> {
    backend.select(selector, value).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector, "value": value })
        );
    } else {
        println!("{} Selected '{}' in: {}", "✓".green(), value, selector);
    }

    Ok(())
}

async fn hover(cli: &Cli, backend: &dyn BrowserBackend, selector: &str) -> Result<()> {
    backend.hover(selector).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector })
        );
    } else {
        println!("{} Hovered: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn focus(cli: &Cli, backend: &dyn BrowserBackend, selector: &str) -> Result<()> {
    backend.focus(selector).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "selector": selector })
        );
    } else {
        println!("{} Focused: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn press(cli: &Cli, backend: &dyn BrowserBackend, key: &str) -> Result<()> {
    backend.press(key).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "key": key })
        );
    } else {
        println!("{} Pressed: {}", "✓".green(), key);
    }

    Ok(())
}

async fn screenshot(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    path: &str,
    full_page: bool,
) -> Result<()> {
    let screenshot_data = backend.screenshot(full_page).await?;

    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, screenshot_data)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "path": path, "fullPage": full_page })
        );
    } else {
        let mode = if full_page { " (full page)" } else { "" };
        println!("{} Screenshot saved{}: {}", "✓".green(), mode, path);
    }

    Ok(())
}

async fn pdf(cli: &Cli, backend: &dyn BrowserBackend, path: &str) -> Result<()> {
    let pdf_data = backend.pdf().await?;

    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, pdf_data)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "path": path })
        );
    } else {
        println!("{} PDF saved: {}", "✓".green(), path);
    }

    Ok(())
}

async fn eval(_cli: &Cli, backend: &dyn BrowserBackend, code: &str) -> Result<()> {
    let value = backend.eval(code).await?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}

async fn html(cli: &Cli, backend: &dyn BrowserBackend, selector: Option<&str>) -> Result<()> {
    let html = backend.html(selector).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "html": html }));
    } else {
        println!("{}", html);
    }

    Ok(())
}

async fn text(cli: &Cli, backend: &dyn BrowserBackend, selector: Option<&str>) -> Result<()> {
    let text = backend.text(selector).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "text": text }));
    } else {
        println!("{}", text);
    }

    Ok(())
}

async fn snapshot(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    let value = backend.snapshot().await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else if let Some(tree) = value.get("tree") {
        let output = render_snapshot_tree(tree, 0);
        print!("{}", output);
    } else {
        println!("(empty)");
    }

    Ok(())
}

async fn inspect(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    x: f64,
    y: f64,
    desc: Option<&str>,
) -> Result<()> {
    // Validate coordinates against viewport bounds
    let (vw, vh) = backend.viewport().await?;
    if x < 0.0 || x > vw as f64 || y < 0.0 || y > vh as f64 {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": false,
                    "message": format!("Coordinates ({}, {}) are outside viewport bounds ({}x{})", x, y, vw, vh)
                })
            );
        } else {
            println!(
                "{} Coordinates ({}, {}) are outside viewport bounds ({}x{})",
                "!".yellow(),
                x,
                y,
                vw,
                vh
            );
        }
        return Ok(());
    }

    let result = backend.inspect(x, y).await?;

    if cli.json {
        let mut output = serde_json::json!({
            "success": true,
            "coordinates": { "x": x, "y": y },
            "viewport": { "width": vw, "height": vh },
            "inspection": result
        });
        if let Some(d) = desc {
            output["description"] = serde_json::json!(d);
        }
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let found = result
            .get("found")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // default true for backends that don't include "found"

        if !found {
            println!("{} No element found at ({}, {})", "!".yellow(), x, y);
            return Ok(());
        }

        if let Some(d) = desc {
            println!("{} Inspecting: {}\n", "?".cyan(), d.bold());
        }

        println!(
            "{} ({}, {}) in {}x{} viewport\n",
            "?".cyan(),
            x,
            y,
            vw,
            vh
        );

        let tag = result
            .get("tagName")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let id = result
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        let class = result
            .get("className")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        print!("{}", "Element: ".bold());
        print!("<{}", tag.cyan());
        if let Some(i) = id {
            print!(" id=\"{}\"", i.green());
        }
        if let Some(c) = class {
            print!(" class=\"{}\"", c.yellow());
        }
        println!(">");

        let interactive = result
            .get("isInteractive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if interactive {
            println!("{} Interactive element", "✓".green());
        }

        if let Some(bbox) = result.get("boundingBox").or_else(|| result.get("boundingRect")) {
            let bx = bbox.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let by = bbox.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let bw = bbox.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let bh = bbox.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
            println!(
                "{} x={:.0}, y={:.0}, {}x{}",
                "?".dimmed(),
                bx,
                by,
                bw as i32,
                bh as i32
            );
        }

        if let Some(text) = result
            .get("textContent")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            println!("\n{}", "Text:".bold());
            println!("  {}", text.dimmed());
        }

        if let Some(selectors) = result.get("suggestedSelectors").and_then(|v| v.as_array()) {
            if !selectors.is_empty() {
                println!("\n{}", "Suggested Selectors:".bold());
                for sel in selectors {
                    if let Some(s) = sel.as_str() {
                        println!("  {} {}", "->".cyan(), s);
                    }
                }
            }
        }

        if let Some(attrs) = result.get("attributes").and_then(|v| v.as_object()) {
            if !attrs.is_empty() {
                println!("\n{}", "Attributes:".bold());
                for (key, value) in attrs {
                    if key != "class" && key != "id" {
                        let val = value.as_str().unwrap_or("");
                        let display_val = if val.len() > 50 {
                            format!("{}...", &val[..50])
                        } else {
                            val.to_string()
                        };
                        println!("  {}={}", key.dimmed(), display_val);
                    }
                }
            }
        }

        if let Some(parents) = result.get("parents").and_then(|v| v.as_array()) {
            if !parents.is_empty() {
                println!("\n{}", "Parent Hierarchy:".bold());
                for (i, parent) in parents.iter().enumerate() {
                    let ptag = parent
                        .get("tagName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let pid = parent
                        .get("id")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty());
                    let pclass = parent
                        .get("className")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty());

                    let indent = "  ".repeat(i + 1);
                    print!("{}^ <{}", indent, ptag);
                    if let Some(i) = pid {
                        print!(" #{}", i);
                    }
                    if let Some(c) = pclass {
                        let short_class = if c.len() > 30 {
                            format!("{}...", &c[..30])
                        } else {
                            c.to_string()
                        };
                        print!(" .{}", short_class);
                    }
                    println!(">");
                }
            }
        }
    }

    Ok(())
}

async fn viewport(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    let (width, height) = backend.viewport().await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "width": width, "height": height })
        );
    } else {
        println!("{} {}x{}", "Viewport:".bold(), width, height);
    }

    Ok(())
}

async fn cookies(
    cli: &Cli,
    backend: &dyn BrowserBackend,
    command: &Option<CookiesCommands>,
) -> Result<()> {
    match command {
        None | Some(CookiesCommands::List) => {
            let cookies = backend.get_cookies().await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&cookies)?);
            } else if cookies.is_empty() {
                println!("{} No cookies", "!".yellow());
            } else {
                println!("{} {} cookies\n", "✓".green(), cookies.len());
                for cookie in &cookies {
                    let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let value = cookie.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    let domain = cookie.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                    println!(
                        "  {} = {} {}",
                        name.bold(),
                        value,
                        format!("({})", domain).dimmed()
                    );
                }
            }
        }
        Some(CookiesCommands::Get { name }) => {
            let cookies = backend.get_cookies().await?;
            let cookie = cookies
                .iter()
                .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name));

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&cookie)?);
            } else {
                match cookie {
                    Some(c) => {
                        let value = c.get("value").and_then(|v| v.as_str()).unwrap_or("");
                        println!("{} = {}", name, value);
                    }
                    None => println!("{} Cookie not found: {}", "!".yellow(), name),
                }
            }
        }
        Some(CookiesCommands::Set {
            name,
            value,
            domain,
        }) => {
            backend
                .set_cookie(name, value, domain.as_deref())
                .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "name": name, "value": value })
                );
            } else {
                println!("{} Cookie set: {} = {}", "✓".green(), name, value);
            }
        }
        Some(CookiesCommands::Delete { name }) => {
            backend.delete_cookie(name).await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "name": name })
                );
            } else {
                println!("{} Cookie deleted: {}", "✓".green(), name);
            }
        }
        Some(CookiesCommands::Clear {
            domain,
            dry_run,
            yes,
        }) => {
            if *dry_run {
                let cookies = backend.get_cookies().await?;
                let filtered: Vec<_> = match domain.as_deref() {
                    Some(d) => cookies
                        .iter()
                        .filter(|c| {
                            c.get("domain")
                                .and_then(|v| v.as_str())
                                .is_some_and(|cd| cd.ends_with(d))
                        })
                        .collect(),
                    None => cookies.iter().collect(),
                };

                let target = domain.as_deref().unwrap_or("all");

                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "dry_run": true,
                            "domain": target,
                            "count": filtered.len(),
                            "cookies": filtered.iter().map(|c| {
                                serde_json::json!({
                                    "name": c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                    "domain": c.get("domain").and_then(|v| v.as_str()).unwrap_or(""),
                                })
                            }).collect::<Vec<_>>()
                        })
                    );
                } else {
                    println!(
                        "{} Dry run: {} cookies would be cleared for {}",
                        "!".yellow(),
                        filtered.len(),
                        target
                    );
                    for cookie in &filtered {
                        let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let cdomain =
                            cookie.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                        println!(
                            "  {} {}",
                            name.bold(),
                            format!("({})", cdomain).dimmed()
                        );
                    }
                }
                return Ok(());
            }

            if !yes {
                let cookies = backend.get_cookies().await?;
                let count = match domain.as_deref() {
                    Some(d) => cookies
                        .iter()
                        .filter(|c| {
                            c.get("domain")
                                .and_then(|v| v.as_str())
                                .is_some_and(|cd| cd.ends_with(d))
                        })
                        .count(),
                    None => cookies.len(),
                };
                let target = domain.as_deref().unwrap_or("all");

                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "error": "confirmation_required",
                            "message": "Pass --yes to confirm clearing cookies",
                            "count": count,
                            "domain": target
                        })
                    );
                } else {
                    println!(
                        "{} About to clear {} cookies for {}",
                        "!".yellow(),
                        count,
                        target
                    );
                    println!(
                        "  Re-run with {} to confirm, or use {} to preview details",
                        "--yes".bold(),
                        "--dry-run".bold()
                    );
                }
                return Ok(());
            }

            backend.clear_cookies(domain.as_deref()).await?;

            if cli.json {
                println!("{}", serde_json::json!({ "success": true }));
            } else {
                let target = domain.as_deref().unwrap_or("all");
                println!("{} Cookies cleared for {}", "✓".green(), target);
            }
        }
    }

    Ok(())
}

/// Close in extension mode without auto-starting the bridge.
///
/// If the bridge isn't running, there's nothing to close — report success.
/// If the bridge is running, attempt a single detachTab (no 30s retry).
/// Detach failure is non-fatal: the user asked to close, so we succeed
/// regardless (the tab is not "owned" by us in extension mode).
async fn close_extension(cli: &Cli) -> Result<()> {
    let port = cli.extension_port;

    if extension_bridge::is_bridge_running(port).await {
        match extension_bridge::send_command(
            port,
            "Extension.detachTab",
            serde_json::json!({}),
        )
        .await
        {
            Ok(_) => tracing::debug!("Extension tab detached"),
            Err(e) => tracing::debug!("Extension detach skipped (non-fatal): {}", e),
        }
    }

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Browser closed", "✓".green());
    }

    Ok(())
}

async fn close(
    cli: &Cli,
    config: &Config,
    backend: &dyn BrowserBackend,
    bridge_auto_started: bool,
) -> Result<()> {
    backend.close().await?;

    // Only stop the bridge if *this* CLI invocation auto-started it.
    // The bridge is a shared daemon — other CLI sessions or MCP tools may
    // still be using it. Use `actionbook extension stop` for explicit shutdown.
    if bridge_auto_started && resolve_mode(cli, config) == BrowserMode::Extension {
        bridge_lifecycle::stop_bridge(cli.extension_port).await?;
    }

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Browser closed", "✓".green());
    }

    Ok(())
}

async fn restart(cli: &Cli, backend: &dyn BrowserBackend) -> Result<()> {
    backend.restart().await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Browser restarted", "✓".green());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshot tree rendering (output formatting, stays in browser.rs)
// ---------------------------------------------------------------------------

/// Render a snapshot tree node as indented text lines.
fn render_snapshot_tree(node: &serde_json::Value, depth: usize) -> String {
    let mut output = String::new();
    let indent = "  ".repeat(depth);

    let role = node
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("generic");

    if role == "text" {
        if let Some(content) = node.get("content").and_then(|v| v.as_str()) {
            if !content.is_empty() {
                output.push_str(&format!("{}- text: {}\n", indent, content));
            }
        }
        return output;
    }

    let name = node.get("name").and_then(|v| v.as_str());
    let ref_id = node.get("ref").and_then(|v| v.as_str());
    let url = node.get("url").and_then(|v| v.as_str());
    let children = node.get("children").and_then(|v| v.as_array());
    let has_children = children.is_some_and(|c| !c.is_empty());

    let mut line = format!("{}- {}", indent, role);

    if let Some(n) = name {
        line.push_str(&format!(" \"{}\"", n));
    }

    if let Some(r) = ref_id {
        line.push_str(&format!(" [ref={}]", r));
    }

    if let Some(level) = node.get("level").and_then(|v| v.as_u64()) {
        line.push_str(&format!(" [level={}]", level));
    }
    if let Some(checked) = node.get("checked").and_then(|v| v.as_bool()) {
        line.push_str(&format!(" [checked={}]", checked));
    }
    if let Some(val) = node.get("value").and_then(|v| v.as_str()) {
        if !val.is_empty() {
            line.push_str(&format!(" [value=\"{}\"]", val));
        }
    }

    if has_children || url.is_some() {
        line.push(':');
    }

    output.push_str(&line);
    output.push('\n');

    if let Some(u) = url {
        output.push_str(&format!("{}  - /url: {}\n", indent, u));
    }

    if let Some(kids) = children {
        for child in kids {
            output.push_str(&render_snapshot_tree(child, depth + 1));
        }
    }

    output
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{effective_profile_name, normalize_navigation_url, render_snapshot_tree};
    use crate::cli::{BrowserCommands, Cli, Commands};
    use crate::config::Config;
    use serde_json::json;

    fn test_cli(profile: Option<&str>, command: BrowserCommands) -> Cli {
        Cli {
            browser_path: None,
            cdp: None,
            profile: profile.map(ToString::to_string),
            headless: false,
            stealth: false,
            stealth_os: None,
            stealth_gpu: None,
            api_key: None,
            json: false,
            browser_mode: None,
            extension: false,
            extension_port: 19222,
            verbose: false,
            command: Commands::Browser { command },
        }
    }

    #[test]
    fn normalize_domain_without_scheme() {
        assert_eq!(
            normalize_navigation_url("google.com").unwrap(),
            "https://google.com"
        );
    }

    #[test]
    fn normalize_domain_with_path_and_query() {
        assert_eq!(
            normalize_navigation_url("google.com/search?q=a").unwrap(),
            "https://google.com/search?q=a"
        );
    }

    #[test]
    fn normalize_localhost_with_port() {
        assert_eq!(
            normalize_navigation_url("localhost:3000").unwrap(),
            "https://localhost:3000"
        );
    }

    #[test]
    fn normalize_https_keeps_original() {
        assert_eq!(
            normalize_navigation_url("https://example.com").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_http_keeps_original() {
        assert_eq!(
            normalize_navigation_url("http://example.com").unwrap(),
            "http://example.com"
        );
    }

    #[test]
    fn normalize_about_keeps_original() {
        assert_eq!(
            normalize_navigation_url("about:blank").unwrap(),
            "about:blank"
        );
    }

    #[test]
    fn normalize_mailto_keeps_original() {
        assert_eq!(
            normalize_navigation_url("mailto:test@example.com").unwrap(),
            "mailto:test@example.com"
        );
    }

    #[test]
    fn normalize_protocol_relative_url() {
        assert_eq!(
            normalize_navigation_url("//example.com/path").unwrap(),
            "https://example.com/path"
        );
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(
            normalize_navigation_url("  google.com  ").unwrap(),
            "https://google.com"
        );
    }

    #[test]
    fn normalize_empty_input_returns_error() {
        assert!(normalize_navigation_url("").is_err());
        assert!(normalize_navigation_url("   ").is_err());
    }

    #[test]
    fn effective_profile_name_prefers_cli_profile() {
        let cli = test_cli(Some("work"), BrowserCommands::Status);
        let mut config = Config::default();
        config.browser.default_profile = "team".to_string();

        assert_eq!(effective_profile_name(&cli, &config), "work");
    }

    #[test]
    fn effective_profile_name_uses_config_default_profile() {
        let cli = test_cli(None, BrowserCommands::Status);
        let mut config = Config::default();
        config.browser.default_profile = "team".to_string();

        assert_eq!(effective_profile_name(&cli, &config), "team");
    }

    #[test]
    fn effective_profile_name_falls_back_to_actionbook() {
        let cli = test_cli(None, BrowserCommands::Status);
        let mut config = Config::default();
        config.browser.default_profile = "   ".to_string();

        assert_eq!(effective_profile_name(&cli, &config), "actionbook");
    }

    #[test]
    fn connect_uses_same_effective_profile_resolution() {
        let cli = test_cli(
            None,
            BrowserCommands::Connect {
                endpoint: "ws://127.0.0.1:9222".to_string(),
            },
        );
        let mut config = Config::default();
        config.browser.default_profile = "team-connect".to_string();

        assert_eq!(effective_profile_name(&cli, &config), "team-connect");
    }

    #[test]
    fn render_simple_button() {
        let node = json!({
            "role": "button",
            "name": "Submit",
            "ref": "e1"
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- button \"Submit\" [ref=e1]\n");
    }

    #[test]
    fn render_heading_with_level() {
        let node = json!({
            "role": "heading",
            "name": "Welcome",
            "ref": "e1",
            "level": 1
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- heading \"Welcome\" [ref=e1] [level=1]\n");
    }

    #[test]
    fn render_checkbox_with_checked() {
        let node = json!({
            "role": "checkbox",
            "name": "Accept terms",
            "ref": "e1",
            "checked": true
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(
            output,
            "- checkbox \"Accept terms\" [ref=e1] [checked=true]\n"
        );
    }

    #[test]
    fn render_textbox_with_value() {
        let node = json!({
            "role": "textbox",
            "name": "Email",
            "ref": "e1",
            "value": "test@example.com"
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(
            output,
            "- textbox \"Email\" [ref=e1] [value=\"test@example.com\"]\n"
        );
    }

    #[test]
    fn render_empty_value_not_shown() {
        let node = json!({
            "role": "textbox",
            "name": "Search",
            "ref": "e1",
            "value": ""
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- textbox \"Search\" [ref=e1]\n");
    }

    #[test]
    fn render_text_node() {
        let node = json!({
            "role": "text",
            "content": "Hello world"
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- text: Hello world\n");
    }

    #[test]
    fn render_node_with_text_children() {
        let node = json!({
            "role": "generic",
            "children": [
                { "role": "text", "content": "Hello world" }
            ]
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- generic:\n  - text: Hello world\n");
    }

    #[test]
    fn render_nested_tree() {
        let tree = json!({
            "role": "navigation",
            "children": [
                {
                    "role": "list",
                    "children": [
                        {
                            "role": "listitem",
                            "children": [
                                { "role": "link", "name": "Home", "ref": "e1" }
                            ]
                        },
                        {
                            "role": "listitem",
                            "children": [
                                { "role": "link", "name": "About", "ref": "e2" }
                            ]
                        }
                    ]
                }
            ]
        });
        let output = render_snapshot_tree(&tree, 0);
        let expected = "\
- navigation:
  - list:
    - listitem:
      - link \"Home\" [ref=e1]
    - listitem:
      - link \"About\" [ref=e2]
";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_respects_depth_indentation() {
        let node = json!({
            "role": "button",
            "name": "Deep",
            "ref": "e5"
        });
        let output = render_snapshot_tree(&node, 3);
        assert_eq!(output, "      - button \"Deep\" [ref=e5]\n");
    }

    #[test]
    fn render_no_ref_no_name() {
        let node = json!({ "role": "generic" });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- generic\n");
    }

    #[test]
    fn render_children_adds_colon() {
        let node = json!({
            "role": "form",
            "children": [
                { "role": "button", "name": "Go", "ref": "e1" }
            ]
        });
        let output = render_snapshot_tree(&node, 0);
        assert!(output.starts_with("- form:\n"));
    }

    #[test]
    fn render_leaf_no_colon() {
        let node = json!({
            "role": "link",
            "name": "Click me",
            "ref": "e1"
        });
        let output = render_snapshot_tree(&node, 0);
        assert!(!output.contains(':'));
    }

    #[test]
    fn render_link_with_url() {
        let node = json!({
            "role": "link",
            "ref": "e1",
            "url": "https://example.com",
            "children": [
                { "role": "text", "content": "Example" }
            ]
        });
        let output = render_snapshot_tree(&node, 0);
        let expected = "\
- link [ref=e1]:
  - /url: https://example.com
  - text: Example
";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_link_with_name_and_url() {
        let node = json!({
            "role": "link",
            "name": "Home",
            "ref": "e1",
            "url": "https://example.com/home",
            "children": [
                { "role": "text", "content": "Home" }
            ]
        });
        let output = render_snapshot_tree(&node, 0);
        assert!(output.starts_with("- link \"Home\" [ref=e1]:"));
        assert!(output.contains("- /url: https://example.com/home"));
        assert!(output.contains("- text: Home"));
    }

    #[test]
    fn render_inline_strong() {
        let node = json!({
            "role": "strong",
            "children": [
                { "role": "text", "content": "bold text" }
            ]
        });
        let output = render_snapshot_tree(&node, 0);
        assert_eq!(output, "- strong:\n  - text: bold text\n");
    }

    #[test]
    fn render_url_adds_colon() {
        let node = json!({
            "role": "link",
            "name": "Click",
            "ref": "e1",
            "url": "https://example.com"
        });
        let output = render_snapshot_tree(&node, 0);
        assert!(output.contains("- link \"Click\" [ref=e1]:"));
        assert!(output.contains("- /url: https://example.com"));
    }

    #[test]
    fn render_realistic_page() {
        let tree = json!({
            "role": "generic",
            "children": [
                {
                    "role": "banner",
                    "children": [
                        {
                            "role": "navigation",
                            "name": "Main",
                            "ref": "e1",
                            "children": [
                                { "role": "link", "name": "Home", "ref": "e2" },
                                { "role": "link", "name": "Products", "ref": "e3" }
                            ]
                        }
                    ]
                },
                {
                    "role": "main",
                    "children": [
                        { "role": "heading", "name": "Welcome", "ref": "e4", "level": 1 },
                        {
                            "role": "form",
                            "children": [
                                { "role": "textbox", "name": "Email", "ref": "e5", "value": "" },
                                { "role": "button", "name": "Subscribe", "ref": "e6" }
                            ]
                        }
                    ]
                }
            ]
        });
        let output = render_snapshot_tree(&tree, 0);

        assert!(output.contains("- navigation \"Main\" [ref=e1]:"));
        assert!(output.contains("  - link \"Home\" [ref=e2]"));
        assert!(output.contains("- heading \"Welcome\" [ref=e4] [level=1]"));
        assert!(output.contains("- textbox \"Email\" [ref=e5]"));
        assert!(output.contains("- button \"Subscribe\" [ref=e6]"));

        let lines: Vec<&str> = output.lines().collect();
        assert!(lines[0].starts_with("- generic:"));
        assert!(lines[1].starts_with("  - banner:"));
    }
}
