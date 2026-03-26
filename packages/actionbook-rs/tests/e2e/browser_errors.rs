//! Error-path E2E tests: verify that invalid inputs produce non-zero exits.
//!
//! Each test starts a headless session, issues a command expected to fail,
//! asserts non-zero exit, then closes the session.

use crate::harness::{
    assert_failure, assert_success, ensure_no_sessions, headless, skip, stderr_str, stdout_str,
};

// ── 1. ELEMENT_NOT_FOUND: click nonexistent selector ───────────────

#[test]
fn err_click_nonexistent() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    let out = headless(
        &[
            "browser",
            "click",
            "#does_not_exist_at_all",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_failure(&out, "click nonexistent element");
    let output = stdout_str(&out) + &stderr_str(&out);
    assert!(
        output.to_lowercase().contains("not found") || output.to_lowercase().contains("error"),
        "click nonexistent should mention error in output, got: {}",
        output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session");
}

// ── 2. NAVIGATION_FAILED: goto invalid URL ─────────────────────────

#[test]
fn err_goto_invalid_url() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    let out = headless(
        &[
            "browser",
            "goto",
            "not-a-valid-url-at-all",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_failure(&out, "goto invalid URL");
    let output = stdout_str(&out) + &stderr_str(&out);
    assert!(
        output.to_lowercase().contains("error")
            || output.to_lowercase().contains("invalid")
            || output.to_lowercase().contains("fail"),
        "goto invalid URL should mention error in output, got: {}",
        output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session");
}

// ── 3. EVAL_FAILED: eval with syntax error ─────────────────────────

#[test]
fn err_eval_syntax_error() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    let out = headless(
        &[
            "browser",
            "eval",
            "{{{{syntax_error",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_failure(&out, "eval syntax error");
    let output = stdout_str(&out) + &stderr_str(&out);
    assert!(
        output.to_lowercase().contains("error")
            || output.to_lowercase().contains("syntax")
            || output.to_lowercase().contains("fail"),
        "eval syntax error should mention error in output, got: {}",
        output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session");
}

// ── 4. ARTIFACT_WRITE_FAILED: screenshot to nonexistent directory ──

#[test]
fn err_screenshot_bad_path() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    let out = headless(
        &[
            "browser",
            "screenshot",
            "/nonexistent_dir_12345/x.png",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_failure(&out, "screenshot to bad path");
    let output = stdout_str(&out) + &stderr_str(&out);
    assert!(
        output.to_lowercase().contains("error")
            || output.to_lowercase().contains("not found")
            || output.to_lowercase().contains("fail")
            || output.to_lowercase().contains("no such"),
        "screenshot bad path should mention error in output, got: {}",
        output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session");
}

// ── 5. TIMEOUT: wait for nonexistent element ───────────────────────

#[test]
fn err_wait_timeout() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    let out = headless(
        &[
            "browser",
            "wait",
            "element",
            "#absolutely_not_here",
            "-s",
            "s0",
            "-t",
            "t0",
            "--timeout",
            "1000",
        ],
        30,
    );
    assert_failure(&out, "wait for nonexistent element");
    let output = stdout_str(&out) + &stderr_str(&out);
    assert!(
        output.to_lowercase().contains("timeout")
            || output.to_lowercase().contains("error")
            || output.to_lowercase().contains("not found"),
        "wait timeout should mention error in output, got: {}",
        output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session");
}

// ── 6. ELEMENT_NOT_FOUND: fill nonexistent element ─────────────────

#[test]
fn err_fill_nonexistent() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    let out = headless(
        &[
            "browser",
            "fill",
            "#does_not_exist_xyz",
            "text",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_failure(&out, "fill nonexistent element");
    let output = stdout_str(&out) + &stderr_str(&out);
    assert!(
        output.to_lowercase().contains("not found") || output.to_lowercase().contains("error"),
        "fill nonexistent should mention error in output, got: {}",
        output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session");
}
