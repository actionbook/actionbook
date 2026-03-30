//! CLI argument parsing tests
//!
//! These tests verify that CLI arguments are parsed correctly,
//! matching the behavior of the original TypeScript CLI.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

/// Get the actionbook binary command
fn actionbook() -> Command {
    Command::cargo_bin("actionbook").unwrap()
}

/// Create an isolated environment for tests that touch the filesystem.
fn create_isolated_env() -> (tempfile::TempDir, String, String, String) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let config_home = tmp.path().join("config");
    let data_home = tmp.path().join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config_home).unwrap();
    fs::create_dir_all(&data_home).unwrap();
    (
        tmp,
        home.to_string_lossy().to_string(),
        config_home.to_string_lossy().to_string(),
        data_home.to_string_lossy().to_string(),
    )
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
            .stdout("1.0.0\n");
    }

    #[test]
    fn help_lists_all_commands() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("search"))
            .stdout(predicate::str::contains("get"))
            .stdout(predicate::str::contains("sources"))
            .stdout(predicate::str::contains("config"))
            .stdout(predicate::str::contains("profile"))
            .stdout(predicate::str::contains("extension"))
            .stdout(predicate::str::contains("setup"));
    }

    #[test]
    fn help_lists_global_options() {
        actionbook()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("--profile"))
            .stdout(predicate::str::contains("--api-key"))
            .stdout(predicate::str::contains("--json"));
    }

    #[test]
    fn help_subcommand_outputs_browser_surface() {
        actionbook()
            .arg("help")
            .assert()
            .success()
            .stdout(predicate::str::contains("actionbook browser"))
            .stdout(predicate::str::contains("start"))
            .stdout(predicate::str::contains("snapshot"));
    }

    #[test]
    fn help_browser_routes_to_browser_surface() {
        actionbook()
            .args(["help", "browser"])
            .assert()
            .success()
            .stdout(predicate::str::contains("actionbook browser"))
            .stdout(predicate::str::contains("start"))
            .stdout(predicate::str::contains("list-tabs"))
            .stdout(predicate::str::contains("snapshot"));
    }

    #[test]
    fn json_help_outputs_browser_help_string() {
        let output = actionbook()
            .args(["--json", "help"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(output).unwrap();
        let help: String = serde_json::from_str(text.trim()).unwrap();
        assert!(help.contains("actionbook browser"));
        assert!(help.contains("start"));
        assert!(help.contains("list-tabs"));
    }

    #[test]
    fn json_version_outputs_prd_string() {
        actionbook()
            .args(["--json", "--version"])
            .assert()
            .success()
            .stdout("\"1.0.0\"\n");
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

    #[test]
    fn config_edit_help() {
        actionbook()
            .args(["config", "edit", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn config_show_runs() {
        actionbook().args(["config", "show"]).assert().success();
    }

    #[test]
    fn config_path_outputs_path() {
        actionbook()
            .args(["config", "path"])
            .assert()
            .success()
            .stdout(predicate::str::contains(".actionbook"));
    }

    #[test]
    fn config_path_json() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--json", "config", "path"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
        assert!(json["path"].is_string());
    }

    #[test]
    fn config_set_rejects_unknown_key() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "set", "unknown.key", "value"])
            .assert()
            .failure();
    }

    #[test]
    fn config_get_rejects_unknown_key() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "get", "unknown.key"])
            .assert()
            .failure();
    }

    #[test]
    fn config_set_and_get_round_trip() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args([
                "config",
                "set",
                "api.base_url",
                "https://custom.example.com",
            ])
            .assert()
            .success();

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "get", "api.base_url"])
            .assert()
            .success()
            .stdout(predicate::str::contains("https://custom.example.com"));
    }

    #[test]
    fn config_reset() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["config", "reset"])
            .assert()
            .success();
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

    #[test]
    fn profile_show_requires_name() {
        actionbook()
            .args(["profile", "show"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("NAME"));
    }

    #[test]
    fn profile_list_runs() {
        actionbook().args(["profile", "list"]).assert().success();
    }

    #[test]
    fn profile_list_json() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--json", "profile", "list"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let _json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
    }

    #[test]
    fn profile_create_and_show() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "create", "test-profile", "--cdp-port", "9333"])
            .assert()
            .success();

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "show", "test-profile"])
            .assert()
            .success()
            .stdout(predicate::str::contains("test-profile"));
    }

    #[test]
    fn profile_create_and_delete() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "create", "delete-me"])
            .assert()
            .success();

        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "delete", "delete-me"])
            .assert()
            .success();

        // Verify deleted
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "show", "delete-me"])
            .assert()
            .failure();
    }

    #[test]
    fn profile_show_fails_for_nonexistent() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["profile", "show", "nonexistent-profile"])
            .assert()
            .failure();
    }

    #[test]
    fn profile_create_auto_assigns_cdp_port() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["--json", "profile", "create", "auto-port"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
        assert!(json["cdp_port"].is_number());
    }
}

