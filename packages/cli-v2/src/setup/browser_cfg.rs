use colored::Colorize;
use dialoguer::Select;

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::config::ConfigFile;
use crate::error::CliError;
use crate::types::Mode;

/// Configure browser mode (local vs extension), executable, and headless preference.
///
/// Interactive flow:
///   1. Select mode (Local / Extension)
///   2. Mode-specific config (executable+headless for Local, extension guidance for Extension)
///
/// Respects --browser flag for non-interactive use.
pub(crate) async fn configure_browser(
    json: bool,
    env: &EnvironmentInfo,
    browser_flag: Option<Mode>,
    non_interactive: bool,
    config: &mut ConfigFile,
) -> Result<(), CliError> {
    // If flag provided, apply directly
    if let Some(mode) = browser_flag {
        return apply_browser_mode(json, env, mode, config);
    }

    // Non-interactive without flag: preserve existing config.browser.mode
    if non_interactive {
        if config.browser.mode == Mode::Local {
            if let Some(browser) = env.browsers.first() {
                if config.browser.executable_path.is_none() {
                    config.browser.executable_path = Some(browser.path.display().to_string());
                }
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "browser",
                            "mode": "local",
                            "browser": browser.name,
                            "headless": config.browser.headless,
                        })
                    );
                } else {
                    println!(
                        "  {}  Using local mode with: {}",
                        "◇".green(),
                        browser.name
                    );
                }
            } else {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "browser",
                            "mode": "local",
                            "headless": config.browser.headless,
                        })
                    );
                } else {
                    println!(
                        "  {}  No system browser detected, using local mode",
                        "◇".green()
                    );
                }
            }
        } else {
            // Extension mode: no additional setup needed in non-interactive
            if json {
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
    let mode = select_browser_mode(json)?;
    config.browser.mode = mode;

    match mode {
        Mode::Local => configure_local(json, env, config)?,
        Mode::Extension => {
            // TODO: Implement extension installer integration
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "extension",
                    })
                );
            } else {
                println!(
                    "  {}  Extension mode selected. Install the extension from Chrome Web Store.",
                    "◇".green()
                );
            }
        }
        Mode::Cloud => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "cloud",
                    })
                );
            } else {
                println!("  {}  Cloud mode selected.", "◇".green());
            }
        }
    }

    Ok(())
}

/// Interactive prompt to select browser mode.
fn select_browser_mode(json: bool) -> Result<Mode, CliError> {
    let options = vec![
        "local      — Launch dedicated browser (clean environment, no setup needed)",
        "extension  — Control your existing Chrome (install from Chrome Web Store)",
        "cloud      — Connect to a remote cloud browser",
    ];

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Browser Mode")
        .items(&options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {}", e)))?;

    let mode = match selection {
        0 => Mode::Local,
        1 => Mode::Extension,
        _ => Mode::Cloud,
    };

    if !json {
        let label = match mode {
            Mode::Local => "local",
            Mode::Extension => "extension",
            Mode::Cloud => "cloud",
        };
        println!("  {}  Mode: {}", "◇".green(), label);
    }

    Ok(mode)
}

/// Configure local mode: select browser executable + headless/visible.
fn configure_local(json: bool, env: &EnvironmentInfo, config: &mut ConfigFile) -> Result<(), CliError> {
    if env.browsers.is_empty() {
        if !json {
            println!("  {}  No Chromium-based browsers detected.", "■".yellow());
            println!(
                "  {}  Consider installing Chrome, Brave, or Edge.",
                "│".dimmed()
            );
        }
        config.browser.executable_path = None;
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
                b.name,
                ver,
                b.path.display().to_string().dimmed()
            )
        })
        .collect();

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" Select browser for local mode")
        .items(&options)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {}", e)))?;

    let browser = &env.browsers[selection];
    config.browser.executable_path = Some(browser.path.display().to_string());

    if !json {
        println!("  {}  Browser: {}", "◇".green(), browser.name);
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
        .map_err(|e| CliError::Internal(format!("Prompt failed: {}", e)))?;

    config.browser.headless = headless_selection == 1;

    if !json {
        let mode_label = if config.browser.headless {
            "headless"
        } else {
            "visible"
        };
        println!("  {}  Display: {}", "◇".green(), mode_label);
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": "local",
                "executable": config.browser.executable_path,
                "headless": config.browser.headless,
            })
        );
    }

    Ok(())
}

