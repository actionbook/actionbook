use std::fs;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use colored::Colorize;
use futures::StreamExt;
use tokio::time::timeout;

#[cfg(feature = "stealth")]
use crate::browser::apply_stealth_to_page;
use crate::browser::extension_backend::ExtensionBackend;
use crate::browser::{
    bridge_lifecycle, build_stealth_profile, discover_all_browsers,
    BrowserDriver, stealth_status, SessionManager, SessionStatus, StealthConfig,
    ResourceBlockLevel,
};
use crate::cli::{BrowserCommands, BrowserMode, Cli, CookiesCommands, FingerprintCommands, StorageCommands, TabCommands};
use crate::config::{Config, DEFAULT_EXTENSION_PORT};
use crate::error::{ActionbookError, Result};

/// Send a command (CDP or Extension.*) through the extension bridge.
/// Delegates to ExtensionBackend which provides:
/// - Auto-attach: retries with Extension.attachActiveTab on "No tab attached"
/// - Connection retry: waits up to 30s for the extension to connect
async fn extension_send(
    cli: &Cli,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let backend = ExtensionBackend::new(cli.extension_port);
    backend.send(method, params).await
}

/// Evaluate JS via the extension bridge and return the result value
async fn extension_eval(cli: &Cli, expression: &str) -> Result<serde_json::Value> {
    let result = extension_send(
        cli,
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        }),
    )
    .await?;

    // Check for exception
    if let Some(exception) = result.get("exceptionDetails") {
        let msg = exception
            .get("text")
            .or_else(|| exception.get("exception").and_then(|e| e.get("description")))
            .and_then(|v| v.as_str())
            .unwrap_or("JavaScript exception");
        return Err(ActionbookError::ExtensionError(format!(
            "JS error (extension mode): {}",
            msg
        )));
    }

    Ok(result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or_else(|| {
            result
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        }))
}

/// Escape a string for safe embedding in a JS single-quoted string literal.
/// Uses serde_json for comprehensive Unicode escaping, then converts to single-quote context.
fn escape_js_string(s: &str) -> String {
    // serde_json::to_string produces a valid JSON double-quoted string with all
    // special chars escaped (\n, \t, \", \\, \uXXXX, etc.)
    let json = serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s));
    // Strip the surrounding double quotes
    let inner = &json[1..json.len() - 1];
    // In single-quote JS context: unescape \" (not needed) and escape '
    inner.replace("\\\"", "\"").replace('\'', "\\'")
}

/// JavaScript helper that resolves a selector (CSS or [ref=eN] format) and returns the element.
/// This is injected as a prefix for extension-mode commands that operate on selectors.
fn js_resolve_selector(selector: &str) -> String {
    format!(
        r#"(function(selector) {{
    if (/^\[ref=e\d+\]$/.test(selector)) {{
        var refId = selector.match(/^\[ref=(e\d+)\]$/)[1];
        var SKIP = new Set(['script','style','noscript','template','svg','path','defs','clippath','lineargradient','stop','meta','link','br','wbr']);
        var INTERACTIVE = new Set(['button','link','textbox','checkbox','radio','combobox','listbox','menuitem','menuitemcheckbox','menuitemradio','option','searchbox','slider','spinbutton','switch','tab','treeitem']);
        var CONTENT = new Set(['heading','cell','gridcell','columnheader','rowheader','listitem','article','region','main','navigation','img']);
        function getRole(el) {{
            var explicit = el.getAttribute('role');
            if (explicit) return explicit.toLowerCase();
            var tag = el.tagName.toLowerCase();
            var map = {{'a': el.hasAttribute('href')?'link':'generic','button':'button','select':'combobox','textarea':'textbox','img':'img','h1':'heading','h2':'heading','h3':'heading','h4':'heading','h5':'heading','h6':'heading','nav':'navigation','main':'main','header':'banner','footer':'contentinfo','aside':'complementary','form':'form','table':'table','thead':'rowgroup','tbody':'rowgroup','tfoot':'rowgroup','tr':'row','th':'columnheader','td':'cell','ul':'list','ol':'list','li':'listitem','details':'group','summary':'button','dialog':'dialog','article':'article'}};
            if (tag === 'input') {{
                var type = (el.getAttribute('type')||'text').toLowerCase();
                var imap = {{'text':'textbox','email':'textbox','password':'textbox','search':'searchbox','tel':'textbox','url':'textbox','number':'spinbutton','checkbox':'checkbox','radio':'radio','submit':'button','reset':'button','button':'button','range':'slider'}};
                return imap[type]||'textbox';
            }}
            if (tag === 'section') return (el.hasAttribute('aria-label')||el.hasAttribute('aria-labelledby'))?'region':'generic';
            return map[tag]||'generic';
        }}
        function getName(el) {{
            if (el.getAttribute('aria-label')) return el.getAttribute('aria-label').trim();
            return '';
        }}
        var counter = 0;
        function findRef(el, depth) {{
            if (depth > 15) return null;
            var tag = el.tagName.toLowerCase();
            if (SKIP.has(tag)) return null;
            if (el.hidden || el.getAttribute('aria-hidden')==='true') return null;
            var role = getRole(el);
            var name = getName(el);
            var shouldRef = INTERACTIVE.has(role) || (CONTENT.has(role) && name);
            if (shouldRef) {{
                counter++;
                if ('e'+counter === refId) return el;
            }}
            for (var i = 0; i < el.children.length; i++) {{
                var found = findRef(el.children[i], depth+1);
                if (found) return found;
            }}
            return null;
        }}
        return findRef(document.body, 0);
    }}
    return document.querySelector(selector);
}})('{}')"#,
        escape_js_string(selector)
    )
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

/// Create a browser driver — public entry point for other command modules (e.g., batch)
pub async fn create_browser_driver_public(cli: &Cli, config: &Config) -> Result<BrowserDriver> {
    create_browser_driver(cli, config).await
}

/// Create a browser driver for multi-backend support (CDP or Camoufox)
async fn create_browser_driver(cli: &Cli, config: &Config) -> Result<BrowserDriver> {
    // Determine profile
    let profile_name = effective_profile_arg(cli, config).unwrap_or(&config.browser.default_profile);
    let profile = config
        .profiles
        .get(profile_name)
        .ok_or_else(|| ActionbookError::Other(format!("Profile not found: {}", profile_name)))?;

    BrowserDriver::from_config(config, profile, cli).await
}

/// Apply resource blocking based on CLI flags (--block-images, --block-media)
async fn apply_resource_blocking(cli: &Cli, driver: &mut BrowserDriver) {
    let level = if cli.block_media {
        ResourceBlockLevel::Media
    } else if cli.block_images {
        ResourceBlockLevel::Images
    } else {
        ResourceBlockLevel::None
    };
    if level != ResourceBlockLevel::None {
        if let Err(e) = driver.set_resource_blocking(level).await {
            tracing::warn!("Failed to set resource blocking: {}", e);
        }
    }

    // G2: Apply animation disabling if requested
    if cli.no_animations {
        if let Err(e) = driver.disable_animations().await {
            tracing::warn!("Failed to disable animations: {}", e);
        }
    }

    // H3: Apply dialog auto-dismissal if requested
    if cli.auto_dismiss_dialogs {
        if let Err(e) = driver.enable_dialog_auto_dismiss().await {
            tracing::warn!("Failed to enable dialog auto-dismiss: {}", e);
        }
    }
}

/// Resolve a snapshot ref (e.g., "e0") to a backendNodeId by fetching the accessibility tree
async fn resolve_snapshot_ref(driver: &mut BrowserDriver, ref_str: &str) -> Result<i64> {
    let raw = driver.get_accessibility_tree_raw().await?;
    let (_nodes, cache) = crate::browser::snapshot::parse_ax_tree(
        raw,
        crate::browser::snapshot::SnapshotFilter::All,
        None,
        None,
    )?;
    cache
        .refs
        .get(ref_str)
        .copied()
        .ok_or_else(|| ActionbookError::Other(format!("Ref '{}' not found in current snapshot", ref_str)))
}

/// Resolve a CDP endpoint string (port number or ws:// URL) into a (port, ws_url) pair.
/// When given a numeric port, queries `http://127.0.0.1:{port}/json/version` to discover
/// the current browser WebSocket URL.
pub(crate) async fn resolve_cdp_endpoint(endpoint: &str) -> Result<(u16, String)> {
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
/// This is a no-op when `--cdp` is not set.
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

pub(crate) fn effective_profile_name<'a>(cli: &'a Cli, config: &'a Config) -> &'a str {
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

fn is_reusable_initial_blank_page_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    let normalized = normalized.trim_end_matches('/');

    matches!(
        normalized,
        "about:blank"
            | "about:newtab"
            | "chrome://newtab"
            | "chrome://new-tab-page"
            | "edge://newtab"
    )
}

async fn try_open_on_initial_blank_page(
    session_manager: &SessionManager,
    profile_name: Option<&str>,
    normalized_url: &str,
) -> Result<Option<String>> {
    let pages = match session_manager.get_pages(profile_name).await {
        Ok(pages) => pages,
        Err(e) => {
            tracing::debug!(
                "Unable to inspect current tabs for reuse, falling back to new tab: {}",
                e
            );
            return Ok(None);
        }
    };

    if pages.len() != 1 || !is_reusable_initial_blank_page_url(&pages[0].url) {
        return Ok(None);
    }

    match timeout(
        Duration::from_secs(30),
        session_manager.goto(profile_name, normalized_url),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return Err(ActionbookError::Other(format!(
                "Failed to open page on initial tab: {}",
                e
            )));
        }
        Err(_) => {
            return Err(ActionbookError::Timeout(format!(
                "Page load timed out after 30 seconds: {}",
                normalized_url
            )));
        }
    }

    let _ = wait_for_document_complete(session_manager, profile_name, 30_000).await;

    let title = match timeout(
        Duration::from_secs(5),
        session_manager.eval_on_page(profile_name, "document.title"),
    )
    .await
    {
        Ok(Ok(value)) => value.as_str().unwrap_or("").to_string(),
        _ => String::new(),
    };

    Ok(Some(title))
}

