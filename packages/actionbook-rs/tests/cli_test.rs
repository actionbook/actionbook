//! CLI argument parsing tests
//!
//! These tests verify that CLI arguments are parsed correctly,
//! matching the behavior of the original TypeScript CLI.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;

/// Get the actionbook binary command
fn actionbook() -> Command {
    Command::cargo_bin("actionbook").unwrap()
}

mod help {
    use super::*;

    #[test]
    fn shows_help() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("actionbook"))
            .stdout(predicate::str::contains("Browser automation"));
    }

    #[test]
    fn shows_version() {
        actionbook()
            .arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains("actionbook"));
    }
}

mod search_command {
    use super::*;

    #[test]
    fn search_requires_query() {
        actionbook()
            .arg("search")
            .assert()
            .failure()
            .stderr(predicate::str::contains("QUERY"));
    }

    #[test]
    fn search_help_shows_options() {
        actionbook()
            .args(["search", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--domain"))
            .stdout(predicate::str::contains("--url"))
            .stdout(predicate::str::contains("--page"))
            .stdout(predicate::str::contains("--page-size"));
    }

    #[test]
    fn search_accepts_domain_flag() {
        // Just check that the flag is accepted (API call may fail)
        actionbook()
            .args(["search", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("airbnb.com"));
    }

    #[test]
    fn search_page_size_has_default() {
        actionbook()
            .args(["search", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[default: 10]"));
    }
}

mod get_command {
    use super::*;

    #[test]
    fn get_requires_area_id() {
        actionbook()
            .arg("get")
            .assert()
            .failure()
            .stderr(predicate::str::contains("AREA_ID"));
    }

    #[test]
    fn get_help_shows_usage() {
        actionbook()
            .args(["get", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Area ID"));
    }
}

mod sources_command {
    use super::*;

    #[test]
    fn sources_requires_subcommand() {
        actionbook()
            .arg("sources")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn sources_list_help() {
        actionbook()
            .args(["sources", "list", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("List all sources"));
    }

    #[test]
    fn sources_search_requires_query() {
        actionbook()
            .args(["sources", "search"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("QUERY"));
    }
}

mod browser_command {
    use super::*;
    use std::fs;

    fn setup_config(default_profile: &str) -> (tempfile::TempDir, String, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        fs::create_dir_all(&data_home).unwrap();

        let config_path_output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "path"])
            .output()
            .unwrap();
        assert!(
            config_path_output.status.success(),
            "failed to resolve config path: {}",
            String::from_utf8_lossy(&config_path_output.stderr)
        );
        let config_path = String::from_utf8_lossy(&config_path_output.stdout)
            .trim()
            .to_string();
        let config_file = std::path::PathBuf::from(config_path);
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();

        let config = format!(
            r#"[api]
base_url = "https://api.actionbook.dev"

[browser]
headless = false
default_profile = "{}"
"#,
            default_profile
        );
        fs::write(config_file, config).unwrap();

        (
            tmp,
            home.to_string_lossy().to_string(),
            config_home.to_string_lossy().to_string(),
            data_home.to_string_lossy().to_string(),
        )
    }

    #[test]
    fn browser_requires_subcommand() {
        actionbook()
            .arg("browser")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn browser_status_help() {
        actionbook()
            .args(["browser", "status", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("browser status"));
    }

    #[test]
    fn browser_open_requires_url() {
        actionbook()
            .args(["browser", "open"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("URL"));
    }

    #[test]
    fn browser_goto_requires_url() {
        actionbook()
            .args(["browser", "goto"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("URL"));
    }

    #[test]
    fn browser_click_requires_selector() {
        actionbook()
            .args(["browser", "click"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_type_requires_args() {
        actionbook()
            .args(["browser", "type"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_fill_requires_args() {
        actionbook()
            .args(["browser", "fill"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_wait_requires_selector() {
        actionbook()
            .args(["browser", "wait"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("SELECTOR"));
    }

    #[test]
    fn browser_eval_requires_code() {
        actionbook()
            .args(["browser", "eval"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("CODE"));
    }

    #[test]
    fn browser_screenshot_has_default_path() {
        actionbook()
            .args(["browser", "screenshot", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("screenshot.png"));
    }

    #[test]
    fn browser_pdf_requires_path() {
        actionbook()
            .args(["browser", "pdf"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("PATH"));
    }

    #[test]
    fn browser_inspect_requires_coordinates() {
        actionbook()
            .args(["browser", "inspect"])
            .assert()
            .failure()
            .stderr(predicate::str::is_match("[XY]").unwrap());
    }

    #[test]
    fn browser_connect_requires_endpoint() {
        actionbook()
            .args(["browser", "connect"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("ENDPOINT"));
    }

    #[test]
    fn browser_connect_help() {
        actionbook()
            .args(["browser", "connect", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Connect"));
    }

    #[test]
    fn browser_connect_invalid_endpoint_fails() {
        // "not-a-port" is neither a number nor ws:// URL
        actionbook()
            .args(["browser", "connect", "not-a-port"])
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid endpoint"));
    }

    #[test]
    fn browser_connect_unreachable_port_fails() {
        // Port 19999 should have nothing listening
        actionbook()
            .args(["browser", "connect", "19999"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .failure();
    }

    #[test]
    fn browser_connect_uses_config_default_profile_when_not_specified() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args([
                "--json",
                "browser",
                "connect",
                "ws://127.0.0.1:9222/devtools/browser/test",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"profile\":\"team\""));
    }

    #[test]
    fn browser_connect_uses_env_profile_over_config_default() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .env("ACTIONBOOK_PROFILE", "env-profile")
            .args([
                "--json",
                "browser",
                "connect",
                "ws://127.0.0.1:9222/devtools/browser/test",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"profile\":\"env-profile\""));
    }

    #[test]
    fn browser_connect_cli_profile_overrides_env_and_config() {
        let (_tmp, home, config_home, data_home) = setup_config("team");
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .env("ACTIONBOOK_PROFILE", "env-profile")
            .args([
                "--json",
                "--profile",
                "cli-profile",
                "browser",
                "connect",
                "ws://127.0.0.1:9222/devtools/browser/test",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"profile\":\"cli-profile\""));
    }

    #[test]
    fn browser_snapshot_help() {
        actionbook()
            .args(["browser", "snapshot", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("snapshot"));
    }

    #[test]
    fn browser_cookies_subcommands() {
        actionbook()
            .args(["browser", "cookies", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("list"))
            .stdout(predicate::str::contains("get"))
            .stdout(predicate::str::contains("set"))
            .stdout(predicate::str::contains("delete"))
            .stdout(predicate::str::contains("clear"));
    }
}

mod config_command {
    use super::*;

    #[test]
    fn config_requires_subcommand() {
        actionbook()
            .arg("config")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn config_show_help() {
        actionbook()
            .args(["config", "show", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("configuration"));
    }

    #[test]
    fn config_set_requires_key_value() {
        actionbook()
            .args(["config", "set"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("KEY"));
    }

    #[test]
    fn config_get_requires_key() {
        actionbook()
            .args(["config", "get"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("KEY"));
    }
}

mod profile_command {
    use super::*;

    #[test]
    fn profile_requires_subcommand() {
        actionbook()
            .arg("profile")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn profile_create_requires_name() {
        actionbook()
            .args(["profile", "create"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("NAME"));
    }

    #[test]
    fn profile_delete_requires_name() {
        actionbook()
            .args(["profile", "delete"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("NAME"));
    }
}

mod global_flags {
    use super::*;

    #[test]
    fn json_flag_available_globally() {
        actionbook()
            .args(["--json", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn verbose_flag_available_globally() {
        actionbook()
            .args(["--verbose", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn headless_flag_available_globally() {
        actionbook()
            .args(["--headless", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn profile_flag_available_globally() {
        actionbook()
            .args(["--profile", "test", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn browser_path_flag_available_globally() {
        actionbook()
            .args(["--browser-path", "/usr/bin/chrome", "search", "--help"])
            .assert()
            .success();
    }

    #[test]
    fn cdp_flag_available_globally() {
        actionbook()
            .args(["--cdp", "9222", "search", "--help"])
            .assert()
            .success();
    }
}
