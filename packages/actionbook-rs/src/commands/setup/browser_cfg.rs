use colored::Colorize;
use dialoguer::Select;

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::browser::extension_installer;
use crate::cli::{BrowserMode, Cli};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

const CHROME_WEB_STORE_URL: &str =
    "https://chromewebstore.google.com/detail/actionbook/bebchpafpemheedhcdabookaifcijmfo";

/// Configure browser mode (isolated vs extension), executable, and headless preference.
///
/// Interactive flow:
///   1. Select mode (Isolated / Extension)
///   2. Mode-specific config (executable+headless for Isolated, extension guidance for Extension)
///
/// Respects --browser flag for non-interactive use.
pub async fn configure_browser(
    cli: &Cli,
    env: &EnvironmentInfo,
    browser_flag: Option<BrowserMode>,
    non_interactive: bool,
    config: &mut Config,
) -> Result<()> {
    // If flag provided, apply directly
    if let Some(mode) = browser_flag {
        return apply_browser_mode(cli, env, mode, config);
    }

    // Non-interactive without flag: preserve existing config.browser.mode
    if non_interactive {
        if config.browser.mode == BrowserMode::Isolated {
            if let Some(browser) = env.browsers.first() {
                if config.browser.executable.is_none() {
                    config.browser.executable = Some(browser.path.display().to_string());
                }
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "browser",
                            "mode": "isolated",
                            "browser": browser.browser_type.name(),
                            "headless": config.browser.headless,
                        })
                    );
                } else {
                    println!(
                        "  {}  Using isolated mode with: {}",
                        "◇".green(),
                        browser.browser_type.name()
                    );
                }
            } else {
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "browser",
                            "mode": "isolated",
                            "headless": config.browser.headless,
                        })
                    );
                } else {
                    println!(
                        "  {}  No system browser detected, using isolated mode",
                        "◇".green()
                    );
                }
            }
        } else {
            // Extension mode: no additional setup needed in non-interactive
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "extension",
                        "recommended_install_source": "chrome_web_store",
                        "web_store_url": CHROME_WEB_STORE_URL,
                        "fallback_auto_install": config.browser.extension.auto_install,
                    })
                );
            } else {
                println!("  {}  Using extension mode", "◇".green());
                println!(
                    "  {}  Install extension from Chrome Web Store: {}",
                    "│".dimmed(),
                    CHROME_WEB_STORE_URL.cyan()
                );
            }
        }
        return Ok(());
    }

    // Interactive: Step 1 — select browser mode
    let mode = select_browser_mode(cli)?;
    config.browser.mode = mode;

    match mode {
        BrowserMode::Isolated => configure_isolated(cli, env, config)?,
        BrowserMode::Extension => configure_extension(cli, config).await?,
    }

    Ok(())
}

/// Interactive prompt to select browser mode (Isolated vs Extension).
fn select_browser_mode(cli: &Cli) -> Result<BrowserMode> {
    let options = vec![
        "isolated   — Launch dedicated browser (clean environment, no setup needed)",
        "extension  — Control your existing Chrome (install from Chrome Web Store)",
    ];

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Browser Mode")
        .items(&options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    let mode = if selection == 0 {
        BrowserMode::Isolated
    } else {
        BrowserMode::Extension
    };

    if !cli.json {
        let label = match mode {
            BrowserMode::Extension => "extension",
            BrowserMode::Isolated => "isolated",
        };
        println!("  {}  Mode: {}", "◇".green(), label);
    }

    Ok(mode)
}

/// Configure isolated mode: select browser executable + headless/visible.
fn configure_isolated(cli: &Cli, env: &EnvironmentInfo, config: &mut Config) -> Result<()> {
    if env.browsers.is_empty() {
        if !cli.json {
            println!("  {}  No Chromium-based browsers detected.", "■".yellow());
            println!(
                "  {}  Consider installing Chrome, Brave, or Edge.",
                "│".dimmed()
            );
        }
        config.browser.executable = None;
        return Ok(());
    }

    let options: Vec<String> = env
        .browsers
        .iter()
        .map(|b| {
            let ver = b
                .version
                .as_deref()
                .map(|v| format!(" v{}", v))
                .unwrap_or_default();
            format!(
                "{}{} — {}",
                b.browser_type.name(),
                ver,
                b.path.display().to_string().dimmed().to_string()
            )
        })
        .collect();

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Select browser for isolated mode")
        .items(&options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    let browser = &env.browsers[selection];
    config.browser.executable = Some(browser.path.display().to_string());

    if !cli.json {
        println!(
            "  {}  Browser: {}",
            "◇".green(),
            browser.browser_type.name()
        );
    }

    let headless_options = vec![
        "visible   — Shows browser window (recommended for debugging)",
        "headless  — No window, runs in background (recommended for automation)",
    ];
    let headless_selection = Select::with_theme(&setup_theme())
        .with_prompt(" Display Mode")
        .items(&headless_options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    config.browser.headless = headless_selection == 1;

    if !cli.json {
        let mode_label = if config.browser.headless {
            "headless"
        } else {
            "visible"
        };
        println!("  {}  Display: {}", "◇".green(), mode_label);
    }

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": "isolated",
                "executable": config.browser.executable,
                "headless": config.browser.headless,
            })
        );
    }

    Ok(())
}

