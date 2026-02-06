use std::fs;
use std::time::Duration;

use colored::Colorize;
use futures::StreamExt;
use tokio::time::timeout;

use crate::browser::{
    discover_all_browsers, stealth_status, build_stealth_profile,
    SessionManager, SessionStatus, StealthConfig,
};
#[cfg(feature = "stealth")]
use crate::browser::apply_stealth_to_page;
use crate::cli::{BrowserCommands, Cli, CookiesCommands};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Create a SessionManager with appropriate stealth configuration from CLI flags
fn create_session_manager(cli: &Cli, config: &Config) -> SessionManager {
    if cli.stealth {
        let stealth_profile = build_stealth_profile(
            cli.stealth_os.as_deref(),
            cli.stealth_gpu.as_deref(),
        );

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

pub async fn run(cli: &Cli, command: &BrowserCommands) -> Result<()> {
    let config = Config::load()?;

    match command {
        BrowserCommands::Status => status(cli, &config).await,
        BrowserCommands::Open { url } => open(cli, &config, url).await,
        BrowserCommands::Goto { url, timeout: t } => goto(cli, &config, url, *t).await,
        BrowserCommands::Back => back(cli, &config).await,
        BrowserCommands::Forward => forward(cli, &config).await,
        BrowserCommands::Reload => reload(cli, &config).await,
        BrowserCommands::Pages => pages(cli, &config).await,
        BrowserCommands::Switch { page_id } => switch(cli, &config, page_id).await,
        BrowserCommands::Wait { selector, timeout: t } => wait(cli, &config, selector, *t).await,
        BrowserCommands::WaitNav { timeout: t } => wait_nav(cli, &config, *t).await,
        BrowserCommands::Click { selector, wait: w } => click(cli, &config, selector, *w).await,
        BrowserCommands::Type { selector, text, wait: w } => type_text(cli, &config, selector, text, *w).await,
        BrowserCommands::Fill { selector, text, wait: w } => fill(cli, &config, selector, text, *w).await,
        BrowserCommands::Select { selector, value } => select(cli, &config, selector, value).await,
        BrowserCommands::Hover { selector } => hover(cli, &config, selector).await,
        BrowserCommands::Focus { selector } => focus(cli, &config, selector).await,
        BrowserCommands::Press { key } => press(cli, &config, key).await,
        BrowserCommands::Screenshot { path, full_page } => screenshot(cli, &config, path, *full_page).await,
        BrowserCommands::Pdf { path } => pdf(cli, &config, path).await,
        BrowserCommands::Eval { code } => eval(cli, &config, code).await,
        BrowserCommands::Html { selector } => html(cli, &config, selector.as_deref()).await,
        BrowserCommands::Text { selector } => text(cli, &config, selector.as_deref()).await,
        BrowserCommands::Snapshot => snapshot(cli, &config).await,
        BrowserCommands::Inspect { x, y, desc } => inspect(cli, &config, *x, *y, desc.as_deref()).await,
        BrowserCommands::Viewport => viewport(cli, &config).await,
        BrowserCommands::Cookies { command } => cookies(cli, &config, command).await,
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
            let masked = format!("{}...{}", &key[..4], &key[key.len()-4..]);
            println!("  {} Configured ({})", "✓".green(), masked.dimmed());
        }
        Some(_) => {
            println!("  {} Configured", "✓".green());
        }
        None => {
            println!("  {} Not configured (set via --api-key or ACTIONBOOK_API_KEY)", "○".dimmed());
        }
    }
    println!();

    // Show stealth mode status
    println!("{}", "Stealth Mode:".bold());
    let stealth = stealth_status();
    if stealth.starts_with("enabled") {
        println!("  {} {}", "✓".green(), stealth);
        if cli.stealth {
            let profile = build_stealth_profile(
                cli.stealth_os.as_deref(),
                cli.stealth_gpu.as_deref(),
            );
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
    let profile_name = cli.profile.as_deref();
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
    let session_manager = create_session_manager(cli, config);
    let (browser, mut handler) = session_manager
        .get_or_create_session(cli.profile.as_deref())
        .await?;

    // Spawn handler in background
    tokio::spawn(async move {
        while handler.next().await.is_some() {}
    });

    // Navigate to URL with timeout (30 seconds for page creation)
    let page = match timeout(Duration::from_secs(30), browser.new_page(url)).await {
        Ok(Ok(page)) => page,
        Ok(Err(e)) => {
            return Err(ActionbookError::Other(format!("Failed to open page: {}", e)));
        }
        Err(_) => {
            return Err(ActionbookError::Timeout(format!(
                "Page load timed out after 30 seconds: {}",
                url
            )));
        }
    };

    // Apply stealth profile if enabled
    #[cfg(feature = "stealth")]
    if cli.stealth {
        let stealth_profile = build_stealth_profile(
            cli.stealth_os.as_deref(),
            cli.stealth_gpu.as_deref(),
        );
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
                "url": url,
                "title": title
            })
        );
    } else {
        println!("{} {}", "✓".green(), title.bold());
        println!("  {}", url.dimmed());
    }

    Ok(())
}

