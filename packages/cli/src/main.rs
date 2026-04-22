use std::io::Write as _;
use std::time::{Duration, Instant};

use clap::Parser;
use serde_json::json;

use actionbook_cli::action::Action;
use actionbook_cli::action_result::ActionResult;
use actionbook_cli::browser::interaction;
use actionbook_cli::browser::navigation;
use actionbook_cli::cli::{BrowserCommands, Cli, Commands, DaemonCommands, ExtensionCommands};
use actionbook_cli::config;
use actionbook_cli::daemon::cdp_error_classifier::CdpErrorCode;
use actionbook_cli::output::{self, JsonEnvelope};
use actionbook_cli::utils::client::DaemonClient;

/// Flush stdout then exit. On Windows, stdout is fully-buffered when redirected
/// to a file (as in the e2e test harness), so `exit()` without a flush loses
/// any `println!` output still sitting in the buffer.
fn flush_and_exit(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}

fn is_navigation_command(command_name: &str) -> bool {
    command_name == navigation::goto::COMMAND_NAME
        || command_name == navigation::back::COMMAND_NAME
        || command_name == navigation::forward::COMMAND_NAME
        || command_name == navigation::reload::COMMAND_NAME
}

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

        // Only handle --version at the top level (no subcommands).
        // `actionbook --version` shows the version; `actionbook browser --version` does not.
        if positional_args.is_empty() && raw_args.iter().any(|a| a == "--version" || a == "-V") {
            handle_version(json_mode);
            return;
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
            let (code, hint, retryable, details) =
                match e.downcast_ref::<actionbook_cli::error::CliError>() {
                    Some(cli_err) => (
                        cli_err.error_code().to_string(),
                        cli_err.hint().to_string(),
                        cli_err.is_retryable(),
                        cli_err.envelope_details(),
                    ),
                    None => (
                        "INTERNAL_ERROR".to_string(),
                        String::new(),
                        false,
                        serde_json::Value::Null,
                    ),
                };
            if json_output && !is_setup_command {
                let envelope = JsonEnvelope::error(
                    "unknown",
                    None,
                    &code,
                    &e.to_string(),
                    retryable,
                    details,
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
            flush_and_exit(1);
        }
    }
}

