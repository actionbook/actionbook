use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, Commands};
use crate::config::Config;

const GITHUB_REPO: &str = "actionbook/actionbook";
const RELEASE_TAG_PREFIX: &str = "actionbook-cli-v";
const CACHE_FILE_NAME: &str = "update-check.json";
const HTTP_TIMEOUT_SECS: u64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UpdateCache {
    last_checked_unix: u64,
    latest_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallChannel {
    Brew,
    Script,
    Npm,
    Cargo,
    Unknown,
}

pub async fn maybe_notify(cli: &Cli) {
    if !should_check_for_command(cli) {
        return;
    }

    if update_check_disabled() {
        return;
    }

    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(_) => return,
    };

    if !config.updates.enabled {
        return;
    }

    let current = match Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(v) => v,
        Err(_) => return,
    };

    let now = now_unix();
    let cache_path = cache_path();
    let interval = config.updates.check_interval_seconds.max(300);
    let mut cache = load_cache(&cache_path).unwrap_or_default();

    // Within cache window: only use cached latest version to avoid network.
    if now.saturating_sub(cache.last_checked_unix) < interval {
        if let Some(latest_str) = cache.latest_version.as_deref() {
            if let Ok(latest) = Version::parse(latest_str) {
                if latest > current {
                    print_update_notice(&current, &latest, detect_install_channel());
                }
            }
        }
        return;
    }

    // Slow path: fetch latest version from GitHub API.
    match fetch_latest_version().await {
        Ok(latest) => {
            cache.last_checked_unix = now;
            cache.latest_version = Some(latest.to_string());
            let _ = save_cache(&cache_path, &cache);

            if latest > current {
                print_update_notice(&current, &latest, detect_install_channel());
            }
        }
        Err(_) => {
            // Network / API failure should never block command execution.
            // Still bump last_checked for backoff to avoid repeated fetches.
            cache.last_checked_unix = now;
            let _ = save_cache(&cache_path, &cache);
        }
    }
}

fn should_check_for_command(cli: &Cli) -> bool {
    if cli.json {
        return false;
    }

    // Avoid noisy checks for business execution commands used by agents.
    matches!(
        cli.command,
        Commands::Setup { .. }
            | Commands::Config { .. }
            | Commands::Profile { .. }
            | Commands::Extension { .. }
    )
}

fn update_check_disabled() -> bool {
    env_bool("ACTIONBOOK_NO_UPDATE_CHECK") || env_bool("ACTIONBOOK_DISABLE_UPDATE_NOTIFIER")
}

fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

async fn fetch_latest_version() -> Result<Version, ()> {
    // The repository can contain many non-CLI releases. Paginate to avoid
    // missing the latest `actionbook-cli-v*` tag when it is not on page 1.
    const PER_PAGE: u32 = 100;
    const MAX_PAGES: u32 = 5;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|_| ())?;

    let mut latest: Option<Version> = None;

    for page in 1..=MAX_PAGES {
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page={}&page={}",
            GITHUB_REPO, PER_PAGE, page
        );

        let resp = client
            .get(url)
            .header("User-Agent", "actionbook-update-notifier")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|_| ())?;

        if !resp.status().is_success() {
            return Err(());
        }

        let releases: Vec<serde_json::Value> = resp.json().await.map_err(|_| ())?;
        if releases.is_empty() {
            break;
        }

        if let Some(page_latest) = max_cli_version_in_releases(&releases) {
            latest = match latest {
                Some(current) => Some(std::cmp::max(current, page_latest)),
                None => Some(page_latest),
            };
        }
    }

    latest.ok_or(())
}

fn max_cli_version_in_releases(releases: &[serde_json::Value]) -> Option<Version> {
    let mut latest: Option<Version> = None;

    for release in releases {
        let is_draft = release
            .get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_prerelease = release
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_draft || is_prerelease {
            continue;
        }

        let tag = release
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !tag.starts_with(RELEASE_TAG_PREFIX) {
            continue;
        }

        let version_str = tag.trim_start_matches(RELEASE_TAG_PREFIX);
        if let Ok(version) = Version::parse(version_str) {
            latest = match latest {
                Some(current) => Some(std::cmp::max(current, version)),
                None => Some(version),
            };
        }
    }

    latest
}

