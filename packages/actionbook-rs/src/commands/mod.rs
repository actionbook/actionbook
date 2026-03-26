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
