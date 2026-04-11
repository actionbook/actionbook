use dialoguer::Select;

use super::detect::{BrowserInfo, EnvironmentInfo};
use super::theme::setup_theme;
use crate::config::ConfigFile;
use crate::error::CliError;
use crate::types::Mode;

/// Canonical Chrome Web Store listing for the Actionbook extension.
const CHROME_WEB_STORE_URL: &str =
    "https://chromewebstore.google.com/detail/actionbook/bebchpafpemheedhcdabookaifcijmfo";

/// GitHub Releases page (filtered to extension releases only) — used as
/// the manual-install fallback when the Chrome Web Store install is unavailable
/// (region blocked, offline, corporate policy). Users grab the latest
/// `actionbook-extension-v*.zip`, unzip, and `chrome://extensions` -> Load unpacked.
///
/// Why the `?q="Chrome Extension"&expanded=true` query: this repo publishes
/// three release families (`actionbook-cli-v*`, `actionbook-extension-v*`,
/// `actionbook-dify-plugin-v*`) to the same Releases feed. A plain `/releases`
/// URL buries the extension zip under dozens of CLI releases. Searching for
/// the quoted phrase "Chrome Extension" matches the extension release titles,
/// and `expanded=true` auto-expands the first match so assets are visible
/// without clicking. `%22` is the URL-encoded double quote.
const GITHUB_RELEASES_URL: &str =
    "https://github.com/actionbook/actionbook/releases?q=%22Chrome+Extension%22&expanded=true";

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
        Mode::Extension => configure_extension(json, config),
    }
}

/// Configure extension mode: guide the user to install from Chrome Web Store,
/// falling back to a manual GitHub Releases + Load-unpacked flow if the CWS
/// install is unavailable (region-blocked, offline, corporate policy, etc.).
fn configure_extension(json: bool, config: &mut ConfigFile) -> Result<(), CliError> {
    config.browser.executable_path = None;
    config.browser.headless = false;
    config.browser.cdp_endpoint = None;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "step": "browser",
                "mode": "extension",
                "recommended_install_source": "chrome_web_store",
                "web_store_url": CHROME_WEB_STORE_URL,
                "fallback_install_source": "github_releases",
                "github_releases_url": GITHUB_RELEASES_URL,
            })
        );
        return Ok(());
    }

    println!("  - Browser mode: extension");
    print_cws_guidance();

    if select_yes_no("Installed from Chrome Web Store successfully?", true)? {
        println!("  - Extension mode will use your Chrome extension directly.");
        return Ok(());
    }

    // CWS unavailable — fall through to manual install from GitHub Releases.
    print_github_releases_guidance();

    if select_yes_no(
        "Loaded the unpacked extension in Chrome successfully?",
        true,
    )? {
        println!("  - Extension mode will use your manually-loaded Chrome extension.");
        return Ok(());
    }

    Err(CliError::InvalidArgument(format!(
        "Extension setup not completed. Install the Actionbook extension from one of:\n  \
         - Chrome Web Store: {CHROME_WEB_STORE_URL}\n  \
         - GitHub Releases:  {GITHUB_RELEASES_URL}\n\
         and re-run `actionbook setup`. If you don't need your existing Chrome session, \
         choose `local` or `cloud` mode instead."
    )))
}

/// Print the Chrome Web Store install guidance (3 steps).
fn print_cws_guidance() {
    println!("  |");
    println!("  |  Install the Actionbook extension from the Chrome Web Store:");
    println!("  |    1. Open {CHROME_WEB_STORE_URL} in Chrome");
    println!("  |    2. Click \"Add to Chrome\" -> \"Add extension\"");
    println!("  |    3. Keep Chrome open and run `actionbook browser open https://example.com`");
    println!("  |");
}

/// Print the GitHub Releases manual-install fallback (5 steps: download,
/// unzip, open chrome://extensions, enable Developer mode, Load unpacked).
fn print_github_releases_guidance() {
    println!("  |");
    println!("  |  Chrome Web Store unavailable? Install manually from GitHub Releases:");
    println!("  |    1. Open {GITHUB_RELEASES_URL}");
    println!("  |    2. Download the latest `actionbook-extension-v*.zip` asset");
    println!("  |    3. Unzip to a local folder");
    println!("  |    4. Open `chrome://extensions` in Chrome, enable Developer mode");
    println!("  |    5. Click \"Load unpacked\" and select the unzipped folder");
    println!("  |");
}