fn detect_install_channel() -> InstallChannel {
    if let Ok(channel) = std::env::var("ACTIONBOOK_INSTALL_CHANNEL") {
        match channel.trim().to_ascii_lowercase().as_str() {
            "brew" => return InstallChannel::Brew,
            "script" => return InstallChannel::Script,
            "npm" => return InstallChannel::Npm,
            "cargo" => return InstallChannel::Cargo,
            _ => {}
        }
    }

    let exe_path = std::env::current_exe()
        .ok()
        .unwrap_or_else(|| PathBuf::from(""));
    let path = exe_path.to_string_lossy().to_ascii_lowercase();

    if path.contains("/cellar/actionbook/") || path.contains("homebrew") {
        return InstallChannel::Brew;
    }
    if path.contains("node_modules") || path.contains(".nvm/") || path.contains("npm") {
        return InstallChannel::Npm;
    }
    if path.contains("/.cargo/bin/") {
        return InstallChannel::Cargo;
    }
    if path.contains("/.actionbook/bin/") || path == "/usr/local/bin/actionbook" {
        return InstallChannel::Script;
    }

    InstallChannel::Unknown
}

fn upgrade_command(channel: InstallChannel) -> &'static str {
    match channel {
        InstallChannel::Brew => "brew upgrade actionbook",
        InstallChannel::Script => "curl -fsSL https://actionbook.dev/install.sh | bash",
        InstallChannel::Npm => "npm install -g @actionbookdev/cli",
        InstallChannel::Cargo => "cargo install actionbook --locked",
        InstallChannel::Unknown => {
            "See release notes: https://github.com/actionbook/actionbook/releases"
        }
    }
}

fn print_update_notice(current: &Version, latest: &Version, channel: InstallChannel) {
    eprintln!(
        "\n[update] A newer Actionbook CLI is available: {} -> {}",
        current, latest
    );
    eprintln!("         Upgrade with: {}", upgrade_command(channel));
    eprintln!(
        "         Disable checks: ACTIONBOOK_NO_UPDATE_CHECK=1 or `actionbook config set updates.enabled false`"
    );
}

fn cache_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".actionbook")
        .join(CACHE_FILE_NAME)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_cache(path: &PathBuf) -> Result<UpdateCache, ()> {
    let text = std::fs::read_to_string(path).map_err(|_| ())?;
    serde_json::from_str::<UpdateCache>(&text).map_err(|_| ())
}

