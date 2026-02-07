use colored::Colorize;
use dialoguer::{Confirm, Password};

use super::detect::EnvironmentInfo;
use super::theme::setup_theme;
use crate::cli::Cli;
use crate::config::Config;
use crate::error::{ActionbookError, Result};

/// Configure the API key with priority: flag > env > config > interactive input.
///
/// In non-interactive mode without a key, skips gracefully.
/// When a key already exists, prompts the user to keep or replace it.
/// Supports skipping — users can configure the key later.
pub async fn configure_api_key(
    cli: &Cli,
    env: &EnvironmentInfo,
    api_key_flag: Option<&str>,
    non_interactive: bool,
    config: &mut Config,
) -> Result<()> {
    // Priority: flag > env > existing config
    let (existing_key, source) = resolve_existing_key(api_key_flag, env, config);

    if let Some(ref key) = existing_key {
        let masked = mask_key(key);

        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "api_key",
                    "status": "detected",
                    "source": source,
                    "masked_key": masked,
                })
            );
        } else {
            println!(
                "  {}  API key detected (from {}): {}",
                "◇".green(),
                source,
                masked.dimmed()
            );
        }

        // If from flag or non-interactive, just use it directly
        if api_key_flag.is_some() || non_interactive {
            config.api.api_key = existing_key;
            return Ok(());
        }

        // Interactive: ask if they want to change
        let keep = Confirm::with_theme(&setup_theme())
            .with_prompt(" Keep this API key?")
            .default(true)
            .report(false)
            .interact()
            .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

        if keep {
            config.api.api_key = existing_key;
            return Ok(());
        }

        // User rejected the key — clear it so it won't persist if they skip
        config.api.api_key = None;
    } else if non_interactive {
        // No key in non-interactive mode — skip gracefully
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "api_key",
                    "status": "skipped",
                })
            );
        } else {
            println!(
                "  {}  No API key configured. Use {} or set {} later.",
                "◇".dimmed(),
                "--api-key".cyan(),
                "ACTIONBOOK_API_KEY".cyan()
            );
        }
        return Ok(());
    } else {
        // No key — show helpful context before prompting
        if !cli.json {
            let bar = "│".dimmed();
            println!(
                "  {}  {}",
                bar,
                "Actionbook uses an API key to look up selectors and actions for you.".dimmed()
            );
            println!(
                "  {}  Don't have one yet? Grab it here: {}",
                bar,
                "https://actionbook.dev/dashboard".cyan().underline()
            );
            println!("  {}", bar);
        }
    }

    // Interactive input — leave blank to skip
    let key: String = Password::with_theme(&setup_theme())
        .with_prompt(" Enter your API key (leave blank to skip)")
        .allow_empty_password(true)
        .report(false)
        .interact()
        .map_err(|e| ActionbookError::SetupError(format!("Prompt failed: {}", e)))?;

    let key = key.trim().to_string();

    if key.is_empty() {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "step": "api_key",
                    "status": "skipped",
                })
            );
        } else {
            println!(
                "  {}  Skipped. You can configure it later with:",
                "◇".dimmed()
            );
            println!(
                "  {}  {}",
                "│".dimmed(),
                "actionbook config set api.api_key <your-key>".cyan()
            );
        }
        return Ok(());
    }

    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "step": "api_key",
                "status": "configured",
                "masked_key": mask_key(&key),
            })
        );
    } else {
        println!(
            "  {}  API key configured: {}",
            "◇".green(),
            mask_key(&key).dimmed()
        );
    }

    config.api.api_key = Some(key);
    Ok(())
}

/// Resolve the best available key and its source name.
fn resolve_existing_key(
    flag: Option<&str>,
    env: &EnvironmentInfo,
    config: &Config,
) -> (Option<String>, &'static str) {
    if let Some(key) = flag {
        return (Some(key.to_string()), "flag");
    }
    if let Some(ref key) = env.existing_api_key {
        return (Some(key.clone()), "env");
    }
    if let Some(ref key) = config.api.api_key {
        return (Some(key.clone()), "config");
    }
    (None, "none")
}

/// Mask an API key for display, showing only first 4 and last 4 chars.
pub(super) fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{}...{}", prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_key_long() {
        assert_eq!(mask_key("abcdefghijklmnop"), "abcd...mnop");
    }

    #[test]
    fn test_mask_key_short() {
        assert_eq!(mask_key("abcd"), "****");
    }

    #[test]
    fn test_mask_key_exactly_8() {
        assert_eq!(mask_key("abcdefgh"), "********");
    }

    #[test]
    fn test_resolve_flag_wins() {
        let env = EnvironmentInfo {
            os: String::new(),
            arch: String::new(),
            shell: None,
            browsers: vec![],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: Some("env_key".to_string()),
        };
        let config = Config::default();
        let (key, source) = resolve_existing_key(Some("flag_key"), &env, &config);
        assert_eq!(key.unwrap(), "flag_key");
        assert_eq!(source, "flag");
    }

    #[test]
    fn test_resolve_env_wins_over_config() {
        let env = EnvironmentInfo {
            os: String::new(),
            arch: String::new(),
            shell: None,
            browsers: vec![],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: Some("env_key".to_string()),
        };
        let mut config = Config::default();
        config.api.api_key = Some("config_key".to_string());
        let (key, source) = resolve_existing_key(None, &env, &config);
        assert_eq!(key.unwrap(), "env_key");
        assert_eq!(source, "env");
    }

    #[test]
    fn test_resolve_config_fallback() {
        let env = EnvironmentInfo {
            os: String::new(),
            arch: String::new(),
            shell: None,
            browsers: vec![],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: None,
        };
        let mut config = Config::default();
        config.api.api_key = Some("config_key".to_string());
        let (key, source) = resolve_existing_key(None, &env, &config);
        assert_eq!(key.unwrap(), "config_key");
        assert_eq!(source, "config");
    }

    #[test]
    fn test_resolve_none() {
        let env = EnvironmentInfo {
            os: String::new(),
            arch: String::new(),
            shell: None,
            browsers: vec![],
            npx_available: false,
            node_version: None,
            existing_config: false,
            existing_api_key: None,
        };
        let config = Config::default();
        let (key, source) = resolve_existing_key(None, &env, &config);
        assert!(key.is_none());
        assert_eq!(source, "none");
    }
}