async fn goto(cli: &Cli, config: &Config, url: &str, _timeout_ms: u64) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    session_manager.goto(cli.profile.as_deref(), url).await?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "url": url
            })
        );
    } else {
        println!("{} Navigated to: {}", "✓".green(), url);
    }

    Ok(())
}

async fn back(cli: &Cli, config: &Config) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    session_manager.go_back(cli.profile.as_deref()).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Went back", "✓".green());
    }

    Ok(())
}

async fn forward(cli: &Cli, config: &Config) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    session_manager.go_forward(cli.profile.as_deref()).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Went forward", "✓".green());
    }

    Ok(())
}

async fn reload(cli: &Cli, config: &Config) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    session_manager.reload(cli.profile.as_deref()).await?;

    if cli.json {
        println!("{}", serde_json::json!({ "success": true }));
    } else {
        println!("{} Page reloaded", "✓".green());
    }

    Ok(())
}

async fn pages(cli: &Cli, config: &Config) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    let pages = session_manager.get_pages(cli.profile.as_deref()).await?;

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

async fn switch(_cli: &Cli, _config: &Config, page_id: &str) -> Result<()> {
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
    let session_manager = create_session_manager(cli, config);
    session_manager
        .wait_for_element(cli.profile.as_deref(), selector, timeout_ms)
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
    let session_manager = create_session_manager(cli, config);
    let new_url = session_manager
        .wait_for_navigation(cli.profile.as_deref(), timeout_ms)
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
    let session_manager = create_session_manager(cli, config);

    if wait_ms > 0 {
        session_manager
            .wait_for_element(cli.profile.as_deref(), selector, wait_ms)
            .await?;
    }

    session_manager
        .click_on_page(cli.profile.as_deref(), selector)
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
        println!("{} Clicked: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn type_text(cli: &Cli, config: &Config, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
    let session_manager = create_session_manager(cli, config);

    if wait_ms > 0 {
        session_manager
            .wait_for_element(cli.profile.as_deref(), selector, wait_ms)
            .await?;
    }

    session_manager
        .type_on_page(cli.profile.as_deref(), selector, text)
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
        println!("{} Typed into: {}", "✓".green(), selector);
    }

    Ok(())
}

async fn fill(cli: &Cli, config: &Config, selector: &str, text: &str, wait_ms: u64) -> Result<()> {
    let session_manager = create_session_manager(cli, config);

    if wait_ms > 0 {
        session_manager
            .wait_for_element(cli.profile.as_deref(), selector, wait_ms)
            .await?;
    }

    session_manager
        .fill_on_page(cli.profile.as_deref(), selector, text)
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
    let session_manager = create_session_manager(cli, config);
    session_manager
        .select_on_page(cli.profile.as_deref(), selector, value)
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
    let session_manager = create_session_manager(cli, config);
    session_manager
        .hover_on_page(cli.profile.as_deref(), selector)
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
    let session_manager = create_session_manager(cli, config);
    session_manager
        .focus_on_page(cli.profile.as_deref(), selector)
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
    let session_manager = create_session_manager(cli, config);
    session_manager
        .press_key(cli.profile.as_deref(), key)
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
    let session_manager = create_session_manager(cli, config);

    let screenshot_data = if full_page {
        session_manager
            .screenshot_full_page(cli.profile.as_deref())
            .await?
    } else {
        session_manager
            .screenshot_page(cli.profile.as_deref())
            .await?
    };

    fs::write(path, screenshot_data)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "path": path,
                "fullPage": full_page
            })
        );
    } else {
        let mode = if full_page { " (full page)" } else { "" };
        println!("{} Screenshot saved{}: {}", "✓".green(), mode, path);
    }

    Ok(())
}

async fn pdf(cli: &Cli, config: &Config, path: &str) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    let pdf_data = session_manager
        .pdf_page(cli.profile.as_deref())
        .await?;

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
    let session_manager = create_session_manager(cli, config);
    let value = session_manager
        .eval_on_page(cli.profile.as_deref(), code)
        .await?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }

    Ok(())
}

async fn html(cli: &Cli, config: &Config, selector: Option<&str>) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    let html = session_manager
        .get_html(cli.profile.as_deref(), selector)
        .await?;

    if cli.json {
        println!("{}", serde_json::json!({ "html": html }));
    } else {
        println!("{}", html);
    }

    Ok(())
}

