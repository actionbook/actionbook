mod api;
mod browser;
mod cli;
mod commands;
mod config;
mod error;

use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use cli::Cli;
use error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Check if invoked as Chrome Native Messaging host.
    // Chrome passes "chrome-extension://<id>/" as the first argument.
    let args: Vec<String> = std::env::args().collect();
    let expected_origin = format!(
        "chrome-extension://{}/",
        browser::native_messaging::EXTENSION_ID
    );
    if args.len() >= 2 && args[1] == expected_origin {
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

    let cli = Cli::parse();
    cli.run().await
}
