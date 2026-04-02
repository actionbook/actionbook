pub mod api_key;
pub mod browser_cfg;
pub mod detect;
pub mod theme;

use std::time::Duration;

use clap::Args;
use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};

use self::theme::setup_theme;
use crate::config::{self, ConfigFile};
use crate::error::CliError;
use crate::types::Mode;

#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct Cmd {
    /// Configuration target
    #[arg(long)]
    pub target: Option<String>,

    /// API key
    #[arg(long)]
    pub api_key: Option<String>,

    /// Browser configuration (local|cloud)
    #[arg(long)]
    pub browser: Option<String>,

    /// Non-interactive mode
    #[arg(long)]
    pub non_interactive: bool,

    /// Reset configuration
    #[arg(long)]
    pub reset: bool,
}

const TOTAL_STEPS: u8 = 4;

/// Run the setup wizard. Orchestrates all steps in order.
pub async fn execute(cmd: &Cmd, json: bool) -> Result<(), CliError> {
    let non_interactive = cmd.non_interactive || json;

    // Handle existing config (re-run protection)
    let mut config = handle_existing_config(json, non_interactive, cmd.reset)?;

    // Step 1: Welcome + environment detection
    if !json {
        print_welcome();
        print_step_header(1, "Environment");
    }
    let spinner = create_spinner(json, non_interactive, "Scanning environment...");
    let env = detect::detect_environment();
    finish_spinner(spinner, "Environment detected");
    detect::print_environment_report(&env, json);
    if !json {
        print_step_connector();
    }

    let browser_flag = parse_browser_flag(cmd.browser.as_deref())?;

    // Step 2: API Key
    if !json {
        print_step_header(2, "API Key");
    }
    api_key::configure_api_key(
        json,
        &env,
        cmd.api_key.as_deref(),
        non_interactive,
        &mut config,
    )
    .await?;

    // Step 3: Browser
    if !json {
        print_step_connector();
        print_step_header(3, "Browser");
    }
    browser_cfg::configure_browser(json, &env, browser_flag, non_interactive, &mut config).await?;

    // Step 4: Save configuration
    if !json {
        print_step_connector();
        print_step_header(4, "Save");
    }

    // Show recap (interactive only)
    if !json && !non_interactive {
        let bar = "|";
        let api_display = config.api.api_key.as_deref().unwrap_or("not configured");
        let mode_display = match config.browser.mode {
            Mode::Local => {
                let browser_name = config
                    .browser
                    .executable_path
                    .as_deref()
                    .unwrap_or("auto-detect");
                let headless_label = if config.browser.headless {
                    "headless"
                } else {
                    "visible"
                };
                format!("local ({browser_name}, {headless_label})")
            }
            Mode::Cloud => config
                .browser
                .cdp_endpoint
                .as_deref()
                .map(|endpoint| format!("cloud ({endpoint})"))
                .unwrap_or_else(|| "cloud (endpoint not configured)".to_string()),
            Mode::Extension => "coming soon".to_string(),
        };

        println!("  {bar}  Configuration summary:");
        println!("  {}    API Key   {}", bar, api_display);
        println!("  {}    Browser   {}", bar, mode_display);
        println!("  {}    Path      {}", bar, config::config_path().display());
    }

    let path = config::save_config(&config)?;
    if !json {
        println!("  - Configuration saved to {}", path.display());
    }

    // TODO: Step 5: Health check (API connectivity) — requires ApiClient
    // TODO: Step 6: Install Skills — requires SetupTarget + npx skills integration

    // Completion summary
    print_completion(json, &config);

    Ok(())
}

/// Print a step header, e.g. `◆  Environment`
fn print_step_header(step: u8, title: &str) {
    println!("  * {} ({}/{})", title, step, TOTAL_STEPS);
    println!("  |");
}

/// Print a vertical connector between steps.
fn print_step_connector() {
    println!("  |");
}