async fn wait_for_document_complete(
    session_manager: &SessionManager,
    profile_name: Option<&str>,
    timeout_ms: u64,
) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    loop {
        let ready_state = session_manager
            .eval_on_page(profile_name, "document.readyState")
            .await?;

        if ready_state.as_str() == Some("complete") {
            return Ok(());
        }

        if start.elapsed() > timeout {
            return Err(ActionbookError::Timeout(format!(
                "Page did not reach complete state within {}ms",
                timeout_ms
            )));
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
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

/// Resolve effective browser mode.
/// Priority: --browser-mode > --extension (deprecated) > config.browser.mode
fn resolve_browser_mode(
    browser_mode: Option<BrowserMode>,
    extension_flag: bool,
    extension_port: u16,
    config_mode: BrowserMode,
    config_port: u16,
) -> (bool, u16) {
    if browser_mode == Some(BrowserMode::Extension) {
        let port = if extension_port == DEFAULT_EXTENSION_PORT { config_port } else { extension_port };
        (true, port)
    } else if browser_mode == Some(BrowserMode::Isolated) {
        (false, extension_port)
    } else if extension_flag {
        let port = if extension_port == DEFAULT_EXTENSION_PORT { config_port } else { extension_port };
        (true, port)
    } else if browser_mode.is_none() && matches!(config_mode, BrowserMode::Extension) {
        (true, config_port)
    } else {
        (false, extension_port)
    }
}

/// Check if a command requires an active session (bridge/browser)
fn requires_active_session(command: &BrowserCommands) -> bool {
    !matches!(
        command,
        // Read-only commands that can work without active session
        BrowserCommands::Status | BrowserCommands::Pages |
        // Close is safe to call even when no session exists
        BrowserCommands::Close
    )
}

pub async fn run(cli: &Cli, command: &BrowserCommands) -> Result<()> {
    let mut config = Config::load()?;

    // Resolve effective extension mode.
    // Priority: --browser-mode > --extension (deprecated) > config.browser.mode
    let (ext_enabled, ext_port) = resolve_browser_mode(
        cli.browser_mode, cli.extension, cli.extension_port,
        config.browser.mode, config.browser.extension.port,
    );
    let cli = {
        let mut effective = cli.clone();
        effective.extension = ext_enabled;
        effective.extension_port = ext_port;
        effective
    };
    let cli = &cli;

    // --profile is not supported in extension mode: extension operates on the live Chrome profile
    if cli.extension && cli.profile.is_some() {
        return Err(ActionbookError::Other(
            "--profile is not supported in extension mode. Extension operates on your live Chrome profile. \
             Remove --profile to use the default profile, or remove --extension to use isolated mode.".to_string()
        ));
    }

    // Apply CLI overrides (--browser-path, --headless) to the active profile
    if cli.browser_path.is_some() || cli.headless {
        let profile_name = cli
            .profile
            .as_deref()
            .unwrap_or(&config.browser.default_profile)
            .to_string();
        let mut profile = config.get_profile(&profile_name).unwrap_or_default();
        if let Some(ref path) = cli.browser_path {
            profile.browser_path = Some(path.clone());
        }
        if cli.headless {
            profile.headless = true;
        }
        config.set_profile(&profile_name, profile);
    }

    // When --cdp is set, resolve it to a fresh WebSocket URL and persist it
    // as the active session *before* any command runs. Skip for `connect`
    // which has its own CDP resolution logic.
    if !matches!(command, BrowserCommands::Connect { .. }) {
        ensure_cdp_override(cli, &config).await?;
    }

    // Auto-start extension bridge when in extension mode
    // Skip for read-only commands (status, pages, etc.)
    if cli.extension && requires_active_session(command) {
        bridge_lifecycle::ensure_bridge_running(cli.extension_port).await?;
    }

    match command {
        BrowserCommands::Status => status(cli, &config).await,
        BrowserCommands::Open { url } => open(cli, &config, url).await,
        BrowserCommands::Goto { url, timeout: t } => goto(cli, &config, url, *t).await,
        BrowserCommands::Back => back(cli, &config).await,
        BrowserCommands::Forward => forward(cli, &config).await,
        BrowserCommands::Reload => reload(cli, &config).await,
        BrowserCommands::Pages => pages(cli, &config).await,
        BrowserCommands::Switch { page_id } => switch(cli, &config, page_id).await,
        BrowserCommands::Wait {
            selector,
            timeout: t,
        } => wait(cli, &config, selector, *t).await,
        BrowserCommands::WaitNav { timeout: t } => wait_nav(cli, &config, *t).await,
        BrowserCommands::Click { selector, wait: w, ref_id, human } => {
            click(cli, &config, selector.as_deref(), *w, ref_id.as_deref(), *human).await
        }
        BrowserCommands::Type {
            selector,
            text,
            wait: w,
            ref_id,
            human,
        } => {
            type_text(cli, &config, selector.as_deref(), text, *w, ref_id.as_deref(), *human).await
        },
        BrowserCommands::Fill {
            selector,
            text,
            wait: w,
            ref_id,
        } => {
            fill(cli, &config, selector.as_deref(), text, *w, ref_id.as_deref()).await
        },
        BrowserCommands::Select { selector, value } => select(cli, &config, selector, value).await,
        BrowserCommands::Hover { selector } => hover(cli, &config, selector).await,
        BrowserCommands::Focus { selector } => focus(cli, &config, selector).await,
        BrowserCommands::Press { key } => press(cli, &config, key).await,
        BrowserCommands::Hotkey { keys } => hotkey(cli, &config, keys).await,
        BrowserCommands::Screenshot { path, full_page } => {
            screenshot(cli, &config, path, *full_page).await
        }
        BrowserCommands::Pdf { path } => pdf(cli, &config, path).await,
        BrowserCommands::Eval { code } => eval(cli, &config, code).await,
        BrowserCommands::Html { selector } => html(cli, &config, selector.as_deref()).await,
        BrowserCommands::Text { selector, mode } => text(cli, &config, selector.as_deref(), mode).await,
        BrowserCommands::Snapshot { interactive, cursor, compact, format, depth, selector, diff, max_tokens } => {
            snapshot(cli, &config, *interactive, *cursor, *compact, format, *depth, selector.as_deref(), *diff, *max_tokens).await
        }
        BrowserCommands::Inspect { x, y, desc } => {
            inspect(cli, &config, *x, *y, desc.as_deref()).await
        }
        BrowserCommands::Viewport => viewport(cli, &config).await,
        BrowserCommands::Cookies { command } => cookies(cli, &config, command).await,
        BrowserCommands::Scroll { direction, smooth, wait } => {
            scroll(cli, &config, direction, *smooth, *wait).await
        }
        BrowserCommands::Batch { file, delay } => {
            crate::commands::batch::run(cli, &config, file.as_deref(), *delay).await
        }
        BrowserCommands::Fingerprint { command } => fingerprint(cli, &config, command).await,
        BrowserCommands::Console { duration, level } => {
            console_log(cli, &config, *duration, level).await
        }
        BrowserCommands::WaitIdle { timeout, idle_time } => {
            wait_idle(cli, &config, *timeout, *idle_time).await
        }
        BrowserCommands::Info { selector } => info(cli, &config, selector).await,
        BrowserCommands::Storage { command } => storage(cli, &config, command).await,
        BrowserCommands::Emulate { device } => emulate(cli, &config, device).await,
        BrowserCommands::WaitFn { expression, timeout, interval } => {
            wait_fn(cli, &config, expression, *timeout, *interval).await
        }
        BrowserCommands::Upload { files, selector, ref_id, wait: w } => {
            upload(cli, &config, files, selector.as_deref(), ref_id.as_deref(), *w).await
        }
        BrowserCommands::Fetch { url, format, max_tokens, timeout: t, lite } => {
            fetch(cli, &config, url, format, *max_tokens, *t, *lite).await
        }
        BrowserCommands::Close => close(cli, &config).await,
        BrowserCommands::Restart => restart(cli, &config).await,
        BrowserCommands::Connect { endpoint, headers } => connect(cli, &config, endpoint, headers).await,
        BrowserCommands::Tab { command } => tab_command(cli, &config, command).await,
        BrowserCommands::SwitchFrame { target } => switch_frame(cli, &config, target).await,
    }
}

pub(crate) async fn status(cli: &Cli, config: &Config) -> Result<()> {
    // Show API key status
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

    // Show stealth mode status
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

    // Show detected browsers
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

    // Show session status
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

            // Show open pages
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

pub(crate) async fn open(cli: &Cli, config: &Config, url: &str) -> Result<()> {
    let normalized_url = normalize_navigation_url(url)?;
    let normalized_url = if cli.rewrite_urls {
        let (rewritten, was_rewritten) = crate::browser::url_rewrite::maybe_rewrite(&normalized_url);
        if was_rewritten {
            tracing::info!("URL rewritten: {} -> {}", normalized_url, rewritten);
        }
        rewritten
    } else {
        normalized_url
    };

    if cli.extension {
        let result = extension_send(
            cli,
            "Extension.createTab",
            serde_json::json!({ "url": normalized_url }),
        )
        .await?;

        let title = result
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
                    "url": normalized_url,
                    "title": title
                })
            );
        } else {
            println!("{} {} (extension)", "✓".green(), title.bold());
            println!("  {}", normalized_url.dimmed());
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let profile_arg = effective_profile_arg(cli, config);
    let (browser, mut handler) = session_manager.get_or_create_session(profile_arg).await?;

    // Spawn handler in background
    tokio::spawn(async move { while handler.next().await.is_some() {} });

    if let Some(title) =
        match try_open_on_initial_blank_page(&session_manager, profile_arg, &normalized_url).await
        {
            Ok(title) => title,
            Err(e) => {
                tracing::debug!("Failed to reuse initial blank tab, opening a new tab: {}", e);
                None
            }
        }
    {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
                    "url": normalized_url,
                    "title": title
                })
            );
        } else {
            println!("{} {}", "✓".green(), title.bold());
            println!("  {}", normalized_url.dimmed());
        }
        return Ok(());
    }

    // Navigate to URL with timeout (30 seconds for page creation)
    let page = match timeout(Duration::from_secs(30), browser.new_page(&normalized_url)).await {
        Ok(Ok(page)) => page,
        Ok(Err(e)) => {
            return Err(ActionbookError::Other(format!(
                "Failed to open page: {}",
                e
            )));
        }
        Err(_) => {
            return Err(ActionbookError::Timeout(format!(
                "Page load timed out after 30 seconds: {}",
                normalized_url
            )));
        }
    };

    // Apply stealth profile if enabled
    #[cfg(feature = "stealth")]
    if cli.stealth {
        let stealth_profile =
            build_stealth_profile(cli.stealth_os.as_deref(), cli.stealth_gpu.as_deref());
        if let Err(e) = apply_stealth_to_page(&page, &stealth_profile).await {
            tracing::warn!("Failed to apply stealth profile: {}", e);
        } else {
            tracing::info!("Applied stealth profile to page");
        }
    }

    // Wait for page to fully load (additional 30 seconds)
    let _ = timeout(Duration::from_secs(30), page.wait_for_navigation()).await;

    // Get page title with timeout
    let title = match timeout(Duration::from_secs(5), page.get_title()).await {
        Ok(Ok(Some(t))) => t,
        _ => String::new(),
    };

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "url": normalized_url,
                "title": title
            })
        );
    } else {
        println!("{} {}", "✓".green(), title.bold());
        println!("  {}", normalized_url.dimmed());
    }

    Ok(())
}

pub(crate) async fn goto(cli: &Cli, config: &Config, url: &str, _timeout_ms: u64) -> Result<()> {
    let normalized_url = normalize_navigation_url(url)?;
    let normalized_url = if cli.rewrite_urls {
        let (rewritten, was_rewritten) = crate::browser::url_rewrite::maybe_rewrite(&normalized_url);
        if was_rewritten {
            tracing::info!("URL rewritten: {} -> {}", normalized_url, rewritten);
        }
        rewritten
    } else {
        normalized_url
    };

    if cli.extension {
        // Extension + Camoufox mode: use Camoufox backend through bridge
        if cli.camofox {
            extension_send(
                cli,
                "Camoufox.goto",
                serde_json::json!({ "url": normalized_url }),
            )
            .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "url": normalized_url, "backend": "Camofox" })
                );
            } else {
                println!(
                    "{} Navigated to: {} (extension + camoufox)",
                    "✓".green(),
                    normalized_url
                );
            }
        } else {
            // Extension + CDP mode (default)
            extension_send(
                cli,
                "Page.navigate",
                serde_json::json!({ "url": normalized_url }),
            )
            .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "url": normalized_url })
                );
            } else {
                println!(
                    "{} Navigated to: {} (extension)",
                    "✓".green(),
                    normalized_url
                );
            }
        }
        return Ok(());
    }

    // Use BrowserDriver for multi-backend support (CDP or Camoufox)
    let mut driver = create_browser_driver(cli, config).await?;
    driver.goto(&normalized_url).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "url": normalized_url,
                "backend": format!("{:?}", driver.backend())
            })
        );
    } else {
        let backend_label = if driver.is_camofox() { " (camoufox)" } else { "" };
        println!("{} Navigated to: {}{}", "✓".green(), normalized_url, backend_label);
    }

    Ok(())
}

pub(crate) async fn back(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        extension_eval(cli, "history.back()").await?;

        if cli.json {
            println!("{}", serde_json::json!({ "success": true }));
        } else {
            println!("{} Went back (extension)", "✓".green());
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .go_back(effective_profile_arg(cli, config))
        .await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Went back", "✓".green());
    }

    Ok(())
}

pub(crate) async fn forward(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        extension_eval(cli, "history.forward()").await?;

        if cli.json {
            println!("{}", serde_json::json!({ "success": true }));
        } else {
            println!("{} Went forward (extension)", "✓".green());
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .go_forward(effective_profile_arg(cli, config))
        .await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Went forward", "✓".green());
    }

    Ok(())
}

pub(crate) async fn reload(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        extension_send(cli, "Page.reload", serde_json::json!({})).await?;

        if cli.json {
            println!("{}", serde_json::json!({ "success": true }));
        } else {
            println!("{} Page reloaded (extension)", "✓".green());
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .reload(effective_profile_arg(cli, config))
        .await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Page reloaded", "✓".green());
    }

    Ok(())
}

pub(crate) async fn pages(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        let result = extension_send(
            cli,
            "Extension.listTabs",
            serde_json::json!({}),
        )
        .await?;

        let tabs = result
            .get("tabs")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&tabs)?);
        } else if tabs.is_empty() {
            println!("{} No tabs found", "!".yellow());
        } else {
            println!("{} {} tabs open (extension mode)\n", "✓".green(), tabs.len());
            for (i, tab) in tabs.iter().enumerate() {
                let title = tab.get("title").and_then(|t| t.as_str()).unwrap_or("(no title)");
                let url = tab.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let id = tab.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                println!(
                    "{}. {} {}",
                    (i + 1).to_string().cyan(),
                    title.bold(),
                    format!("(tab:{})", id).dimmed()
                );
                println!("   {}", url.dimmed());
            }
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let pages = session_manager
        .get_pages(effective_profile_arg(cli, config))
        .await?;

    if cli.json {
        let pages_json: Vec<_> = pages
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
    } else {
        if pages.is_empty() {
            println!("{} No pages open", "!".yellow());
        } else {
            println!("{} {} pages open\n", "✓".green(), pages.len());
            for (i, page) in pages.iter().enumerate() {
                println!(
                    "{}. {} {}",
                    (i + 1).to_string().cyan(),
                    page.title.bold(),
                    format!("({})", &page.id[..8.min(page.id.len())]).dimmed()
                );
                println!("   {}", page.url.dimmed());
            }
        }
    }

    Ok(())
}

pub(crate) async fn switch(cli: &Cli, _config: &Config, page_id: &str) -> Result<()> {
    if cli.extension {
        // In extension mode, page_id is expected to be a tab ID (numeric)
        let tab_id: u64 = page_id.strip_prefix("tab:").unwrap_or(page_id).parse().map_err(|_| {
            ActionbookError::Other(format!(
                "Invalid tab ID: {}. Use the numeric ID from 'pages' command (extension mode)",
                page_id
            ))
        })?;

        extension_send(
            cli,
            "Extension.activateTab",
            serde_json::json!({ "tabId": tab_id }),
        )
        .await?;

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "tabId": tab_id })
            );
        } else {
            println!(
                "{} Switched to tab {} (extension)",
                "✓".green(),
                tab_id
            );
        }
        return Ok(());
    }

    // Note: This would require storing the active page ID in session state
    // For now, we just acknowledge the command
    println!(
        "{} Page switching requires session state management (not yet implemented)",
        "!".yellow()
    );
    println!("  Requested page: {}", page_id);
    Ok(())
}

pub(crate) async fn wait(cli: &Cli, config: &Config, selector: &str, timeout_ms: u64) -> Result<()> {
    if cli.extension {
        let resolve_js = js_resolve_selector(selector);
        let poll_js = format!(
            r#"(async function() {{
                var deadline = Date.now() + {};
                while (Date.now() < deadline) {{
                    var el = {};
                    if (el) return true;
                    await new Promise(r => setTimeout(r, 100));
                }}
                return false;
            }})()"#,
            timeout_ms, resolve_js
        );
        let found = extension_eval(cli, &poll_js).await?;
        if found.as_bool() != Some(true) {
            return Err(ActionbookError::Timeout(format!(
                "Element not found within {}ms (extension mode): {}",
                timeout_ms, selector
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector })
            );
        } else {
            println!("{} Element found: {} (extension)", "✓".green(), selector);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .wait_for_element(effective_profile_arg(cli, config), selector, timeout_ms)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector
            })
        );
    } else {
        println!("{} Element found: {}", "✓".green(), selector);
    }

    Ok(())
}

pub(crate) async fn wait_nav(cli: &Cli, config: &Config, timeout_ms: u64) -> Result<()> {
    if cli.extension {
        // Poll document.readyState until "complete" or timeout
        let poll_js = format!(
            r#"(async function() {{
                var deadline = Date.now() + {};
                while (Date.now() < deadline) {{
                    if (document.readyState === 'complete') return window.location.href;
                    await new Promise(r => setTimeout(r, 100));
                }}
                return document.readyState === 'complete' ? window.location.href : null;
            }})()"#,
            timeout_ms
        );
        let result = extension_eval(cli, &poll_js).await?;
        let new_url = result.as_str().unwrap_or("").to_string();

        if new_url.is_empty() {
            return Err(ActionbookError::Timeout(format!(
                "Navigation did not complete within {}ms (extension mode)",
                timeout_ms
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "url": new_url })
            );
        } else {
            println!(
                "{} Navigation complete: {} (extension)",
                "✓".green(),
                new_url
            );
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let new_url = session_manager
        .wait_for_navigation(effective_profile_arg(cli, config), timeout_ms)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "url": new_url
            })
        );
    } else {
        println!("{} Navigation complete: {}", "✓".green(), new_url);
    }

    Ok(())
}