/// Configure extension mode:
/// - Prefer Chrome Web Store install
/// - Use local debug install as fallback when needed
async fn configure_extension(cli: &Cli, config: &mut Config) -> Result<()> {
    let ext_dir = extension_installer::extension_dir()?;
    let installed_local = extension_installer::is_installed();

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": "extension",
                "recommended_install_source": "chrome_web_store",
                "web_store_url": CHROME_WEB_STORE_URL,
                "local_fallback_installed": installed_local,
                "fallback_auto_install": config.browser.extension.auto_install,
                "extension_port": config.browser.extension.port,
            })
        );
        return Ok(());
    }

    println!("  {}", "│".dimmed());
    println!(
        "  {}  {}",
        "│".dimmed(),
        "Install Actionbook extension from Chrome Web Store (recommended):".dimmed()
    );
    println!(
        "  {}  1. Open {} in Chrome",
        "│".dimmed(),
        CHROME_WEB_STORE_URL.cyan()
    );
    println!(
        "  {}  2. Click \"Add to Chrome\" → \"Add extension\"",
        "│".dimmed()
    );
    println!(
        "  {}  3. Keep Chrome open and run {}",
        "│".dimmed(),
        "actionbook browser open https://example.com".cyan()
    );

    let web_store_installed =
        select_yes_no(" Installed from Chrome Web Store successfully?", true)?;

    if web_store_installed {
        println!(
            "  {}  Great. Extension mode will use your Chrome extension directly.",
            "◇".green()
        );
        return Ok(());
    }

    println!(
        "  {}  Web Store install failed. Falling back to local debug install.",
        "■".yellow()
    );

    if config.browser.extension.auto_install {
        if installed_local {
            println!(
                "  {}  auto_install=true, local fallback is already installed.",
                "◇".green()
            );
        } else {
            println!(
                "  {}  auto_install=true, running fallback: {}",
                "⏳".yellow(),
                "actionbook extension install".cyan()
            );

            match extension_installer::download_and_install(false).await {
                Ok(version) => {
                    println!(
                        "  {}  Fallback extension v{} installed at {}",
                        "✓".green(),
                        version,
                        ext_dir.display()
                    );
                }
                Err(e) => {
                    println!("  {}  Fallback install failed: {}", "✗".red(), e);
                    println!(
                        "  {}  Please run {} manually",
                        "→".yellow(),
                        "actionbook extension install --force".cyan()
                    );
                }
            }
        }
    } else {
        println!(
            "  {}  auto_install=false, skipping automatic fallback install.",
            "◇".yellow()
        );
        println!(
            "  {}  Run {} if you want local debug fallback.",
            "→".yellow(),
            "actionbook extension install".cyan()
        );
    }

    if installed_local || extension_installer::is_installed() {
        if let Some(version) = extension_installer::installed_version() {
            println!(
                "  {}  Local fallback extension v{} available at {}",
                "◇".green(),
                version,
                ext_dir.display()
            );
        } else {
            println!(
                "  {}  Local fallback extension available at {}",
                "◇".green(),
                ext_dir.display()
            );
        }
    }

    println!("  {}", "│".dimmed());
    println!(
        "  {}  {}",
        "│".dimmed(),
        "Fallback (local debug) setup:".dimmed()
    );
    println!(
        "  {}  1. Open {} in Chrome",
        "│".dimmed(),
        "chrome://extensions".cyan()
    );
    println!(
        "  {}  2. Enable \"Developer mode\" (top right toggle)",
        "│".dimmed()
    );
    println!(
        "  {}  3. Click \"Load unpacked\" → select {}",
        "│".dimmed(),
        ext_dir.display().to_string().cyan()
    );
    println!(
        "  {}  4. Run {} to verify connection",
        "│".dimmed(),
        "actionbook browser open https://example.com".cyan()
    );

    let fallback_ready =
        select_yes_no(" Completed local debug fallback setup successfully?", true)?;

    if !fallback_ready {
        println!();
        println!("  {}", "✖ Extension setup incomplete".red().bold());
        println!();
        println!("  Actionbook could not finish installing the Chrome extension.");
        println!();
        println!("  You can try one of the following options:");
        println!();
        println!("  1) Retry actionbook setup using isolated browser mode (recommended)");
        println!();
        println!("     This runs Actionbook in a separate browser environment.");
        println!();
        println!("  2) Contact us on Discord");
        println!();
        println!("     {}", "https://actionbook.dev/discord".cyan());

        return Err(ActionbookError::SetupError(
            "Extension setup incomplete".to_string(),
        ));
    }

    Ok(())
}