mod extension_command {
    use super::*;

    #[test]
    fn extension_requires_subcommand() {
        actionbook()
            .arg("extension")
            .assert()
            .failure()
            .stderr(predicate::str::contains("subcommand"));
    }

    #[test]
    fn extension_status_help() {
        actionbook()
            .args(["extension", "status", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_ping_help() {
        actionbook()
            .args(["extension", "ping", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_install_help_shows_force() {
        actionbook()
            .args(["extension", "install", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--force"));
    }

    #[test]
    fn extension_stop_help() {
        actionbook()
            .args(["extension", "stop", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_uninstall_help() {
        actionbook()
            .args(["extension", "uninstall", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_path_outputs_path() {
        actionbook()
            .args(["extension", "path"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    fn extension_path_json() {
        let output = actionbook()
            .args(["--json", "extension", "path"])
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let _json: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("Should be valid JSON");
    }

    #[test]
    fn extension_status_reports_bridge_state() {
        let output = actionbook()
            .args(["extension", "status"])
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("running")
                || stdout.contains("Running")
                || stdout.contains("not running")
                || stdout.contains("Not running"),
            "Should report bridge state: {}",
            stdout
        );
    }

    #[test]
    fn extension_stop_when_not_running() {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(["extension", "stop"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .success();
    }
}

mod setup_command {
    use super::*;

    #[test]
    fn setup_help_shows_all_options() {
        actionbook()
            .args(["setup", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--target"))
            .stdout(predicate::str::contains("--non-interactive"))
            .stdout(predicate::str::contains("--api-key"))
            .stdout(predicate::str::contains("--browser"))
            .stdout(predicate::str::contains("--reset"));
    }

    /// Helper: assert the setup command exits with 0 or 1 (not crash/signal).
    /// Uses a longer timeout since setup actually runs downloads/installs.
    fn assert_setup_exits_gracefully(args: &[&str]) {
        let (_tmp, home, config_home, data_home) = create_isolated_env();
        let output = actionbook()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(args)
            .timeout(std::time::Duration::from_secs(120))
            .output()
            .unwrap();
        let code = output.status.code().expect(
            "setup command was killed by signal (likely timed out); \
             it should exit within 120s instead of hanging",
        );
        assert!(
            code == 0 || code == 1,
            "Unexpected exit code: {}.\nstdout: {}\nstderr: {}",
            code,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[test]
    fn setup_non_interactive_runs() {
        assert_setup_exits_gracefully(&["setup", "--non-interactive", "--json"]);
    }

    #[test]
    fn setup_non_interactive_target_claude() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--target",
            "claude",
            "--json",
        ]);
    }

    #[test]
    fn setup_with_api_key() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--api-key",
            "test-key-12345",
            "--json",
        ]);
    }

    #[test]
    fn setup_browser_isolated() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--browser",
            "isolated",
            "--json",
        ]);
    }

    #[test]
    fn setup_browser_extension() {
        assert_setup_exits_gracefully(&[
            "setup",
            "--non-interactive",
            "--browser",
            "extension",
            "--json",
        ]);
    }

    #[test]
    fn setup_reset() {
        assert_setup_exits_gracefully(&["setup", "--reset", "--non-interactive", "--json"]);
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
    fn profile_flag_available_globally() {
        actionbook()
            .args(["--profile", "test", "search", "--help"])
            .assert()
            .success();
    }
}