pub(crate) async fn click(
    cli: &Cli,
    config: &Config,
    selector: Option<&str>,
    wait_ms: u64,
    ref_id: Option<&str>,
    human: bool,
) -> Result<()> {
    // Require either selector or --ref
    if selector.is_none() && ref_id.is_none() {
        return Err(ActionbookError::Other(
            "Either a CSS selector or --ref must be provided".to_string(),
        ));
    }

    // --ref mode: use snapshot ref to click by backendNodeId
    if let Some(ref_str) = ref_id {
        let mut driver = create_browser_driver(cli, config).await?;
        apply_resource_blocking(cli, &mut driver).await;
        let backend_node_id = resolve_snapshot_ref(&mut driver, ref_str).await?;

        if human {
            // Human-like click: resolve actual element coords, then bezier path
            let (target_x, target_y) = driver.get_element_center_by_node_id(backend_node_id).await?;
            let (start_x, start_y) = crate::browser::human_input::random_start_offset(target_x, target_y);
            let path = crate::browser::human_input::bezier_mouse_path(start_x, start_y, target_x, target_y);
            let _ = driver.dispatch_mouse_moves(&path).await;
            tokio::time::sleep(Duration::from_millis(crate::browser::human_input::pre_click_delay_ms())).await;
        }

        driver.click_by_node_id(backend_node_id).await?;

        let label = format!("ref={}", ref_str);
        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "ref": ref_str, "backendNodeId": backend_node_id })
            );
        } else {
            println!("{} Clicked: {} (nodeId={})", "✓".green(), label, backend_node_id);
        }
        return Ok(());
    }

    let selector = selector.unwrap();

    if cli.extension {
        if cli.camofox {
            extension_send(
                cli,
                "Camoufox.click",
                serde_json::json!({ "selector": selector }),
            )
            .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "selector": selector })
                );
            } else {
                println!(
                    "{} Clicked: {} (extension + camoufox)",
                    "✓".green(),
                    selector
                );
            }
            return Ok(());
        }

        // CDP Extension mode
        let resolve_js = js_resolve_selector(selector);
        let click_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.scrollIntoView({{ block: 'center', behavior: 'instant' }});
                el.click();
                return {{ success: true }};
            }})()"#,
            resolve_js
        );

        if wait_ms > 0 {
            let poll_js = format!(
                r#"(async function() {{
                    var deadline = Date.now() + {};
                    while (Date.now() < deadline) {{
                        var el = {};
                        if (el) return true;
                        await new Promise(r => setTimeout(r, 100));
                    }}
                    return false;
                }})()"#,
                wait_ms, resolve_js
            );
            let found = extension_eval(cli, &poll_js).await?;
            if found.as_bool() != Some(true) {
                return Err(ActionbookError::Timeout(format!(
                    "Element not found within {}ms (extension mode): {}",
                    wait_ms, selector
                )));
            }
        }

        let result = extension_eval(cli, &click_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "Click failed (extension mode): {}",
                err
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector })
            );
        } else {
            println!("{} Clicked: {} (extension)", "✓".green(), selector);
        }
        return Ok(());
    }

    // Use BrowserDriver for multi-backend support (CDP or Camoufox)
    let mut driver = create_browser_driver(cli, config).await?;
    apply_resource_blocking(cli, &mut driver).await;

    // Wait is only supported for CDP backend
    if wait_ms > 0 {
        if let Some(mgr) = driver.as_cdp_mut() {
            mgr.wait_for_element(effective_profile_arg(cli, config), selector, wait_ms)
                .await?;
        }
    }

    if human {
        // Human-like click: resolve actual element coords, then bezier path
        let (target_x, target_y) = driver.get_element_center(selector).await.unwrap_or((400.0, 300.0));
        let (start_x, start_y) = crate::browser::human_input::random_start_offset(target_x, target_y);
        let path = crate::browser::human_input::bezier_mouse_path(start_x, start_y, target_x, target_y);
        let _ = driver.dispatch_mouse_moves(&path).await;
        tokio::time::sleep(Duration::from_millis(crate::browser::human_input::pre_click_delay_ms())).await;
    }

    driver.click(selector).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector,
                "backend": format!("{:?}", driver.backend())
            })
        );
    } else {
        let backend_label = if driver.is_camofox() { " (camoufox)" } else { "" };
        println!("{} Clicked: {}{}", "✓".green(), selector, backend_label);
    }

    Ok(())
}

pub(crate) async fn type_text(
    cli: &Cli,
    config: &Config,
    selector: Option<&str>,
    text: &str,
    wait_ms: u64,
    ref_id: Option<&str>,
    human: bool,
) -> Result<()> {
    // Require either selector or --ref
    if selector.is_none() && ref_id.is_none() {
        return Err(ActionbookError::Other(
            "Either a CSS selector or --ref must be provided".to_string(),
        ));
    }

    // --ref mode: use snapshot ref to type by backendNodeId
    if let Some(ref_str) = ref_id {
        let mut driver = create_browser_driver(cli, config).await?;
        apply_resource_blocking(cli, &mut driver).await;
        let backend_node_id = resolve_snapshot_ref(&mut driver, ref_str).await?;

        if human {
            // Human-like typing with natural delays
            let delays = crate::browser::human_input::typing_delays(text, false);
            driver.focus_by_node_id(backend_node_id).await?;
            for (ch, delay_ms) in &delays {
                // For backspace, we'd need special handling; for now just type chars
                if *ch == '\u{0008}' {
                    // Dispatch Backspace key event via JS
                    if let Some(mgr) = driver.as_cdp_mut() {
                        mgr.press_key(None, "Backspace").await?;
                    }
                } else {
                    if let Some(mgr) = driver.as_cdp_mut() {
                        mgr.dispatch_key_char(None, *ch).await?;
                    }
                }
                tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
            }
        } else {
            driver.type_by_node_id(backend_node_id, text).await?;
        }

        let label = format!("ref={}", ref_str);
        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "ref": ref_str, "text": text })
            );
        } else {
            println!("{} Typed into: {}", "✓".green(), label);
        }
        return Ok(());
    }

    let selector = selector.unwrap();

    if cli.extension {
        if cli.camofox {
            extension_send(
                cli,
                "Camoufox.type",
                serde_json::json!({ "selector": selector, "text": text }),
            )
            .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "selector": selector, "text": text })
                );
            } else {
                println!(
                    "{} Typed into: {} (extension + camoufox)",
                    "✓".green(),
                    selector
                );
            }
            return Ok(());
        }

        // CDP Extension mode
        let resolve_js = js_resolve_selector(selector);
        let escaped_text = escape_js_string(text);

        if wait_ms > 0 {
            let poll_js = format!(
                r#"(async function() {{
                    var deadline = Date.now() + {};
                    while (Date.now() < deadline) {{
                        var el = {};
                        if (el) return true;
                        await new Promise(r => setTimeout(r, 100));
                    }}
                    return false;
                }})()"#,
                wait_ms, resolve_js
            );
            let found = extension_eval(cli, &poll_js).await?;
            if found.as_bool() != Some(true) {
                return Err(ActionbookError::Timeout(format!(
                    "Element not found within {}ms (extension mode): {}",
                    wait_ms, selector
                )));
            }
        }

        let type_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.focus();
                var text = '{}';
                for (var i = 0; i < text.length; i++) {{
                    el.dispatchEvent(new KeyboardEvent('keydown', {{ key: text[i], bubbles: true }}));
                    el.dispatchEvent(new KeyboardEvent('keypress', {{ key: text[i], bubbles: true }}));
                    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {{
                        el.value += text[i];
                    }} else if (el.isContentEditable) {{
                        el.textContent += text[i];
                    }}
                    el.dispatchEvent(new InputEvent('input', {{ data: text[i], inputType: 'insertText', bubbles: true }}));
                    el.dispatchEvent(new KeyboardEvent('keyup', {{ key: text[i], bubbles: true }}));
                }}
                return {{ success: true }};
            }})()"#,
            resolve_js, escaped_text
        );

        let result = extension_eval(cli, &type_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "Type failed (extension mode): {}",
                err
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector, "text": text })
            );
        } else {
            println!("{} Typed into: {} (extension)", "✓".green(), selector);
        }
        return Ok(());
    }

    // Use BrowserDriver for multi-backend support (CDP or Camoufox)
    let mut driver = create_browser_driver(cli, config).await?;
    apply_resource_blocking(cli, &mut driver).await;

    // Wait is only supported for CDP backend
    if wait_ms > 0 {
        if let Some(mgr) = driver.as_cdp_mut() {
            mgr.wait_for_element(effective_profile_arg(cli, config), selector, wait_ms)
                .await?;
        }
    }

    if human {
        // Human-like typing: focus first, then type char by char with delays
        driver.focus(selector).await?;
        let delays = crate::browser::human_input::typing_delays(text, false);
        for (ch, delay_ms) in &delays {
            if *ch == '\u{0008}' {
                if let Some(mgr) = driver.as_cdp_mut() {
                    mgr.press_key(None, "Backspace").await?;
                }
            } else {
                if let Some(mgr) = driver.as_cdp_mut() {
                    mgr.dispatch_key_char(None, *ch).await?;
                }
            }
            tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
        }
    } else {
        driver.type_text(selector, text).await?;
    }

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector,
                "text": text,
                "backend": format!("{:?}", driver.backend())
            })
        );
    } else {
        let backend_label = if driver.is_camofox() { " (camoufox)" } else { "" };
        println!("{} Typed into: {}{}", "✓".green(), selector, backend_label);
    }

    Ok(())
}

pub(crate) async fn fill(
    cli: &Cli,
    config: &Config,
    selector: Option<&str>,
    text: &str,
    wait_ms: u64,
    ref_id: Option<&str>,
) -> Result<()> {
    // Require either selector or --ref
    if selector.is_none() && ref_id.is_none() {
        return Err(ActionbookError::Other(
            "Either a CSS selector or --ref must be provided".to_string(),
        ));
    }

    // --ref mode: use snapshot ref to fill by backendNodeId
    if let Some(ref_str) = ref_id {
        let mut driver = create_browser_driver(cli, config).await?;
        apply_resource_blocking(cli, &mut driver).await;
        let backend_node_id = resolve_snapshot_ref(&mut driver, ref_str).await?;

        driver.fill_by_node_id(backend_node_id, text).await?;

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "ref": ref_str, "text": text })
            );
        } else {
            println!("{} Filled: ref={}", "✓".green(), ref_str);
        }
        return Ok(());
    }

    let selector = selector.unwrap();

    if cli.extension {
        let resolve_js = js_resolve_selector(selector);
        let escaped_text = escape_js_string(text);

        if wait_ms > 0 {
            let poll_js = format!(
                r#"(async function() {{
                    var deadline = Date.now() + {};
                    while (Date.now() < deadline) {{
                        var el = {};
                        if (el) return true;
                        await new Promise(r => setTimeout(r, 100));
                    }}
                    return false;
                }})()"#,
                wait_ms, resolve_js
            );
            let found = extension_eval(cli, &poll_js).await?;
            if found.as_bool() != Some(true) {
                return Err(ActionbookError::Timeout(format!(
                    "Element not found within {}ms (extension mode): {}",
                    wait_ms, selector
                )));
            }
        }

        let fill_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.focus();
                if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {{
                    var nativeSetter = Object.getOwnPropertyDescriptor(
                        window.HTMLInputElement.prototype, 'value'
                    ) || Object.getOwnPropertyDescriptor(
                        window.HTMLTextAreaElement.prototype, 'value'
                    );
                    if (nativeSetter && nativeSetter.set) {{
                        nativeSetter.set.call(el, '{}');
                    }} else {{
                        el.value = '{}';
                    }}
                }} else if (el.isContentEditable) {{
                    el.textContent = '{}';
                }}
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ success: true }};
            }})()"#,
            resolve_js, escaped_text, escaped_text, escaped_text
        );

        let result = extension_eval(cli, &fill_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "Fill failed (extension mode): {}",
                err
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector, "text": text })
            );
        } else {
            println!("{} Filled: {} (extension)", "✓".green(), selector);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);

    if wait_ms > 0 {
        session_manager
            .wait_for_element(effective_profile_arg(cli, config), selector, wait_ms)
            .await?;
    }

    session_manager
        .fill_on_page(effective_profile_arg(cli, config), selector, text)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector,
                "text": text
            })
        );
    } else {
        println!("{} Filled: {}", "✓".green(), selector);
    }

    Ok(())
}

pub(crate) async fn select(cli: &Cli, config: &Config, selector: &str, value: &str) -> Result<()> {
    if cli.extension {
        let resolve_js = js_resolve_selector(selector);
        let escaped_value = escape_js_string(value);
        let select_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'Element not found' }};
                if (el.tagName !== 'SELECT') return {{ success: false, error: 'Element is not a <select>' }};
                el.value = '{}';
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ success: true }};
            }})()"#,
            resolve_js, escaped_value
        );

        let result = extension_eval(cli, &select_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "Select failed (extension mode): {}",
                err
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector, "value": value })
            );
        } else {
            println!(
                "{} Selected '{}' in: {} (extension)",
                "✓".green(),
                value,
                selector
            );
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .select_on_page(effective_profile_arg(cli, config), selector, value)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector,
                "value": value
            })
        );
    } else {
        println!("{} Selected '{}' in: {}", "✓".green(), value, selector);
    }

    Ok(())
}

pub(crate) async fn hover(cli: &Cli, config: &Config, selector: &str) -> Result<()> {
    if cli.extension {
        let resolve_js = js_resolve_selector(selector);
        let hover_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.scrollIntoView({{ block: 'center', behavior: 'instant' }});
                el.dispatchEvent(new MouseEvent('mouseenter', {{ bubbles: true }}));
                el.dispatchEvent(new MouseEvent('mouseover', {{ bubbles: true }}));
                return {{ success: true }};
            }})()"#,
            resolve_js
        );

        let result = extension_eval(cli, &hover_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "Hover failed (extension mode): {}",
                err
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector })
            );
        } else {
            println!("{} Hovered: {} (extension)", "✓".green(), selector);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .hover_on_page(effective_profile_arg(cli, config), selector)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector
            })
        );
    } else {
        println!("{} Hovered: {}", "✓".green(), selector);
    }

    Ok(())
}

pub(crate) async fn focus(cli: &Cli, config: &Config, selector: &str) -> Result<()> {
    if cli.extension {
        let resolve_js = js_resolve_selector(selector);
        let focus_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'Element not found' }};
                el.focus();
                return {{ success: true }};
            }})()"#,
            resolve_js
        );

        let result = extension_eval(cli, &focus_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(ActionbookError::ExtensionError(format!(
                "Focus failed (extension mode): {}",
                err
            )));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "selector": selector })
            );
        } else {
            println!("{} Focused: {} (extension)", "✓".green(), selector);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .focus_on_page(effective_profile_arg(cli, config), selector)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": selector
            })
        );
    } else {
        println!("{} Focused: {}", "✓".green(), selector);
    }

    Ok(())
}

pub(crate) async fn press(cli: &Cli, config: &Config, key: &str) -> Result<()> {
    if cli.extension {
        let escaped_key = escape_js_string(key);
        let press_js = format!(
            r#"(function() {{
                var key = '{}';
                var el = document.activeElement || document.body;
                var opts = {{ key: key, code: 'Key' + key, bubbles: true, cancelable: true }};
                // Map common key names
                var keyMap = {{
                    'Enter': {{ key: 'Enter', code: 'Enter' }},
                    'Tab': {{ key: 'Tab', code: 'Tab' }},
                    'Escape': {{ key: 'Escape', code: 'Escape' }},
                    'Backspace': {{ key: 'Backspace', code: 'Backspace' }},
                    'Delete': {{ key: 'Delete', code: 'Delete' }},
                    'ArrowUp': {{ key: 'ArrowUp', code: 'ArrowUp' }},
                    'ArrowDown': {{ key: 'ArrowDown', code: 'ArrowDown' }},
                    'ArrowLeft': {{ key: 'ArrowLeft', code: 'ArrowLeft' }},
                    'ArrowRight': {{ key: 'ArrowRight', code: 'ArrowRight' }},
                    'Space': {{ key: ' ', code: 'Space' }},
                    'Home': {{ key: 'Home', code: 'Home' }},
                    'End': {{ key: 'End', code: 'End' }},
                    'PageUp': {{ key: 'PageUp', code: 'PageUp' }},
                    'PageDown': {{ key: 'PageDown', code: 'PageDown' }},
                }};
                if (keyMap[key]) {{
                    opts.key = keyMap[key].key;
                    opts.code = keyMap[key].code;
                }}
                el.dispatchEvent(new KeyboardEvent('keydown', opts));
                el.dispatchEvent(new KeyboardEvent('keypress', opts));
                el.dispatchEvent(new KeyboardEvent('keyup', opts));
                return {{ success: true }};
            }})()"#,
            escaped_key
        );

        let result = extension_eval(cli, &press_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            return Err(ActionbookError::ExtensionError(
                "Press failed (extension mode)".to_string(),
            ));
        }

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "success": true, "key": key })
            );
        } else {
            println!("{} Pressed: {} (extension)", "✓".green(), key);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .press_key(effective_profile_arg(cli, config), key)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "key": key
            })
        );
    } else {
        println!("{} Pressed: {}", "✓".green(), key);
    }

    Ok(())
}

