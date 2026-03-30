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
            "isolated",
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
            "extension",
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
fn setup_non_interactive_existing_cloud_config_returns_migration_hint() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let home = tmp.path().join("actionbook-home");
    std::fs::create_dir_all(&home).expect("create actionbook home");
    std::fs::write(
        home.join("config.toml"),
        r#"[browser]
mode = "cloud"
headless = false
profile_name = "actionbook"
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
        !output.status.success(),
        "expected existing cloud config to fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("browser.mode='cloud'"),
        "stderr should mention the unsupported cloud mode"
    );
    assert!(
        stderr.contains("actionbook setup --reset"),
        "stderr should include the migration hint"
    );
    assert!(
        !home.join("daemon.sock").exists(),
        "setup should not go through the daemon on config migration failure"
    );
    assert!(
        !home.join("daemon.pid").exists(),
        "setup should not spawn a daemon process on config migration failure"
    );
}
