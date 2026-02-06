pub mod api_key;
pub mod browser_cfg;
pub mod detect;
pub mod mode;
pub mod templates;

use std::time::{Duration, Instant};

use colored::Colorize;
use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};

use crate::api::ApiClient;
use crate::cli::{BrowserMode, Cli, SetupTarget};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Grouped arguments for the setup command.
pub struct SetupArgs<'a> {
    pub target: Option<SetupTarget>,
    pub api_key: Option<&'a str>,
    pub browser: Option<BrowserMode>,
    pub mode: Option<&'a [SetupTarget]>,
    pub non_interactive: bool,
    pub force: bool,
    pub reset: bool,
}

/// Run the setup wizard. Orchestrates all steps in order.
///
/// Quick mode: if `--target` is provided without other flags, only generate
/// integration files for the specified target(s), skipping the full wizard.
pub async fn run(cli: &Cli, args: SetupArgs<'_>) -> Result<()> {
    // Quick mode: --target only → generate integration files and exit
    if let Some(t) = args.target {
        return run_target_only(cli, t, args.force, args.non_interactive).await;
    }

    // Handle existing config (re-run protection)
    let mut config = handle_existing_config(cli, args.non_interactive, args.reset)?;

    // Step 1: Welcome + environment detection
    if !cli.json {
        print_welcome();
        print_step_header(1, "Environment");
    }
    let spinner = create_spinner(cli.json, args.non_interactive, "Scanning environment...");
    let env = detect::detect_environment();
    finish_spinner(spinner, "Environment detected");
    detect::print_environment_report(&env, cli.json);

    // Steps 2–5: configure → recap → save (with restart loop)
    let (config, results) = loop {
        // Step 2: API Key
        if !cli.json {
            print_divider();
            print_step_header(2, "API Key");
        }
        api_key::configure_api_key(cli, &env, args.api_key, args.non_interactive, &mut config)
            .await?;

        // Step 3: Browser
        if !cli.json {
            print_divider();
            print_step_header(3, "Browser");
        }
        browser_cfg::configure_browser(
            cli,
            &env,
            args.browser,
            args.non_interactive,
            &mut config,
        )?;

        // Step 4: Integration + file generation
        if !cli.json {
            print_divider();
            print_step_header(4, "Integration");
        }
        let targets = mode::select_modes(cli, &env, args.mode, args.non_interactive)?;
        let results =
            mode::generate_integration_files(cli, &targets, args.force, args.non_interactive)?;

        // Step 5: Save configuration
        if !cli.json {
            print_divider();
            print_step_header(5, "Save");
        }

        // Show recap and confirm before saving (interactive only)
        if !cli.json && !args.non_interactive {
            let api_display = config
                .api
                .api_key
                .as_deref()
                .map(api_key::mask_key)
                .unwrap_or_else(|| "not configured".to_string());
            let browser_display = config
                .browser
                .executable
                .as_deref()
                .unwrap_or("built-in");
            let headless_display = if config.browser.headless {
                "headless"
            } else {
                "visible"
            };
            let mode_names: Vec<&str> = results
                .iter()
                .map(|r| mode::target_display_name(&r.target))
                .collect();
            let modes_display = if mode_names.is_empty() {
                "Standalone".to_string()
            } else {
                mode_names.join(", ")
            };

            println!("  {}", "Configuration summary:".dimmed());
            println!("    API Key   {}", api_display);
            println!("    Browser   {} ({})", browser_display, headless_display);
            println!("    Modes     {}", modes_display);
            println!(
                "    Path      {}\n",
                Config::config_path().display().to_string().dimmed()
            );

            let choices = vec![
                "Save and continue",
                "Restart setup",
                "Discard and exit",
            ];
            let selection = Select::new()
                .with_prompt("  What would you like to do?")
                .items(&choices)
                .default(0)
                .interact()
                .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

            match selection {
                0 => break (config, results), // Save
                1 => {
                    // Restart: reset config and loop
                    config = Config::default();
                    println!("\n  {} Restarting setup...\n", "↻".cyan());
                    continue;
                }
                _ => {
                    // Discard: clean exit
                    println!(
                        "\n  {} Setup discarded. Run {} to start again.\n",
                        "−".dimmed(),
                        "actionbook setup".cyan()
                    );
                    return Ok(());
                }
            }
        }

        // Non-interactive / JSON: save directly
        break (config, results);
    };

    config.save()?;
    if !cli.json {
        println!(
            "  {} Configuration saved to {}",
            "✓".green(),
            Config::config_path().display()
        );
    }

    // Step 6: Health check (API connectivity)
    if !cli.json {
        print_divider();
        print_step_header(6, "Health Check");
    }
    run_health_check(cli, &config, args.non_interactive).await;

    // Completion summary
    if !cli.json {
        print_divider();
    }
    print_completion(cli, &config, &results);

    Ok(())
}

const TOTAL_STEPS: u8 = 6;

/// Print a step header with progress counter, e.g. `[1/6] Environment`
fn print_step_header(step: u8, title: &str) {
    println!(
        "\n  {} {}\n",
        format!("[{}/{}]", step, TOTAL_STEPS).dimmed(),
        title.cyan().bold()
    );
}

