mod api;
mod browser;
mod cli;
mod commands;
mod config;
mod daemon;
mod error;
mod update_notifier;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use cli::Cli;
use error::{ActionbookError, Result};

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

    // Route browser commands through the daemon.
    // If args contain "browser", always use the daemon CLI — never fall through
    // to the legacy CLI (which no longer has a browser subcommand).
    {
        use clap::Parser as _;
        let has_browser_arg = args.iter().any(|a| a == "browser" || a == "b");
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
