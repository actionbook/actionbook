pub mod api_key;
pub mod browser_cfg;
pub mod detect;
pub mod mode;
pub mod theme;

use std::time::{Duration, Instant};

use colored::Colorize;
use dialoguer::Select;

use self::theme::setup_theme;
use indicatif::{ProgressBar, ProgressStyle};

use crate::api::ApiClient;
use crate::cli::{AgentMode, BrowserMode, Cli, SetupTarget};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Grouped arguments for the setup command.
pub struct SetupArgs<'a> {
    pub target: Option<SetupTarget>,
    pub api_key: Option<&'a str>,
    pub browser: Option<BrowserMode>,
    pub non_interactive: bool,
    pub reset: bool,
    pub agent_mode: Option<AgentMode>,
}

fn should_run_target_only(args: &SetupArgs<'_>) -> bool {
    args.target.is_some()
        && args.api_key.is_none()
        && args.browser.is_none()
        && !args.non_interactive
        && !args.reset
        && args.agent_mode.is_none()
}

fn requires_agent_api_key(target: Option<SetupTarget>) -> bool {
    target
        .as_ref()
        .and_then(mode::target_to_agent_flag)
        .is_some()
}

fn should_install_skills_for_target(target: Option<SetupTarget>) -> bool {
    !matches!(target, Some(SetupTarget::Standalone))
}

/// Run the setup wizard. Orchestrates all steps in order.
///
/// Quick mode: if `--target` is provided without other flags, only run
/// `npx skills add` for the specified target, skipping the full wizard.
pub async fn run(cli: &Cli, args: SetupArgs<'_>) -> Result<()> {
    if args.target.is_some() && args.agent_mode.is_some() {
        return Err(ActionbookError::SetupError(
            "Cannot combine --target with --agent-mode. Pick one and try again.".to_string(),
        ));
    }

    // Quick mode: --target only → run npx skills add and exit
    if should_run_target_only(&args) {
        return run_target_only(cli, args.target.expect("target exists in quick mode")).await;
    }

    // In full setup flow, use --target to select skills installation target.
    let agent_target = args.target.or(args.agent_mode.map(|mode| mode.to_setup_target()));
    let effective_non_interactive = args.non_interactive || agent_target.is_some();

    // Handle existing config (re-run protection)
    let mut config = handle_existing_config(cli, effective_non_interactive, args.reset)?;

    // Step 1: Welcome + environment detection
    if !cli.json {
        print_welcome();
        print_step_header(1, "Environment");
    }
    let spinner = create_spinner(cli.json, effective_non_interactive, "Scanning environment...");
    let env = detect::detect_environment();
    finish_spinner(spinner, "Environment detected");
    detect::print_environment_report(&env, cli.json);
    if !cli.json {
        print_step_connector();
    }

    let browser_flag = if args.browser.is_some() {
        args.browser
    } else if agent_target.is_some() {
        Some(BrowserMode::Isolated)
    } else {
        None
    };

    // Steps 2–4: configure → recap → save (with restart loop)
    let config = loop {
        // Step 2: API Key
        if !cli.json {
            print_step_header(2, "API Key");
        }
        api_key::configure_api_key(cli, &env, args.api_key, effective_non_interactive, &mut config)
            .await?;

        if requires_agent_api_key(agent_target) && config.api.api_key.is_none() {
            return Err(ActionbookError::SetupError(
                "Agent mode requires an API key. Provide one via --api-key, ACTIONBOOK_API_KEY, or existing config.".to_string(),
            ));
        }

        // Step 3: Browser
        if !cli.json {
            print_step_connector();
            print_step_header(3, "Browser");
        }
        browser_cfg::configure_browser(cli, &env, browser_flag, effective_non_interactive, &mut config).await?;

        // Step 4: Save configuration
        if !cli.json {
            print_step_connector();
            print_step_header(4, "Save");
        }

        // Show recap (interactive only)
        if !cli.json && !effective_non_interactive {
            let bar = "│".dimmed();
            let api_display = config
                .api
                .api_key
                .as_deref()
                .unwrap_or("not configured");
            let mode_display = match config.browser.mode {
                BrowserMode::Isolated => {
                    let browser_name = config.browser.executable.as_deref().unwrap_or("built-in");
                    let headless_label = if config.browser.headless { "headless" } else { "visible" };
                    format!("isolated — {} ({})", browser_name, headless_label)
                }
                BrowserMode::Extension => "extension".to_string(),
            };

            println!("  {}  {}", bar, "Configuration summary:".dimmed());
            println!("  {}    API Key   {}", bar, api_display);
            println!("  {}    Browser   {}", bar, mode_display);
            println!(
                "  {}    Path      {}",
                bar,
                Config::config_path().display().to_string().dimmed()
            );
        }

        // Save directly without confirmation
        break config;
    };

    config.save()?;
    if !cli.json {
        println!(
            "  {}  Configuration saved to {}",
            "◇".green(),
            Config::config_path().display()
        );
    }

    // Step 5: Health check (API connectivity)
    if !cli.json {
        print_step_connector();
        print_step_header(5, "Health Check");
    }
    run_health_check(cli, &config, effective_non_interactive).await;

    // Step 6: Install Skills
    let skills_result = if should_install_skills_for_target(agent_target) {
        if !cli.json {
            print_step_connector();
            print_step_header(6, "Skills");
        }
        mode::install_skills(cli, &env, effective_non_interactive, agent_target.as_ref())?
    } else {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "skills",
                    "npx_available": true,
                    "action": "skipped",
                    "reason": "standalone_target",
                })
            );
        }
        mode::SkillsResult {
            npx_available: true,
            action: mode::SkillsAction::Skipped,
            command: "npx skills add actionbook/actionbook".to_string(),
        }
    };

    // Completion summary
    print_completion(cli, &config, &skills_result);

    if skills_result.action == mode::SkillsAction::Failed {
        return Err(ActionbookError::SetupError(
            "Skills installation failed.".to_string(),
        ));
    }

    Ok(())
}

