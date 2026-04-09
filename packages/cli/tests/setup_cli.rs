use assert_cmd::Command;

fn read_config(home: &std::path::Path) -> String {
    std::fs::read_to_string(home.join("config.toml")).expect("read config")
}

#[test]
fn setup_json_non_interactive_writes_config_without_daemon_side_effects() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args([
            "--json",
            "setup",
            "--non-interactive",
            "--api-key",
            "sk-test",
            "--browser",
            "local",
        ])
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "expected setup success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let config = read_config(&home);
    assert!(config.contains("[api]"));
    assert!(config.contains("api_key = \"sk-test\""));
    assert!(config.contains("[browser]"));
    assert!(config.contains("mode = \"local\""));

    assert!(
        !home.join("daemon.sock").exists(),
        "setup should not go through the daemon"
    );
    assert!(
        !home.join("daemon.pid").exists(),
        "setup should not spawn a daemon process"
    );
}

#[test]
fn setup_json_non_interactive_cloud_mode_writes_config_without_daemon_side_effects() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args(["--json", "setup", "--non-interactive", "--browser", "cloud"])
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "expected cloud setup success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let config = read_config(&home);
    assert!(config.contains("[browser]"));
    assert!(config.contains("mode = \"cloud\""));

    assert!(
        !home.join("daemon.sock").exists(),
        "setup should not go through the daemon"
    );
    assert!(
        !home.join("daemon.pid").exists(),
        "setup should not spawn a daemon process"
    );
}

#[test]
fn setup_json_existing_config_does_not_require_tty() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");
    std::fs::create_dir_all(&home).expect("create actionbook home");
    std::fs::write(
        home.join("config.toml"),
        r#"[api]
api_key = "sk-config"

[browser]
mode = "local"
headless = false
profile_name = "actionbook"
"#,
    )
    .expect("seed config");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args(["--json", "setup"])
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "expected json setup rerun success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("Prompt failed"),
        "json setup should not try to prompt on rerun"
    );
}

#[test]
fn setup_json_with_env_api_key_does_not_persist_env_value() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .env("ACTIONBOOK_API_KEY", "sk-env")
        .args(["--json", "setup"])
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "expected json setup success with env api key\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let config = read_config(&home);
    assert!(config.contains("[api]"));
    assert!(
        !config.contains("api_key = \"sk-env\""),
        "env-sourced api key should not be written into config without confirmation"
    );
}

#[test]
fn setup_reset_recreates_default_config() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let seed = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args([
            "--json",
            "setup",
            "--non-interactive",
            "--api-key",
            "sk-test",
            "--browser",
            "cloud",
        ])
        .output()
        .expect("seed config");
    assert!(seed.status.success(), "seed setup should succeed");

    let reset = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args(["--json", "setup", "--reset", "--non-interactive"])
        .output()
        .expect("run reset");

    assert!(
        reset.status.success(),
        "expected setup reset success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&reset.stdout),
        String::from_utf8_lossy(&reset.stderr),
    );

    let config = read_config(&home);
    assert!(config.contains("[api]"));
    assert!(!config.contains("api_key = \"sk-test\""));
    assert!(config.contains("[browser]"));
    assert!(config.contains("mode = \"local\""));
    assert!(config.contains("profile_name = \"actionbook\""));
}

#[test]
fn setup_invalid_browser_value_exits_non_zero() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args([
            "--json",
            "setup",
            "--non-interactive",
            "--browser",
            "invalid",
        ])
        .output()
        .expect("run setup");

    assert!(
        !output.status.success(),
        "expected invalid --browser to fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid --browser value 'invalid'"),
        "stderr should explain invalid browser value"
    );
    assert!(
        !home.join("daemon.sock").exists(),
        "setup should not go through the daemon on failure"
    );
    assert!(
        !home.join("daemon.pid").exists(),
        "setup should not spawn a daemon process on failure"
    );
}

#[test]
fn setup_isolated_browser_value_exits_non_zero() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args([
            "--json",
            "setup",
            "--non-interactive",
            "--browser",
            "isolated",
        ])
        .output()
        .expect("run setup");

    assert!(
        !output.status.success(),
        "expected isolated --browser to fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid --browser value 'isolated'"),
        "stderr should explain that isolated is no longer a valid browser value"
    );
}

#[test]
fn setup_extension_browser_value_succeeds() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args([
            "--json",
            "setup",
            "--non-interactive",
            "--browser",
            "extension",
        ])
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "expected extension --browser to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"mode\":\"extension\""),
        "stdout should contain extension mode"
    );
}

#[test]
fn setup_non_interactive_existing_cloud_config_is_preserved() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");
    std::fs::create_dir_all(&home).expect("create actionbook home");
    std::fs::write(
        home.join("config.toml"),
        r#"version = 1

[browser]
mode = "cloud"
headless = false
profile_name = "actionbook"
cdp_endpoint = "wss://browser.example.com"
"#,
    )
    .expect("seed cloud config");

    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("ACTIONBOOK_HOME", &home)
        .args(["setup", "--non-interactive"])
        .output()
        .expect("run setup");

    assert!(
        output.status.success(),
        "expected existing cloud config to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        read_config(&home).contains("mode = \"cloud\""),
        "setup should preserve the supported cloud mode"
    );
    assert!(
        read_config(&home).contains("cdp_endpoint = \"wss://browser.example.com\""),
        "setup should preserve the configured cloud endpoint"
    );
    assert!(
        !home.join("daemon.sock").exists(),
        "setup should not go through the daemon on cloud config rerun"
    );
    assert!(
        !home.join("daemon.pid").exists(),
        "setup should not spawn a daemon process on cloud config rerun"
    );
}
