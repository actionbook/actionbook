use assert_cmd::Command;

#[test]
fn browser_stop_help_defines_profile_preserving_semantics() {
    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .args(["browser", "stop", "--help"])
        .output()
        .expect("run browser stop --help");

    assert!(
        output.status.success(),
        "expected browser stop --help success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("retaining the named profile directory and authentication state"),
        "browser stop help should make profile preservation unambiguous\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Use `browser close` instead"),
        "browser stop help should identify destructive close separately\nstdout:\n{stdout}"
    );
}

#[test]
fn top_level_help_lists_setup_command() {
    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .arg("--help")
        .output()
        .expect("run --help");

    assert!(
        output.status.success(),
        "expected --help success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("setup      Configure actionbook"),
        "top-level help should list setup as an available command\nstdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("setup      Coming soon"),
        "top-level help should no longer mark setup as coming soon\nstdout:\n{stdout}"
    );
}