async fn text(cli: &Cli, config: &Config, selector: Option<&str>) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    let text = session_manager
        .get_text(cli.profile.as_deref(), selector)
        .await?;

    if cli.json {
        println!("{}", serde_json::json!({ "text": text }));
    } else {
        println!("{}", text);
    }

    Ok(())
}

async fn snapshot(cli: &Cli, config: &Config) -> Result<()> {
    let session_manager = create_session_manager(cli, config);

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

    let value = session_manager
        .eval_on_page(cli.profile.as_deref(), js)
        .await?;

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

    let role = node.get("role").and_then(|v| v.as_str()).unwrap_or("generic");

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
    let session_manager = create_session_manager(cli, config);

    // Get viewport to validate coordinates
    let (vp_width, vp_height) = session_manager.get_viewport(cli.profile.as_deref()).await?;

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
        .inspect_at(cli.profile.as_deref(), x, y)
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
        let found = result.get("found").and_then(|v| v.as_bool()).unwrap_or(false);

        if !found {
            println!("{} No element found at ({}, {})", "!".yellow(), x, y);
            return Ok(());
        }

        if let Some(d) = desc {
            println!("{} Inspecting: {}\n", "🔍".cyan(), d.bold());
        }

        println!("{} ({}, {}) in {}x{} viewport\n", "📍".cyan(), x, y, vp_width, vp_height);

        // Tag and basic info
        let tag = result.get("tagName").and_then(|v| v.as_str()).unwrap_or("unknown");
        let id = result.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        let class = result.get("className").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

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
        let interactive = result.get("isInteractive").and_then(|v| v.as_bool()).unwrap_or(false);
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
        if let Some(text) = result.get("textContent").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
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
                    let ptag = parent.get("tagName").and_then(|v| v.as_str()).unwrap_or("?");
                    let pid = parent.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    let pclass = parent.get("className").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

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
    let session_manager = create_session_manager(cli, config);
    let (width, height) = session_manager.get_viewport(cli.profile.as_deref()).await?;

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
    let session_manager = create_session_manager(cli, config);

    match command {
        None | Some(CookiesCommands::List) => {
            let cookies = session_manager.get_cookies(cli.profile.as_deref()).await?;

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
                        println!("  {} = {} {}", name.bold(), value, format!("({})", domain).dimmed());
                    }
                }
            }
        }
        Some(CookiesCommands::Get { name }) => {
            let cookies = session_manager.get_cookies(cli.profile.as_deref()).await?;
            let cookie = cookies.iter().find(|c| {
                c.get("name").and_then(|v| v.as_str()) == Some(name)
            });

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
        Some(CookiesCommands::Set { name, value, domain }) => {
            session_manager
                .set_cookie(cli.profile.as_deref(), name, value, domain.as_deref())
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
                .delete_cookie(cli.profile.as_deref(), name)
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
        Some(CookiesCommands::Clear) => {
            session_manager
                .clear_cookies(cli.profile.as_deref())
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

async fn close(cli: &Cli, config: &Config) -> Result<()> {
    let session_manager = create_session_manager(cli, config);
    session_manager.close_session(cli.profile.as_deref()).await?;

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
    // Close existing session
    close(cli, config).await?;

    // Open a blank page to restart
    let session_manager = create_session_manager(cli, config);
    let (_browser, mut handler) = session_manager
        .get_or_create_session(cli.profile.as_deref())
        .await?;

    tokio::spawn(async move {
        while handler.next().await.is_some() {}
    });

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
    let profile_name = cli.profile.as_deref().unwrap_or("default");

    // Parse endpoint: either a WebSocket URL or a port number
    let (cdp_port, cdp_url) = if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        // Extract port from WebSocket URL for health checks (e.g., ws://127.0.0.1:9222/...)
        let port = endpoint
            .split("://")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .and_then(|host_port| host_port.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(9222);
        (port, endpoint.to_string())
    } else if let Ok(port) = endpoint.parse::<u16>() {
        // Validate that the CDP port is reachable
        let version_url = format!("http://127.0.0.1:{}/json/version", port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let resp = client.get(&version_url).send().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!(
                "Cannot reach CDP at port {}. Is the browser running with --remote-debugging-port={}? Error: {}",
                port, port, e
            ))
        })?;

        let version_info: serde_json::Value = resp.json().await.map_err(|e| {
            ActionbookError::CdpConnectionFailed(format!("Invalid response from CDP endpoint: {}", e))
        })?;

        // Get the browser WebSocket URL from /json/version
        let ws_url = version_info
            .get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ws://127.0.0.1:{}", port));

        (port, ws_url)
    } else {
        return Err(ActionbookError::CdpConnectionFailed(
            "Invalid endpoint. Use a port number or WebSocket URL (ws://...).".to_string(),
        ));
    };

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
    use super::render_snapshot_tree;
    use serde_json::json;

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
