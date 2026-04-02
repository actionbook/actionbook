use dialoguer::Select;

use super::detect::{BrowserInfo, EnvironmentInfo};
use super::theme::setup_theme;
use crate::config::ConfigFile;
use crate::error::CliError;
use crate::types::Mode;

/// Configure browser mode (local vs cloud), executable, and headless preference.
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
        Mode::Cloud => configure_cloud(json, config),
        Mode::Extension => Err(unsupported_setup_mode_error("extension")),
    }
}

fn configure_cloud(json: bool, config: &mut ConfigFile) -> Result<(), CliError> {
    config.browser.executable_path = None;
    config.browser.headless = false;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": "cloud",
                "cdp_endpoint": config.browser.cdp_endpoint,
            })
        );
    } else {
        println!("  - Browser mode: cloud");
    }

    Ok(())
}

fn cloud_display_label(config: &ConfigFile) -> String {
    match config.browser.cdp_endpoint.as_deref() {
        Some(endpoint) => format!("cloud ({endpoint})"),
        None => "cloud (endpoint not configured)".to_string(),
    }
}

fn unsupported_setup_mode_error(mode: &str) -> CliError {
    CliError::InvalidArgument(format!(
        "setup does not currently allow browser.mode='{mode}'; use 'local' or 'cloud'"
    ))
}

fn print_extension_coming_soon_hint() {
    println!("  - extension  Coming soon");
}

fn browser_mode_options() -> Vec<String> {
    vec![
        "local      Launch a dedicated browser".to_string(),
        "cloud      Connect to a remote CDP browser".to_string(),
    ]
}

fn browser_mode_default(current_mode: Mode) -> usize {
    if current_mode == Mode::Cloud { 1 } else { 0 }
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
            config.browser.cdp_endpoint = None;
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
                println!("  - Browser mode: local");
            }
        }
        Mode::Cloud => {
            config.browser.executable_path = None;
            config.browser.headless = false;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "cloud",
                        "cdp_endpoint": config.browser.cdp_endpoint,
                    })
                );
            } else {
                println!("  - Browser mode: cloud");
            }
        }
        Mode::Extension => {
            return Err(unsupported_setup_mode_error("extension"));
        }
    }

    Ok(())
}

/// Interactive prompt to select browser mode.
fn select_browser_mode(current_mode: Mode) -> Result<Mode, CliError> {
    print_extension_coming_soon_hint();
    let options = browser_mode_options();

    let default = browser_mode_default(current_mode);
    let selection = Select::with_theme(&setup_theme())
        .with_prompt("Browser mode")
        .items(&options)
        .default(default)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    Ok(if selection == 0 {
        Mode::Local
    } else {
        Mode::Cloud
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
    config.browser.cdp_endpoint = None;

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
        println!("  - Browser mode: local");
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
            config.browser.cdp_endpoint = None;
        }
        Mode::Cloud => {
            config.browser.executable_path = None;
            config.browser.headless = false;
        }
        Mode::Extension => {
            return Err(unsupported_setup_mode_error("extension"));
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
                "cdp_endpoint": config.browser.cdp_endpoint,
            })
        );
    } else {
        let mode_label = match mode {
            Mode::Local => "local".to_string(),
            Mode::Cloud => cloud_display_label(config),
            Mode::Extension => unreachable!("extension is rejected for setup"),
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
    fn test_apply_cloud_mode_clears_local_fields() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.executable_path = Some("/usr/bin/chrome".to_string());
        config.browser.headless = true;
        config.browser.cdp_endpoint = Some("wss://browser.example.com".to_string());

        let result = apply_browser_mode(false, &env, Mode::Cloud, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Cloud);
        assert!(config.browser.executable_path.is_none());
        assert!(!config.browser.headless);
        assert_eq!(
            config.browser.cdp_endpoint.as_deref(),
            Some("wss://browser.example.com")
        );
    }

    #[test]
    fn test_apply_local_mode_clears_cloud_endpoint() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.cdp_endpoint = Some("wss://browser.example.com".to_string());

        let result = apply_browser_mode(false, &env, Mode::Local, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Local);
        assert!(config.browser.cdp_endpoint.is_none());
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
    fn test_apply_existing_cloud_mode_succeeds() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Cloud;
        config.browser.cdp_endpoint = Some("wss://browser.example.com".to_string());

        let result = apply_existing_browser_mode(false, &env, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Cloud);
        assert_eq!(
            config.browser.cdp_endpoint.as_deref(),
            Some("wss://browser.example.com")
        );
    }

    #[test]
    fn test_apply_existing_extension_mode_returns_hint() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Extension;

        let err = apply_existing_browser_mode(false, &env, &mut config)
            .expect_err("extension should fail");

        assert_eq!(err.error_code(), "INVALID_ARGUMENT");
        assert_eq!(
            err.to_string(),
            "invalid argument: setup does not currently allow browser.mode='extension'; use 'local' or 'cloud'"
        );
    }

    #[test]
    fn test_browser_mode_options_include_local_and_cloud_only() {
        let options = browser_mode_options();
        assert_eq!(options.len(), 2);
        assert!(options[0].starts_with("local"));
        assert!(options[1].starts_with("cloud"));
    }

    #[test]
    fn test_browser_mode_default_prefers_cloud_when_configured() {
        assert_eq!(browser_mode_default(Mode::Cloud), 1);
        assert_eq!(browser_mode_default(Mode::Local), 0);
        assert_eq!(browser_mode_default(Mode::Extension), 0);
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