/// Create a spinner with the given message. Returns `None` if in json or non-interactive mode.
fn create_spinner(json: bool, non_interactive: bool, message: &str) -> Option<ProgressBar> {
    if json || non_interactive {
        return None;
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("  │  {spinner} {msg}")
            .expect("valid spinner template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    Some(pb)
}

/// Finish a spinner with a success message.
fn finish_spinner(pb: Option<ProgressBar>, message: &str) {
    if let Some(pb) = pb {
        pb.finish_with_message(format!("  - {message}"));
    }
}

/// Handle re-run protection: detect existing config and offer choices.
fn handle_existing_config(
    json: bool,
    non_interactive: bool,
    reset: bool,
) -> Result<ConfigFile, CliError> {
    if reset {
        if !json {
            println!("  - Resetting configuration...");
        }
        return Ok(ConfigFile::default());
    }

    let config_exists = config::config_path().exists();

    if !config_exists {
        return Ok(ConfigFile::default());
    }

    // Load existing config
    let existing = config::load_config()?;

    if non_interactive {
        return Ok(existing);
    }

    if !json {
        println!("\n  - Existing configuration found\n");
    }

    let choices = vec![
        "Re-run setup (current values as defaults)",
        "Reset and start fresh",
        "Cancel",
    ];

    let selection = Select::with_theme(&setup_theme())
        .with_prompt(" What would you like to do?")
        .items(&choices)
        .default(0)
        .report(false)
        .interact()
        .map_err(|e| CliError::Internal(format!("Prompt failed: {}", e)))?;

    match selection {
        0 => Ok(existing),
        1 => {
            if !json {
                println!("  - Starting fresh...");
            }
            Ok(ConfigFile::default())
        }
        _ => Err(CliError::Internal("Setup cancelled.".to_string())),
    }
}

/// Parse --browser flag value into Mode.
fn parse_browser_flag(value: Option<&str>) -> Result<Option<Mode>, CliError> {
    let Some(value) = value else {
        return Ok(None);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(Some(Mode::Local)),
        "cloud" => Ok(Some(Mode::Cloud)),
        other => Err(CliError::InvalidArgument(format!(
            "invalid --browser value '{other}': expected local|cloud"
        ))),
    }
}

/// Return setup logo symbol. Prefer natural-join on UTF-8 terminals.
fn setup_logo_symbol() -> &'static str {
    let locale = std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LC_CTYPE").ok().filter(|v| !v.is_empty()))
        .or_else(|| std::env::var("LANG").ok().filter(|v| !v.is_empty()));

    match locale {
        Some(value) => {
            let upper = value.to_uppercase();
            if upper.contains("UTF-8") || upper.contains("UTF8") {
                "⋈"
            } else {
                "><"
            }
        }
        None => "><",
    }
}

/// Print the welcome banner.
fn print_welcome() {
    println!();
    println!("  {}  Actionbook", setup_logo_symbol());
    println!();
    println!("  +  Setup Wizard v{}", env!("CARGO_PKG_VERSION"));
    println!("  |");
}

/// Print the completion summary with next steps.
fn print_completion(json: bool, config: &ConfigFile) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "setup",
                "status": "complete",
                "config_path": config::config_path().display().to_string(),
                "browser_mode": format!("{}", config.browser.mode),
                "browser": match config.browser.mode {
                    Mode::Local => config.browser.executable_path.as_deref().unwrap_or("built-in"),
                    Mode::Cloud => config
                        .browser
                        .cdp_endpoint
                        .as_deref()
                        .unwrap_or("endpoint not configured"),
                    Mode::Extension => "coming soon",
                },
                "headless": config.browser.headless,
            })
        );
        return;
    }

    println!("  |");
    println!("  +  Setup completed.");

    // Configuration recap
    let api_display = config
        .api
        .api_key
        .as_deref()
        .unwrap_or("not configured")
        .to_string();

    let browser_display = match config.browser.mode {
        Mode::Local => {
            let name = config
                .browser
                .executable_path
                .as_deref()
                .map(shorten_browser_path)
                .unwrap_or_else(|| "built-in".to_string());
            let headless_str = if config.browser.headless {
                "headless"
            } else {
                "visible"
            };
            format!("local ({name}, {headless_str})")
        }
        Mode::Cloud => config
            .browser
            .cdp_endpoint
            .as_deref()
            .map(|endpoint| format!("cloud ({endpoint})"))
            .unwrap_or_else(|| "cloud (endpoint not configured)".to_string()),
        Mode::Extension => "coming soon".to_string(),
    };

    println!();
    println!(
        "     Config  {}",
        shorten_home_path(&config::config_path().display().to_string())
    );
    println!("     Key     {}", api_display);
    println!("     Browser {}", browser_display);

    // Next steps
    println!();
    println!("     Next steps");
    println!("       $ actionbook search \"<goal>\" --json");
    println!("       $ actionbook get \"<area_id>\" --json");
    println!();
}

