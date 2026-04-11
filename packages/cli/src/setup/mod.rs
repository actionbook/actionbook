pub mod api_key;
pub mod browser_cfg;
pub mod detect;
pub mod skills;
pub mod theme;

use std::time::Duration;

use clap::Args;
use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};

use self::skills::{SetupTarget, SkillsAction, SkillsResult};
use self::theme::setup_theme;
use crate::config::{self, ConfigFile};
use crate::error::CliError;
use crate::types::Mode;

#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct Cmd {
    /// AI coding tool target. When set, skips the wizard and only installs
    /// skills for the given agent via `npx skills add` (quick mode).
    ///
    /// Mutually exclusive with full setup options like `--api-key`,
    /// `--browser`, and `--reset` to avoid silently ignoring them.
    #[arg(
        short = 't',
        long,
        value_enum,
        conflicts_with_all = ["api_key", "browser", "reset"]
    )]
    pub target: Option<SetupTarget>,

    /// API key (non-interactive). Overrides the global --api-key / ACTIONBOOK_API_KEY.
    #[arg(long)]
    pub api_key: Option<String>,

    /// Browser configuration (local|cloud|extension)
    #[arg(long)]
    pub browser: Option<String>,

    /// Skip all interactive prompts. Requires that every value be resolvable
    /// from flags, env vars, or an existing config.
    #[arg(long)]
    pub non_interactive: bool,

    /// Reset existing configuration and start fresh
    #[arg(long)]
    pub reset: bool,
}

const TOTAL_STEPS: u8 = 5;

/// Run the setup wizard. Orchestrates all steps in order.
///
/// Quick mode: if `--target` is set, the full wizard is skipped and only
/// `npx skills add` runs for the specified agent. This matches the CI /
/// non-interactive "one-shot install" path from the previous CLI.
pub async fn execute(cmd: &Cmd, json: bool) -> Result<(), CliError> {
    let non_interactive = cmd.non_interactive || json;

    // Quick mode: --target only → install skills for that target and exit.
    if let Some(target) = cmd.target {
        return run_target_only(json, target);
    }

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
            Mode::Extension => "extension".to_string(),
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

    // TODO: Health check (API connectivity) — requires ApiClient

    // Step 5: Install Skills
    if !json {
        print_step_connector();
        print_step_header(5, "Skills");
    }
    let skills_result = skills::install_skills(json, &env, non_interactive)?;

    // Completion summary
    print_completion(json, &config, &skills_result);

    // Propagate skills failure so non-interactive / CI callers see a non-zero exit.
    if skills_result.action == SkillsAction::Failed {
        return Err(CliError::Internal(
            "Skills installation failed.".to_string(),
        ));
    }

    Ok(())
}

/// Quick mode: only install skills for the given target via `npx skills add`.
/// Used by `actionbook setup --target <agent>` for one-shot CI / bootstrap runs.
fn run_target_only(json: bool, target: SetupTarget) -> Result<(), CliError> {
    // Standalone = CLI only, no agent integration.
    if target == SetupTarget::Standalone {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "command": "setup",
                    "mode": "target_only",
                    "target": "Standalone CLI",
                    "action": "skipped",
                    "reason": "no_agent_integration_needed",
                })
            );
        } else {
            println!();
            println!("  - Standalone CLI requires no skills integration.");
            println!("     Run `actionbook setup` to configure the CLI.");
            println!();
        }
        return Ok(());
    }

    if !json {
        println!();
        println!(
            "  +  Installing skills for {}",
            skills::target_display_name(&target)
        );
        println!("  |");
    }

    let result = skills::install_skills_for_target(json, &target)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "setup",
                "mode": "target_only",
                "target": skills::target_display_name(&target),
                "npx_available": result.npx_available,
                "action": format!("{}", result.action),
                "skills_command": result.command,
            })
        );
    } else if result.action == SkillsAction::Installed {
        println!("  +  Done!");
        println!();
    }

    target_only_exit_status(&result, &target)
}