pub(crate) async fn hotkey(cli: &Cli, config: &Config, keys: &str) -> Result<()> {
    // Parse keys string (e.g., "Control+A" or "Control+Shift+C")
    let key_parts: Vec<&str> = keys.split('+').map(|s| s.trim()).collect();

    if key_parts.is_empty() {
        return Err(ActionbookError::Other("Empty key sequence".to_string()));
    }

    // Extension mode not supported for hotkeys (requires complex modifier state)
    if cli.extension {
        return Err(ActionbookError::Other(
            "Hotkey not supported in extension mode, use CDP mode".to_string()
        ));
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .send_hotkey(effective_profile_arg(cli, config), &key_parts)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "keys": keys
            })
        );
    } else {
        println!("{} Hotkey pressed: {}", "✓".green(), keys);
    }

    Ok(())
}

pub(crate) async fn screenshot(cli: &Cli, config: &Config, path: &str, full_page: bool) -> Result<()> {
    if cli.extension {
        if cli.camofox {
            // Route through Extension Bridge with Camoufox backend
            let result = extension_send(cli, "Camoufox.screenshot", serde_json::json!({})).await?;
            let b64_data = result
                .get("data")
                .and_then(|d| d.as_str())
                .ok_or_else(|| {
                    ActionbookError::ExtensionError(
                        "Screenshot response missing 'data' field (extension + camoufox mode)"
                            .to_string(),
                    )
                })?;

            let screenshot_data = base64::engine::general_purpose::STANDARD
                .decode(b64_data)
                .map_err(|e| {
                    ActionbookError::ExtensionError(format!(
                        "Failed to decode screenshot base64 (extension + camoufox mode): {}",
                        e
                    ))
                })?;

            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, screenshot_data)?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "path": path })
                );
            } else {
                println!(
                    "{} Screenshot saved: {} (extension + camoufox)",
                    "✓".green(),
                    path
                );
            }
            return Ok(());
        }

        // CDP Extension mode
        let mut params = serde_json::json!({ "format": "png" });
        if full_page {
            params["captureBeyondViewport"] = serde_json::json!(true);
        }

        let result = extension_send(cli, "Page.captureScreenshot", params).await?;
        let b64_data = result
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| {
                ActionbookError::ExtensionError(
                    "Screenshot response missing 'data' field (extension mode)".to_string(),
                )
            })?;

        let screenshot_data = base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to decode screenshot base64 (extension mode): {}",
                    e
                ))
            })?;

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
            println!(
                "{} Screenshot saved{}: {} (extension)",
                "✓".green(),
                mode,
                path
            );
        }
        return Ok(());
    }

    // Use BrowserDriver for multi-backend support (CDP or Camoufox)
    let mut driver = create_browser_driver(cli, config).await?;

    // Full page is CDP-only feature
    if full_page && driver.is_camofox() {
        eprintln!(
            "{} --full-page is not supported in Camoufox backend, using viewport screenshot",
            "!".yellow()
        );
    }

    let screenshot_data = if full_page && driver.is_cdp() {
        driver
            .as_cdp_mut()
            .unwrap()
            .screenshot_full_page(effective_profile_arg(cli, config))
            .await?
    } else {
        driver.screenshot().await?
    };

    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, screenshot_data)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "path": path,
                "fullPage": full_page && driver.is_cdp(),
                "backend": format!("{:?}", driver.backend())
            })
        );
    } else {
        let mode = if full_page && driver.is_cdp() {
            " (full page)"
        } else {
            ""
        };
        let backend_label = if driver.is_camofox() { " (camoufox)" } else { "" };
        println!(
            "{} Screenshot saved{}: {}{}",
            "✓".green(),
            mode,
            path,
            backend_label
        );
    }

    Ok(())
}

pub(crate) async fn pdf(cli: &Cli, config: &Config, path: &str) -> Result<()> {
    if cli.extension {
        let result = extension_send(cli, "Page.printToPDF", serde_json::json!({})).await?;
        let b64_data = result
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| {
                ActionbookError::ExtensionError(
                    "PDF response missing 'data' field (extension mode)".to_string(),
                )
            })?;

        let pdf_data = base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| {
                ActionbookError::ExtensionError(format!(
                    "Failed to decode PDF base64 (extension mode): {}",
                    e
                ))
            })?;

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
            println!("{} PDF saved: {} (extension)", "✓".green(), path);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let pdf_data = session_manager
        .pdf_page(effective_profile_arg(cli, config))
        .await?;

    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, pdf_data)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "path": path
            })
        );
    } else {
        println!("{} PDF saved: {}", "✓".green(), path);
    }

    Ok(())
}

pub(crate) async fn eval(cli: &Cli, config: &Config, code: &str) -> Result<()> {
    let value = if cli.extension {
        let result = extension_send(
            cli,
            "Runtime.evaluate",
            serde_json::json!({
                "expression": code,
                "returnByValue": true,
            }),
        )
        .await?;

        // Extract the value from CDP response
        result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or_else(|| {
                result
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null)
            })
    } else {
        let session_manager = create_session_manager(cli, config);
        session_manager
            .eval_on_page(effective_profile_arg(cli, config), code)
            .await?
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }

    Ok(())
}

pub(crate) async fn html(cli: &Cli, config: &Config, selector: Option<&str>) -> Result<()> {
    if cli.extension {
        if cli.camofox {
            // Route through Extension Bridge with Camoufox backend
            // Camoufox returns accessibility tree instead of HTML
            let result = extension_send(cli, "Camoufox.html", serde_json::json!({})).await?;

            if cli.json {
                println!("{}", serde_json::to_string(&result)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            return Ok(());
        }

        // CDP Extension mode
        let js = match selector {
            Some(sel) => {
                let resolve_js = js_resolve_selector(sel);
                format!(
                    r#"(function() {{
                        var el = {};
                        return el ? el.outerHTML : null;
                    }})()"#,
                    resolve_js
                )
            }
            None => "document.documentElement.outerHTML".to_string(),
        };

        let value = extension_eval(cli, &js).await?;
        let html = value.as_str().unwrap_or("").to_string();

        if selector.is_some() && html.is_empty() {
            return Err(ActionbookError::ExtensionError(format!(
                "Element not found (extension mode): {}",
                selector.unwrap_or("")
            )));
        }

        if cli.json {
            println!("{}", serde_json::json!({ "html": html }));
        } else {
            println!("{}", html);
        }
        return Ok(());
    }

    // Use BrowserDriver for multi-backend support (CDP or Camoufox)
    let mut driver = create_browser_driver(cli, config).await?;

    // Selector parameter is CDP-only feature
    if selector.is_some() && driver.is_camofox() {
        return Err(ActionbookError::BrowserOperation(
            "Selector filtering not supported in Camoufox backend. Use `actionbook browser html` without selector to get accessibility tree.".to_string()
        ));
    }

    let content = if driver.is_cdp() {
        driver
            .as_cdp_mut()
            .unwrap()
            .get_html(effective_profile_arg(cli, config), selector)
            .await?
    } else {
        driver.get_content().await?
    };

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "content": content,
                "backend": format!("{:?}", driver.backend()),
                "format": if driver.is_camofox() { "accessibility_tree" } else { "html" }
            })
        );
    } else {
        println!("{}", content);
    }

    Ok(())
}

pub(crate) async fn text(cli: &Cli, config: &Config, selector: Option<&str>, mode: &str) -> Result<()> {
    if cli.extension {
        // Extension mode: always uses JS-based extraction
        let js = match selector {
            Some(sel) => {
                let resolve_js = js_resolve_selector(sel);
                format!(
                    r#"(function() {{
                        var el = {};
                        return el ? el.innerText : null;
                    }})()"#,
                    resolve_js
                )
            }
            None => {
                if mode == "readability" {
                    // Use readability extraction in extension mode
                    crate::browser::readability::READABILITY_JS.to_string()
                } else {
                    "document.body.innerText".to_string()
                }
            }
        };

        let value = extension_eval(cli, &js).await?;
        let text = value.as_str().unwrap_or("").to_string();

        if selector.is_some() && value.is_null() {
            return Err(ActionbookError::ExtensionError(format!(
                "Element not found (extension mode): {}",
                selector.unwrap_or("")
            )));
        }

        if cli.json {
            println!("{}", serde_json::json!({ "text": text, "mode": mode }));
        } else {
            println!("{}", text);
        }
        return Ok(());
    }

    // If a selector is provided, use the old get_text method (raw innerText of element)
    if selector.is_some() {
        let session_manager = create_session_manager(cli, config);
        let text = session_manager
            .get_text(effective_profile_arg(cli, config), selector)
            .await?;

        if cli.json {
            println!("{}", serde_json::json!({ "text": text, "mode": "raw" }));
        } else {
            println!("{}", text);
        }
        return Ok(());
    }

    // Use BrowserDriver with readability/raw mode
    let mut driver = create_browser_driver(cli, config).await?;
    apply_resource_blocking(cli, &mut driver).await;

    let extraction_mode = match mode {
        "raw" => crate::browser::TextExtractionMode::Raw,
        _ => crate::browser::TextExtractionMode::Readability,
    };

    let text = driver.get_readable_text(extraction_mode).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "text": text, "mode": mode }));
    } else {
        println!("{}", text);
    }

    Ok(())
}

/// Get the path for persisting the last snapshot (for --diff across CLI invocations)
fn snapshot_cache_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".actionbook").join("last_snapshot.json"))
}

/// Load the last snapshot from disk
fn load_last_snapshot() -> Option<Vec<crate::browser::snapshot::A11yNode>> {
    let path = snapshot_cache_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Save the current snapshot to disk
fn save_last_snapshot(nodes: &[crate::browser::snapshot::A11yNode]) {
    if let Some(path) = snapshot_cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = serde_json::to_string(nodes)
            .map(|json| std::fs::write(&path, json));
    }
}

