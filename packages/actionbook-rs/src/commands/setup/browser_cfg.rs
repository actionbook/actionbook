use colored::Colorize;
use dialoguer::Select;

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::cli::{BrowserMode, Cli};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Configure the browser mode (system vs builtin) and headless preference.
///
/// When browsers are detected, offers the user a choice.
/// Respects --browser flag for non-interactive use.
pub fn configure_browser(
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

    // Non-interactive without flag: use best detected browser or keep current
    if non_interactive {
        if let Some(browser) = env.browsers.first() {
            config.browser.executable = Some(browser.path.display().to_string());
            config.browser.headless = true;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "system",
                        "browser": browser.browser_type.name(),
                        "headless": true,
                    })
                );
            } else {
                println!(
                    "  {}  Using system browser: {}",
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
                        "mode": "builtin",
                        "headless": true,
                    })
                );
            } else {
                println!(
                    "  {}  No system browser detected, using built-in",
                    "◇".green()
                );
            }
        }
        return Ok(());
    }

    // Interactive mode
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

    // Build selection options
    let mut options: Vec<String> = env
        .browsers
        .iter()
        .map(|b| {
            let ver = b
                .version
                .as_deref()
                .map(|v| format!(" v{}", v))
                .unwrap_or_default();
            format!("{}{} (detected)", b.browser_type.name(), ver)
        })
        .collect();
    options.push("Built-in (recommended for agents)".to_string());

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Select browser")
        .items(&options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    if selection < env.browsers.len() {
        let browser = &env.browsers[selection];
        config.browser.executable = Some(browser.path.display().to_string());
        if !cli.json {
            println!(
                "  {}  Browser: {}",
                "◇".green(),
                browser.browser_type.name()
            );
        }
    } else {
        config.browser.executable = None;
        if !cli.json {
            println!("  {}  Browser: Built-in", "◇".green());
        }
    }

    let headless_options = vec![
        "Headless — no window, ideal for automation",
        "Visible — opens a browser window you can see",
    ];
    let headless_selection = Select::with_theme(&setup_theme())
        .with_prompt(" Display mode")
        .items(&headless_options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    config.browser.headless = headless_selection == 0;

    if !cli.json {
        let mode_label = if config.browser.headless {
            "Headless"
        } else {
            "Visible"
        };
        println!("  {}  Display: {}", "◇".green(), mode_label);
    }

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": if config.browser.executable.is_some() { "system" } else { "builtin" },
                "executable": config.browser.executable,
                "headless": config.browser.headless,
            })
        );
    }

    Ok(())
}

fn apply_browser_mode(
    cli: &Cli,
    env: &EnvironmentInfo,
    mode: BrowserMode,
    config: &mut Config,
) -> Result<()> {
    match mode {
        BrowserMode::System => {
            if let Some(browser) = env.browsers.first() {
                config.browser.executable = Some(browser.path.display().to_string());
                if !cli.json {
                    println!(
                        "  {}  Using system browser: {}",
                        "◇".green(),
                        browser.browser_type.name()
                    );
                }
            } else {
                return Err(ActionbookError::SetupError(
                    "No system browser detected. Install Chrome, Brave, or Edge.".to_string(),
                ));
            }
        }
        BrowserMode::Builtin => {
            config.browser.executable = None;
            if !cli.json {
                println!("  {}  Using built-in browser", "◇".green());
            }
        }
    }

    // Default to headless when using flags (agent scenario)
    config.browser.headless = true;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": format!("{:?}", mode).to_lowercase(),
                "executable": config.browser.executable,
                "headless": config.browser.headless,
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

    #[test]
    fn test_apply_builtin_mode() {
        let cli = Cli {
            browser_path: None,
            cdp: None,
            profile: None,
            headless: false,
            stealth: false,
            stealth_os: None,
            stealth_gpu: None,
            api_key: None,
            json: false,
            verbose: false,
            command: crate::cli::Commands::Config {
                command: crate::cli::ConfigCommands::Show,
            },
        };
        let env = make_env_with_browsers(vec![]);
        let mut config = Config::default();

        let result = apply_browser_mode(&cli, &env, BrowserMode::Builtin, &mut config);
        assert!(result.is_ok());
        assert!(config.browser.executable.is_none());
        assert!(config.browser.headless);
    }

    #[test]
    fn test_apply_system_mode_no_browser() {
        let cli = Cli {
            browser_path: None,
            cdp: None,
            profile: None,
            headless: false,
            stealth: false,
            stealth_os: None,
            stealth_gpu: None,
            api_key: None,
            json: false,
            verbose: false,
            command: crate::cli::Commands::Config {
                command: crate::cli::ConfigCommands::Show,
            },
        };
        let env = make_env_with_browsers(vec![]);
        let mut config = Config::default();

        let result = apply_browser_mode(&cli, &env, BrowserMode::System, &mut config);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_system_mode_with_browser() {
        let cli = Cli {
            browser_path: None,
            cdp: None,
            profile: None,
            headless: false,
            stealth: false,
            stealth_os: None,
            stealth_gpu: None,
            api_key: None,
            json: false,
            verbose: false,
            command: crate::cli::Commands::Config {
                command: crate::cli::ConfigCommands::Show,
            },
        };
        let browser = BrowserInfo {
            browser_type: BrowserType::Chrome,
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = Config::default();

        let result = apply_browser_mode(&cli, &env, BrowserMode::System, &mut config);
        assert!(result.is_ok());
        assert_eq!(
            config.browser.executable,
            Some("/usr/bin/chrome".to_string())
        );
        assert!(config.browser.headless);
    }
}
