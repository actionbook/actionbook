use dialoguer::Select;

use super::detect::{BrowserInfo, EnvironmentInfo};
use super::theme::setup_theme;
use crate::config::ConfigFile;
use crate::error::CliError;
use crate::types::Mode;

/// Configure browser mode (isolated/local vs extension), executable, and headless preference.
pub(crate) async fn configure_browser(
    json: bool,
    env: &EnvironmentInfo,
    browser_flag: Option<Mode>,
    non_interactive: bool,
    config: &mut ConfigFile,
) -> Result<(), CliError> {
    if non_interactive {
        if let Some(mode) = browser_flag {
            return apply_browser_mode(json, env, mode, config);
        }
        return apply_existing_browser_mode(json, env, config);
    }

    let mode = match browser_flag {
        Some(mode) => mode,
        None => select_browser_mode(config.browser.mode)?,
    };
    config.browser.mode = mode;

    match mode {
        Mode::Local => configure_local(json, env, config),
        Mode::Extension => {
            config.browser.executable_path = None;
            config.browser.headless = false;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "extension",
                    })
                );
            } else {
                println!("  - Browser mode: extension");
            }
            Ok(())
        }
        Mode::Cloud => Err(unsupported_setup_mode_error()),
    }
}

fn apply_existing_browser_mode(
    json: bool,
    env: &EnvironmentInfo,
    config: &mut ConfigFile,
) -> Result<(), CliError> {
    match config.browser.mode {
        Mode::Local => {
            if config.browser.executable_path.is_none()
                && let Some(browser) = env.browsers.first()
            {
                config.browser.executable_path = Some(browser.path.display().to_string());
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
            } else {
                println!("  - Browser mode: isolated");
            }
        }
        Mode::Extension => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "extension",
                    })
                );
            } else {
                println!("  - Browser mode: extension");
            }
        }
        Mode::Cloud => {
            return Err(unsupported_setup_mode_error());
        }
    }

    Ok(())
}