pub(crate) async fn snapshot(
    cli: &Cli,
    config: &Config,
    interactive: bool,
    cursor: bool,
    compact: bool,
    format: &str,
    depth: Option<usize>,
    selector: Option<&str>,
    diff: bool,
    max_tokens: Option<usize>,
) -> Result<()> {
    use crate::browser::snapshot::{
        self, SnapshotFilter, SnapshotFormat,
    };

    // Parse filter from boolean flag
    let snap_filter = if interactive {
        SnapshotFilter::Interactive
    } else {
        SnapshotFilter::All
    };

    // Parse format
    let snap_format = match format {
        "compact" => SnapshotFormat::Compact,
        "json" => SnapshotFormat::Json,
        f => return Err(ActionbookError::Other(format!("Unknown format: '{}'. Use 'compact' or 'json'.", f))),
    };

    // Get the AX tree and optional driver (needed for cursor mode in non-extension path)
    let (raw, mut driver_opt) = if cli.extension {
        // Extension mode: use CDP Accessibility.getFullAXTree through the bridge
        let _ = extension_send(cli, "DOM.enable", serde_json::json!({})).await;
        let _ = extension_send(cli, "Accessibility.enable", serde_json::json!({})).await;
        let raw = extension_send(cli, "Accessibility.getFullAXTree", serde_json::json!({})).await?;
        (raw, None)
    } else {
        let mut driver = create_browser_driver(cli, config).await?;
        apply_resource_blocking(cli, &mut driver).await;
        let raw = driver.get_accessibility_tree_raw().await?;
        (raw, Some(driver))
    };

    // If scoping by CSS selector, resolve to backendNodeId first
    let scope_backend_id = if let Some(sel) = selector {
        if let Some(ref mut driver) = driver_opt {
            driver.get_backend_node_id(sel).await?
        } else {
            None // selector scoping not supported in extension mode
        }
    } else {
        None
    };

    let (mut nodes, cache) = snapshot::parse_ax_tree(raw, snap_filter, depth, scope_backend_id)?;

    // Always remove empty leaf structural nodes (empty <div>/<span> wrappers)
    nodes = snapshot::remove_empty_leaves(&nodes);

    // Apply compact tree filtering (-c): keep only ref'd/valued nodes + ancestors
    if compact {
        nodes = snapshot::compact_tree_nodes(&nodes);
    }

    // Append cursor-interactive elements (-C) — requires a browser driver (non-extension only)
    if cursor {
        let driver = driver_opt.as_mut().ok_or_else(|| {
            ActionbookError::Other("Cursor mode (-C) is not supported in extension mode".to_string())
        })?;
        let cursor_nodes = snapshot::find_cursor_interactive_elements(driver, selector).await?;
        if !cursor_nodes.is_empty() {
            // Continue ref numbering from where AX tree left off
            let next_ref = cache.next_ref;
            for (i, cn) in cursor_nodes.into_iter().enumerate() {
                nodes.push(snapshot::A11yNode {
                    ref_id: Some(format!("e{}", next_ref + i)),
                    role: if cn.has_cursor_pointer || cn.has_onclick {
                        "clickable".to_string()
                    } else {
                        "focusable".to_string()
                    },
                    name: cn.text,
                    value: None,
                    depth: 0,
                    disabled: false,
                    focused: false,
                    level: None,
                    checked: None,
                    expanded: None,
                    selected: false,
                    required: false,
                    url: None,
                    backend_node_id: -1,
                });
            }
        }
    }

    // Handle --diff mode
    if diff {
        let prev_nodes = load_last_snapshot();
        save_last_snapshot(&nodes);

        match prev_nodes {
            None => {
                // First snapshot, no diff available
                if cli.json {
                    println!("{}", serde_json::json!({
                        "message": "First snapshot captured. Run again with --diff to see changes.",
                        "nodeCount": nodes.len()
                    }));
                } else {
                    println!("{} First snapshot captured ({} nodes). Run again with --diff to see changes.",
                        "i".blue(), nodes.len());
                }
            }
            Some(prev) => {
                let (added, changed, removed) = snapshot::diff_snapshots(&prev, &nodes);

                if cli.json {
                    println!("{}", serde_json::json!({
                        "added": added.len(),
                        "changed": changed.len(),
                        "removed": removed.len(),
                        "addedNodes": format_nodes_for_json(&added),
                        "changedNodes": format_nodes_for_json(&changed),
                        "removedNodes": format_nodes_for_json(&removed),
                    }));
                } else {
                    if added.is_empty() && changed.is_empty() && removed.is_empty() {
                        println!("{} No changes detected", "=".blue());
                    } else {
                        if !added.is_empty() {
                            println!("{} Added ({}):", "+".green(), added.len());
                            print!("{}", snapshot::format_compact(&added));
                        }
                        if !changed.is_empty() {
                            println!("{} Changed ({}):", "~".yellow(), changed.len());
                            print!("{}", snapshot::format_compact(&changed));
                        }
                        if !removed.is_empty() {
                            println!("{} Removed ({}):", "-".red(), removed.len());
                            print!("{}", snapshot::format_compact(&removed));
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    // Store for future --diff
    save_last_snapshot(&nodes);

    // Apply token truncation if requested
    let (nodes, truncated) = if let Some(max_tok) = max_tokens {
        snapshot::truncate_to_tokens(&nodes, max_tok, snap_format)
    } else {
        (nodes, false)
    };

    // Output
    if cli.json || snap_format == SnapshotFormat::Json {
        let mut json_val = serde_json::to_value(&nodes)?;
        if truncated {
            if let Some(obj) = json_val.as_object_mut() {
                // Wrap in an object with metadata
                let wrapped = serde_json::json!({
                    "nodes": obj.clone(),
                    "truncated": true,
                    "maxTokens": max_tokens.unwrap_or(0),
                });
                println!("{}", serde_json::to_string_pretty(&wrapped)?);
            } else {
                let wrapped = serde_json::json!({
                    "nodes": json_val,
                    "truncated": true,
                    "maxTokens": max_tokens.unwrap_or(0),
                });
                println!("{}", serde_json::to_string_pretty(&wrapped)?);
            }
        } else {
            println!("{}", serde_json::to_string_pretty(&nodes)?);
        }
    } else {
        let output = match snap_format {
            SnapshotFormat::Compact => snapshot::format_compact(&nodes),
            SnapshotFormat::Json => serde_json::to_string_pretty(&nodes)?,
        };
        let tokens = snapshot::estimate_tokens(&output, snap_format);
        print!("{}", output);
        if truncated {
            println!("(truncated to ~{} tokens)", max_tokens.unwrap_or(0));
        }
        if cli.verbose {
            eprintln!("--- {} nodes, ~{} tokens ---", nodes.len(), tokens);
        }
    }

    Ok(())
}

/// Format A11yNode list as JSON-friendly values
fn format_nodes_for_json(nodes: &[crate::browser::snapshot::A11yNode]) -> Vec<serde_json::Value> {
    nodes
        .iter()
        .map(|n| {
            let mut obj = serde_json::json!({
                "ref": n.ref_id.as_deref().unwrap_or(""),
                "role": n.role,
                "name": n.name,
            });
            if let Some(ref v) = n.value {
                obj["value"] = serde_json::json!(v);
            }
            if n.focused {
                obj["focused"] = serde_json::json!(true);
            }
            if n.disabled {
                obj["disabled"] = serde_json::json!(true);
            }
            obj
        })
        .collect()
}


pub(crate) async fn inspect(cli: &Cli, config: &Config, x: f64, y: f64, desc: Option<&str>) -> Result<()> {
    if cli.extension {
        // In extension mode, use JS elementFromPoint + gather info
        let inspect_js = format!(
            r#"(function() {{
                var vw = window.innerWidth, vh = window.innerHeight;
                var x = {}, y = {};
                if (x < 0 || x > vw || y < 0 || y > vh) {{
                    return {{ outOfBounds: true, viewport: {{ width: vw, height: vh }} }};
                }}
                var el = document.elementFromPoint(x, y);
                if (!el) return {{ found: false, viewport: {{ width: vw, height: vh }} }};
                var rect = el.getBoundingClientRect();
                var attrs = {{}};
                for (var i = 0; i < el.attributes.length && i < 20; i++) {{
                    attrs[el.attributes[i].name] = el.attributes[i].value.substring(0, 100);
                }}
                var parents = [];
                var p = el.parentElement;
                for (var i = 0; i < 5 && p && p !== document.body; i++) {{
                    parents.push({{ tagName: p.tagName.toLowerCase(), id: p.id || '', className: (p.className || '').substring(0, 60) }});
                    p = p.parentElement;
                }}
                var interactive = ['A','BUTTON','INPUT','SELECT','TEXTAREA'].indexOf(el.tagName) >= 0
                    || el.getAttribute('role') === 'button'
                    || el.getAttribute('tabindex') !== null;
                var selectors = [];
                if (el.id) selectors.push('#' + el.id);
                if (el.className && typeof el.className === 'string') {{
                    var cls = el.className.trim().split(/\\s+/).slice(0,2).join('.');
                    if (cls) selectors.push(el.tagName.toLowerCase() + '.' + cls);
                }}
                selectors.push(el.tagName.toLowerCase());
                return {{
                    found: true,
                    viewport: {{ width: vw, height: vh }},
                    tagName: el.tagName.toLowerCase(),
                    id: el.id || '',
                    className: (el.className || '').substring(0, 100),
                    textContent: (el.textContent || '').trim().substring(0, 200),
                    isInteractive: interactive,
                    boundingBox: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }},
                    attributes: attrs,
                    suggestedSelectors: selectors,
                    parents: parents
                }};
            }})()"#,
            x, y
        );

        let result = extension_eval(cli, &inspect_js).await?;

        if result.get("outOfBounds").and_then(|v| v.as_bool()) == Some(true) {
            let vp = result.get("viewport").unwrap_or(&serde_json::Value::Null);
            let vw = vp.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let vh = vp.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
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
                    "{} Coordinates ({}, {}) are outside viewport bounds ({}x{}) (extension)",
                    "!".yellow(), x, y, vw as i32, vh as i32
                );
            }
            return Ok(());
        }

        if cli.json {
            let mut output = serde_json::json!({
                "success": true,
                "coordinates": { "x": x, "y": y },
                "inspection": result
            });
            if let Some(d) = desc {
                output["description"] = serde_json::json!(d);
            }
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            let found = result.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
            if !found {
                println!("{} No element found at ({}, {}) (extension)", "!".yellow(), x, y);
                return Ok(());
            }
            if let Some(d) = desc {
                println!("{} Inspecting: {} (extension)\n", "?".cyan(), d.bold());
            }
            let tag = result.get("tagName").and_then(|v| v.as_str()).unwrap_or("unknown");
            let id = result.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
            let class = result.get("className").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
            print!("{}", "Element: ".bold());
            print!("<{}", tag.cyan());
            if let Some(i) = id { print!(" id=\"{}\"", i.green()); }
            if let Some(c) = class { print!(" class=\"{}\"", c.yellow()); }
            println!(">");
            if let Some(text) = result.get("textContent").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                println!("{}", "Text:".bold());
                println!("  {}", text.dimmed());
            }
            if let Some(selectors) = result.get("suggestedSelectors").and_then(|v| v.as_array()) {
                if !selectors.is_empty() {
                    println!("{}", "Suggested Selectors:".bold());
                    for sel in selectors {
                        if let Some(s) = sel.as_str() {
                            println!("  {} {}", "->".cyan(), s);
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);

    // Get viewport to validate coordinates
    let (vp_width, vp_height) = session_manager
        .get_viewport(effective_profile_arg(cli, config))
        .await?;

    if x < 0.0 || x > vp_width || y < 0.0 || y > vp_height {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": false,
                    "message": format!("Coordinates ({}, {}) are outside viewport bounds ({}x{})", x, y, vp_width, vp_height)
                })
            );
        } else {
            println!(
                "{} Coordinates ({}, {}) are outside viewport bounds ({}x{})",
                "!".yellow(),
                x,
                y,
                vp_width,
                vp_height
            );
        }
        return Ok(());
    }

    let result = session_manager
        .inspect_at(effective_profile_arg(cli, config), x, y)
        .await?;

    if cli.json {
        let mut output = serde_json::json!({
            "success": true,
            "coordinates": { "x": x, "y": y },
            "viewport": { "width": vp_width, "height": vp_height },
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
            .unwrap_or(false);

        if !found {
            println!("{} No element found at ({}, {})", "!".yellow(), x, y);
            return Ok(());
        }

        if let Some(d) = desc {
            println!("{} Inspecting: {}\n", "🔍".cyan(), d.bold());
        }

        println!(
            "{} ({}, {}) in {}x{} viewport\n",
            "📍".cyan(),
            x,
            y,
            vp_width,
            vp_height
        );

        // Tag and basic info
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

        // Interactive status
        let interactive = result
            .get("isInteractive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if interactive {
            println!("{} Interactive element", "✓".green());
        }

        // Bounding box
        if let Some(bbox) = result.get("boundingBox") {
            let bx = bbox.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let by = bbox.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let bw = bbox.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let bh = bbox.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
            println!(
                "{} x={:.0}, y={:.0}, {}x{}",
                "📐".dimmed(),
                bx,
                by,
                bw as i32,
                bh as i32
            );
        }

        // Text content
        if let Some(text) = result
            .get("textContent")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            println!("\n{}", "Text:".bold());
            println!("  {}", text.dimmed());
        }

        // Suggested selectors
        if let Some(selectors) = result.get("suggestedSelectors").and_then(|v| v.as_array()) {
            if !selectors.is_empty() {
                println!("\n{}", "Suggested Selectors:".bold());
                for sel in selectors {
                    if let Some(s) = sel.as_str() {
                        println!("  {} {}", "→".cyan(), s);
                    }
                }
            }
        }

        // Attributes
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

        // Parent hierarchy
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
                    print!("{}↑ <{}", indent, ptag);
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

pub(crate) async fn viewport(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        let value = extension_eval(
            cli,
            "JSON.stringify({width: window.innerWidth, height: window.innerHeight})",
        )
        .await?;

        let dims: serde_json::Value = match value.as_str() {
            Some(s) => serde_json::from_str(s).unwrap_or(serde_json::Value::Null),
            None => value,
        };
        let width = dims
            .get("width")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let height = dims
            .get("height")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "width": width, "height": height })
            );
        } else {
            println!(
                "{} {}x{} (extension)",
                "Viewport:".bold(),
                width as i32,
                height as i32
            );
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let (width, height) = session_manager
        .get_viewport(effective_profile_arg(cli, config))
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "width": width,
                "height": height
            })
        );
    } else {
        println!("{} {}x{}", "Viewport:".bold(), width as i32, height as i32);
    }

    Ok(())
}

pub(crate) async fn cookies(cli: &Cli, config: &Config, command: &Option<CookiesCommands>) -> Result<()> {
    if cli.extension {
        return cookies_extension(cli, command).await;
    }

    let session_manager = create_session_manager(cli, config);

    match command {
        None | Some(CookiesCommands::List) => {
            let cookies = session_manager
                .get_cookies(effective_profile_arg(cli, config))
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&cookies)?);
            } else {
                if cookies.is_empty() {
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
        }
        Some(CookiesCommands::Get { name }) => {
            let cookies = session_manager
                .get_cookies(effective_profile_arg(cli, config))
                .await?;
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
            session_manager
                .set_cookie(
                    effective_profile_arg(cli, config),
                    name,
                    value,
                    domain.as_deref(),
                )
                .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "name": name,
                        "value": value
                    })
                );
            } else {
                println!("{} Cookie set: {} = {}", "✓".green(), name, value);
            }
        }
        Some(CookiesCommands::Delete { name }) => {
            session_manager
                .delete_cookie(effective_profile_arg(cli, config), name)
                .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "name": name
                    })
                );
            } else {
                println!("{} Cookie deleted: {}", "✓".green(), name);
            }
        }
        Some(CookiesCommands::Clear { domain, dry_run, .. }) => {
            if domain.is_some() || *dry_run {
                return Err(ActionbookError::Other(
                    "--domain and --dry-run are only supported in extension mode (--extension). \
                     In CDP mode, 'cookies clear' clears all cookies for the session.".to_string()
                ));
            }

            session_manager
                .clear_cookies(effective_profile_arg(cli, config))
                .await?;

            if cli.json {
                println!("{}", serde_json::json!({ "success": true }));
            } else {
                println!("{} All cookies cleared", "✓".green());
            }
        }
    }

    Ok(())
}

async fn cookies_extension(cli: &Cli, command: &Option<CookiesCommands>) -> Result<()> {
    // Get current page URL for cookie operations.
    // chrome.cookies API requires a valid http(s) URL to scope all operations —
    // we never allow cross-domain wildcard reads/writes.
    let current_url = extension_eval(cli, "window.location.href")
        .await
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .unwrap_or_default();

    /// Build a URL for cookie operations: explicit domain takes priority, fall back to current_url.
    fn resolve_cookie_url(current_url: &str, domain: Option<&str>) -> std::result::Result<String, ActionbookError> {
        // Domain first: user explicitly asked for this domain
        if let Some(d) = domain {
            let clean = d.trim_start_matches('.');
            return Ok(format!("https://{}/", clean));
        }
        // Fallback to current page URL
        if !current_url.is_empty() {
            return Ok(current_url.to_string());
        }
        Err(ActionbookError::ExtensionError(
            "Cannot perform cookie operation: no valid page URL (navigate to an http(s) page first)".to_string(),
        ))
    }

    match command {
        None | Some(CookiesCommands::List) => {
            let url = resolve_cookie_url(&current_url, None)?;
            let result = extension_send(
                cli,
                "Extension.getCookies",
                serde_json::json!({ "url": url }),
            )
            .await?;
            let cookies = result
                .get("cookies")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&cookies)?);
            } else if cookies.is_empty() {
                println!("{} No cookies (extension)", "!".yellow());
            } else {
                println!(
                    "{} {} cookies (extension)\n",
                    "✓".green(),
                    cookies.len()
                );
                for cookie in &cookies {
                    let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let value = cookie.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    let domain = cookie
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
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
            let url = resolve_cookie_url(&current_url, None)?;
            let result = extension_send(
                cli,
                "Extension.getCookies",
                serde_json::json!({ "url": url }),
            )
            .await?;
            let cookies = result
                .get("cookies")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
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
                    None => println!("{} Cookie not found: {} (extension)", "!".yellow(), name),
                }
            }
        }
        Some(CookiesCommands::Set {
            name,
            value,
            domain,
        }) => {
            let url = resolve_cookie_url(&current_url, domain.as_deref())?;
            let mut params = serde_json::json!({
                "name": name,
                "value": value,
                "url": url,
            });
            if let Some(d) = domain {
                params["domain"] = serde_json::json!(d);
            }

            extension_send(cli, "Extension.setCookie", params).await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "name": name, "value": value })
                );
            } else {
                println!(
                    "{} Cookie set: {} = {} (extension)",
                    "✓".green(),
                    name,
                    value
                );
            }
        }
        Some(CookiesCommands::Delete { name }) => {
            let url = resolve_cookie_url(&current_url, None)?;
            let params = serde_json::json!({
                "name": name,
                "url": url,
            });

            extension_send(cli, "Extension.removeCookie", params).await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "name": name })
                );
            } else {
                println!(
                    "{} Cookie deleted: {} (extension)",
                    "✓".green(),
                    name
                );
            }
        }
        Some(CookiesCommands::Clear { domain, dry_run, yes }) => {
            let url = resolve_cookie_url(&current_url, domain.as_deref())?;

            // Fetch cookies to preview count.
            // When --domain is specified, pass it so the extension can use
            // chrome.cookies.getAll({ domain }) which returns cookies for ALL
            // paths, not just the root path that { url } would match.
            let mut get_params = serde_json::json!({ "url": url });
            if let Some(d) = domain.as_deref() {
                get_params["domain"] = serde_json::json!(d.trim_start_matches('.'));
            }
            let preview = extension_send(
                cli,
                "Extension.getCookies",
                get_params,
            )
            .await?;
            let cookies = preview
                .get("cookies")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();

            let target_domain = domain.as_deref().unwrap_or_else(|| {
                url.split("://")
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("unknown")
            });

            if *dry_run {
                // Preview mode: show cookies without deleting
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "dry_run": true,
                            "domain": target_domain,
                            "count": cookies.len(),
                            "cookies": cookies.iter().map(|c| {
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
                        cookies.len(),
                        target_domain
                    );
                    for cookie in &cookies {
                        let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let cdomain = cookie.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                        println!("  {} {}", name.bold(), format!("({})", cdomain).dimmed());
                    }
                }
                return Ok(());
            }

            // Require --yes to actually clear (both interactive and JSON modes)
            if !yes {
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "error": "confirmation_required",
                            "message": "Pass --yes to confirm clearing cookies",
                            "count": cookies.len(),
                            "domain": target_domain
                        })
                    );
                } else {
                    println!(
                        "{} About to clear {} cookies for {}",
                        "!".yellow(),
                        cookies.len(),
                        target_domain
                    );
                    println!(
                        "  Re-run with {} to confirm, or use {} to preview details",
                        "--yes".bold(),
                        "--dry-run".bold()
                    );
                }
                return Ok(());
            }

            let mut clear_params = serde_json::json!({ "url": url });
            if let Some(d) = domain.as_deref() {
                clear_params["domain"] = serde_json::json!(d.trim_start_matches('.'));
            }
            extension_send(
                cli,
                "Extension.clearCookies",
                clear_params,
            )
            .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "success": true, "cleared": cookies.len() })
                );
            } else {
                println!(
                    "{} Cleared {} cookies for {} (extension)",
                    "✓".green(),
                    cookies.len(),
                    target_domain
                );
            }
        }
    }
    Ok(())
}

