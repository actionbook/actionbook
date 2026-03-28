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
    assert!(
        output.stdout.is_empty(),
        "setup --json should not invent a JSON schema"
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