/// Interactive prompt to select browser mode.
fn select_browser_mode(current_mode: Mode) -> Result<Mode, CliError> {
    let options = vec![
        "isolated   Launch a dedicated browser".to_string(),
        "extension  Use your existing Chrome extension".to_string(),
    ];

    let default = if current_mode == Mode::Extension {
        1
    } else {
        0
    };
    let selection = Select::with_theme(&setup_theme())
        .with_prompt("Browser mode")
        .items(&options)
        .default(default)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    Ok(if selection == 1 {
        Mode::Extension
    } else {
        Mode::Local
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExecutableChoice {
    AutoDetect,
    Path(String),
}

/// Configure local mode: select browser executable + headless/visible.
fn configure_local(
    json: bool,
    env: &EnvironmentInfo,
    config: &mut ConfigFile,
) -> Result<(), CliError> {
    let (choices, labels, default) = browser_options(env, config.browser.executable_path.clone());

    let selection = Select::with_theme(&setup_theme())
        .with_prompt("Browser executable")
        .items(&labels)
        .default(default)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    config.browser.executable_path = match &choices[selection] {
        ExecutableChoice::AutoDetect => None,
        ExecutableChoice::Path(path) => Some(path.clone()),
    };

    let headless_options = vec!["visible".to_string(), "headless".to_string()];
    let headless_default = usize::from(config.browser.headless);
    let headless_selection = Select::with_theme(&setup_theme())
        .with_prompt("Display mode")
        .items(&headless_options)
        .default(headless_default)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    config.browser.headless = headless_selection == 1;

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
    } else {
        println!("  - Browser mode: isolated");
    }

    Ok(())
}

fn browser_options(
    env: &EnvironmentInfo,
    current_executable: Option<String>,
) -> (Vec<ExecutableChoice>, Vec<String>, usize) {
    let mut choices = vec![ExecutableChoice::AutoDetect];
    let mut labels = vec!["auto-detect at runtime".to_string()];
    let mut default = 0;

    for browser in &env.browsers {
        let path = browser.path.display().to_string();
        if current_executable.as_deref() == Some(path.as_str()) {
            default = choices.len();
        }
        choices.push(ExecutableChoice::Path(path.clone()));
        labels.push(browser_label(browser));
    }

    if let Some(current) = current_executable {
        let already_present = choices.iter().any(|choice| match choice {
            ExecutableChoice::AutoDetect => false,
            ExecutableChoice::Path(path) => path == &current,
        });
        if !already_present {
            default = choices.len();
            choices.push(ExecutableChoice::Path(current.clone()));
            labels.push(format!("configured path ({current})"));
        }
    }

    (choices, labels, default)
}

fn browser_label(browser: &BrowserInfo) -> String {
    let version = browser
        .version
        .as_deref()
        .map(|version| format!(" v{version}"))
        .unwrap_or_default();
    format!("{}{} ({})", browser.name, version, browser.path.display())
}

fn unsupported_setup_mode_error() -> CliError {
    CliError::InvalidArgument(
        "setup no longer supports browser.mode='cloud'; change it to 'local' or 'extension', or run `actionbook setup --reset`"
            .to_string(),
    )
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
            if config.browser.executable_path.is_none()
                && let Some(browser) = env.browsers.first()
            {
                config.browser.executable_path = Some(browser.path.display().to_string());
            }
        }
        Mode::Extension => {
            config.browser.executable_path = None;
            config.browser.headless = false;
        }
        Mode::Cloud => {
            return Err(unsupported_setup_mode_error());
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
    } else {
        let mode_label = if mode == Mode::Local {
            "isolated"
        } else {
            "extension"
        };
        println!("  - Browser mode: {mode_label}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

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
        let browser = BrowserInfo {
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
    fn test_apply_extension_mode_clears_local_fields() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.executable_path = Some("/usr/bin/chrome".to_string());
        config.browser.headless = true;

        let result = apply_browser_mode(false, &env, Mode::Extension, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Extension);
        assert!(config.browser.executable_path.is_none());
        assert!(!config.browser.headless);
    }

    #[test]
    fn test_apply_local_mode_preserves_existing_executable() {
        let browser = BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = ConfigFile::default();
        config.browser.executable_path = Some("/custom/chrome".to_string());

        let result = apply_browser_mode(false, &env, Mode::Local, &mut config);
        assert!(result.is_ok());
        assert_eq!(
            config.browser.executable_path,
            Some("/custom/chrome".to_string())
        );
    }

    #[test]
    fn test_apply_existing_local_mode_populates_detected_browser() {
        let browser = BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Local;
        config.browser.executable_path = None;

        let result = apply_existing_browser_mode(false, &env, &mut config);

        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Local);
        assert_eq!(
            config.browser.executable_path,
            Some("/usr/bin/chrome".to_string())
        );
    }

    #[test]
    fn test_apply_existing_cloud_mode_returns_migration_hint() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Cloud;

        let err =
            apply_existing_browser_mode(false, &env, &mut config).expect_err("cloud should fail");

        assert_eq!(err.error_code(), "INVALID_ARGUMENT");
        assert_eq!(
            err.to_string(),
            "invalid argument: setup no longer supports browser.mode='cloud'; change it to 'local' or 'extension', or run `actionbook setup --reset`"
        );
    }

    #[test]
    fn test_browser_options_include_current_executable() {
        let browser = BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);

        let (choices, _labels, default) = browser_options(&env, Some("/custom/chrome".to_string()));

        assert_eq!(default, 2);
        assert_eq!(
            choices[default],
            ExecutableChoice::Path("/custom/chrome".to_string())
        );
    }

    #[test]
    fn test_browser_options_keep_auto_detect_as_default_when_current_is_none() {
        let browser = BrowserInfo {
            name: "Google Chrome".to_string(),
            path: PathBuf::from("/usr/bin/chrome"),
            version: Some("131.0".to_string()),
        };
        let env = make_env_with_browsers(vec![browser]);

        let (choices, _labels, default) = browser_options(&env, None);

        assert_eq!(default, 0);
        assert_eq!(choices[default], ExecutableChoice::AutoDetect);
    }
}