/// Shorten a file path by replacing the home directory with `~`.
fn shorten_home_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if let Some(rest) = path.strip_prefix(&home_str) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

/// Extract a short browser name from a full executable path.
fn shorten_browser_path(path: &str) -> String {
    let known = [
        ("Google Chrome", "Chrome"),
        ("Chromium", "Chromium"),
        ("Brave Browser", "Brave"),
        ("Microsoft Edge", "Edge"),
        ("chrome", "Chrome"),
        ("brave", "Brave"),
        ("msedge", "Edge"),
        ("chromium", "Chromium"),
    ];
    for (pattern, short) in &known {
        if path.contains(pattern) {
            return short.to_string();
        }
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_browser_flag_accepts_supported_values() {
        assert_eq!(
            parse_browser_flag(Some("local")).unwrap(),
            Some(Mode::Local)
        );
        assert_eq!(
            parse_browser_flag(Some("cloud")).unwrap(),
            Some(Mode::Cloud)
        );
    }

    #[test]
    fn parse_browser_flag_rejects_unknown() {
        let err = parse_browser_flag(Some("invalid")).expect_err("should reject");
        assert_eq!(err.error_code(), "INVALID_ARGUMENT");
    }

    #[test]
    fn parse_browser_flag_rejects_isolated() {
        let err = parse_browser_flag(Some("isolated")).expect_err("should reject");
        assert_eq!(err.error_code(), "INVALID_ARGUMENT");
    }

    #[test]
    fn parse_browser_flag_rejects_extension() {
        let err = parse_browser_flag(Some("extension")).expect_err("should reject");
        assert_eq!(err.error_code(), "INVALID_ARGUMENT");
    }

    #[test]
    fn parse_browser_flag_none_returns_none() {
        assert_eq!(parse_browser_flag(None).unwrap(), None);
    }

    #[test]
    fn shorten_browser_path_chrome() {
        assert_eq!(
            shorten_browser_path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            "Chrome"
        );
    }

    #[test]
    fn shorten_browser_path_brave() {
        assert_eq!(
            shorten_browser_path("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
            "Brave"
        );
    }

    #[test]
    fn shorten_browser_path_edge() {
        assert_eq!(shorten_browser_path("/usr/bin/msedge"), "Edge");
    }

    #[test]
    fn shorten_browser_path_chromium() {
        assert_eq!(shorten_browser_path("/usr/bin/chromium"), "Chromium");
    }

    #[test]
    fn shorten_browser_path_fallback_to_last_component() {
        assert_eq!(shorten_browser_path("/usr/bin/firefox"), "firefox");
    }

    #[test]
    fn shorten_home_path_replaces_home() {
        if let Some(home) = dirs::home_dir() {
            let home_str = home.display().to_string();
            let path = format!("{}/some/file.toml", home_str);
            let shortened = shorten_home_path(&path);
            assert!(
                shortened.starts_with('~'),
                "Expected '~' prefix, got: {}",
                shortened
            );
        }
    }

    #[test]
    fn shorten_home_path_non_home() {
        let path = "/etc/some/config.toml";
        let shortened = shorten_home_path(path);
        assert_eq!(shortened, path);
    }

    #[test]
    fn create_spinner_returns_none_in_json_mode() {
        let pb = create_spinner(true, false, "Loading...");
        assert!(pb.is_none());
    }

    #[test]
    fn create_spinner_returns_none_in_non_interactive_mode() {
        let pb = create_spinner(false, true, "Loading...");
        assert!(pb.is_none());
    }

    #[test]
    fn finish_spinner_handles_none() {
        finish_spinner(None, "done");
    }

    #[test]
    fn setup_logo_symbol_returns_nonempty() {
        let symbol = setup_logo_symbol();
        assert!(!symbol.is_empty());
    }

    #[test]
    fn handle_existing_config_reset_returns_default() {
        let result = handle_existing_config(true, false, true);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.api.api_key.is_none());
    }
}