async fn run(mut cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let json_mode = cli.json;
    let timeout_ms = cli.timeout;

    let command = cli.command.take().unwrap();
    match command {
        Commands::Search { keyword } => {
            actionbook_cli::commands::search::run(&keyword, json_mode).await?;
        }
        Commands::Manual {
            site,
            group,
            action,
        } => {
            actionbook_cli::commands::manual::run(
                site.as_deref(),
                group.as_deref(),
                action.as_deref(),
                json_mode,
            )
            .await?;
        }
        Commands::Get { area_id } => {
            actionbook_cli::commands::get::run(&cli, &area_id).await?;
        }
        Commands::Browser { command } => {
            handle_browser(command, json_mode, timeout_ms).await?;
        }
        Commands::Daemon { command } => {
            handle_daemon(command, json_mode, timeout_ms).await?;
        }
        Commands::Extension { command } => {
            handle_extension(command, json_mode).await?;
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
            Ok(mut cmd) => {
                // Forward provider env vars from the CLI client's process env
                // to the daemon. The daemon's own env was frozen at spawn time
                // and can't be relied on to match the user's current shell.
                cmd.provider_env =
                    actionbook_cli::browser::session::provider::collect_provider_env_from_process();
                BrowserCommands::Start(cmd)
            }
            Err(err) => {
                let failed_command =
                    BrowserCommands::Start(actionbook_cli::browser::session::start::Cmd {
                        mode: None,
                        headless: None,
                        profile: None,
                        executable_path: None,
                        open_url: None,
                        tab_id: None,
                        cdp_endpoint: None,
                        provider: None,
                        header: vec![],
                        session: None,
                        set_session_id: None,
                        stealth: true,
                        max_tracked_requests: 500,
                        provider_env: Default::default(),
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
                    eprintln!("{text}");
                }
                flush_and_exit(1);
            }
        },
        BrowserCommands::Restart(mut cmd) => {
            // Same env-forwarding rule applies to restart, since stateful
            // providers re-mint their remote session and need fresh creds.
            cmd.provider_env =
                actionbook_cli::browser::session::provider::collect_provider_env_from_process();
            BrowserCommands::Restart(cmd)
        }
        BrowserCommands::Eval(mut cmd) => {
            // stdin and the file's relative path belong to the CLI process,
            // not the daemon's — resolve here so that positional `-` reads
            // the user's pipe and --file is opened against the user's cwd.
            match interaction::eval::resolve_eval_source(&cmd) {
                Ok(expr) => {
                    cmd.expression = Some(expr);
                    cmd.file = None;
                    BrowserCommands::Eval(cmd)
                }
                Err(result) => {
                    let failed_command = BrowserCommands::Eval(cmd);
                    let duration = start.elapsed();
                    let context = failed_command.context(&result);
                    let command_name = failed_command.command_name().to_string();
                    if json_mode {
                        let envelope =
                            JsonEnvelope::from_result(&command_name, context, &result, duration);
                        println!("{}", serde_json::to_string(&envelope)?);
                    } else {
                        let text = output::format_text(&command_name, &context, &result);
                        eprintln!("{text}");
                    }
                    flush_and_exit(1);
                }
            }
        }
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
                eprintln!("{text}");
            }
            flush_and_exit(1);
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
                let result = if command_name == interaction::eval::COMMAND_NAME {
                    interaction::eval::timeout_result(timeout_ms)
                } else if is_navigation_command(&command_name) {
                    let reason = format!("{command_name} timed out after {timeout_ms}ms");
                    ActionResult::fatal_with_details(
                        CdpErrorCode::NavTimeout.code(),
                        reason.clone(),
                        CdpErrorCode::NavTimeout.default_hint(),
                        json!({ "reason": reason, "timeout_ms": timeout_ms }),
                    )
                } else {
                    ActionResult::fatal_with_hint(
                        "TIMEOUT",
                        format!("{command_name} timed out after {timeout_ms}ms"),
                        "increase --timeout or retry the command",
                    )
                };
                let duration = start.elapsed();
                let context = command.context(&result);
                if json_mode {
                    let envelope =
                        JsonEnvelope::from_result(&command_name, context, &result, duration);
                    println!("{}", serde_json::to_string(&envelope)?);
                } else {
                    let text = output::format_text(&command_name, &context, &result);
                    eprintln!("{text}");
                }
                flush_and_exit(1);
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
        if result.is_ok() {
            println!("{text}");
        } else {
            eprintln!("{text}");
        }
    }

    if !result.is_ok() {
        flush_and_exit(1);
    }

    Ok(())
}

async fn handle_daemon(
    command: DaemonCommands,
    json_mode: bool,
    timeout_ms: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    match command {
        DaemonCommands::Restart => {
            // Honor --timeout: restart_daemon_now can block up to ~15s
            // (5s SIGTERM wait + 10s readiness wait). Without an outer
            // timeout, automation that sets --timeout to cap latency is
            // ignored for this command.
            let restart_call = actionbook_cli::utils::client::restart_daemon_now();
            let outcome = if let Some(ms) = timeout_ms {
                match tokio::time::timeout(Duration::from_millis(ms), restart_call).await {
                    Ok(r) => r,
                    Err(_) => Err(actionbook_cli::error::CliError::Internal(format!(
                        "daemon restart timed out after {ms}ms"
                    ))),
                }
            } else {
                restart_call.await
            };
            let duration = start.elapsed();
            match outcome {
                Ok(()) => {
                    if json_mode {
                        let envelope = JsonEnvelope::success(
                            "daemon restart",
                            None,
                            json!({ "status": "restarted" }),
                            duration,
                        );
                        println!("{}", serde_json::to_string(&envelope)?);
                    } else {
                        println!("daemon restarted");
                    }
                }
                Err(e) => {
                    if json_mode {
                        let result = ActionResult::fatal("DAEMON_RESTART_FAILED", e.to_string());
                        let envelope =
                            JsonEnvelope::from_result("daemon restart", None, &result, duration);
                        println!("{}", serde_json::to_string(&envelope)?);
                    } else {
                        eprintln!("error DAEMON_RESTART_FAILED: {e}");
                    }
                    flush_and_exit(1);
                }
            }
        }
    }
    Ok(())
}