pub(crate) async fn scroll(
    cli: &Cli,
    config: &Config,
    direction: &crate::cli::ScrollDirection,
    smooth: bool,
    wait: bool,
) -> Result<()> {
    use crate::cli::ScrollDirection;

    let behavior = if smooth { "smooth" } else { "instant" };

    let js_core = match direction {
        ScrollDirection::Down { pixels } => {
            if *pixels == 0 {
                format!(
                    "window.scrollBy({{ top: window.innerHeight, behavior: '{}' }})",
                    behavior
                )
            } else {
                format!(
                    "window.scrollBy({{ top: {}, behavior: '{}' }})",
                    pixels, behavior
                )
            }
        }

        ScrollDirection::Up { pixels } => {
            if *pixels == 0 {
                format!(
                    "window.scrollBy({{ top: -window.innerHeight, behavior: '{}' }})",
                    behavior
                )
            } else {
                format!(
                    "window.scrollBy({{ top: -{}, behavior: '{}' }})",
                    pixels, behavior
                )
            }
        }

        ScrollDirection::Bottom => {
            format!(
                "window.scrollTo({{ top: document.body.scrollHeight, behavior: '{}' }})",
                behavior
            )
        }

        ScrollDirection::Top => {
            format!("window.scrollTo({{ top: 0, behavior: '{}' }})", behavior)
        }

        ScrollDirection::To { selector, align } => {
            // Validate align value
            let valid_aligns = ["start", "center", "end", "nearest"];
            if !valid_aligns.contains(&align.as_str()) {
                return Err(ActionbookError::Other(format!(
                    "Invalid align value '{}'. Must be one of: start, center, end, nearest",
                    align
                )));
            }

            format!(
                r#"(function() {{
                    const el = document.querySelector('{}');
                    if (!el) throw new Error('Element not found: {}');
                    el.scrollIntoView({{ block: '{}', behavior: '{}' }});
                    return {{ success: true, selector: '{}' }};
                }})()"#,
                selector.replace('\'', "\\'"),
                selector.replace('\'', "\\'"),
                align,
                behavior,
                selector.replace('\'', "\\'")
            )
        }
    };

    // Wrap in Promise if waiting for scrollend
    let js = if wait && !cli.extension {
        format!(
            r#"(async () => {{
                return new Promise((resolve) => {{
                    {};

                    // Set timeout fallback (3 seconds)
                    const timeout = setTimeout(() => {{
                        window.removeEventListener('scrollend', handleScrollEnd);
                        resolve({{ success: true, timedOut: true }});
                    }}, 3000);

                    function handleScrollEnd() {{
                        clearTimeout(timeout);
                        resolve({{ success: true, timedOut: false }});
                    }}

                    window.addEventListener('scrollend', handleScrollEnd, {{ once: true }});
                }});
            }})()"#,
            js_core
        )
    } else {
        js_core
    };

    // Execute scroll command
    if cli.extension {
        extension_eval(cli, &js).await?;
    } else {
        let session_manager = create_session_manager(cli, config);
        if wait {
            // Use Runtime.evaluate with awaitPromise
            let result = session_manager
                .send_cdp_command(
                    effective_profile_arg(cli, config),
                    "Runtime.evaluate",
                    serde_json::json!({
                        "expression": js,
                        "awaitPromise": true,
                        "returnByValue": true,
                    })
                )
                .await?;

            // Check if timed out
            if let Some(result_value) = result.get("result").and_then(|r| r.get("value")) {
                if result_value.get("timedOut").and_then(|v| v.as_bool()).unwrap_or(false) {
                    eprintln!("Warning: Scroll wait timed out after 3 seconds");
                }
            }
        } else {
            session_manager
                .eval_on_page(effective_profile_arg(cli, config), &js)
                .await?;
        }
    }

    // Print success message
    match direction {
        ScrollDirection::Down { pixels } => {
            if *pixels == 0 {
                println!("✅ Scrolled down one viewport");
            } else {
                println!("✅ Scrolled down {} pixels", pixels);
            }
        }
        ScrollDirection::Up { pixels } => {
            if *pixels == 0 {
                println!("✅ Scrolled up one viewport");
            } else {
                println!("✅ Scrolled up {} pixels", pixels);
            }
        }
        ScrollDirection::Bottom => println!("✅ Scrolled to bottom"),
        ScrollDirection::Top => println!("✅ Scrolled to top"),
        ScrollDirection::To { selector, .. } => println!("✅ Scrolled to element: {}", selector),
    }

    Ok(())
}

/// Resolve session tag: use CLI value, env var, or auto-generate.
fn resolve_session_tag(cli: &Cli) -> String {
    cli.session_tag.clone().unwrap_or_else(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("ab-{}", ts)
    })
}

/// One-shot content fetching: navigate → wait → extract → close.
///
/// Combines I2 (HTTP degradation), I3 (session tags), I4 (URL rewriting),
/// and I5 (domain-aware wait hints) into a single high-level command.
pub(crate) async fn fetch(
    cli: &Cli,
    config: &Config,
    url: &str,
    format: &str,
    max_tokens: Option<usize>,
    timeout_ms: u64,
    lite: bool,
) -> Result<()> {
    // Extension mode not supported for one-shot fetch
    if cli.extension {
        return Err(ActionbookError::FeatureNotSupported(
            "browser fetch is not supported in extension mode. Use browser open + snapshot instead."
                .to_string(),
        ));
    }

    let session_tag = resolve_session_tag(cli);
    tracing::info!("[{}] fetch starting: {}", session_tag, url);

    // I4: URL rewriting
    let normalized_url = normalize_navigation_url(url)?;
    let normalized_url = if cli.rewrite_urls {
        let (rewritten, was_rewritten) =
            crate::browser::url_rewrite::maybe_rewrite(&normalized_url);
        if was_rewritten {
            tracing::info!(
                "[{}] URL rewritten: {} -> {}",
                session_tag,
                normalized_url,
                rewritten
            );
        }
        rewritten
    } else {
        normalized_url
    };

    // I2: HTTP-first fetch for --lite mode (skip for snapshot format which needs a11y tree)
    if lite && format != "snapshot" {
        tracing::info!("[{}] trying HTTP fetch (lite mode)", session_tag);
        match crate::browser::http_fetch::try_http_fetch(
            &normalized_url,
            max_tokens,
            Some(&session_tag),
        )
        .await
        {
            Ok(Some(result)) => {
                tracing::info!(
                    "[{}] HTTP fetch succeeded ({} tokens)",
                    session_tag,
                    result.tokens_estimate
                );
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "content": result.content,
                            "format": result.format,
                            "url": result.url,
                            "tokensEstimate": result.tokens_estimate,
                            "truncated": result.truncated,
                            "sessionTag": session_tag,
                            "method": "http",
                        })
                    );
                } else {
                    println!("{}", result.content);
                }
                return Ok(());
            }
            Ok(None) => {
                tracing::info!("[{}] HTTP fetch returned empty/SPA, falling back to browser", session_tag);
            }
            Err(e) => {
                tracing::warn!("[{}] HTTP fetch error, falling back to browser: {}", session_tag, e);
            }
        }
    }

    // Browser-based fetch path
    let fetch_result = timeout(
        Duration::from_millis(timeout_ms),
        fetch_via_browser(cli, config, &normalized_url, format, max_tokens, &session_tag),
    )
    .await;

    // Clean up temporary session regardless of success/failure/timeout
    // Must match the profile_name used in fetch_via_browser
    let profile_name = format!("__fetch_{}__", std::process::id());
    let session_manager = SessionManager::new(config.clone());
    if let Err(e) = session_manager.close_session(Some(&profile_name)).await {
        tracing::debug!("[{}] cleanup: close_session failed: {}", session_tag, e);
    }

    // Clean up session file and profile directory
    let sessions_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".actionbook")
        .join("sessions");
    let session_file = sessions_dir.join(format!("{}.json", profile_name));
    let _ = std::fs::remove_file(&session_file);

    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("actionbook");
    let profile_dir = data_dir.join("profiles").join(&profile_name);
    let _ = std::fs::remove_dir_all(&profile_dir);

    match fetch_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(ActionbookError::Timeout(format!(
            "Fetch timed out after {}ms: {}",
            timeout_ms, normalized_url
        ))),
    }
}

/// Browser-based fetch implementation (used by `fetch()` after HTTP fallback).
async fn fetch_via_browser(
    cli: &Cli,
    config: &Config,
    url: &str,
    format: &str,
    max_tokens: Option<usize>,
    session_tag: &str,
) -> Result<()> {
    // Create a temporary config with headless forced on
    let mut fetch_config = config.clone();
    let profile_name = format!("__fetch_{}__", std::process::id());

    // Set up a temporary profile with headless mode
    let mut profile = fetch_config.get_profile("actionbook").unwrap_or_default();
    profile.headless = true;
    if let Some(ref path) = cli.browser_path {
        profile.browser_path = Some(path.clone());
    }
    fetch_config.set_profile(&profile_name, profile);
    fetch_config.browser.default_profile = profile_name.clone();

    // Create driver with temporary profile
    let temp_cli = Cli {
        browser_path: cli.browser_path.clone(),
        cdp: cli.cdp.clone(),
        profile: Some(profile_name.clone()),
        headless: true,
        stealth: cli.stealth,
        stealth_os: cli.stealth_os.clone(),
        stealth_gpu: cli.stealth_gpu.clone(),
        api_key: cli.api_key.clone(),
        json: cli.json,
        browser_mode: None, // Use default mode
        extension: false,
        extension_port: cli.extension_port,
        verbose: cli.verbose,
        block_images: cli.block_images,
        block_media: cli.block_media,
        no_animations: true, // Always disable animations for fetch
        auto_dismiss_dialogs: true, // Always auto-dismiss for fetch
        session_tag: cli.session_tag.clone(),
        rewrite_urls: false, // Already rewritten above
        wait_hint: cli.wait_hint.clone(),
        camofox: cli.camofox,
        camofox_port: cli.camofox_port,
        command: crate::cli::Commands::Browser {
            command: BrowserCommands::Status,
        },
    };

    // For CDP backend, we need to start the browser session first
    // Otherwise disable_animations() and other methods will fail with BrowserNotRunning
    match BrowserDriver::from_config(&fetch_config, &fetch_config.get_profile(&profile_name).unwrap(), &temp_cli).await? {
        BrowserDriver::Cdp(session_mgr) => {
            // Start browser session
            let (_browser, mut handler) = session_mgr.get_or_create_session(Some(&profile_name)).await?;

            // Spawn handler in background
            tokio::spawn(async move {
                while handler.next().await.is_some() {}
            });

            // Create driver from existing session manager
            let mut driver = BrowserDriver::Cdp(session_mgr);

            // Apply resource blocking
            apply_resource_blocking(&temp_cli, &mut driver).await;

            // Apply animation disabling
            if let Err(e) = driver.disable_animations().await {
                tracing::warn!("[{}] Failed to disable animations: {}", session_tag, e);
            }

            // Apply dialog auto-dismissal
            if let Err(e) = driver.enable_dialog_auto_dismiss().await {
                tracing::warn!("[{}] Failed to enable dialog auto-dismiss: {}", session_tag, e);
            }

            // Navigate
            tracing::info!("[{}] navigating to {}", session_tag, url);
            driver.goto(url).await?;

            // Continue with rest of fetch logic using driver
            return complete_fetch(driver, url, format, max_tokens, session_tag, cli, profile_name, fetch_config).await;
        }
        #[cfg(feature = "camoufox")]
        driver @ (BrowserDriver::Camofox(_) | BrowserDriver::CamofoxWebDriver(_)) => {
            // Camoufox doesn't have the same session state requirements
            let mut driver = driver;

            // Apply resource blocking
            apply_resource_blocking(&temp_cli, &mut driver).await;

            // Apply animation disabling
            if let Err(e) = driver.disable_animations().await {
                tracing::warn!("[{}] Failed to disable animations: {}", session_tag, e);
            }

            // Apply dialog auto-dismissal
            if let Err(e) = driver.enable_dialog_auto_dismiss().await {
                tracing::warn!("[{}] Failed to enable dialog auto-dismiss: {}", session_tag, e);
            }

            // Navigate
            tracing::info!("[{}] navigating to {}", session_tag, url);
            driver.goto(url).await?;

            // Continue with rest of fetch logic using driver
            return complete_fetch(driver, url, format, max_tokens, session_tag, cli, profile_name, fetch_config).await;
        }
    }
}

/// Complete the fetch operation after navigation
async fn complete_fetch(
    mut driver: BrowserDriver,
    url: &str,
    format: &str,
    max_tokens: Option<usize>,
    session_tag: &str,
    cli: &Cli,
    _profile_name: String,
    _config: Config,
) -> Result<()> {

    // I5: Domain-aware wait
    let wait_ms =
        crate::browser::wait_hints::resolve_wait_ms(url, cli.wait_hint.as_deref());
    if wait_ms > 0 {
        tracing::info!("[{}] waiting {}ms (domain hint)", session_tag, wait_ms);
        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
    }

    // Also wait for network idle
    if let Err(e) = driver.wait_for_network_idle(15_000, 500).await {
        tracing::debug!("[{}] network idle wait ended: {}", session_tag, e);
    }

    // Extract content based on format
    let (content, content_format, tokens_estimate, truncated) = match format {
        "snapshot" => {
            let raw = driver.get_accessibility_tree_raw().await?;
            let (nodes, _cache) = crate::browser::snapshot::parse_ax_tree(
                raw,
                crate::browser::snapshot::SnapshotFilter::All,
                None,
                None,
            )?;

            let (final_nodes, was_truncated) = if let Some(max) = max_tokens {
                crate::browser::snapshot::truncate_to_tokens(
                    &nodes,
                    max,
                    crate::browser::snapshot::SnapshotFormat::Compact,
                )
            } else {
                (nodes, false)
            };

            let output = crate::browser::snapshot::format_compact(&final_nodes);
            let est = output.len() / 4;
            (output, "snapshot", est, was_truncated)
        }
        "html" => {
            let html = driver.get_content().await?;
            let est = html.len() / 4;
            let (content, truncated) = if let Some(max) = max_tokens {
                if est > max {
                    let char_limit = max * 4;
                    let end = if html.len() > char_limit {
                        html[..char_limit]
                            .rfind(char::is_whitespace)
                            .unwrap_or(char_limit)
                    } else {
                        html.len()
                    };
                    (html[..end].to_string(), true)
                } else {
                    (html, false)
                }
            } else {
                (html, false)
            };
            let final_est = content.len() / 4;
            (content, "html", final_est, truncated)
        }
        _ => {
            // "text" (default) — use readability
            let text = driver
                .get_readable_text(crate::browser::TextExtractionMode::Readability)
                .await?;
            let est = text.len() / 4;
            let (content, truncated) = if let Some(max) = max_tokens {
                if est > max {
                    let char_limit = max * 4;
                    let end = if text.len() > char_limit {
                        text[..char_limit]
                            .rfind(char::is_whitespace)
                            .unwrap_or(char_limit)
                    } else {
                        text.len()
                    };
                    (text[..end].to_string(), true)
                } else {
                    (text, false)
                }
            } else {
                (text, false)
            };
            let final_est = content.len() / 4;
            (content, "text", final_est, truncated)
        }
    };

    tracing::info!(
        "[{}] extracted {} tokens ({} format)",
        session_tag,
        tokens_estimate,
        content_format
    );

    // Output
    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "content": content,
                "format": content_format,
                "url": url,
                "tokensEstimate": tokens_estimate,
                "truncated": truncated,
                "sessionTag": session_tag,
                "method": "browser",
            })
        );
    } else {
        println!("{}", content);
    }

    // Note: Cleanup is handled by the caller (fetch function)
    // to ensure it runs even on timeout

    Ok(())
}

