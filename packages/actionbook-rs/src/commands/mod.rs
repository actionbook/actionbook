pub mod act;
pub mod config;
#[cfg(unix)]
pub mod daemon;
pub mod extension;
pub mod get;
pub mod profile;
pub mod search;
pub mod setup;
pub mod sources;

use crate::cli::Cli;
use crate::config::Config;

/// Determine the effective profile name from CLI flags and config.
///
/// Priority: CLI --profile > config default_profile > "actionbook"
pub(crate) fn effective_profile_name<'a>(cli: &'a Cli, config: &'a Config) -> &'a str {
    cli.profile
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let default_profile = config.browser.default_profile.trim();
            if default_profile.is_empty() {
                None
            } else {
                Some(default_profile)
            }
        })
        .unwrap_or("actionbook")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApiConfig, BrowserConfig, UpdatesConfig};
    use std::collections::HashMap;

    fn cli_with_profile(profile: Option<&str>) -> Cli {
        let mut cli = Cli::try_parse_from(["actionbook", "search", "demo"]).unwrap();
        cli.profile = profile.map(ToOwned::to_owned);
        cli
    }

    fn config_with_default_profile(default_profile: &str) -> Config {
        Config {
            api: ApiConfig::default(),
            browser: BrowserConfig {
                default_profile: default_profile.to_string(),
                ..BrowserConfig::default()
            },
            updates: UpdatesConfig::default(),
            profiles: HashMap::new(),
        }
    }

    #[test]
    fn effective_profile_prefers_cli_value() {
        let cli = cli_with_profile(Some("  work  "));
        let config = config_with_default_profile("team");

        assert_eq!(effective_profile_name(&cli, &config), "work");
    }

    #[test]
    fn effective_profile_uses_config_default_when_cli_missing() {
        let cli = cli_with_profile(None);
        let config = config_with_default_profile("team");

        assert_eq!(effective_profile_name(&cli, &config), "team");
    }

    #[test]
    fn effective_profile_falls_back_to_actionbook_for_blank_values() {
        let cli = cli_with_profile(Some("   "));
        let config = config_with_default_profile("   ");

        assert_eq!(effective_profile_name(&cli, &config), "actionbook");
    }
}
