use colored::Colorize;

use crate::cli::{Cli, ProfileCommands};
use crate::config::{Config, ProfileConfig};
use crate::error::Result;

pub async fn run(cli: &Cli, command: &ProfileCommands) -> Result<()> {
    match command {
        ProfileCommands::List => list(cli).await,
        ProfileCommands::Create { name, cdp_port } => create(cli, name, *cdp_port).await,
        ProfileCommands::Delete { name } => delete(cli, name).await,
        ProfileCommands::Show { name } => show(cli, name).await,
    }
}

async fn list(cli: &Cli) -> Result<()> {
    let config = Config::load()?;

    if cli.json {
        let profiles: Vec<_> = config
            .profiles
            .iter()
            .map(|(name, profile)| {
                serde_json::json!({
                    "name": name,
                    "cdp_port": profile.cdp_port,
                    "headless": profile.headless,
                    "is_remote": profile.is_remote()
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&profiles)?);
    } else {
        println!("{}", "Profiles:".bold());
        println!();

        for (name, profile) in &config.profiles {
            let default_marker = if name == &config.browser.default_profile {
                " (default)".dimmed()
            } else {
                "".into()
            };

            println!("  {} {}{}", "●".cyan(), name.bold(), default_marker);
            println!("    CDP Port: {}", profile.cdp_port);

            if profile.is_remote() {
                if let Some(ref url) = profile.cdp_url {
                    println!("    CDP URL: {}", url.dimmed());
                }
            }

            if profile.headless {
                println!("    Mode: {}", "headless".dimmed());
            }

            println!();
        }

        // Always show configured default profile if it's implicit.
        let default_name = &config.browser.default_profile;
        if !config.profiles.contains_key(default_name) {
            let default_port = config
                .get_profile(default_name)
                .map(|p| p.cdp_port)
                .unwrap_or(9222);
            println!("  {} {} (implicit)", "●".cyan(), default_name.bold());
            println!("    CDP Port: {}", default_port);
            println!();
        }
    }

    Ok(())
}

async fn create(cli: &Cli, name: &str, cdp_port: Option<u16>) -> Result<()> {
    let mut config = Config::load()?;

    let profile = if let Some(port) = cdp_port {
        ProfileConfig::with_cdp_port(port)
    } else {
        // Auto-assign port based on existing profiles
        let max_port = config
            .profiles
            .values()
            .map(|p| p.cdp_port)
            .max()
            .unwrap_or(9221);
        ProfileConfig::with_cdp_port(max_port + 1)
    };

    config.set_profile(name, profile.clone());
    config.save()?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "name": name,
                "cdp_port": profile.cdp_port
            })
        );
    } else {
        println!(
            "{} Created profile: {} (CDP port: {})",
            "✓".green(),
            name.bold(),
            profile.cdp_port
        );
    }

    Ok(())
}

async fn delete(cli: &Cli, name: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.remove_profile(name)?;
    config.save()?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "name": name
            })
        );
    } else {
        println!("{} Deleted profile: {}", "✓".green(), name);
    }

    Ok(())
}

async fn show(cli: &Cli, name: &str) -> Result<()> {
    let config = Config::load()?;
    let profile = config.get_profile(name)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "name": name,
                "cdp_port": profile.cdp_port,
                "cdp_url": profile.cdp_url,
                "user_data_dir": profile.user_data_dir,
                "browser_path": profile.browser_path,
                "headless": profile.headless,
                "extra_args": profile.extra_args
            })
        );
    } else {
        println!("{} {}", "Profile:".bold(), name.cyan());
        println!();
        println!("  CDP Port: {}", profile.cdp_port);

        if let Some(ref url) = profile.cdp_url {
            println!("  CDP URL: {}", url);
        }

        if let Some(ref dir) = profile.user_data_dir {
            println!("  User Data: {}", dir);
        }

        if let Some(ref path) = profile.browser_path {
            println!("  Browser: {}", path);
        }

        println!("  Headless: {}", profile.headless);

        if !profile.extra_args.is_empty() {
            println!("  Extra Args: {}", profile.extra_args.join(" "));
        }
    }

    Ok(())
}