pub(crate) async fn upload(
    cli: &Cli,
    config: &Config,
    files: &[String],
    selector: Option<&str>,
    ref_id: Option<&str>,
    wait_ms: u64,
) -> Result<()> {
    // 1. Validate all files exist and resolve to absolute paths
    let mut abs_paths = Vec::with_capacity(files.len());
    for f in files {
        let path = std::path::Path::new(f);
        if !path.exists() {
            return Err(ActionbookError::Other(format!("File not found: {}", f)));
        }
        let canonical = std::fs::canonicalize(path).map_err(|e| {
            ActionbookError::Other(format!("Cannot resolve path {}: {}", f, e))
        })?;
        abs_paths.push(canonical.to_string_lossy().to_string());
    }

    // 2. Extension mode: JS fallback (cannot set files programmatically, just click the input)
    if cli.extension {
        let sel = selector.unwrap_or("input[type=\"file\"]");
        let resolve_js = js_resolve_selector(sel);
        let click_js = format!(
            r#"(function() {{
                var el = {};
                if (!el) return {{ success: false, error: 'File input not found' }};
                el.click();
                return {{ success: true, note: 'Triggered native file dialog — extension mode cannot set files programmatically' }};
            }})()"#,
            resolve_js
        );
        let result = extension_eval(cli, &click_js).await?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("File input not found");
            return Err(ActionbookError::ElementNotFound(err.to_string()));
        }
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
                    "selector": sel,
                    "note": "Extension mode: opened native file dialog. Cannot set files programmatically."
                })
            );
        } else {
            println!(
                "{} Opened file dialog for: {} (extension mode — select files manually)",
                "⚠".yellow(),
                sel
            );
        }
        return Ok(());
    }

    // 3. CDP mode
    let mut driver = create_browser_driver(cli, config).await?;
    apply_resource_blocking(cli, &mut driver).await;

    // --ref mode
    if let Some(ref_str) = ref_id {
        let backend_node_id = resolve_snapshot_ref(&mut driver, ref_str).await?;

        driver
            .set_file_input_files_by_node_id(backend_node_id, &abs_paths)
            .await?;

        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
                    "ref": ref_str,
                    "backendNodeId": backend_node_id,
                    "files": abs_paths,
                })
            );
        } else {
            println!(
                "{} Uploaded {} file(s) via ref={} (nodeId={})",
                "✓".green(),
                abs_paths.len(),
                ref_str,
                backend_node_id,
            );
        }
        return Ok(());
    }

    // Selector mode (auto-detect if omitted)
    let sel = selector.unwrap_or("input[type=\"file\"]");

    // Optional wait for element
    if wait_ms > 0 {
        if let Some(mgr) = driver.as_cdp_mut() {
            mgr.wait_for_element(None, sel, wait_ms).await?;
        }
    }

    driver.set_file_input_files(sel, &abs_paths).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "selector": sel,
                "files": abs_paths,
            })
        );
    } else {
        println!(
            "{} Uploaded {} file(s) to: {}",
            "✓".green(),
            abs_paths.len(),
            sel,
        );
    }

    Ok(())
}

pub(crate) async fn close(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        extension_send(cli, "Extension.detachTab", serde_json::json!({})).await?;

        let port = cli.extension_port;
        bridge_lifecycle::stop_bridge(port).await?;

        if cli.json {
            println!("{}", serde_json::json!({ "success": true }));
        } else {
            println!("{} Tab detached and bridge stopped (extension)", "✓".green());
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let profile_name = effective_profile_name(cli, config);
    session_manager
        .close_session(Some(profile_name))
        .await?;

    // G3: Mark clean exit to prevent "Chrome didn't shut down correctly" on next launch
    let profile_dir = crate::browser::launcher::BrowserLauncher::default_user_data_dir(profile_name);
    crate::browser::launcher::mark_clean_exit(&profile_dir);

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true
            })
        );
    } else {
        println!("{} Browser closed", "✓".green());
    }

    Ok(())
}

pub(crate) async fn fingerprint(cli: &Cli, config: &Config, command: &FingerprintCommands) -> Result<()> {
    match command {
        FingerprintCommands::Rotate { os, screen } => {
            use crate::browser::fingerprint_generator::{
                FingerprintGenerator, OperatingSystem, generate_with_os,
            };

            // Generate fingerprint
            let fingerprint = match os.to_lowercase().as_str() {
                "windows" => generate_with_os(OperatingSystem::Windows),
                "mac" | "macos" => generate_with_os(OperatingSystem::MacOsArm),
                "linux" => generate_with_os(OperatingSystem::Linux),
                _ => {
                    let mut gen = FingerprintGenerator::new();
                    gen.generate()
                }
            };

            // Override screen if specified
            let mut fp = fingerprint;
            if screen != "random" {
                if let Some((w, h)) = screen.split_once('x') {
                    if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                        fp.screen_width = w;
                        fp.screen_height = h;
                        fp.avail_width = w;
                        fp.avail_height = h.saturating_sub(40); // taskbar offset
                    }
                }
            }

            let mut driver = create_browser_driver(cli, config).await?;
            driver.rotate_fingerprint(&fp).await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "userAgent": fp.user_agent,
                        "platform": fp.platform,
                        "screen": format!("{}x{}", fp.screen_width, fp.screen_height),
                        "hardwareConcurrency": fp.hardware_concurrency,
                        "deviceMemory": fp.device_memory,
                    })
                );
            } else {
                println!("{} Fingerprint rotated", "✓".green());
                println!("  UA: {}", fp.user_agent);
                println!("  Platform: {}", fp.platform);
                println!(
                    "  Screen: {}x{}",
                    fp.screen_width, fp.screen_height
                );
                println!("  CPU cores: {}", fp.hardware_concurrency);
                println!("  Device memory: {} GB", fp.device_memory);
            }
        }
    }
    Ok(())
}

// ========== H1: Console Log Capture ==========

pub(crate) async fn console_log(cli: &Cli, config: &Config, duration_ms: u64, level: &str) -> Result<()> {
    if cli.extension {
        return Err(ActionbookError::FeatureNotSupported(
            "Console capture is not supported in extension mode".to_string(),
        ));
    }

    let mut driver = create_browser_driver(cli, config).await?;

    // Install interceptor
    driver.install_console_interceptor().await?;

    if duration_ms > 0 {
        // Listen for specified duration
        tokio::time::sleep(Duration::from_millis(duration_ms)).await;
    }

    let logs = driver.capture_console_logs().await?;

    // Filter by level
    let filtered: Vec<&serde_json::Value> = if level == "all" {
        logs.iter().collect()
    } else {
        logs.iter()
            .filter(|l| l.get("level").and_then(|v| v.as_str()) == Some(level))
            .collect()
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else {
        if filtered.is_empty() {
            println!(
                "{} No console messages captured{}",
                "ℹ".blue(),
                if duration_ms > 0 {
                    format!(" (listened for {}ms)", duration_ms)
                } else {
                    String::new()
                }
            );
        } else {
            for entry in &filtered {
                let lvl = entry.get("level").and_then(|v| v.as_str()).unwrap_or("log");
                let text = entry.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let prefix = match lvl {
                    "error" => format!("[{}]", "ERR".red()),
                    "warn" => format!("[{}]", "WRN".yellow()),
                    "info" => format!("[{}]", "INF".blue()),
                    "debug" => format!("[{}]", "DBG".dimmed()),
                    _ => format!("[{}]", "LOG".normal()),
                };
                println!("{} {}", prefix, text);
            }
            println!(
                "\n{} {} message(s) captured",
                "✓".green(),
                filtered.len()
            );
        }
    }
    Ok(())
}

// ========== H2: Network Idle Wait ==========

pub(crate) async fn wait_idle(cli: &Cli, config: &Config, timeout_ms: u64, idle_ms: u64) -> Result<()> {
    if cli.extension {
        return Err(ActionbookError::FeatureNotSupported(
            "Network idle wait is not supported in extension mode".to_string(),
        ));
    }

    let mut driver = create_browser_driver(cli, config).await?;
    driver.wait_for_network_idle(timeout_ms, idle_ms).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true, "idle": true }));
    } else {
        println!("{} Network is idle", "✓".green());
    }
    Ok(())
}

// ========== H4: Element Info ==========

pub(crate) async fn info(cli: &Cli, config: &Config, selector: &str) -> Result<()> {
    if cli.extension {
        let js = format!(
            r#"(function() {{
                var el = document.querySelector('{}');
                if (!el) return null;
                var rect = el.getBoundingClientRect();
                var cs = getComputedStyle(el);
                return {{
                    tagName: el.tagName.toLowerCase(),
                    id: el.id || null,
                    textContent: (el.textContent || '').trim().substring(0, 200),
                    boundingBox: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }},
                    isVisible: rect.width > 0 && rect.height > 0 && cs.visibility !== 'hidden'
                }};
            }})()"#,
            escape_js_string(selector)
        );
        let result = extension_eval(cli, &js).await?;
        if result.is_null() {
            return Err(ActionbookError::ElementNotFound(selector.to_string()));
        }
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    let mut driver = create_browser_driver(cli, config).await?;
    let result = driver.get_element_info(selector).await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        let tag = result.get("tagName").and_then(|v| v.as_str()).unwrap_or("?");
        let id = result.get("id").and_then(|v| v.as_str());
        let text = result.get("textContent").and_then(|v| v.as_str()).unwrap_or("");
        let visible = result.get("isVisible").and_then(|v| v.as_bool()).unwrap_or(false);
        let interactive = result.get("isInteractive").and_then(|v| v.as_bool()).unwrap_or(false);

        println!("{} <{}>", "Element:".bold(), tag);
        if let Some(id) = id {
            if !id.is_empty() {
                println!("  id: {}", id);
            }
        }
        if !text.is_empty() {
            let display = if text.len() > 80 { &text[..80] } else { text };
            println!("  text: \"{}\"", display);
        }
        if let Some(bbox) = result.get("boundingBox") {
            let x = bbox.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = bbox.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let w = bbox.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h = bbox.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
            println!("  bbox: ({:.0}, {:.0}) {}x{}", x, y, w as u32, h as u32);
        }
        println!(
            "  visible: {} | interactive: {}",
            if visible { "yes".green().to_string() } else { "no".red().to_string() },
            if interactive { "yes".green().to_string() } else { "no".dimmed().to_string() }
        );
        if let Some(selectors) = result.get("suggestedSelectors").and_then(|v| v.as_array()) {
            if !selectors.is_empty() {
                println!("  selectors:");
                for s in selectors {
                    if let Some(sel) = s.as_str() {
                        println!("    {}", sel);
                    }
                }
            }
        }
    }
    Ok(())
}

// ========== H5: Local Storage Management ==========

pub(crate) async fn storage(cli: &Cli, config: &Config, command: &StorageCommands) -> Result<()> {
    match command {
        StorageCommands::Get { key, session } => {
            let storage_type = if *session { "sessionStorage" } else { "localStorage" };
            let js = format!(
                "(function() {{ var v = {}.getItem('{}'); return v; }})()",
                storage_type,
                escape_js_string(key)
            );

            let result = if cli.extension {
                extension_eval(cli, &js).await?
            } else {
                let mut driver = create_browser_driver(cli, config).await?;
                let r = driver.eval(&js).await?;
                serde_json::from_str(&r).unwrap_or(serde_json::Value::Null)
            };

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "key": key, "value": result, "storage": storage_type })
                );
            } else if result.is_null() {
                println!("{} Key '{}' not found in {}", "ℹ".blue(), key, storage_type);
            } else {
                println!("{}", result.as_str().unwrap_or(&result.to_string()));
            }
        }
        StorageCommands::Set { key, value, session } => {
            let storage_type = if *session { "sessionStorage" } else { "localStorage" };
            let js = format!(
                "{}.setItem('{}', '{}')",
                storage_type,
                escape_js_string(key),
                escape_js_string(value)
            );

            if cli.extension {
                extension_eval(cli, &js).await?;
            } else {
                let mut driver = create_browser_driver(cli, config).await?;
                driver.eval(&js).await?;
            };

            if cli.json {
                println!("{}", serde_json::json!({ "success": true }));
            } else {
                println!("{} Set {}['{}']", "✓".green(), storage_type, key);
            }
        }
        StorageCommands::Remove { key, session } => {
            let storage_type = if *session { "sessionStorage" } else { "localStorage" };
            let js = format!(
                "{}.removeItem('{}')",
                storage_type,
                escape_js_string(key)
            );

            if cli.extension {
                extension_eval(cli, &js).await?;
            } else {
                let mut driver = create_browser_driver(cli, config).await?;
                driver.eval(&js).await?;
            };

            if cli.json {
                println!("{}", serde_json::json!({ "success": true }));
            } else {
                println!("{} Removed '{}' from {}", "✓".green(), key, storage_type);
            }
        }
        StorageCommands::Clear { session } => {
            let storage_type = if *session { "sessionStorage" } else { "localStorage" };
            let js = format!("{}.clear()", storage_type);

            if cli.extension {
                extension_eval(cli, &js).await?;
            } else {
                let mut driver = create_browser_driver(cli, config).await?;
                driver.eval(&js).await?;
            };

            if cli.json {
                println!("{}", serde_json::json!({ "success": true }));
            } else {
                println!("{} Cleared {}", "✓".green(), storage_type);
            }
        }
        StorageCommands::List { session } => {
            let storage_type = if *session { "sessionStorage" } else { "localStorage" };
            let js = format!(
                "(function() {{ var s = {}; var keys = []; for (var i = 0; i < s.length; i++) {{ var k = s.key(i); keys.push({{ key: k, value: s.getItem(k) }}); }} return keys; }})()",
                storage_type
            );

            let result = if cli.extension {
                extension_eval(cli, &js).await?
            } else {
                let mut driver = create_browser_driver(cli, config).await?;
                let r = driver.eval(&js).await?;
                serde_json::from_str(&r).unwrap_or(serde_json::Value::Null)
            };

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let empty = vec![];
                let items = result.as_array().unwrap_or(&empty);
                if items.is_empty() {
                    println!("{} {} is empty", "ℹ".blue(), storage_type);
                } else {
                    println!("{} ({} keys):", storage_type, items.len());
                    for item in items {
                        let k = item.get("key").and_then(|v| v.as_str()).unwrap_or("?");
                        let v = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
                        let display = if v.len() > 60 { &v[..60] } else { v };
                        println!("  {} = {}", k.bold(), display);
                    }
                }
            }
        }
    }
    Ok(())
}

// ========== H6: Device Emulation ==========