/// Decide the exit status of `run_target_only` based on the skills outcome.
///
/// Quick mode is an explicit "install for this agent now" request, so any
/// outcome other than `Installed` must surface as an error. This differs from
/// the full-wizard Skills step, which treats `Prompted` (npx missing) as a
/// soft prompt — there the user can still complete setup by hand. In quick
/// mode the user's entire intent was to install; there's no other work to do.
fn target_only_exit_status(
    result: &skills::SkillsResult,
    target: &SetupTarget,
) -> Result<(), CliError> {
    match result.action {
        SkillsAction::Installed => Ok(()),
        SkillsAction::Failed => Err(CliError::Internal(format!(
            "Skills installation failed for {}.",
            skills::target_display_name(target)
        ))),
        SkillsAction::Prompted => Err(CliError::Internal(format!(
            "Skills installation skipped for {}: npx is not available. \
             Install Node.js (https://nodejs.org) and re-run, or run \
             `{}` manually.",
            skills::target_display_name(target),
            result.command,
        ))),
        SkillsAction::Skipped => Err(CliError::Internal(format!(
            "Skills installation skipped for {}.",
            skills::target_display_name(target)
        ))),
    }
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
        "extension" => Ok(Some(Mode::Extension)),
        other => Err(CliError::InvalidArgument(format!(
            "invalid --browser value '{other}': expected local|cloud|extension"
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

fn setup_completion_status(skills_result: &SkillsResult) -> &'static str {
    match skills_result.action {
        SkillsAction::Failed => "failed",
        SkillsAction::Installed | SkillsAction::Skipped | SkillsAction::Prompted => "complete",
    }
}

/// Print the completion summary with next steps.
fn print_completion(json: bool, config: &ConfigFile, skills_result: &SkillsResult) {
    if json {
        let mut payload = serde_json::json!({
            "command": "setup",
            "status": setup_completion_status(skills_result),
            "config_path": config::config_path().display().to_string(),
            "browser_mode": format!("{}", config.browser.mode),
            "browser": match config.browser.mode {
                Mode::Local => config.browser.executable_path.as_deref().unwrap_or("built-in"),
                Mode::Cloud => config
                    .browser
                    .cdp_endpoint
                    .as_deref()
                    .unwrap_or("endpoint not configured"),
                Mode::Extension => "extension (bridge)",
            },
            "headless": config.browser.headless,
            "skills": {
                "npx_available": skills_result.npx_available,
                "action": format!("{}", skills_result.action),
                "command": skills_result.command,
            },
        });

        if skills_result.action == SkillsAction::Failed {
            payload["error"] = serde_json::Value::String("Skills installation failed.".to_string());
        }

        println!("{}", payload);
        return;
    }

    println!("  |");
    match skills_result.action {
        SkillsAction::Installed => println!("  +  Actionbook is ready!"),
        SkillsAction::Failed => println!("  +  Setup completed with errors."),
        SkillsAction::Skipped | SkillsAction::Prompted => println!("  +  Setup completed."),
    }

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
        Mode::Extension => "extension".to_string(),
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

    fn make_skills_result(action: SkillsAction) -> skills::SkillsResult {
        skills::SkillsResult {
            npx_available: action != SkillsAction::Prompted,
            action,
            command: "npx skills add actionbook/actionbook -a claude-code".to_string(),
        }
    }

    #[test]
    fn target_only_installed_returns_ok() {
        let result = make_skills_result(SkillsAction::Installed);
        assert!(target_only_exit_status(&result, &SetupTarget::Claude).is_ok());
    }

    #[test]
    fn target_only_failed_returns_err() {
        let result = make_skills_result(SkillsAction::Failed);
        let err = target_only_exit_status(&result, &SetupTarget::Claude)
            .expect_err("failed must propagate");
        assert!(err.to_string().contains("Claude Code"));
    }

    #[test]
    fn target_only_prompted_returns_err_when_npx_missing() {
        // P1 regression guard: quick mode must not silently succeed when
        // npx is unavailable. `install_skills_for_target` returns Prompted
        // in that case, and CI bootstrap relying on `--target` needs a
        // non-zero exit to notice the missing prereq.
        let result = make_skills_result(SkillsAction::Prompted);
        let err = target_only_exit_status(&result, &SetupTarget::Codex)
            .expect_err("prompted must propagate in quick mode");
        assert!(err.to_string().contains("npx is not available"));
        assert!(err.to_string().contains("Codex"));
    }

    #[test]
    fn target_only_skipped_returns_err() {
        // Skipped shouldn't happen in quick mode (auto_confirm=true is passed
        // to run_npx_skills), but if it ever surfaces, quick mode must still
        // fail loudly rather than silently exit 0.
        let result = make_skills_result(SkillsAction::Skipped);
        assert!(target_only_exit_status(&result, &SetupTarget::Cursor).is_err());
    }

    #[test]
    fn setup_completion_status_is_complete_when_skills_install_succeeds() {
        let result = make_skills_result(SkillsAction::Installed);
        assert_eq!(setup_completion_status(&result), "complete");
    }

    #[test]
    fn setup_completion_status_is_failed_when_skills_install_fails() {
        let result = make_skills_result(SkillsAction::Failed);
        assert_eq!(setup_completion_status(&result), "failed");
    }

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
    fn parse_browser_flag_accepts_extension() {
        assert_eq!(
            parse_browser_flag(Some("extension")).unwrap(),
            Some(Mode::Extension)
        );
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
