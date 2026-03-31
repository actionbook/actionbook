use std::time::{Duration, Instant};

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

    // Intercept help-like invocations before clap to show our custom help
    // messages instead of clap's auto-generated output.
    //
    // All of these should show the same custom output:
    //   actionbook              → top-level help
    //   actionbook help         → top-level help
    //   actionbook --help / -h  → top-level help
    //   actionbook browser              → browser grouped help
    //   actionbook browser help         → browser grouped help
    //   actionbook browser --help / -h  → browser grouped help
    {
        let raw_args: Vec<String> = std::env::args().collect();
        let json_mode = raw_args.iter().any(|a| a == "--json");

        // Intercept --version before positional dispatch so it doesn't fall through to help
        if raw_args.iter().any(|a| a == "--version" || a == "-V") {
            handle_version(json_mode);
            return;
        }

        // Collect non-flag args after the binary name, skipping --timeout's value
        let mut positional_args: Vec<&str> = Vec::new();
        let mut skip_next = false;
        for arg in &raw_args[1..] {
            if skip_next {
                skip_next = false;
                continue;
            }
            if arg == "--timeout" {
                skip_next = true;
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
            positional_args.push(arg);
        }

        match positional_args.as_slice() {
            // `actionbook` (no args), `actionbook --help`, `actionbook help`
            [] | ["help"] => {
                handle_help(json_mode);
                return;
            }
            // `actionbook browser`, `actionbook browser --help`, `actionbook browser help`
            ["browser"] | ["browser", "help"] => {
                handle_browser_help(json_mode);
                return;
            }
            _ => {}
        }
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
        handle_help(json_output);
        return;
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
    let timeout_ms = cli.timeout;

    match cli.command.unwrap() {
        Commands::Browser { command } => {
            handle_browser(command, json_mode, timeout_ms).await?;
        }
        Commands::Setup(cmd) => {
            actionbook_cli::setup::execute(&cmd, json_mode).await?;
        }
        Commands::Help => {
            handle_help(json_mode);
        }
        Commands::Version => {
            handle_version(json_mode);
        }
    }
    Ok(())
}

async fn handle_browser(
    command: BrowserCommands,
    json_mode: bool,
    timeout_ms: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(command, BrowserCommands::Help) {
        handle_browser_help(json_mode);
        return Ok(());
    }

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
                        stealth: true,
                    });
                let result = ActionResult::fatal(err.error_code(), err.to_string());
                let duration = start.elapsed();
                let context = failed_command.context(&result);
                if json_mode {
                    let envelope =
                        JsonEnvelope::from_result("browser start", context, &result, duration);
                    println!("{}", serde_json::to_string(&envelope)?);
                } else {
                    let text = output::format_text("browser start", &context, &result);
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

    // Connect to daemon and execute, with optional global timeout across the whole request.
    let result = if let Some(timeout_ms) = timeout_ms {
        let execution = async {
            let mut client = DaemonClient::connect().await?;
            client.send_action(&action).await
        };
        match tokio::time::timeout(Duration::from_millis(timeout_ms), execution).await {
            Ok(result) => result?,
            Err(_) => {
                let result = ActionResult::fatal_with_hint(
                    "TIMEOUT",
                    format!("{command_name} timed out after {timeout_ms}ms"),
                    "increase --timeout or retry the command",
                );
                let duration = start.elapsed();
                let context = command.context(&result);
                if json_mode {
                    let envelope =
                        JsonEnvelope::from_result(&command_name, context, &result, duration);
                    println!("{}", serde_json::to_string(&envelope)?);
                } else {
                    let text = output::format_text(&command_name, &context, &result);
                    println!("{text}");
                }
                std::process::exit(1);
            }
        }
    } else {
        let mut client = DaemonClient::connect().await?;
        client.send_action(&action).await?
    };
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
    let help_text = "\
Actionbook — browser automation for AI agents

Every command is stateless: pass --session and --tab explicitly.
No \"current tab\" — run commands on any session/tab in parallel.

Usage: actionbook <command> [options]

Commands:
  browser    Control browser sessions, tabs, and page interactions
  setup      Configure actionbook
  help       Show this help
  --version  Show version

Global flags:
  --json          Output as JSON envelope
  --timeout <ms>  Set command timeout

Quick start:
  actionbook browser start --set-session-id my-session
  actionbook browser goto https://example.com --session my-session --tab t1
  actionbook browser snapshot --session my-session --tab t1
  actionbook browser click \"#login\" --session my-session --tab t1

Run actionbook browser --help to see all browser subcommands.";

    if json_mode {
        let envelope =
            JsonEnvelope::success("help", None, json!(help_text), std::time::Duration::ZERO);
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    } else {
        println!("{help_text}");
    }
}