const TOTAL_STEPS: u8 = 6;

/// Print a step header, e.g. `◆  Environment`
fn print_step_header(step: u8, title: &str) {
    println!(
        "  {}  {} {}",
        "◆".cyan(),
        title.cyan().bold(),
        format!("({}/{})", step, TOTAL_STEPS).dimmed()
    );
    println!("  {}", "│".dimmed());
}

/// Print a vertical connector between steps.
fn print_step_connector() {
    println!("  {}", "│".dimmed());
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
        pb.finish_with_message(format!("{} {}", "◇".green(), message));
    }
}

/// Quick mode: only run `npx skills add` for the specified target.
async fn run_target_only(cli: &Cli, target: SetupTarget) -> Result<()> {
    // Standalone means "CLI only, no AI tool integration"
    if target == SetupTarget::Standalone {
        if cli.json {
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
            println!(
                "\n  {}  Standalone CLI requires no skills integration.",
                "◇".green()
            );
            println!(
                "     Run {} to configure the CLI.\n",
                "actionbook setup".cyan()
            );
        }
        return Ok(());
    }

    if !cli.json {
        println!();
        println!(
            "  {}  Installing skills for {}",
            "┌".cyan(),
            mode::target_display_name(&target).bold()
        );
        println!("  {}", "│".dimmed());
    }

    let result = mode::install_skills_for_target(cli, &target)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "command": "setup",
                "mode": "target_only",
                "target": mode::target_display_name(&target),
                "npx_available": result.npx_available,
                "action": format!("{}", result.action),
                "skills_command": result.command,
            })
        );
    } else if result.action == mode::SkillsAction::Installed {
        println!("  {}  {}", "└".green(), "Done!".green().bold());
        println!();
    }

    if result.action == mode::SkillsAction::Failed {
        return Err(ActionbookError::SetupError(
            "Skills installation failed.".to_string(),
        ));
    }

    Ok(())
}

