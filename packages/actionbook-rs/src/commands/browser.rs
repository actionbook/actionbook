use std::fs;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use colored::Colorize;
use futures::StreamExt;
use tokio::time::timeout;

#[cfg(feature = "stealth")]
use crate::browser::apply_stealth_to_page;
use crate::browser::{
    build_stealth_profile, discover_all_browsers, extension_bridge, BrowserDriver,
    stealth_status, SessionManager, SessionStatus, StealthConfig,
};
use crate::cli::{BrowserCommands, Cli, CookiesCommands};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Send a command (CDP or Extension.*) through the extension bridge.
/// For CDP methods, auto-attaches the active tab if no tab is currently attached.
async fn extension_send(
    cli: &Cli,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let result = extension_bridge::send_command(cli.extension_port, method, params.clone()).await;

    // Auto-attach: if a CDP method fails because no tab is attached, attach the active tab and retry
    if let Err(ActionbookError::ExtensionError(ref msg)) = result {
        if msg.contains("No tab attached") && !method.starts_with("Extension.") {
            tracing::debug!("Auto-attaching active tab for {}", method);
            extension_bridge::send_command(
                cli.extension_port,
                "Extension.attachActiveTab",
                serde_json::json!({}),
            )
            .await?;
            return extension_bridge::send_command(cli.extension_port, method, params).await;
        }
    }

    result
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

/// Resolve a CDP endpoint string (port number or ws:// URL) into a (port, ws_url) pair.
/// When given a numeric port, queries `http://127.0.0.1:{port}/json/version` to discover
/// the current browser WebSocket URL.
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

pub async fn run(cli: &Cli, command: &BrowserCommands) -> Result<()> {
    // --profile is not supported in extension mode: extension operates on the live Chrome profile
    if cli.extension && cli.profile.is_some() {
        return Err(ActionbookError::Other(
            "--profile is not supported in extension mode. Extension operates on your live Chrome profile. \
             Remove --profile to use the default profile, or remove --extension to use isolated mode.".to_string()
        ));
    }

    let config = Config::load()?;

    // When --cdp is set, resolve it to a fresh WebSocket URL and persist it
    // as the active session *before* any command runs. Skip for `connect`
    // which has its own CDP resolution logic.
    if !matches!(command, BrowserCommands::Connect { .. }) {
        ensure_cdp_override(cli, &config).await?;
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
        BrowserCommands::Click { selector, wait: w } => click(cli, &config, selector, *w).await,
        BrowserCommands::Type {
            selector,
            text,
            wait: w,
        } => type_text(cli, &config, selector, text, *w).await,
        BrowserCommands::Fill {
            selector,
            text,
            wait: w,
        } => fill(cli, &config, selector, text, *w).await,
        BrowserCommands::Select { selector, value } => select(cli, &config, selector, value).await,
        BrowserCommands::Hover { selector } => hover(cli, &config, selector).await,
        BrowserCommands::Focus { selector } => focus(cli, &config, selector).await,
        BrowserCommands::Press { key } => press(cli, &config, key).await,
        BrowserCommands::Screenshot { path, full_page } => {
            screenshot(cli, &config, path, *full_page).await
        }
        BrowserCommands::Pdf { path } => pdf(cli, &config, path).await,
        BrowserCommands::Eval { code } => eval(cli, &config, code).await,
        BrowserCommands::Html { selector } => html(cli, &config, selector.as_deref()).await,
        BrowserCommands::Text { selector } => text(cli, &config, selector.as_deref()).await,
        BrowserCommands::Snapshot => snapshot(cli, &config).await,
        BrowserCommands::Inspect { x, y, desc } => {
            inspect(cli, &config, *x, *y, desc.as_deref()).await
        }
        BrowserCommands::Viewport => viewport(cli, &config).await,
        BrowserCommands::Cookies { command } => cookies(cli, &config, command).await,
        BrowserCommands::Scroll { direction, smooth } => {
            scroll(cli, &config, direction, *smooth).await
        }
        BrowserCommands::Close => close(cli, &config).await,
        BrowserCommands::Restart => restart(cli, &config).await,
        BrowserCommands::Connect { endpoint } => connect(cli, &config, endpoint).await,
    }
}

async fn status(cli: &Cli, config: &Config) -> Result<()> {
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

async fn open(cli: &Cli, config: &Config, url: &str) -> Result<()> {
    let normalized_url = normalize_navigation_url(url)?;

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

async fn goto(cli: &Cli, config: &Config, url: &str, _timeout_ms: u64) -> Result<()> {
    let normalized_url = normalize_navigation_url(url)?;

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

async fn back(cli: &Cli, config: &Config) -> Result<()> {
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

async fn forward(cli: &Cli, config: &Config) -> Result<()> {
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

async fn reload(cli: &Cli, config: &Config) -> Result<()> {
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

async fn pages(cli: &Cli, config: &Config) -> Result<()> {
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

async fn switch(cli: &Cli, _config: &Config, page_id: &str) -> Result<()> {
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

async fn wait(cli: &Cli, config: &Config, selector: &str, timeout_ms: u64) -> Result<()> {
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

async fn wait_nav(cli: &Cli, config: &Config, timeout_ms: u64) -> Result<()> {
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

async fn click(cli: &Cli, config: &Config, selector: &str, wait_ms: u64) -> Result<()> {
    if cli.extension {
        if cli.camofox {
            // Route through Extension Bridge with Camoufox backend
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
            // Reuse the wait logic
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

    // Wait is only supported for CDP backend
    if wait_ms > 0 {
        if let Some(mgr) = driver.as_cdp_mut() {
            mgr.wait_for_element(effective_profile_arg(cli, config), selector, wait_ms)
                .await?;
        }
        // Camoufox: snapshot refresh handles waiting implicitly
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

async fn type_text(
    cli: &Cli,
    config: &Config,
    selector: &str,
    text: &str,
    wait_ms: u64,
) -> Result<()> {
    if cli.extension {
        if cli.camofox {
            // Route through Extension Bridge with Camoufox backend
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

    // Wait is only supported for CDP backend
    if wait_ms > 0 {
        if let Some(mgr) = driver.as_cdp_mut() {
            mgr.wait_for_element(effective_profile_arg(cli, config), selector, wait_ms)
                .await?;
        }
        // Camoufox: snapshot refresh handles waiting implicitly
    }

    driver.type_text(selector, text).await?;

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

async fn fill(cli: &Cli, config: &Config, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
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

async fn select(cli: &Cli, config: &Config, selector: &str, value: &str) -> Result<()> {
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

async fn hover(cli: &Cli, config: &Config, selector: &str) -> Result<()> {
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

async fn focus(cli: &Cli, config: &Config, selector: &str) -> Result<()> {
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

async fn press(cli: &Cli, config: &Config, key: &str) -> Result<()> {
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

async fn screenshot(cli: &Cli, config: &Config, path: &str, full_page: bool) -> Result<()> {
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

async fn pdf(cli: &Cli, config: &Config, path: &str) -> Result<()> {
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

async fn eval(cli: &Cli, config: &Config, code: &str) -> Result<()> {
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

async fn html(cli: &Cli, config: &Config, selector: Option<&str>) -> Result<()> {
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

async fn text(cli: &Cli, config: &Config, selector: Option<&str>) -> Result<()> {
    if cli.extension {
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
            None => "document.body.innerText".to_string(),
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
            println!("{}", serde_json::json!({ "text": text }));
        } else {
            println!("{}", text);
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    let text = session_manager
        .get_text(effective_profile_arg(cli, config), selector)
        .await?;

    if cli.json {
        println!("{}", serde_json::json!({ "text": text }));
    } else {
        println!("{}", text);
    }

    Ok(())
}

async fn snapshot(cli: &Cli, config: &Config) -> Result<()> {
    // Build accessibility tree with proper tree structure, filtering, and refs.
    // Modeled after agent-browser's snapshot.ts output format.
    // Text nodes are captured as { role: "text", content: "..." } children.
    // Links include { url: "href" }. Inline tags use their tag name as role.
    let js = r#"
        (function() {
            // Tags to skip entirely
            const SKIP_TAGS = new Set([
                'script', 'style', 'noscript', 'template', 'svg',
                'path', 'defs', 'clippath', 'lineargradient', 'stop',
                'meta', 'link', 'br', 'wbr'
            ]);

            // Inline tags - use tag name as role
            const INLINE_TAGS = new Set([
                'strong', 'b', 'em', 'i', 'code', 'span', 'small',
                'sup', 'sub', 'abbr', 'mark', 'u', 's', 'del', 'ins',
                'time', 'q', 'cite', 'dfn', 'var', 'samp', 'kbd'
            ]);

            // Interactive roles get [ref=eN]
            const INTERACTIVE_ROLES = new Set([
                'button', 'link', 'textbox', 'checkbox', 'radio', 'combobox',
                'listbox', 'menuitem', 'menuitemcheckbox', 'menuitemradio',
                'option', 'searchbox', 'slider', 'spinbutton', 'switch',
                'tab', 'treeitem'
            ]);

            // Content roles also get refs when they have a name
            const CONTENT_ROLES = new Set([
                'heading', 'cell', 'gridcell', 'columnheader', 'rowheader',
                'listitem', 'article', 'region', 'main', 'navigation', 'img'
            ]);

            // Map HTML tags to ARIA roles
            function getRole(el) {
                const explicit = el.getAttribute('role');
                if (explicit) return explicit.toLowerCase();
                const tag = el.tagName.toLowerCase();
                // Inline tags use their tag name as role
                if (INLINE_TAGS.has(tag)) return tag;
                const roleMap = {
                    'a': el.hasAttribute('href') ? 'link' : 'generic',
                    'button': 'button',
                    'input': getInputRole(el),
                    'select': 'combobox',
                    'textarea': 'textbox',
                    'img': 'img',
                    'h1': 'heading', 'h2': 'heading', 'h3': 'heading',
                    'h4': 'heading', 'h5': 'heading', 'h6': 'heading',
                    'nav': 'navigation',
                    'main': 'main',
                    'header': 'banner',
                    'footer': 'contentinfo',
                    'aside': 'complementary',
                    'form': 'form',
                    'table': 'table',
                    'thead': 'rowgroup', 'tbody': 'rowgroup', 'tfoot': 'rowgroup',
                    'tr': 'row',
                    'th': 'columnheader',
                    'td': 'cell',
                    'ul': 'list', 'ol': 'list',
                    'li': 'listitem',
                    'details': 'group',
                    'summary': 'button',
                    'dialog': 'dialog',
                    'section': el.hasAttribute('aria-label') || el.hasAttribute('aria-labelledby') ? 'region' : 'generic',
                    'article': 'article'
                };
                return roleMap[tag] || 'generic';
            }

            function getInputRole(el) {
                const type = (el.getAttribute('type') || 'text').toLowerCase();
                const map = {
                    'text': 'textbox', 'email': 'textbox', 'password': 'textbox',
                    'search': 'searchbox', 'tel': 'textbox', 'url': 'textbox',
                    'number': 'spinbutton',
                    'checkbox': 'checkbox', 'radio': 'radio',
                    'submit': 'button', 'reset': 'button', 'button': 'button',
                    'range': 'slider'
                };
                return map[type] || 'textbox';
            }

            function getAccessibleName(el) {
                const ariaLabel = el.getAttribute('aria-label');
                if (ariaLabel) return ariaLabel.trim();

                const labelledBy = el.getAttribute('aria-labelledby');
                if (labelledBy) {
                    const label = document.getElementById(labelledBy);
                    if (label) return label.textContent?.trim()?.substring(0, 100) || '';
                }

                const tag = el.tagName.toLowerCase();
                if (tag === 'img') return el.getAttribute('alt') || '';
                if (tag === 'input' || tag === 'textarea' || tag === 'select') {
                    if (el.id) {
                        const label = document.querySelector('label[for="' + el.id + '"]');
                        if (label) return label.textContent?.trim()?.substring(0, 100) || '';
                    }
                    return el.getAttribute('placeholder') || el.getAttribute('title') || '';
                }
                if (tag === 'a' || tag === 'button' || tag === 'summary') {
                    // For links/buttons, don't use textContent as name if we'll walk childNodes
                    // Only use aria-label or explicit label
                    return '';
                }
                if (['h1','h2','h3','h4','h5','h6'].includes(tag)) {
                    return el.textContent?.trim()?.substring(0, 150) || '';
                }

                const title = el.getAttribute('title');
                if (title) return title.trim();

                return '';
            }

            function isHidden(el) {
                if (el.hidden) return true;
                if (el.getAttribute('aria-hidden') === 'true') return true;
                const style = el.style;
                if (style.display === 'none' || style.visibility === 'hidden') return true;
                if (el.offsetParent === null && el.tagName.toLowerCase() !== 'body' &&
                    getComputedStyle(el).position !== 'fixed' && getComputedStyle(el).position !== 'sticky') {
                    const cs = getComputedStyle(el);
                    if (cs.display === 'none' || cs.visibility === 'hidden') return true;
                }
                return false;
            }

            let refCounter = 0;

            function walk(el, depth) {
                if (depth > 15) return null;
                const tag = el.tagName.toLowerCase();
                if (SKIP_TAGS.has(tag)) return null;
                if (isHidden(el)) return null;

                const role = getRole(el);
                const name = getAccessibleName(el);
                const isInteractive = INTERACTIVE_ROLES.has(role);
                const isContent = CONTENT_ROLES.has(role);
                const shouldRef = isInteractive || (isContent && name);

                let ref = null;
                if (shouldRef) {
                    refCounter++;
                    ref = 'e' + refCounter;
                }

                // Collect children by walking childNodes (captures text nodes)
                const children = [];
                for (const child of el.childNodes) {
                    if (child.nodeType === 1) {
                        // Element node
                        const c = walk(child, depth + 1);
                        if (c) children.push(c);
                    } else if (child.nodeType === 3) {
                        // Text node
                        const t = child.textContent?.trim();
                        if (t) {
                            const content = t.length > 200 ? t.substring(0, 200) + '...' : t;
                            children.push({ role: 'text', content });
                        }
                    }
                }

                // Skip generic elements with no name, no ref, and only one child (pass-through)
                if (role === 'generic' && !name && !ref && children.length === 1) {
                    return children[0];
                }

                // Skip generic elements with no content at all
                if (role === 'generic' && !name && !ref && children.length === 0) {
                    return null;
                }

                // Build node info
                const node = { role };
                if (name) node.name = name;
                if (ref) node.ref = ref;
                if (children.length > 0) node.children = children;

                // URL for links
                if (role === 'link') {
                    const href = el.getAttribute('href');
                    if (href) node.url = href;
                }

                // Extra attributes
                if (role === 'heading') {
                    const level = tag.match(/^h(\d)$/);
                    if (level) node.level = parseInt(level[1]);
                }
                if (role === 'textbox' || role === 'searchbox') {
                    node.value = el.value || '';
                }
                if (role === 'checkbox' || role === 'radio' || role === 'switch') {
                    node.checked = el.checked || false;
                }

                return node;
            }

            const tree = walk(document.body, 0);
            return { tree, refCount: refCounter };
        })()
    "#;

    let value = if cli.extension {
        extension_eval(cli, js).await?
    } else {
        let session_manager = create_session_manager(cli, config);
        session_manager
            .eval_on_page(effective_profile_arg(cli, config), js)
            .await?
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        // Render tree as indented text (matching agent-browser format)
        if let Some(tree) = value.get("tree") {
            let output = render_snapshot_tree(tree, 0);
            print!("{}", output);
        } else {
            println!("(empty)");
        }
    }

    Ok(())
}

/// Render a snapshot tree node as indented text lines.
/// Output format matches agent-browser:
///   - heading "Title" [ref=e1] [level=1]
///   - button "Submit" [ref=e2]
///   - link "Home" [ref=e3]:
///     - /url: https://example.com
///     - text: Home
///   - text: Hello world
fn render_snapshot_tree(node: &serde_json::Value, depth: usize) -> String {
    let mut output = String::new();
    let indent = "  ".repeat(depth);

    let role = node
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("generic");

    // Text nodes: - text: content
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

    // Build the line: - role "name" [ref=eN] [extra]
    let mut line = format!("{}- {}", indent, role);

    if let Some(n) = name {
        line.push_str(&format!(" \"{}\"", n));
    }

    if let Some(r) = ref_id {
        line.push_str(&format!(" [ref={}]", r));
    }

    // Extra attributes
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

    // URL for links
    if let Some(u) = url {
        output.push_str(&format!("{}  - /url: {}\n", indent, u));
    }

    // Children
    if let Some(kids) = children {
        for child in kids {
            output.push_str(&render_snapshot_tree(child, depth + 1));
        }
    }

    output
}

async fn inspect(cli: &Cli, config: &Config, x: f64, y: f64, desc: Option<&str>) -> Result<()> {
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

async fn viewport(cli: &Cli, config: &Config) -> Result<()> {
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

async fn cookies(cli: &Cli, config: &Config, command: &Option<CookiesCommands>) -> Result<()> {
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

async fn scroll(
    cli: &Cli,
    config: &Config,
    direction: &crate::cli::ScrollDirection,
    smooth: bool,
) -> Result<()> {
    use crate::cli::ScrollDirection;

    let behavior = if smooth { "smooth" } else { "instant" };

    let js = match direction {
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

    // Execute scroll command
    if cli.extension {
        extension_eval(cli, &js).await?;
    } else {
        let session_manager = create_session_manager(cli, config);
        session_manager
            .eval_on_page(effective_profile_arg(cli, config), &js)
            .await?;
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

async fn close(cli: &Cli, config: &Config) -> Result<()> {
    if cli.extension {
        extension_send(cli, "Extension.detachTab", serde_json::json!({})).await?;

        if cli.json {
            println!("{}", serde_json::json!({ "success": true }));
        } else {
            println!("{} Tab detached (extension)", "✓".green());
        }
        return Ok(());
    }

    let session_manager = create_session_manager(cli, config);
    session_manager
        .close_session(effective_profile_arg(cli, config))
        .await?;

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

async fn restart(cli: &Cli, config: &Config) -> Result<()> {
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

async fn connect(cli: &Cli, config: &Config, endpoint: &str) -> Result<()> {
    let profile_name = effective_profile_name(cli, config);
    let (cdp_port, cdp_url) = resolve_cdp_endpoint(endpoint).await?;

    // Persist the session so subsequent commands can reuse this browser
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

#[cfg(test)]
mod tests {
    use super::{
        effective_profile_name, is_reusable_initial_blank_page_url, normalize_navigation_url,
        render_snapshot_tree,
    };
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
            extension: false,
            extension_port: 19222,
            verbose: false,
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

        // Verify key structural elements
        assert!(output.contains("- navigation \"Main\" [ref=e1]:"));
        assert!(output.contains("  - link \"Home\" [ref=e2]"));
        assert!(output.contains("- heading \"Welcome\" [ref=e4] [level=1]"));
        assert!(output.contains("- textbox \"Email\" [ref=e5]"));
        assert!(output.contains("- button \"Subscribe\" [ref=e6]"));

        // Verify nesting depth
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines[0].starts_with("- generic:"));
        assert!(lines[1].starts_with("  - banner:"));
    }
}
