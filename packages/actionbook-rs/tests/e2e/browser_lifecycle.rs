//! Browser lifecycle E2E tests: start, status, list, restart, close.
//!
//! Each test is self-contained: start → operate → assert → close.
//! Uses daemon v2 CLI format with --session and --tab addressing.

use crate::harness::{
    assert_failure, assert_success, ensure_no_sessions, headless, headless_json, skip, stdout_str,
};

// ---------------------------------------------------------------------------
// 1. lifecycle_open_and_close
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_open_and_close() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start a headless browser session
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // Status should show session info
    let out = headless(&["browser", "status", "-s", "s0"], 10);
    assert_success(&out, "status");
    let status = stdout_str(&out);
    assert!(
        status.contains("s0") || status.contains("running"),
        "status should show session info, got: {}",
        status
    );

    // Close the session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 2. lifecycle_open_headless
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_open_headless() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start headless — should succeed
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start headless");

    // Cleanup
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 3. lifecycle_open_with_url
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_open_with_url() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session with a URL
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
    assert_success(&out, "start with url");

    // Eval location.href should contain example.com
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
        stdout_str(&out).contains("example.com"),
        "location should contain example.com, got: {}",
        stdout_str(&out)
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 4. lifecycle_status_shows_info
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_status_shows_info() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // Status should contain session info
    let out = headless(&["browser", "status", "-s", "s0"], 10);
    assert_success(&out, "status");
    let status = stdout_str(&out);
    assert!(
        status.contains("s0") || status.contains("running") || status.contains("local"),
        "status should show session info, got: {}",
        status
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 5. lifecycle_list_sessions
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_list_sessions() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // list-sessions should show s0
    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    assert!(
        stdout_str(&out).contains("s0"),
        "list-sessions should contain s0, got: {}",
        stdout_str(&out)
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 6. lifecycle_restart
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_restart() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // Restart the session
    let out = headless(&["browser", "restart", "-s", "s0"], 30);
    assert_success(&out, "restart");

    // Status should still show session info after restart
    let out = headless(&["browser", "status", "-s", "s0"], 10);
    assert_success(&out, "status after restart");
    let status = stdout_str(&out);
    assert!(
        status.contains("s0") || status.contains("running"),
        "status after restart should show session info, got: {}",
        status
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 7. lifecycle_close_after_operations
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_close_after_operations() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session
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

    // Goto example.com
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
    assert_success(&out, "goto");

    // Snapshot
    let out = headless_json(&["browser", "snapshot", "-s", "s0", "-t", "t0"], 30);
    assert_success(&out, "snapshot");

    // Close should still succeed after operations
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close after operations");
}

// ---------------------------------------------------------------------------
// 8. lifecycle_close_s1t2_closes_all
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_close_s1t2_closes_all() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session
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

    // Open a second tab in the same session
    let out = headless(&["browser", "open", "https://example.com", "-s", "s0"], 30);
    assert_success(&out, "open second tab");

    // Close the session — should close everything (both tabs)
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close session with multiple tabs");
}

// ---------------------------------------------------------------------------
// 9. lifecycle_double_close
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_double_close() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // First close should succeed
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "first close");

    // Second close should fail — session no longer exists
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_failure(&out, "second close should fail");
}