fn apply_browser_mode(
    json: bool,
    env: &EnvironmentInfo,
    mode: Mode,
    config: &mut ConfigFile,
) -> Result<(), CliError> {
    config.browser.mode = mode;

    match mode {
        Mode::Local => {
            if let Some(browser) = env.browsers.first() {
                if config.browser.executable_path.is_none() {
                    config.browser.executable_path = Some(browser.path.display().to_string());
                }
                if !json {
                    println!(
                        "  {}  Using local mode with: {}",
                        "◇".green(),
                        browser.name
                    );
                }
            } else if !json {
                println!("  {}  Using local mode", "◇".green());
            }
        }
        Mode::Extension => {
            if !json {
                println!("  {}  Using extension mode", "◇".green());
            }
        }
        Mode::Cloud => {
            if !json {
                println!("  {}  Using cloud mode", "◇".green());
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": format!("{}", mode),
                "executable": config.browser.executable_path,
                "headless": config.browser.headless,
            })
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env_with_browsers(browsers: Vec<super::super::detect::BrowserInfo>) -> EnvironmentInfo {
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
    fn test_apply_local_mode() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();

        let result = apply_browser_mode(false, &env, Mode::Local, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Local);
        assert!(config.browser.executable_path.is_none());
    }

    #[test]
    fn test_apply_local_mode_with_browser() {
        use std::path::PathBuf;
        let browser = super::super::detect::BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = ConfigFile::default();

        let result = apply_browser_mode(false, &env, Mode::Local, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Local);
        assert_eq!(
            config.browser.executable_path,
            Some("/usr/bin/chrome".to_string())
        );
    }

    #[test]
    fn test_apply_extension_mode() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();

        let result = apply_browser_mode(false, &env, Mode::Extension, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Extension);
    }

    #[test]
    fn test_apply_local_mode_preserves_existing_executable() {
        use std::path::PathBuf;
        let browser = super::super::detect::BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = ConfigFile::default();
        config.browser.executable_path = Some("/custom/browser".to_string());
        config.browser.headless = true;

        let result = apply_browser_mode(false, &env, Mode::Local, &mut config);
        assert!(result.is_ok());
        assert_eq!(
            config.browser.executable_path,
            Some("/custom/browser".to_string())
        );
        assert!(config.browser.headless);
    }

    #[tokio::test]
    async fn test_configure_browser_with_local_flag_no_browsers() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();

        let result =
            configure_browser(false, &env, Some(Mode::Local), false, &mut config).await;
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Local);
    }

    #[tokio::test]
    async fn test_configure_browser_with_extension_flag() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();

        let result =
            configure_browser(false, &env, Some(Mode::Extension), false, &mut config).await;
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Extension);
    }

    #[tokio::test]
    async fn test_configure_browser_non_interactive_local_no_browsers() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Local;

        let result = configure_browser(false, &env, None, true, &mut config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_configure_browser_non_interactive_local_with_browser() {
        use std::path::PathBuf;
        let browser = super::super::detect::BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Local;

        let result = configure_browser(false, &env, None, true, &mut config).await;
        assert!(result.is_ok());
        assert_eq!(
            config.browser.executable_path,
            Some("/usr/bin/chrome".to_string())
        );
    }

    #[tokio::test]
    async fn test_configure_browser_non_interactive_extension_mode() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Extension;

        let result = configure_browser(false, &env, None, true, &mut config).await;
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Extension);
    }
}
