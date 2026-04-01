use assert_cmd::Command;

#[test]
fn version_flag_prints_plain_version_string() {
    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .args(["--version"])
        .output()
        .expect("run --version");

    assert!(
        output.status.success(),
        "expected --version success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), env!("CARGO_PKG_VERSION"));
    assert!(
        !stdout.contains("Usage:"),
        "--version must not fall through to help output: {stdout}"
    );
}

#[test]
fn version_flag_json_returns_version_envelope() {
    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .args(["--json", "--version"])
        .output()
        .expect("run --json --version");

    assert!(
        output.status.success(),
        "expected --json --version success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "version");
    assert!(json["context"].is_null());
    assert_eq!(json["data"], env!("CARGO_PKG_VERSION"));
    assert!(json["error"].is_null());
}