fn handle_browser_help(json_mode: bool) {
    let help_text = "\
Usage: actionbook browser <subcommand> [options]

Most commands require --session <SID> and --tab <TID>.
Session-level commands need only --session. Start and list-sessions need neither.

Session:
  start                              Start or attach a browser session
  list-sessions                      List all active sessions
  status              --session      Show session status
  close               --session      Close a session (alias: stop)
  restart             --session      Restart a session

Tab:
  list-tabs           --session      List tabs in a session
  new-tab <url>       --session      Open a new tab (alias: open)
  close-tab           --session --tab  Close a tab

Navigation:
  goto <url>          --session --tab  Navigate to a URL
  back                --session --tab  Go back
  forward             --session --tab  Go forward
  reload              --session --tab  Reload the page

Observation:
  snapshot            --session --tab  Capture accessibility snapshot
  screenshot <path>   --session --tab  Take a screenshot
  title               --session --tab  Get page title
  url                 --session --tab  Get current URL
  viewport            --session --tab  Get viewport size
  html [<selector>]   --session --tab  Read element/page HTML
  text [<selector>]   --session --tab  Read element/page text
  value <selector>    --session --tab  Read input value
  attr <selector> <name>  --session --tab  Read element attribute
  attrs <selector>        --session --tab  Read all element attributes
  box <selector>          --session --tab  Read element bounding box
  styles <selector> [names...]  --session --tab  Read computed styles
  describe <selector>     --session --tab  Describe element properties
  state <selector>        --session --tab  Get element state flags
  inspect-point <x,y>    --session --tab  Inspect element at coordinates
  query one|all|count <selector>  --session --tab  Query elements
  query nth <n> <selector>        --session --tab  Query nth element (1-based)

Logs:
  logs console        --session --tab  Get console logs
  logs errors         --session --tab  Get error logs (exceptions + rejections)

Wait:
  wait element <selector>  --session --tab  Wait for element to appear
  wait navigation          --session --tab  Wait for navigation to complete
  wait network-idle        --session --tab  Wait for network to become idle
  wait condition <expr>    --session --tab  Wait for JS expression to be truthy

Cookies:
  cookies list        --session      List all cookies
  cookies get <name>  --session      Get a cookie by name
  cookies set <name> <value>  --session  Set a cookie
  cookies delete <name>  --session   Delete a cookie
  cookies clear       --session      Clear cookies

Storage (local-storage | session-storage):
  <storage> list      --session --tab  List all key-value entries
  <storage> get <key> --session --tab  Get a value by key
  <storage> set <key> <value>  --session --tab  Set a key-value entry
  <storage> delete <key>  --session --tab  Delete a key
  <storage> clear <key>   --session --tab  Clear a key

Interaction:
  click <selector|x,y>   --session --tab  Click element or coordinates
  hover <selector>        --session --tab  Hover over an element
  focus <selector>        --session --tab  Focus an element
  press <key>             --session --tab  Press a key or key combo
  type <text>             --session --tab  Type text keystroke by keystroke
  fill <selector> <text>  --session --tab  Fill an input field directly
  select <selector> <value>  --session --tab  Select from a dropdown
  drag <source> <target>  --session --tab  Drag element to a target
  upload <selector> <file...>  --session --tab  Upload files to a file input
  eval <code>             --session --tab  Evaluate JavaScript
  mouse-move <x,y>       --session --tab  Move mouse to coordinates
  cursor-position         --session --tab  Get current cursor position
  scroll <direction|edge|into-view>  --session --tab  Scroll page or container

Global flags (apply to all subcommands):
  --json          Output as JSON envelope
  --timeout <ms>  Set command timeout

Quick start:
  actionbook browser start --set-session-id s1
  actionbook browser goto https://example.com --session s1 --tab t1
  actionbook browser snapshot --session s1 --tab t1
  actionbook browser click \"#login\" --session s1 --tab t1
  actionbook browser fill \"#email\" \"user@test.com\" --session s1 --tab t1
  actionbook browser press Enter --session s1 --tab t1

Run actionbook browser <subcommand> --help for full usage and examples.";

    if json_mode {
        let envelope = JsonEnvelope::success(
            "browser help",
            None,
            json!(help_text),
            std::time::Duration::ZERO,
        );
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    } else {
        println!("{help_text}");
    }
}