/// Handle re-run protection: detect existing config and offer choices.
fn handle_existing_config(cli: &Cli, non_interactive: bool, reset: bool) -> Result<Config> {
    if reset {
        if !cli.json {
            println!("  {}  Resetting configuration...", "◇".cyan());
        }
        return Ok(Config::default());
    }

    let config_exists = Config::config_path().exists();

    if !config_exists {
        return Ok(Config::default());
    }

    // Load existing config
    let existing = Config::load()?;

    if non_interactive {
        // Non-interactive: reuse existing config as defaults
        return Ok(existing);
    }

    if !cli.json {
        println!("\n  {}  Existing configuration found\n", "◇".blue());
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
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    match selection {
        0 => Ok(existing),
        1 => {
            if !cli.json {
                println!("  {}  Starting fresh...", "◇".cyan());
            }
            Ok(Config::default())
        }
        _ => Err(ActionbookError::SetupError("Setup cancelled.".to_string())),
    }
}

/// Print the welcome banner with gradient Actionbook logo.
fn print_welcome() {
    println!();
    let lines = [
        r"     _        _   _             _                 _     ",
        r"    / \   ___| |_(_) ___  _ __ | |__   ___   ___ | | __ ",
        r"   / _ \ / __| __| |/ _ \| '_ \| '_ \ / _ \ / _ \| |/ /",
        r"  / ___ \ (__| |_| | (_) | | | | |_) | (_) | (_) |   < ",
        r" /_/   \_\___|\__|_|\___/|_| |_|_.__/ \___/ \___/|_|\_\",
    ];
    // Gradient: bright_cyan → cyan → blue
    println!("  {}", lines[0].bright_cyan().bold());
    println!("  {}", lines[1].bright_cyan());
    println!("  {}", lines[2].cyan());
    println!("  {}", lines[3].cyan());
    println!("  {}", lines[4].blue());
    println!();
    println!(
        "  {}  {}  {}",
        "┌".cyan(),
        "Setup Wizard".bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed()
    );
    println!("  {}", "│".dimmed());
}

/// Run a health check by testing API connectivity.
async fn run_health_check(cli: &Cli, config: &Config, non_interactive: bool) {
    // API key + connectivity check
    if config.api.api_key.is_none() {
        // No API key configured — skip connectivity test
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "health_check",
                    "api_key": "not_configured",
                })
            );
        } else {
            println!(
                "  {}  API key not configured — run {} to add it later",
                "◇".dimmed(),
                "actionbook config set api.api_key <your-key>".cyan()
            );
        }
    } else {
        // API key present — test connectivity
        let client = match ApiClient::from_config(config) {
            Ok(c) => Some(c),
            Err(e) => {
                let err_msg = e.to_string();
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "step": "health_check",
                            "api_key": "configured",
                            "api_connection": "failed",
                            "error": err_msg,
                        })
                    );
                } else {
                    println!(
                        "  {}  API client creation failed: {}",
                        "■".red(),
                        err_msg.dimmed()
                    );
                }
                None
            }
        };

        if let Some(client) = client {
            let spinner = create_spinner(cli.json, non_interactive, "Testing API connection...");
            let start = Instant::now();
            match client.list_sources(Some(1)).await {
                Ok(_) => {
                    let elapsed = start.elapsed().as_millis();
                    finish_spinner(spinner, &format!("API connection ({}ms)", elapsed));
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "step": "health_check",
                                "api_key": "configured",
                                "api_connection": "ok",
                                "latency_ms": elapsed,
                            })
                        );
                    }
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    if let Some(pb) = spinner {
                        pb.finish_with_message(format!("{} API connection failed", "■".red()));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "step": "health_check",
                                "api_key": "configured",
                                "api_connection": "failed",
                                "error": err_msg,
                            })
                        );
                    } else {
                        println!(
                            "  {}  {}",
                            "│".dimmed(),
                            format!("Error: {}", err_msg).dimmed()
                        );
                        println!(
                            "  {}  {}",
                            "│".dimmed(),
                            "Check your API key and network connection.".dimmed()
                        );
                    }
                }
            }
        }
    }

    // Config file check
    let config_path = Config::config_path();
    if config_path.exists() {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "health_check",
                    "config_file": "ok",
                    "path": config_path.display().to_string(),
                })
            );
        } else {
            println!("  {}  Config saved", "◇".green());
        }
    }
}

