use colored::Colorize;
use dialoguer::Confirm;

use crate::cli::{Cli, ConfigCommands};
use crate::config::Config;
use crate::error::{ActionbookError, Result};

pub async fn run(cli: &Cli, command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Show => show(cli).await,
        ConfigCommands::Set { key, value } => set(cli, key, value).await,
        ConfigCommands::Get { key } => get(cli, key).await,
        ConfigCommands::Edit => edit(cli).await,
        ConfigCommands::Path => path(cli).await,
        ConfigCommands::Reset => reset(cli).await,
    }
}

async fn show(cli: &Cli) -> Result<()> {
    let config = Config::load()?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        let toml_str = toml::to_string_pretty(&config)
            .map_err(|e| ActionbookError::ConfigError(e.to_string()))?;
        println!("{}", toml_str);
    }

    Ok(())
}

async fn set(_cli: &Cli, key: &str, value: &str) -> Result<()> {
    let mut config = Config::load()?;

    // Simple key-value setting (expand as needed)
    match key {
        "api.base_url" => config.api.base_url = value.to_string(),
        "api.api_key" => config.api.api_key = Some(value.to_string()),
        "browser.executable" => config.browser.executable = Some(value.to_string()),
        "browser.default_profile" => config.browser.default_profile = value.to_string(),
        "browser.headless" => {
            config.browser.headless = value.parse().map_err(|_| {
                ActionbookError::ConfigError("headless must be true or false".to_string())
            })?
        }
        _ => {
            return Err(ActionbookError::ConfigError(format!(
                "Unknown config key: {}",
                key
            )))
        }
    }

    config.save()?;
    println!("{} Set {} = {}", "✓".green(), key, value);

    Ok(())
}

async fn get(cli: &Cli, key: &str) -> Result<()> {
    let config = Config::load()?;

    let value = match key {
        "api.base_url" => Some(config.api.base_url.clone()),
        "api.api_key" => config.api.api_key.clone(),
        "browser.executable" => config.browser.executable.clone(),
        "browser.default_profile" => Some(config.browser.default_profile.clone()),
        "browser.headless" => Some(config.browser.headless.to_string()),
        _ => {
            return Err(ActionbookError::ConfigError(format!(
                "Unknown config key: {}",
                key
            )))
        }
    };

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "key": key,
                "value": value
            })
        );
    } else {
        match value {
            Some(v) => println!("{}", v),
            None => println!("{}", "(not set)".dimmed()),
        }
    }

    Ok(())
}

async fn edit(_cli: &Cli) -> Result<()> {
    let path = Config::config_path();

    // Ensure config file exists
    if !path.exists() {
        let config = Config::default();
        config.save()?;
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    println!("Opening {} with {}", path.display(), editor);

    std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| ActionbookError::Other(format!("Failed to open editor: {}", e)))?;

    Ok(())
}

async fn reset(cli: &Cli) -> Result<()> {
    let path = Config::config_path();

    if !path.exists() {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "status": "no_config", "path": path.display().to_string() })
            );
        } else {
            println!("{} No config file to remove.", "✓".green());
        }
        return Ok(());
    }

    if !cli.json {
        let confirm = Confirm::new()
            .with_prompt(format!("Delete {}?", path.display()))
            .default(false)
            .interact()
            .map_err(|e| ActionbookError::Other(format!("Prompt failed: {}", e)))?;

        if !confirm {
            println!("  Cancelled.");
            return Ok(());
        }
    }

    std::fs::remove_file(&path)?;

    if cli.json {
        println!(
            "{}",
            serde_json::json!({ "status": "removed", "path": path.display().to_string() })
        );
    } else {
        println!(
            "{} Config removed: {}",
            "✓".green(),
            path.display().to_string().dimmed()
        );
    }

    Ok(())
}

async fn path(cli: &Cli) -> Result<()> {
    let path = Config::config_path();

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "path": path.display().to_string()
            })
        );
    } else {
        println!("{}", path.display());
    }

    Ok(())
}
