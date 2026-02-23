use colored::Colorize;
use dialoguer::Select;

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::browser::extension_installer;
use crate::cli::{BrowserMode, Cli};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

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
    // (already loaded from existing config or defaults to Isolated)
    if non_interactive {
        // For isolated mode, ensure executable and headless are set
        if config.browser.mode == BrowserMode::Isolated {
            if let Some(browser) = env.browsers.first() {
                config.browser.executable = Some(browser.path.display().to_string());
                config.browser.headless = true;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "browser",
                            "mode": "isolated",
                            "browser": browser.browser_type.name(),
                            "headless": true,
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
                config.browser.executable = None;
                config.browser.headless = true;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "browser",
                            "mode": "isolated",
                            "headless": true,
                        })
                    );
                } else {
                    println!(
                        "  {}  No system browser detected, using isolated mode with built-in",
                        "◇".green()
                    );
                }
            }
        } else {
            // Extension mode: no additional setup needed in non-interactive
            // (bridge port and auto_install already have defaults from ExtensionConfig::default())
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "extension",
                    })
                );
            } else {
                println!("  {}  Using extension mode", "◇".green());
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
        "extension  — Control your existing Chrome (requires extension install)",
    ];

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Browser Mode")
        .items(&options)
        .default(0) // Default to isolated (recommended for most users)
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

    // Show all detected browsers for user to choose
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
        .default(0) // Default to first detected browser (highest priority)
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

    // Headless selection (default: visible — most users want to see what's happening)
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

/// Configure extension mode: auto-install if needed, show setup guidance.
async fn configure_extension(cli: &Cli, config: &mut Config) -> Result<()> {
    let ext_dir = extension_installer::extension_dir()?;
    let installed = extension_installer::is_installed();

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": "extension",
                "extension_installed": installed,
                "extension_port": config.browser.extension.port,
            })
        );
        return Ok(());
    }

    // Auto-install extension if not present and auto_install is enabled
    if !installed && config.browser.extension.auto_install {
        println!(
            "  {}  Extension not found. Downloading from GitHub...",
            "⏳".yellow()
        );

        match extension_installer::download_and_install(false).await {
            Ok(version) => {
                println!(
                    "  {}  Extension v{} installed at {}",
                    "✓".green(),
                    version,
                    ext_dir.display()
                );
            }
            Err(e) => {
                println!(
                    "  {}  Failed to auto-install extension: {}",
                    "✗".red(),
                    e
                );
                println!(
                    "  {}  Please run {} manually",
                    "→".yellow(),
                    "actionbook extension install".cyan()
                );
            }
        }
    } else if installed {
        if let Some(version) = extension_installer::installed_version() {
            println!(
                "  {}  Extension v{} installed at {}",
                "◇".green(),
                version,
                ext_dir.display()
            );
        } else {
            println!(
                "  {}  Extension installed at {}",
                "◇".green(),
                ext_dir.display()
            );
        }
    } else {
        println!(
            "  {}  Extension not found. Run {} to install.",
            "■".yellow(),
            "actionbook extension install".cyan()
        );
    }

    println!("  {}", "│".dimmed());
    println!(
        "  {}  {}",
        "│".dimmed(),
        "To use extension mode:".dimmed()
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
        "  {}  4. The extension auto-connects when you run browser commands",
        "│".dimmed()
    );

    Ok(())
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
                config.browser.executable = Some(browser.path.display().to_string());
                if !cli.json {
                    println!(
                        "  {}  Using isolated mode with: {}",
                        "◇".green(),
                        browser.browser_type.name()
                    );
                }
            } else {
                config.browser.executable = None;
                if !cli.json {
                    println!("  {}  Using isolated mode with built-in browser", "◇".green());
                }
            }
            // Default to headless when using flags (agent scenario)
            config.browser.headless = true;
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
            browser_path: None,
            cdp: None,
            profile: None,
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
            camofox: false,
            camofox_port: None,
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
        assert!(config.browser.headless);
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