/// Print the completion summary with next steps.
fn print_completion(cli: &Cli, config: &Config, skills_result: &mode::SkillsResult) {
    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "command": "setup",
                "status": "complete",
                "config_path": Config::config_path().display().to_string(),
                "browser_mode": serde_json::to_value(&config.browser.mode).unwrap_or(serde_json::Value::Null),
                "browser": config.browser.executable.as_deref().unwrap_or("built-in"),
                "headless": config.browser.headless,
                "extension_port": config.browser.extension.port,
                "skills": {
                    "npx_available": skills_result.npx_available,
                    "action": format!("{}", skills_result.action),
                    "command": skills_result.command,
                },
            })
        );
        return;
    }

    // --- Status header (varies by skills outcome) ---
    println!("  {}", "│".dimmed());
    match skills_result.action {
        mode::SkillsAction::Installed => {
            println!(
                "  {}  {}",
                "└".green(),
                "Actionbook is ready!".green().bold()
            );
        }
        mode::SkillsAction::Failed => {
            println!(
                "  {}  {}",
                "└".red(),
                "Setup completed with errors.".red().bold()
            );
        }
        _ => {
            // Skipped / Prompted
            println!("  {}  {}", "└".cyan(), "Setup completed.".bold());
        }
    }

    // --- Configuration recap ---
    let api_display = config
        .api
        .api_key
        .as_deref()
        .unwrap_or("not configured")
        .to_string();

    let browser_display = match config.browser.mode {
        BrowserMode::Isolated => {
            let name = config
                .browser
                .executable
                .as_deref()
                .map(shorten_browser_path)
                .unwrap_or_else(|| "built-in".to_string());
            let headless_str = if config.browser.headless { "headless" } else { "visible" };
            format!("isolated — {} ({})", name, headless_str)
        }
        BrowserMode::Extension => "extension".to_string(),
    };

    println!();
    println!(
        "     {}  {}",
        "Config".dimmed(),
        shorten_home_path(&Config::config_path().display().to_string())
    );
    println!("     {}  {}", "Key".dimmed(), api_display);
    println!("     {}  {}", "Browser".dimmed(), browser_display);
    // --- Next steps ---
    println!();
    println!("     {}", "Next steps".bold());
    println!(
        "       {} {}",
        "$".dimmed(),
        "actionbook search \"<goal>\" --json".cyan()
    );
    println!(
        "       {} {}",
        "$".dimmed(),
        "actionbook get \"<area_id>\" --json".cyan()
    );
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
    // Known browser names to match against
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
    // Fallback: last path component
    path.rsplit('/').next().unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args<'a>() -> SetupArgs<'a> {
        SetupArgs {
            target: None,
            api_key: None,
            browser: None,
            non_interactive: false,
            reset: false,
            agent_mode: None,
        }
    }

    #[test]
    fn target_only_triggers_quick_mode() {
        let mut args = base_args();
        args.target = Some(SetupTarget::Codex);
        assert!(should_run_target_only(&args));
    }

    #[test]
    fn target_with_non_interactive_runs_full_setup() {
        let mut args = base_args();
        args.target = Some(SetupTarget::Codex);
        args.non_interactive = true;
        assert!(!should_run_target_only(&args));
    }

    #[test]
    fn target_with_browser_runs_full_setup() {
        let mut args = base_args();
        args.target = Some(SetupTarget::Codex);
        args.browser = Some(BrowserMode::Isolated);
        assert!(!should_run_target_only(&args));
    }

    #[test]
    fn standalone_target_does_not_require_api_key() {
        assert!(!requires_agent_api_key(Some(SetupTarget::Standalone)));
    }

    #[test]
    fn codex_target_requires_api_key() {
        assert!(requires_agent_api_key(Some(SetupTarget::Codex)));
    }

    #[test]
    fn standalone_target_skips_skills_install() {
        assert!(!should_install_skills_for_target(Some(SetupTarget::Standalone)));
    }
}
