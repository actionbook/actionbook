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

/// Apply a key-value pair to the config object. Returns an error for unknown
/// keys or invalid values. Does NOT save the config — caller is responsible.
fn apply_config_set(config: &mut Config, key: &str, value: &str) -> Result<()> {
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
        "updates.enabled" => {
            config.updates.enabled = value.parse().map_err(|_| {
                ActionbookError::ConfigError("updates.enabled must be true or false".to_string())
            })?
        }
        "updates.check_interval_seconds" => {
            config.updates.check_interval_seconds = value.parse().map_err(|_| {
                ActionbookError::ConfigError(
                    "updates.check_interval_seconds must be a positive integer".to_string(),
                )
            })?
        }
        _ => {
            return Err(ActionbookError::ConfigError(format!(
                "Unknown config key: {}",
                key
            )))
        }
    }
    Ok(())
}

/// Retrieve a config value by key. Returns `Ok(Some(value))` for set keys,
/// `Ok(None)` for unset optional keys, or an error for unknown keys.
fn get_config_value(config: &Config, key: &str) -> Result<Option<String>> {
    match key {
        "api.base_url" => Ok(Some(config.api.base_url.clone())),
        "api.api_key" => Ok(config.api.api_key.clone()),
        "browser.executable" => Ok(config.browser.executable.clone()),
        "browser.default_profile" => Ok(Some(config.browser.default_profile.clone())),
        "browser.headless" => Ok(Some(config.browser.headless.to_string())),
        "updates.enabled" => Ok(Some(config.updates.enabled.to_string())),
        "updates.check_interval_seconds" => {
            Ok(Some(config.updates.check_interval_seconds.to_string()))
        }
        _ => Err(ActionbookError::ConfigError(format!(
            "Unknown config key: {}",
            key
        ))),
    }
}

async fn set(_cli: &Cli, key: &str, value: &str) -> Result<()> {
    let mut config = Config::load()?;
    apply_config_set(&mut config, key, value)?;
    config.save()?;
    println!("{} Set {} = {}", "✓".green(), key, value);
    Ok(())
}

async fn get(cli: &Cli, key: &str) -> Result<()> {
    let config = Config::load()?;
    let value = get_config_value(&config, key)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_set_api_base_url() {
        let mut config = Config::default();
        apply_config_set(&mut config, "api.base_url", "https://custom.api").unwrap();
        assert_eq!(config.api.base_url, "https://custom.api");
    }

    #[test]
    fn apply_set_api_key() {
        let mut config = Config::default();
        apply_config_set(&mut config, "api.api_key", "sk-test").unwrap();
        assert_eq!(config.api.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn apply_set_browser_executable() {
        let mut config = Config::default();
        apply_config_set(&mut config, "browser.executable", "/usr/bin/chrome").unwrap();
        assert_eq!(
            config.browser.executable.as_deref(),
            Some("/usr/bin/chrome")
        );
    }

    #[test]
    fn apply_set_browser_default_profile() {
        let mut config = Config::default();
        apply_config_set(&mut config, "browser.default_profile", "work").unwrap();
        assert_eq!(config.browser.default_profile, "work");
    }

    #[test]
    fn apply_set_browser_headless_true() {
        let mut config = Config::default();
        apply_config_set(&mut config, "browser.headless", "true").unwrap();
        assert!(config.browser.headless);
    }

    #[test]
    fn apply_set_browser_headless_false() {
        let mut config = Config::default();
        config.browser.headless = true;
        apply_config_set(&mut config, "browser.headless", "false").unwrap();
        assert!(!config.browser.headless);
    }

    #[test]
    fn apply_set_browser_headless_invalid() {
        let mut config = Config::default();
        let result = apply_config_set(&mut config, "browser.headless", "yes");
        assert!(result.is_err());
    }

    #[test]
    fn apply_set_updates_enabled() {
        let mut config = Config::default();
        apply_config_set(&mut config, "updates.enabled", "false").unwrap();
        assert!(!config.updates.enabled);
    }

    #[test]
    fn apply_set_updates_enabled_invalid() {
        let mut config = Config::default();
        let result = apply_config_set(&mut config, "updates.enabled", "nope");
        assert!(result.is_err());
    }

    #[test]
    fn apply_set_check_interval() {
        let mut config = Config::default();
        apply_config_set(&mut config, "updates.check_interval_seconds", "3600").unwrap();
        assert_eq!(config.updates.check_interval_seconds, 3600);
    }

    #[test]
    fn apply_set_check_interval_invalid() {
        let mut config = Config::default();
        let result = apply_config_set(&mut config, "updates.check_interval_seconds", "abc");
        assert!(result.is_err());
    }

    #[test]
    fn apply_set_unknown_key_error() {
        let mut config = Config::default();
        let result = apply_config_set(&mut config, "nonexistent.key", "value");
        assert!(result.is_err());
    }

    #[test]
    fn get_value_api_base_url() {
        let config = Config::default();
        let value = get_config_value(&config, "api.base_url").unwrap();
        assert_eq!(value.as_deref(), Some("https://api.actionbook.dev"));
    }

    #[test]
    fn get_value_api_key_none_by_default() {
        let config = Config::default();
        let value = get_config_value(&config, "api.api_key").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn get_value_browser_executable_none_by_default() {
        let config = Config::default();
        let value = get_config_value(&config, "browser.executable").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn get_value_browser_default_profile() {
        let config = Config::default();
        let value = get_config_value(&config, "browser.default_profile").unwrap();
        assert_eq!(value.as_deref(), Some("actionbook"));
    }

    #[test]
    fn get_value_browser_headless() {
        let config = Config::default();
        let value = get_config_value(&config, "browser.headless").unwrap();
        assert_eq!(value.as_deref(), Some("false"));
    }

    #[test]
    fn get_value_updates_enabled() {
        let config = Config::default();
        let value = get_config_value(&config, "updates.enabled").unwrap();
        assert_eq!(value.as_deref(), Some("true"));
    }

    #[test]
    fn get_value_check_interval() {
        let config = Config::default();
        let value = get_config_value(&config, "updates.check_interval_seconds").unwrap();
        assert!(value.is_some());
    }

    #[test]
    fn get_value_unknown_key_error() {
        let config = Config::default();
        let result = get_config_value(&config, "unknown.key");
        assert!(result.is_err());
    }

    #[test]
    fn set_then_get_round_trip() {
        let mut config = Config::default();
        apply_config_set(&mut config, "api.base_url", "https://example.com").unwrap();
        let value = get_config_value(&config, "api.base_url").unwrap();
        assert_eq!(value.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn set_then_get_boolean_round_trip() {
        let mut config = Config::default();
        apply_config_set(&mut config, "browser.headless", "true").unwrap();
        let value = get_config_value(&config, "browser.headless").unwrap();
        assert_eq!(value.as_deref(), Some("true"));
    }

    #[test]
    fn set_then_get_integer_round_trip() {
        let mut config = Config::default();
        apply_config_set(&mut config, "updates.check_interval_seconds", "7200").unwrap();
        let value = get_config_value(&config, "updates.check_interval_seconds").unwrap();
        assert_eq!(value.as_deref(), Some("7200"));
    }
}