/// Device presets for emulation
fn resolve_device(name: &str) -> Result<(u32, u32, f64, bool, Option<&'static str>)> {
    // (width, height, scale, mobile, user_agent)
    match name.to_lowercase().as_str() {
        "iphone-14" | "iphone14" => Ok((
            390, 844, 3.0, true,
            Some("Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1"),
        )),
        "iphone-se" | "iphonese" => Ok((
            375, 667, 2.0, true,
            Some("Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.0 Mobile/15E148 Safari/604.1"),
        )),
        "pixel-7" | "pixel7" => Ok((
            412, 915, 2.625, true,
            Some("Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"),
        )),
        "ipad" | "ipad-air" => Ok((
            820, 1180, 2.0, true,
            Some("Mozilla/5.0 (iPad; CPU OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1"),
        )),
        "desktop-hd" | "1080p" => Ok((1920, 1080, 1.0, false, None)),
        "desktop-4k" | "4k" => Ok((3840, 2160, 2.0, false, None)),
        _ => {
            // Try to parse WxH format (e.g., "1280x720")
            let parts: Vec<&str> = name.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0].parse::<u32>().map_err(|_| {
                    ActionbookError::Other(format!("Invalid width in '{}'", name))
                })?;
                let h = parts[1].parse::<u32>().map_err(|_| {
                    ActionbookError::Other(format!("Invalid height in '{}'", name))
                })?;
                Ok((w, h, 1.0, false, None))
            } else {
                Err(ActionbookError::Other(format!(
                    "Unknown device '{}'. Available: iphone-14, iphone-se, pixel-7, ipad, desktop-hd, desktop-4k, or WxH (e.g., 1280x720)",
                    name
                )))
            }
        }
    }
}

pub(crate) async fn emulate(cli: &Cli, config: &Config, device: &str) -> Result<()> {
    let (width, height, scale, mobile, ua) = resolve_device(device)?;

    if cli.extension {
        return Err(ActionbookError::FeatureNotSupported(
            "Device emulation is not supported in extension mode".to_string(),
        ));
    }

    let mut driver = create_browser_driver(cli, config).await?;
    driver
        .emulate_device(width, height, scale, mobile, ua)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "device": device,
                "width": width,
                "height": height,
                "scale": scale,
                "mobile": mobile
            })
        );
    } else {
        println!(
            "{} Emulating {} ({}x{} @{}x{})",
            "✓".green(),
            device,
            width,
            height,
            scale,
            if mobile { " mobile" } else { "" }
        );
    }
    Ok(())
}

// ========== H7: Wait for JS Condition ==========

pub(crate) async fn wait_fn(
    cli: &Cli,
    config: &Config,
    expression: &str,
    timeout_ms: u64,
    interval_ms: u64,
) -> Result<()> {
    if cli.extension {
        // Extension mode: poll via extension_eval
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        loop {
            let result = extension_eval(cli, expression).await?;
            let truthy = match &result {
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::Number(n) => n.as_f64().map_or(false, |f| f != 0.0),
                serde_json::Value::String(s) => !s.is_empty(),
                serde_json::Value::Null => false,
                serde_json::Value::Array(a) => !a.is_empty(),
                serde_json::Value::Object(_) => true,
            };
            if truthy {
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "success": true, "value": result })
                    );
                } else {
                    println!("{} Expression returned truthy", "✓".green());
                }
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Err(ActionbookError::Timeout(format!(
                    "Expression did not become truthy within {}ms",
                    timeout_ms
                )));
            }
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }

    let mut driver = create_browser_driver(cli, config).await?;
    let result = driver
        .wait_for_function(expression, timeout_ms, interval_ms)
        .await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "value": result })
        );
    } else {
        println!("{} Expression returned truthy", "✓".green());
    }
    Ok(())
}

pub(crate) async fn restart(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        // In extension mode, reload the page as a "restart"
        extension_send(cli, "Page.reload", serde_json::json!({})).await?;

        if cli.json {
            println!("{}", serde_json::json!({ "success": true }));
        } else {
            println!("{} Page reloaded (extension restart)", "✓".green());
        }
        return Ok(());
    }

    // Close existing session
    close(cli, config).await?;

    // Open a blank page to restart
    let session_manager = create_session_manager(cli, config);
    let (_browser, mut handler) = session_manager
        .get_or_create_session(effective_profile_arg(cli, config))
        .await?;

    tokio::spawn(async move { while handler.next().await.is_some() {} });

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true
            })
        );
    } else {
        println!("{} Browser restarted", "✓".green());
    }

    Ok(())
}

pub(crate) async fn connect(cli: &Cli, config: &Config, endpoint: &str, raw_headers: &[String]) -> Result<()> {
    let profile_name = effective_profile_name(cli, config);
    let (cdp_port, cdp_url) = resolve_cdp_endpoint(endpoint).await?;

    // Parse key:value header pairs
    let ws_headers: Option<std::collections::HashMap<String, String>> = if raw_headers.is_empty() {
        None
    } else {
        let mut map = std::collections::HashMap::new();
        for h in raw_headers {
            if let Some((k, v)) = h.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            } else {
                return Err(ActionbookError::Other(format!(
                    "Invalid header format '{}'. Expected KEY:VALUE",
                    h
                )));
            }
        }
        Some(map)
    };

    // Persist the session so subsequent commands can reuse this browser
    let session_manager = create_session_manager(cli, config);
    session_manager.save_external_session_full(profile_name, cdp_port, &cdp_url, None, ws_headers)?;

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

pub(crate) async fn tab_command(cli: &Cli, config: &Config, cmd: &TabCommands) -> Result<()> {
    match cmd {
        TabCommands::List => tab_list(cli, config).await,
        TabCommands::New { url } => tab_new(cli, config, url.as_deref()).await,
        TabCommands::Switch { page_id } => tab_switch(cli, config, page_id).await,
        TabCommands::Close { page_id } => tab_close(cli, config, page_id.as_deref()).await,
        TabCommands::Active => tab_active(cli, config).await,
    }
}

pub(crate) async fn tab_list(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        // Extension mode
        let result = extension_send(cli, "Extension.listTabs", serde_json::json!({})).await?;
        let tabs = result.get("tabs").and_then(|t| t.as_array()).cloned().unwrap_or_default();

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&tabs)?);
        } else {
            println!("{}", "Open tabs:".bold());
            for (i, tab) in tabs.iter().enumerate() {
                let id = tab.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
                let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let active = tab.get("active").and_then(|v| v.as_bool()).unwrap_or(false);

                let prefix = if active { "→".green() } else { " ".normal() };
                println!("{} {}. [{}] {}", prefix, i + 1, id, title.cyan());
                println!("     {}", url.dimmed());
            }
        }
    } else {
        // CDP mode
        let driver = create_browser_driver(cli, config).await?;
        let pages = driver.list_pages().await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&pages)?);
        } else {
            let active_page = driver.get_active_page().await.ok();
            let active_id = active_page.as_ref().map(|p| p.id.as_str());

            println!("{}", "Open tabs:".bold());
            for (i, page) in pages.iter().enumerate() {
                let is_active = Some(page.id.as_str()) == active_id;
                let prefix = if is_active { "→".green() } else { " ".normal() };

                let id_display = if page.id.len() > 12 {
                    &page.id[..12]
                } else {
                    &page.id
                };

                println!("{} {}. [{}] {}", prefix, i + 1, id_display, page.title.cyan());
                println!("     {}", page.url.dimmed());
            }
        }
    }

    Ok(())
}

pub(crate) async fn tab_new(cli: &Cli, config: &Config, url: Option<&str>) -> Result<()> {
    if cli.extension {
        let params = if let Some(url) = url {
            serde_json::json!({ "url": url })
        } else {
            serde_json::json!({})
        };
        let result = extension_send(cli, "Extension.createTab", params).await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            let tab_id = result.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("{} Created new tab [{}]", "✓".green(), tab_id);
        }
    } else {
        let mut driver = create_browser_driver(cli, config).await?;
        let page = driver.new_page(url).await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&page)?);
        } else {
            let id_display = if page.id.len() > 12 {
                &page.id[..12]
            } else {
                &page.id
            };
            println!("{} Created new tab [{}]", "✓".green(), id_display);
            if let Some(url) = url {
                println!("  Navigated to: {}", url);
            }
        }
    }

    Ok(())
}

pub(crate) async fn tab_switch(cli: &Cli, config: &Config, page_id: &str) -> Result<()> {
    if cli.extension {
        let tab_id: u64 = page_id.parse()
            .map_err(|_| ActionbookError::InvalidArgument("Invalid tab ID".to_string()))?;
        let result = extension_send(
            cli,
            "Extension.activateTab",
            serde_json::json!({ "tabId": tab_id })
        ).await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("{} Switched to tab [{}]", "✓".green(), tab_id);
        }
    } else {
        let mut driver = create_browser_driver(cli, config).await?;
        let page = driver.switch_to_page(page_id).await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&page)?);
        } else {
            let id_display = if page.id.len() > 12 {
                &page.id[..12]
            } else {
                &page.id
            };
            println!("{} Switched to tab [{}]", "✓".green(), id_display);
            println!("  {}", page.title);
            println!("  {}", page.url.dimmed());
        }
    }

    Ok(())
}

pub(crate) async fn tab_close(cli: &Cli, config: &Config, page_id: Option<&str>) -> Result<()> {
    if cli.extension {
        let tab_id = if let Some(id) = page_id {
            id.parse::<u64>()
                .map_err(|_| ActionbookError::InvalidArgument("Invalid tab ID".to_string()))?
        } else {
            // Get active tab ID
            let result = extension_send(cli, "Extension.getActiveTab", serde_json::json!({})).await?;
            result.get("id").and_then(|v| v.as_u64())
                .ok_or_else(|| ActionbookError::InvalidOperation("No active tab".to_string()))?
        };

        extension_send(cli, "Extension.closeTab", serde_json::json!({ "tabId": tab_id })).await?;

        if !cli.json {
            println!("{} Closed tab [{}]", "✓".green(), tab_id);
        }
    } else {
        let mut driver = create_browser_driver(cli, config).await?;

        let target_id = if let Some(id) = page_id {
            id.to_string()
        } else {
            // Get active page ID
            driver.get_active_page().await?.id
        };

        driver.close_page(&target_id).await?;

        if !cli.json {
            let id_display = if target_id.len() > 12 {
                &target_id[..12]
            } else {
                &target_id
            };
            println!("{} Closed tab [{}]", "✓".green(), id_display);
        }
    }

    Ok(())
}

pub(crate) async fn tab_active(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        let result = extension_send(cli, "Extension.getActiveTab", serde_json::json!({})).await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            let tab_id = result.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
            let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");

            println!("{}", "Active tab:".bold());
            println!("  ID: {}", tab_id);
            println!("  Title: {}", title.cyan());
            println!("  URL: {}", url.dimmed());
        }
    } else {
        let driver = create_browser_driver(cli, config).await?;
        let page = driver.get_active_page().await?;

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&page)?);
        } else {
            let id_display = if page.id.len() > 12 {
                &page.id[..12]
            } else {
                &page.id
            };
            println!("{}", "Active tab:".bold());
            println!("  ID: {}", id_display);
            println!("  Title: {}", page.title.cyan());
            println!("  URL: {}", page.url.dimmed());
        }
    }

    Ok(())
}

pub(crate) async fn switch_frame(cli: &Cli, config: &Config, target: &str) -> Result<()> {
    if cli.extension {
        return Err(ActionbookError::Other(
            "Frame switching not supported in extension mode".to_string()
        ));
    }

    let session_manager = create_session_manager(cli, config);

    match target.to_lowercase().as_str() {
        "default" | "main" => {
            session_manager
                .switch_to_default_frame(effective_profile_arg(cli, config))
                .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "frame": "default"
                    })
                );
            } else {
                println!("{} Switched to main frame", "✓".green());
            }
        }
        "parent" => {
            session_manager
                .switch_to_parent_frame(effective_profile_arg(cli, config))
                .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "frame": "parent"
                    })
                );
            } else {
                println!("{} Switched to parent frame", "✓".green());
            }
        }
        _ => {
            // Treat as iframe selector
            let frame_id = session_manager
                .switch_to_frame(effective_profile_arg(cli, config), target)
                .await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "frame": frame_id,
                        "selector": target
                    })
                );
            } else {
                println!("{} Switched to iframe: {}", "✓".green(), target);
                println!("  Frame ID: {}", frame_id);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        effective_profile_name, is_reusable_initial_blank_page_url, normalize_navigation_url,
    };
    use crate::cli::{BrowserCommands, BrowserMode, Cli, Commands};
    use crate::config::Config;

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
            block_images: false,
            block_media: false,
            no_animations: false,
            auto_dismiss_dialogs: false,
            session_tag: None,
            rewrite_urls: false,
            wait_hint: None,
            camofox: false,
            camofox_port: None,
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
    fn reusable_initial_blank_page_urls() {
        assert!(is_reusable_initial_blank_page_url("about:blank"));
        assert!(is_reusable_initial_blank_page_url(" ABOUT:BLANK "));
        assert!(is_reusable_initial_blank_page_url("about:newtab"));
        assert!(is_reusable_initial_blank_page_url("chrome://newtab/"));
        assert!(is_reusable_initial_blank_page_url("chrome://new-tab-page/"));
        assert!(is_reusable_initial_blank_page_url("edge://newtab/"));
    }

    #[test]
    fn non_reusable_page_urls() {
        assert!(!is_reusable_initial_blank_page_url(""));
        assert!(!is_reusable_initial_blank_page_url("https://example.com"));
        assert!(!is_reusable_initial_blank_page_url("chrome://settings"));
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
                headers: vec![],
            },
        );
        let mut config = Config::default();
        config.browser.default_profile = "team-connect".to_string();

        assert_eq!(effective_profile_name(&cli, &config), "team-connect");
    }

    // --- resolve_browser_mode tests ---

    #[test]
    fn browser_mode_extension_enables_extension() {
        let (ext, _port) = super::resolve_browser_mode(
            Some(BrowserMode::Extension), false, 19222, BrowserMode::Isolated, 19222,
        );
        assert!(ext, "--browser-mode=extension should enable extension");
    }

    #[test]
    fn browser_mode_isolated_overrides_extension_flag() {
        let (ext, _port) = super::resolve_browser_mode(
            Some(BrowserMode::Isolated), true, 19222, BrowserMode::Extension, 19222,
        );
        assert!(!ext, "--browser-mode=isolated should override --extension flag");
    }

    #[test]
    fn extension_flag_alone_enables_extension() {
        let (ext, _port) = super::resolve_browser_mode(
            None, true, 19222, BrowserMode::Isolated, 19222,
        );
        assert!(ext, "--extension alone should enable extension");
    }

    #[test]
    fn config_extension_mode_activates_when_no_flags() {
        let (ext, port) = super::resolve_browser_mode(
            None, false, 19222, BrowserMode::Extension, 18000,
        );
        assert!(ext, "Config extension mode should activate when no flags");
        assert_eq!(port, 18000, "Should use config port");
    }

    #[test]
    fn default_stays_isolated() {
        let (ext, _port) = super::resolve_browser_mode(
            None, false, 19222, BrowserMode::Isolated, 19222,
        );
        assert!(!ext, "Default should stay isolated");
    }

    #[test]
    fn custom_cli_port_preserved() {
        let (ext, port) = super::resolve_browser_mode(
            Some(BrowserMode::Extension), false, 20000, BrowserMode::Isolated, 19222,
        );
        assert!(ext);
        assert_eq!(port, 20000, "Non-default CLI port should be preserved");
    }

    #[test]
    fn default_port_falls_back_to_config_port() {
        let (ext, port) = super::resolve_browser_mode(
            Some(BrowserMode::Extension), false, 19222, BrowserMode::Isolated, 18500,
        );
        assert!(ext);
        assert_eq!(port, 18500, "Default port (19222) should fall back to config port");
    }

    // Tests for the new CDP Accessibility Tree snapshot formatting are in
    // browser/snapshot.rs (format_compact, parse_ax_tree, diff_snapshots)
}
