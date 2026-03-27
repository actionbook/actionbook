mod api;
mod browser;
mod cli;
mod commands;
mod config;
#[cfg(unix)]
mod daemon;
mod error;
mod update_notifier;

use std::ffi::OsString;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use cli::Cli;
use error::{ActionbookError, Result};

const ROOT_PRD_VERSION: &str = "1.0.0";

fn print_root_contract_output(json: bool, text: &str) {
    let text = text.trim_end();
    if json {
        println!(
            "{}",
            serde_json::to_string(text).expect("serialize root contract string")
        );
    } else {
        println!("{text}");
    }
}

#[cfg(unix)]
fn render_browser_help(path: &[String]) -> Option<String> {
    let mut args = vec![OsString::from("actionbook"), OsString::from("browser")];
    args.extend(path.iter().cloned().map(OsString::from));
    args.push(OsString::from("--help"));
    daemon::cli_v2::CliV2::render_augmented_help(args)
}

fn maybe_handle_root_help_or_version(args: &[String]) -> bool {
    let json = args.iter().skip(1).any(|arg| arg == "--json");
    let positionals: Vec<&str> = args
        .iter()
        .skip(1)
        .filter(|arg| arg.as_str() != "--json")
        .map(String::as_str)
        .collect();

    match positionals.as_slice() {
        ["--version"] | ["-V"] => {
            print_root_contract_output(json, ROOT_PRD_VERSION);
            true
        }
        ["help"] => {
            #[cfg(unix)]
            if let Some(help) = render_browser_help(&[]) {
                print_root_contract_output(json, &help);
                return true;
            }
            false
        }
        ["help", "browser"] => {
            #[cfg(unix)]
            if let Some(help) = render_browser_help(&[]) {
                print_root_contract_output(json, &help);
                return true;
            }
            false
        }
        [first, second, rest @ ..] if *first == "help" && *second == "browser" => {
            #[cfg(unix)]
            if let Some(help) = render_browser_help(
                &rest
                    .iter()
                    .map(|segment| (*segment).to_string())
                    .collect::<Vec<_>>(),
            ) {
                print_root_contract_output(json, &help);
                return true;
            }
            false
        }
        _ => false,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Check if invoked as Chrome Native Messaging host.
    // Chrome passes "chrome-extension://<id>/" as the first argument.
    let args: Vec<String> = std::env::args().collect();
    let is_native_messaging = args.len() >= 2
        && browser::native_messaging::EXTENSION_IDS
            .iter()
            .any(|id| args[1] == format!("chrome-extension://{}/", id));
    if is_native_messaging {
        return browser::native_messaging::run().await;
    }

    // Initialize tracing with filters to suppress noisy chromiumoxide errors
    // These errors are harmless - they occur when Chrome sends CDP events that
    // the library doesn't recognize (common with newer Chrome versions)
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info")
            .add_directive("chromiumoxide::conn=warn".parse().unwrap())
            .add_directive("chromiumoxide::handler=warn".parse().unwrap())
    });

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    if maybe_handle_root_help_or_version(&args) {
        return Ok(());
    }

    // Route browser commands through the daemon (Unix only).
    // If args contain "browser", always use the daemon CLI — never fall through
    // to the legacy CLI (which no longer has a browser subcommand).
    #[cfg(unix)]
    {
        // Only check the first positional arg (subcommand position), not all argv.
        // This avoids misrouting when "browser" or "b" appears as a search query value.
        let has_browser_arg = args
            .get(1)
            .map(|a| a.as_str() == "browser" || a.as_str() == "b")
            .unwrap_or(false);
        if has_browser_arg {
            if let Some(help) = daemon::cli_v2::CliV2::render_augmented_help(std::env::args_os()) {
                print!("{help}");
                std::process::exit(0);
            }
        }
        match daemon::cli_v2::CliV2::try_parse() {
            Ok(cli_v2) => {
                cli_v2.run().await;
            }
            Err(e) if has_browser_arg => {
                // User intended a browser command but it failed to parse.
                // Show the daemon CLI error (e.g. missing --session), not the
                // legacy CLI's "unrecognized subcommand" error.
                e.exit();
            }
            Err(_) => {
                // Not a browser command — fall through to legacy CLI.
            }
        }
    }

    let cli = Cli::parse();
    if let Err(e) = cli.run().await {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "success": false,
                    "error": {
                        "code": e.error_code(),
                        "message": e.to_string(),
                    }
                })
            );
        } else {
            // Some setup flows already print a full user-facing message block.
            let suppress_default_error_line = matches!(
                &e,
                ActionbookError::SetupError(msg)
                    if msg.trim() == "Extension setup incomplete"
            );
            if !suppress_default_error_line {
                eprintln!("Error: {}", e);
            }
        }
        std::process::exit(1);
    }
    Ok(())
}
