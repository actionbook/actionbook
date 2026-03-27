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
#[allow(dead_code)]
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
    use clap::Parser as _;

    fn cli_with_profile(profile: Option<&str>) -> Cli {
        let mut args = vec!["actionbook"];
        if let Some(p) = profile {
            args.push("--profile");
            args.push(p);
        }
        args.push("config");
        args.push("show");
        Cli::try_parse_from(args).unwrap()
    }

    fn config_with_default_profile(name: &str) -> Config {
        Config {
            browser: crate::config::BrowserConfig {
                default_profile: name.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn cli_profile_takes_highest_priority() {
        let cli = cli_with_profile(Some("from-cli"));
        let config = config_with_default_profile("from-config");
        assert_eq!(effective_profile_name(&cli, &config), "from-cli");
    }

    #[test]
    fn config_default_profile_used_when_cli_absent() {
        let cli = cli_with_profile(None);
        let config = config_with_default_profile("from-config");
        assert_eq!(effective_profile_name(&cli, &config), "from-config");
    }

    #[test]
    fn falls_back_to_actionbook_when_both_empty() {
        let cli = cli_with_profile(None);
        let config = config_with_default_profile("");
        assert_eq!(effective_profile_name(&cli, &config), "actionbook");
    }

    #[test]
    fn cli_whitespace_only_profile_ignored() {
        let cli = cli_with_profile(Some("   "));
        let config = config_with_default_profile("from-config");
        assert_eq!(effective_profile_name(&cli, &config), "from-config");
    }

    #[test]
    fn config_whitespace_only_profile_falls_back() {
        let cli = cli_with_profile(None);
        let config = config_with_default_profile("   ");
        assert_eq!(effective_profile_name(&cli, &config), "actionbook");
    }

    #[test]
    fn cli_profile_trimmed() {
        let cli = cli_with_profile(Some("  my-profile  "));
        let config = config_with_default_profile("other");
        assert_eq!(effective_profile_name(&cli, &config), "my-profile");
    }
}
