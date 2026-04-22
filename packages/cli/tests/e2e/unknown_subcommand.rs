use crate::harness::{
    assert_failure, assert_meta, assert_success, headless, parse_json, skip, stderr_str,
};

#[test]
fn unknown_browser_subcmd_json() {
    if skip() {
        return;
    }

    let out = headless(&["--json", "browser", "tabs"], 10);
    assert_failure(&out, "browser tabs --json should fail structurally");
    assert_eq!(
        out.status.code(),
        Some(64),
        "unknown browser subcommand should exit EX_USAGE (64)"
    );

    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "UNKNOWN_SUBCOMMAND");
    assert!(v["error"]["message"].is_string());
    assert_eq!(v["error"]["retryable"], false);
    assert_eq!(v["error"]["details"]["parent"], "browser");
    assert!(
        v["error"]["details"]["did_you_mean"]
            .as_array()
            .is_some_and(|values| values
                .iter()
                .any(|value| value.as_str() == Some("list-tabs"))),
        "did_you_mean should include list-tabs: {v:?}"
    );
    assert_meta(&v);
}

#[test]
fn unknown_browser_subcmd_text() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "tabs"], 10);
    assert_failure(&out, "browser tabs text should fail structurally");
    assert_eq!(
        out.status.code(),
        Some(64),
        "unknown browser subcommand text path should exit EX_USAGE (64)"
    );
    let stderr = stderr_str(&out);
    assert!(
        stderr.contains("UNKNOWN_SUBCOMMAND"),
        "stderr should contain UNKNOWN_SUBCOMMAND, got:\n{stderr}"
    );
    assert!(
        stderr.contains("did you mean") || stderr.contains("try one of"),
        "stderr should contain a suggestion hint, got:\n{stderr}"
    );
}

#[test]
fn valid_browser_subcmd_arg_error_unchanged() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "click"], 10);
    assert_failure(
        &out,
        "browser click without args should remain a clap arg error",
    );
    assert_ne!(
        out.status.code(),
        Some(64),
        "valid subcommand with missing args must not be reclassified as UNKNOWN_SUBCOMMAND"
    );
    let stderr = stderr_str(&out);
    assert!(
        stderr.contains("Usage:") || stderr.contains("required"),
        "clap arg error should still print usage/required guidance, got:\n{stderr}"
    );
}

#[test]
fn unknown_toplevel_subcmd_json() {
    if skip() {
        return;
    }

    let out = headless(&["--json", "browsr"], 10);
    assert_failure(&out, "top-level typo should fail structurally");
    assert_eq!(
        out.status.code(),
        Some(64),
        "unknown top-level subcommand should exit EX_USAGE (64)"
    );

    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "UNKNOWN_SUBCOMMAND");
    assert!(v["error"]["message"].is_string());
    assert_eq!(v["error"]["retryable"], false);
    assert!(
        v["error"]["details"]["parent"].is_null(),
        "top-level unknown subcommand should set parent=null: {v:?}"
    );
    assert!(
        v["error"]["details"]["did_you_mean"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some("browser"))),
        "did_you_mean should include browser: {v:?}"
    );
    assert_meta(&v);
}

#[test]
fn browser_help_not_classified_as_unknown() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "--help", "--json"], 10);
    assert_success(
        &out,
        "browser --help --json should stay on the custom help path",
    );

    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser help");
    assert!(
        v["error"].is_null(),
        "help output should not contain an error"
    );
    assert!(
        v["data"]
            .as_str()
            .is_some_and(|help| help.contains("Usage: actionbook browser <subcommand> [options]")),
        "browser help JSON should still contain the grouped help text: {v:?}"
    );
    assert_meta(&v);
}