fn save_cache(path: &PathBuf, cache: &UpdateCache) -> Result<(), ()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }

    let text = serde_json::to_string(cache).map_err(|_| ())?;
    std::fs::write(path, text).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_bool_parses_common_truthy_values() {
        std::env::set_var("ACTIONBOOK_TEST_BOOL", "true");
        assert!(env_bool("ACTIONBOOK_TEST_BOOL"));

        std::env::set_var("ACTIONBOOK_TEST_BOOL", "1");
        assert!(env_bool("ACTIONBOOK_TEST_BOOL"));

        std::env::set_var("ACTIONBOOK_TEST_BOOL", "yes");
        assert!(env_bool("ACTIONBOOK_TEST_BOOL"));

        std::env::set_var("ACTIONBOOK_TEST_BOOL", "off");
        assert!(!env_bool("ACTIONBOOK_TEST_BOOL"));
    }

    #[test]
    fn upgrade_command_matches_channel() {
        assert_eq!(
            upgrade_command(InstallChannel::Brew),
            "brew upgrade actionbook"
        );
        assert_eq!(
            upgrade_command(InstallChannel::Script),
            "curl -fsSL https://actionbook.dev/install.sh | bash"
        );
        assert_eq!(
            upgrade_command(InstallChannel::Npm),
            "npm install -g @actionbookdev/cli"
        );
    }

    #[test]
    fn max_cli_version_uses_semver_max_not_first_match() {
        let releases = vec![
            serde_json::json!({ "tag_name": "actionbook-cli-v0.8.2", "draft": false, "prerelease": false }),
            serde_json::json!({ "tag_name": "actionbook-cli-v0.8.10", "draft": false, "prerelease": false }),
            serde_json::json!({ "tag_name": "actionbook-cli-v0.8.3", "draft": false, "prerelease": false }),
        ];

        let latest = max_cli_version_in_releases(&releases).unwrap();
        assert_eq!(latest, Version::parse("0.8.10").unwrap());
    }

    #[test]
    fn max_cli_version_ignores_non_cli_and_prerelease_tags() {
        let releases = vec![
            serde_json::json!({ "tag_name": "actionbook-extension-v0.1.0", "draft": false, "prerelease": false }),
            serde_json::json!({ "tag_name": "actionbook-cli-v0.9.0", "draft": false, "prerelease": true }),
            serde_json::json!({ "tag_name": "actionbook-cli-v0.8.9", "draft": false, "prerelease": false }),
        ];

        let latest = max_cli_version_in_releases(&releases).unwrap();
        assert_eq!(latest, Version::parse("0.8.9").unwrap());
    }

    #[test]
    fn cache_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let cache = UpdateCache {
            last_checked_unix: 123,
            latest_version: Some("0.8.3".to_string()),
        };

        save_cache(&path, &cache).unwrap();
        let loaded = load_cache(&path).unwrap();

        assert_eq!(loaded.last_checked_unix, 123);
        assert_eq!(loaded.latest_version.as_deref(), Some("0.8.3"));
    }

    #[test]
    fn upgrade_command_cargo_and_unknown() {
        assert_eq!(
            upgrade_command(InstallChannel::Cargo),
            "cargo install actionbook --locked"
        );
        assert_eq!(
            upgrade_command(InstallChannel::Unknown),
            "See release notes: https://github.com/actionbook/actionbook/releases"
        );
    }

    #[test]
    fn env_bool_returns_false_when_var_unset() {
        std::env::remove_var("ACTIONBOOK_TEST_BOOL_UNSET_XYZ");
        assert!(!env_bool("ACTIONBOOK_TEST_BOOL_UNSET_XYZ"));
    }

    #[test]
    fn env_bool_parses_on_and_false_values() {
        std::env::set_var("ACTIONBOOK_TEST_BOOL_ON", "on");
        assert!(env_bool("ACTIONBOOK_TEST_BOOL_ON"));

        std::env::set_var("ACTIONBOOK_TEST_BOOL_FALSE", "false");
        assert!(!env_bool("ACTIONBOOK_TEST_BOOL_FALSE"));

        std::env::set_var("ACTIONBOOK_TEST_BOOL_ZERO", "0");
        assert!(!env_bool("ACTIONBOOK_TEST_BOOL_ZERO"));
    }

    #[test]
    fn max_cli_version_returns_none_for_empty_list() {
        let result = max_cli_version_in_releases(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn max_cli_version_skips_draft_releases() {
        let releases = vec![
            serde_json::json!({ "tag_name": "actionbook-cli-v1.0.0", "draft": true, "prerelease": false }),
            serde_json::json!({ "tag_name": "actionbook-cli-v0.9.9", "draft": false, "prerelease": false }),
        ];
        let latest = max_cli_version_in_releases(&releases).unwrap();
        assert_eq!(latest, Version::parse("0.9.9").unwrap());
    }

    #[test]
    fn max_cli_version_skips_invalid_semver_tags() {
        let releases = vec![
            serde_json::json!({ "tag_name": "actionbook-cli-vnot-semver", "draft": false, "prerelease": false }),
            serde_json::json!({ "tag_name": "actionbook-cli-v0.5.0", "draft": false, "prerelease": false }),
        ];
        let latest = max_cli_version_in_releases(&releases).unwrap();
        assert_eq!(latest, Version::parse("0.5.0").unwrap());
    }

    #[test]
    fn now_unix_returns_nonzero_timestamp() {
        let ts = now_unix();
        // Should be a reasonable Unix timestamp (after year 2020 = 1577836800)
        assert!(ts > 1_577_836_800);
    }

    #[test]
    fn load_cache_returns_err_for_missing_file() {
        let path = std::path::PathBuf::from("/tmp/actionbook-nonexistent-cache-xyz.json");
        assert!(load_cache(&path).is_err());
    }

    #[test]
    fn load_cache_returns_err_for_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad-cache.json");
        std::fs::write(&path, "{not valid json}").unwrap();
        assert!(load_cache(&path).is_err());
    }

    #[test]
    fn save_cache_creates_parent_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("cache.json");
        let cache = UpdateCache {
            last_checked_unix: 999,
            latest_version: None,
        };
        assert!(save_cache(&path, &cache).is_ok());
        let loaded = load_cache(&path).unwrap();
        assert_eq!(loaded.last_checked_unix, 999);
        assert!(loaded.latest_version.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn detect_install_channel_uses_env_var() {
        std::env::set_var("ACTIONBOOK_INSTALL_CHANNEL", "brew");
        assert_eq!(detect_install_channel(), InstallChannel::Brew);

        std::env::set_var("ACTIONBOOK_INSTALL_CHANNEL", "npm");
        assert_eq!(detect_install_channel(), InstallChannel::Npm);

        std::env::set_var("ACTIONBOOK_INSTALL_CHANNEL", "cargo");
        assert_eq!(detect_install_channel(), InstallChannel::Cargo);

        std::env::set_var("ACTIONBOOK_INSTALL_CHANNEL", "script");
        assert_eq!(detect_install_channel(), InstallChannel::Script);

        std::env::set_var("ACTIONBOOK_INSTALL_CHANNEL", "unknown_channel");
        // Falls through to path-based detection; just verify it doesn't panic.
        let _ = detect_install_channel();

        std::env::remove_var("ACTIONBOOK_INSTALL_CHANNEL");
    }

    #[test]
    fn install_channel_equality() {
        assert_eq!(InstallChannel::Brew, InstallChannel::Brew);
        assert_ne!(InstallChannel::Brew, InstallChannel::Npm);
        assert_ne!(InstallChannel::Cargo, InstallChannel::Script);
        assert_eq!(InstallChannel::Unknown, InstallChannel::Unknown);
    }
}