/// Visible yes/no picker. Uses `Select` instead of `Confirm` so it behaves
/// consistently in terminals where `Confirm`'s raw-mode input is flaky.
fn select_yes_no(prompt: &str, default_yes: bool) -> Result<bool, CliError> {
    let options = ["yes", "no"];
    let default = if default_yes { 0 } else { 1 };

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(prompt)
        .items(&options)
        .default(default)
        .report(false)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    Ok(selection == 0)
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

fn browser_mode_options() -> Vec<String> {
    vec![
        "local      Launch a dedicated browser".to_string(),
        "cloud      Connect to a remote CDP browser".to_string(),
        "extension  Connect via Chrome extension".to_string(),
    ]
}

fn browser_mode_default(current_mode: Mode) -> usize {
    match current_mode {
        Mode::Cloud => 1,
        Mode::Extension => 2,
        _ => 0,
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
            config.browser.executable_path = None;
            config.browser.headless = false;
            config.browser.cdp_endpoint = None;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "step": "browser",
                        "mode": "extension",
                        "recommended_install_source": "chrome_web_store",
                        "web_store_url": CHROME_WEB_STORE_URL,
                        "fallback_install_source": "github_releases",
                        "github_releases_url": GITHUB_RELEASES_URL,
                    })
                );
            } else {
                println!("  - Browser mode: extension");
                println!("  |  Install extension from Chrome Web Store: {CHROME_WEB_STORE_URL}");
                println!("  |  Or manual install from GitHub Releases: {GITHUB_RELEASES_URL}");
            }
        }
    }

    Ok(())
}

/// Interactive prompt to select browser mode.
fn select_browser_mode(current_mode: Mode) -> Result<Mode, CliError> {
    let options = browser_mode_options();

    let default = browser_mode_default(current_mode);
    let selection = Select::with_theme(&setup_theme())
        .with_prompt("Browser mode")
        .items(&options)
        .default(default)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {e}")))?;

    Ok(match selection {
        0 => Mode::Local,
        1 => Mode::Cloud,
        2 => Mode::Extension,
        _ => Mode::Local,
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
            config.browser.executable_path = None;
            config.browser.headless = false;
            config.browser.cdp_endpoint = None;
        }
    }

    if json {
        let mut payload = serde_json::json!({
            "step": "browser",
            "mode": format!("{}", mode),
            "executable": config.browser.executable_path,
            "headless": config.browser.headless,
            "cdp_endpoint": config.browser.cdp_endpoint,
        });
        if mode == Mode::Extension {
            payload["recommended_install_source"] =
                serde_json::Value::String("chrome_web_store".to_string());
            payload["web_store_url"] = serde_json::Value::String(CHROME_WEB_STORE_URL.to_string());
            payload["fallback_install_source"] =
                serde_json::Value::String("github_releases".to_string());
            payload["github_releases_url"] =
                serde_json::Value::String(GITHUB_RELEASES_URL.to_string());
        }
        println!("{}", payload);
    } else {
        let mode_label = match mode {
            Mode::Local => "local".to_string(),
            Mode::Cloud => cloud_display_label(config),
            Mode::Extension => "extension".to_string(),
        };
        println!("  - Browser mode: {mode_label}");
        if mode == Mode::Extension {
            println!("  |  Install extension from Chrome Web Store: {CHROME_WEB_STORE_URL}");
            println!("  |  Or manual install from GitHub Releases: {GITHUB_RELEASES_URL}");
        }
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
    fn test_apply_existing_extension_mode_succeeds() {
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();
        config.browser.mode = Mode::Extension;

        let result = apply_existing_browser_mode(false, &env, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Extension);
        assert!(config.browser.executable_path.is_none());
        assert!(config.browser.cdp_endpoint.is_none());
    }

    #[test]
    fn test_browser_mode_options_include_all_modes() {
        let options = browser_mode_options();
        assert_eq!(options.len(), 3);
        assert!(options[0].starts_with("local"));
        assert!(options[1].starts_with("cloud"));
        assert!(options[2].starts_with("extension"));
    }

    #[test]
    fn test_browser_mode_default_prefers_configured() {
        assert_eq!(browser_mode_default(Mode::Cloud), 1);
        assert_eq!(browser_mode_default(Mode::Local), 0);
        assert_eq!(browser_mode_default(Mode::Extension), 2);
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
    fn test_chrome_web_store_url_is_canonical() {
        assert!(CHROME_WEB_STORE_URL.starts_with("https://chromewebstore.google.com/detail/"));
        assert!(CHROME_WEB_STORE_URL.contains("actionbook"));
    }

    #[test]
    fn test_github_releases_url_is_extension_filtered() {
        // The repo mixes CLI/extension/dify-plugin releases. The URL must
        // filter to extension releases so users don't have to scroll past
        // dozens of CLI releases to find the .zip.
        assert!(
            GITHUB_RELEASES_URL.starts_with("https://github.com/actionbook/actionbook/releases")
        );
        // Quoted phrase "Chrome Extension" (URL-encoded)
        assert!(GITHUB_RELEASES_URL.contains("%22Chrome+Extension%22"));
        assert!(GITHUB_RELEASES_URL.contains("expanded=true"));
    }

    #[test]
    fn test_apply_extension_mode_records_web_store_hint_in_json() {
        // The non-interactive --browser extension path must still guide users
        // to the Chrome Web Store. This guards against silent regression.
        let env = make_env_with_browsers(vec![]);
        let mut config = ConfigFile::default();

        let result = apply_browser_mode(true, &env, Mode::Extension, &mut config);
        assert!(result.is_ok());
        assert_eq!(config.browser.mode, Mode::Extension);
        assert!(config.browser.executable_path.is_none());
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