/// Print a visual divider between steps.
fn print_divider() {
    println!("  {}", "─".repeat(40).dimmed());
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
            .template("  {spinner} {msg}")
            .expect("valid spinner template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    Some(pb)
}

/// Finish a spinner with a success message.
fn finish_spinner(pb: Option<ProgressBar>, message: &str) {
    if let Some(pb) = pb {
        pb.finish_with_message(format!("{} {}", "✓".green(), message));
    }
}

/// Quick mode: only generate integration files for the specified target.
async fn run_target_only(
    cli: &Cli,
    target: SetupTarget,
    force: bool,
    non_interactive: bool,
) -> Result<()> {
    let targets = match target {
        SetupTarget::All => vec![SetupTarget::Claude, SetupTarget::Cursor, SetupTarget::Codex],
        other => vec![other],
    };

    if !cli.json {
        println!("\n  {} Generating integration files...\n", "→".bold());
    }

    let results = mode::generate_integration_files(cli, &targets, force, non_interactive)?;

    if cli.json {
        let summary: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "target": mode::target_display_name(&r.target),
                    "path": r.path.display().to_string(),
                    "status": format!("{}", r.status),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "command": "setup",
                "mode": "target_only",
                "results": summary,
            })
        );
    } else {
        println!("\n  {}", "Done!".green().bold());
    }

    Ok(())
}

/// Handle re-run protection: detect existing config and offer choices.
fn handle_existing_config(cli: &Cli, non_interactive: bool, reset: bool) -> Result<Config> {
    if reset {
        if !cli.json {
            println!("  {} Resetting configuration...", "→".bold());
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
        println!("\n  {} Existing configuration found\n", "ℹ".blue());
    }

    let choices = vec![
        "Re-run setup (current values as defaults)",
        "Reset and start fresh",
        "Cancel",
    ];

    let selection = Select::new()
        .with_prompt("  What would you like to do?")
        .items(&choices)
        .default(0)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    match selection {
        0 => Ok(existing),
        1 => {
            if !cli.json {
                println!("  {} Starting fresh...", "→".bold());
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
        "  {}  {}\n",
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed(),
        "Setup Wizard".bold()
    );
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
                "  {} API key not configured — run {} to add it later",
                "−".yellow(),
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
                        "  {} API client creation failed: {}",
                        "✗".red(),
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
                    finish_spinner(
                        spinner,
                        &format!("API connection ({}ms)", elapsed),
                    );
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
                        pb.finish_with_message(format!(
                            "{} API connection failed",
                            "✗".red()
                        ));
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
                            "    {}",
                            format!("Error: {}", err_msg).dimmed()
                        );
                        println!(
                            "    {}",
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
            println!("  {} Config saved", "✓".green());
        }
    }
}

/// Print the completion summary with next steps.
fn print_completion(cli: &Cli, config: &Config, results: &[mode::TargetResult]) {
    if cli.json {
        let file_results: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "target": mode::target_display_name(&r.target),
                    "path": r.path.display().to_string(),
                    "status": format!("{}", r.status),
                })
            })
            .collect();

        let mode_names: Vec<&str> = results
            .iter()
            .map(|r| mode::target_display_name(&r.target))
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "command": "setup",
                "status": "complete",
                "config_path": Config::config_path().display().to_string(),
                "browser": config.browser.executable.as_deref().unwrap_or("built-in"),
                "headless": config.browser.headless,
                "modes": mode_names,
                "files": file_results,
            })
        );
        return;
    }

    // --- Success header ---
    println!();
    println!(
        "  {}  {}",
        "✓".green().bold(),
        "Actionbook is ready!".green().bold()
    );

    // --- Configuration recap ---
    let api_display = config
        .api
        .api_key
        .as_deref()
        .map(api_key::mask_key)
        .unwrap_or_else(|| "not configured".dimmed().to_string());

    let browser_name = config
        .browser
        .executable
        .as_deref()
        .map(shorten_browser_path)
        .unwrap_or_else(|| "built-in".to_string());
    let headless_str = if config.browser.headless {
        "headless"
    } else {
        "visible"
    };

    let mode_names: Vec<&str> = results
        .iter()
        .map(|r| mode::target_display_name(&r.target))
        .collect();
    let modes_str = if mode_names.is_empty() {
        "Standalone".to_string()
    } else {
        mode_names.join(", ")
    };

    println!();
    println!(
        "  {}  {}",
        "Config".dimmed(),
        shorten_home_path(&Config::config_path().display().to_string())
    );
    println!("  {}  {}", "Key".dimmed(), api_display);
    println!(
        "  {}  {} ({})",
        "Browser".dimmed(),
        browser_name,
        headless_str
    );
    println!("  {}  {}", "Modes".dimmed(), modes_str);

    // --- Generated files ---
    let active_results: Vec<&mode::TargetResult> = results
        .iter()
        .filter(|r| r.status != mode::FileStatus::Skipped)
        .collect();

    if !active_results.is_empty() {
        println!();
        for r in &active_results {
            let status_icon = match r.status {
                mode::FileStatus::Created => "✓".green(),
                mode::FileStatus::Updated => "✓".green(),
                mode::FileStatus::AlreadyUpToDate => "·".dimmed(),
                mode::FileStatus::Skipped => "○".dimmed(),
            };
            println!("  {}  {}", status_icon, r.path.display().to_string().dimmed());
        }
    }

    // --- Next steps ---
    println!();
    println!("  {}", "Next steps".bold());
    println!(
        "    {} {}",
        "$".dimmed(),
        "actionbook search \"<goal>\" --json".cyan()
    );
    println!(
        "    {} {}",
        "$".dimmed(),
        "actionbook get \"<area_id>\" --json".cyan()
    );
    println!();
}

/// Shorten a file path by replacing the home directory with `~`.
fn shorten_home_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if path.starts_with(&home_str) {
            return format!("~{}", &path[home_str.len()..]);
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