async fn handle_extension(
    command: ExtensionCommands,
    json_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    let (command_name, result) = match command {
        ExtensionCommands::Status => {
            let action = Action::ExtensionStatus(actionbook_cli::extension::status::Cmd::default());
            let mut client = DaemonClient::connect().await?;
            let result = client.send_action(&action).await?;
            (actionbook_cli::extension::status::COMMAND_NAME, result)
        }
        ExtensionCommands::Ping => {
            let result = actionbook_cli::extension::ping::execute().await;
            (actionbook_cli::extension::ping::COMMAND_NAME, result)
        }
        ExtensionCommands::Path => {
            let result = actionbook_cli::extension::installer::execute_path();
            (
                actionbook_cli::extension::installer::COMMAND_NAME_PATH,
                result,
            )
        }
        ExtensionCommands::Install(args) => {
            let result = actionbook_cli::extension::installer::execute_install(args.force);
            (
                actionbook_cli::extension::installer::COMMAND_NAME_INSTALL,
                result,
            )
        }
        ExtensionCommands::Uninstall => {
            let result = actionbook_cli::extension::installer::execute_uninstall();
            (
                actionbook_cli::extension::installer::COMMAND_NAME_UNINSTALL,
                result,
            )
        }
    };

    let duration = start.elapsed();

    if json_mode {
        let envelope = JsonEnvelope::from_result(command_name, None, &result, duration);
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        let text = output::format_text(command_name, &None, &result);
        if result.is_ok() {
            println!("{text}");
        } else {
            eprintln!("{text}");
        }
    }

    if !result.is_ok() {
        flush_and_exit(1);
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
  search            Search for action manuals by keyword
  manual            Get detailed manual for a site, group, or action (alias: man)
  browser           Control browser sessions, tabs, and page interactions
  extension         Manage the Chrome extension (status, ping, install, uninstall, path)
  daemon restart    Stop the running daemon (next CLI call auto-respawns one)
  setup             Configure actionbook (or --target <agent> for quick skills install)
  help       Show this help
  --version  Show version

Global flags:
  --json          Output as JSON envelope
  --timeout <ms>  Set command timeout

Quick start:
  actionbook browser start --set-session-id s1
  actionbook browser goto https://airbnb.com --session s1 --tab t1
  actionbook browser snapshot --session s1 --tab t1
  actionbook browser fill @e3 \"San Francisco\" --session s1 --tab t1
  actionbook browser click @e7 --session s1 --tab t1
  actionbook browser wait navigation --session s1 --tab t1

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
  new-tab <url>...    --session      Open one or more tabs (alias: open)
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

Network:
  network requests    --session --tab  List tracked network requests
  network request <id>  --session --tab  Get detail for a single request (incl. body)
  network har start   --session --tab  Start HAR recording for a tab
  network har stop    --session --tab  Stop recording and export HAR 1.2 file

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

Batch:
  batch-new-tab --urls <url...>  --session  Open multiple tabs (alias: batch-open)
  batch-snapshot --tabs <tab...>  --session  Snapshot multiple tabs
  batch-click <sel...>  --session --tab  Click multiple elements sequentially

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