/// Visible yes/no picker to avoid ambiguous TTY behavior in Confirm input mode.
fn select_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let options = vec!["yes", "no"];
    let default = if default_yes { 0 } else { 1 };

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(prompt)
        .items(&options)
        .default(default)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    Ok(selection == 0)
}

fn apply_browser_mode(
    cli: &Cli,
    env: &EnvironmentInfo,
    mode: BrowserMode,
    config: &mut Config,
) -> Result<()> {
    config.browser.mode = mode;

    match mode {
        BrowserMode::Isolated => {
            if let Some(browser) = env.browsers.first() {
                if config.browser.executable.is_none() {
                    config.browser.executable = Some(browser.path.display().to_string());
                }
                if !cli.json {
                    println!(
                        "  {}  Using isolated mode with: {}",
                        "◇".green(),
                        browser.browser_type.name()
                    );
                }
            } else if !cli.json {
                println!("  {}  Using isolated mode", "◇".green());
            }
        }
        BrowserMode::Extension => {
            if !cli.json {
                println!("  {}  Using extension mode", "◇".green());
            }
        }
    }

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": format!("{:?}", mode).to_lowercase(),
                "executable": config.browser.executable,
                "headless": config.browser.headless,
                "extension_port": config.browser.extension.port,
            })
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{BrowserInfo, BrowserType};
    use std::path::PathBuf;

    fn make_env_with_browsers(browsers: Vec<BrowserInfo>) -> EnvironmentInfo {
        EnvironmentInfo {
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            shell: None,
            browsers,
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: None,
        }
    }

    fn make_test_cli() -> Cli {
        Cli {
            profile: None,
            api_key: None,
            json: false,
            command: crate::cli::Commands::Config {
                command: crate::cli::ConfigCommands::Show,
            },
        }
    }

    #[test]
    fn test_apply_isolated_mode() {
        let cli = make_test_cli();
        let env = make_env_with_browsers(vec![]);
        let mut config = Config::default();

        let result = apply_browser_mode(&cli, &env, BrowserMode::Isolated, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, BrowserMode::Isolated);
        assert!(config.browser.executable.is_none());
        // headless is preserved from config (default: false)
        assert!(!config.browser.headless);
    }

    #[test]
    fn test_apply_isolated_mode_with_browser() {
        let cli = make_test_cli();
        let browser = BrowserInfo {
            browser_type: BrowserType::Chrome,
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = Config::default();

        let result = apply_browser_mode(&cli, &env, BrowserMode::Isolated, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, BrowserMode::Isolated);
        assert_eq!(
            config.browser.executable,
            Some("/usr/bin/chrome".to_string())
        );
        // headless is preserved from config (default: false)
        assert!(!config.browser.headless);
    }

    #[test]
    fn test_apply_isolated_mode_preserves_existing_executable() {
        let cli = make_test_cli();
        let browser = BrowserInfo {
            browser_type: BrowserType::Chrome,
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = Config::default();
        config.browser.executable = Some("/custom/browser".to_string());
        config.browser.headless = true;

        let result = apply_browser_mode(&cli, &env, BrowserMode::Isolated, &mut config);
        assert!(result.is_ok());
        // Existing executable should be preserved
        assert_eq!(
            config.browser.executable,
            Some("/custom/browser".to_string())
        );
        // Existing headless setting should be preserved
        assert!(config.browser.headless);
    }

    #[test]
    fn test_apply_extension_mode() {
        let cli = make_test_cli();
        let env = make_env_with_browsers(vec![]);
        let mut config = Config::default();

        let result = apply_browser_mode(&cli, &env, BrowserMode::Extension, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, BrowserMode::Extension);
        assert_eq!(config.browser.extension.port, 19222);
    }
}
