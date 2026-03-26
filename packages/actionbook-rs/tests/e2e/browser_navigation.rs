//! Browser navigation E2E tests: goto, back, forward, reload.
//!
//! Uses daemon v2 CLI format with --session and --tab addressing.
//! Each test runs as a single function to guarantee execution order.

use crate::harness::{assert_success, ensure_no_sessions, headless, skip, stdout_str};

// ── Test 1: nav_goto_and_verify_url ────────────────────────────────────

/// S1T1: start → goto example.org → eval location.href → verify → close
#[test]
fn nav_goto_and_verify_url() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start a headless session with example.com
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
    assert_success(&out, "start");

    // Navigate to example.org
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.org");

    // Verify location
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval location");
    assert!(
        stdout_str(&out).contains("example.org"),
        "location should contain example.org, got: {}",
        stdout_str(&out)
    );

    // Close
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Test 2: nav_goto_seq_two_urls ──────────────────────────────────────

/// SEQ: start → goto url_a → verify → goto url_b → verify url_b → close
#[test]
fn nav_goto_seq_two_urls() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Navigate to first URL
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.org");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval location after first goto");
    assert!(
        stdout_str(&out).contains("example.org"),
        "first goto should land on example.org, got: {}",
        stdout_str(&out)
    );

    // Navigate to second URL
    let out = headless(
        &[
            "browser",
            "goto",
            "https://httpbin.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto httpbin.org");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval location after second goto");
    assert!(
        stdout_str(&out).contains("httpbin.org"),
        "second goto should land on httpbin.org, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Test 3: nav_goto_s1t2_cross_tab ────────────────────────────────────

/// S1T2: start with url_a → open url_b as t1 → goto url_c on t1 →
///       eval on t0 → t0 still url_a → close
#[test]
fn nav_goto_s1t2_cross_tab() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start with example.com on t0
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
    assert_success(&out, "start");

    // Open example.org in a new tab (t1)
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open t1");

    // Navigate t1 to httpbin.org
    let out = headless(
        &[
            "browser",
            "goto",
            "https://httpbin.org",
            "-s",
            "s0",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto httpbin.org on t1");

    // Verify t1 is on httpbin.org
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "eval t1 location");
    assert!(
        stdout_str(&out).contains("httpbin.org"),
        "t1 should be on httpbin.org, got: {}",
        stdout_str(&out)
    );

    // Verify t0 is still on example.com
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval t0 location");
    assert!(
        stdout_str(&out).contains("example.com"),
        "t0 should still be on example.com, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Test 4: nav_back_forward ───────────────────────────────────────────

/// S1T1: start → goto url_a → goto url_b → back → eval (url_a) →
///       forward → eval (url_b) → close
#[test]
fn nav_back_forward() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Navigate to url_a
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.org");

    // Navigate to url_b
    let out = headless(
        &[
            "browser",
            "goto",
            "https://httpbin.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto httpbin.org");

    // Back — should return to example.org
    let out = headless(&["browser", "back", "-s", "s0", "-t", "t0"], 30);
    assert_success(&out, "back");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval after back");
    assert!(
        stdout_str(&out).contains("example.org"),
        "after back should be on example.org, got: {}",
        stdout_str(&out)
    );

    // Forward — should return to httpbin.org
    let out = headless(&["browser", "forward", "-s", "s0", "-t", "t0"], 30);
    assert_success(&out, "forward");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval after forward");
    assert!(
        stdout_str(&out).contains("httpbin.org"),
        "after forward should be on httpbin.org, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Test 5: nav_reload ─────────────────────────────────────────────────

/// S1T1: start → goto → eval set marker → reload → eval marker gone → close
#[test]
fn nav_reload() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Navigate to a page
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto");

    // Set a JS marker on the page
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__test_marker = 42",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval set marker");

    // Verify marker is set
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__test_marker",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval verify marker");
    assert!(
        stdout_str(&out).contains("42"),
        "marker should be 42, got: {}",
        stdout_str(&out)
    );

    // Reload the page — marker should be cleared
    let out = headless(&["browser", "reload", "-s", "s0", "-t", "t0"], 30);
    assert_success(&out, "reload");

    // Verify marker is gone (should be undefined/null)
    let out = headless(
        &[
            "browser",
            "eval",
            "typeof window.__test_marker",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval after reload");
    let marker_output = stdout_str(&out);
    assert!(
        marker_output.contains("undefined"),
        "after reload marker should be undefined, got: {}",
        marker_output
    );

    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}
