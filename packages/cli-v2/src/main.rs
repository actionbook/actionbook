use std::time::Instant;

use clap::Parser;
use serde_json::json;

use actionbook_cli::action_result::ActionResult;
use actionbook_cli::cli::{BrowserCommands, Cli, Commands};
use actionbook_cli::config;
use actionbook_cli::output::{self, JsonEnvelope};
use actionbook_cli::utils::client::DaemonClient;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Internal: daemon auto-start passes a hidden arg before clap parsing
    if std::env::args().nth(1).as_deref() == Some("__daemon") {
        if let Err(e) = actionbook_cli::daemon::server::run_daemon().await {
            eprintln!("daemon error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let cli = Cli::parse();
    let json_output = cli.json;
    let is_setup_command = matches!(cli.command.as_ref(), Some(Commands::Setup(_)));

    // Handle --version before subcommand dispatch
    if cli.version {
        handle_version(json_output);
        return;
    }

    if cli.command.is_none() {
        eprintln!("error: no subcommand provided. Run `actionbook --help` for usage.");
        std::process::exit(1);
    }

    let result = run(cli).await;

    match result {
        Ok(()) => {}
        Err(e) => {
            let (code, hint) = match e.downcast_ref::<actionbook_cli::error::CliError>() {
                Some(cli_err) => (cli_err.error_code().to_string(), cli_err.hint().to_string()),
                None => ("INTERNAL_ERROR".to_string(), String::new()),
            };
            if json_output && !is_setup_command {
                let envelope = JsonEnvelope::error(
                    "unknown",
                    None,
                    &code,
                    &e.to_string(),
                    false,
                    serde_json::Value::Null,
                    &hint,
                    std::time::Duration::ZERO,
                );
                println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
            } else {
                eprintln!("error {code}: {e}");
                if !hint.is_empty() {
                    eprintln!("hint: {hint}");
                }
            }
            std::process::exit(1);
        }
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let json_mode = cli.json;

    match cli.command.unwrap() {
        Commands::Browser { command } => {
            handle_browser(command, json_mode).await?;
        }
        Commands::Setup(cmd) => {
            actionbook_cli::setup::execute(&cmd, json_mode).await?;
        }
        Commands::Help => {
            handle_help(json_mode);
        }
    }
    Ok(())
}

async fn handle_browser(
    command: BrowserCommands,
    json_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let command = match command {
        BrowserCommands::Start(cmd) => match config::resolve_start_command(cmd) {
            Ok(cmd) => BrowserCommands::Start(cmd),
            Err(err) => {
                let failed_command =
                    BrowserCommands::Start(actionbook_cli::browser::session::start::Cmd {
                        mode: None,
                        headless: None,
                        profile: None,
                        executable_path: None,
                        open_url: None,
                        cdp_endpoint: None,
                        header: vec![],
                        set_session_id: None,
                    });
                let result = ActionResult::fatal(err.error_code(), err.to_string());
                let duration = start.elapsed();
                let context = failed_command.context(&result);
                if json_mode {
                    let envelope =
                        JsonEnvelope::from_result("browser.start", context, &result, duration);
                    println!("{}", serde_json::to_string(&envelope)?);
                } else {
                    let text = output::format_text("browser.start", &context, &result);
                    println!("{text}");
                }
                std::process::exit(1);
            }
        },
        other => other,
    };

    let command_name = command.command_name().to_string();

    // Build action from CLI args
    let action = match command.to_action() {
        Some(a) => a,
        None => {
            let result = ActionResult::fatal(
                "UNSUPPORTED_OPERATION",
                format!("{command_name} is not yet implemented"),
            );
            let duration = start.elapsed();
            let context = command.context(&result);
            if json_mode {
                let envelope = JsonEnvelope::from_result(&command_name, context, &result, duration);
                println!("{}", serde_json::to_string(&envelope)?);
            } else {
                let text = output::format_text(&command_name, &context, &result);
                println!("{text}");
            }
            std::process::exit(1);
        }
    };

    // Connect to daemon and execute
    let mut client = DaemonClient::connect().await?;
    let result = client.send_action(&action).await?;
    let duration = start.elapsed();

    // Build context from command + result
    let context = command.context(&result);

    if json_mode {
        let envelope = JsonEnvelope::from_result(&command_name, context.clone(), &result, duration);
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        let text = output::format_text(&command_name, &context, &result);
        println!("{text}");
    }

    if !result.is_ok() {
        std::process::exit(1);
    }

    Ok(())
}

fn handle_version(json_mode: bool) {
    let version = env!("CARGO_PKG_VERSION");
    if json_mode {
        let envelope =
            JsonEnvelope::success("version", None, json!(version), std::time::Duration::ZERO);
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    } else {
        println!("{version}");
    }
}

fn handle_help(json_mode: bool) {
    let help_text = "actionbook browser <subcommand>\n\nstart         Start or attach a browser session\nlist-sessions List all active sessions\nstatus        Show session status\nclose         Close a session\nrestart       Restart a session\nlist-tabs     List tabs in a session\nnew-tab       Open a new tab\ngoto          Navigate to URL\nsnapshot      Capture accessibility snapshot\neval          Evaluate JavaScript\nclick         Click an element\ntype          Type text keystroke by keystroke\nfill          Fill an input field directly\nselect        Select a value from a dropdown";

    if json_mode {
        let envelope =
            JsonEnvelope::success("help", None, json!(help_text), std::time::Duration::ZERO);
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    } else {
        println!("{help_text}");
    }
}
