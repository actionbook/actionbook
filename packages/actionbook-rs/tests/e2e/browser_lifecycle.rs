//! Browser lifecycle E2E tests: start, status, list, restart, close.
//!
//! Each test is self-contained: start → operate → assert → close.
//! Uses daemon v2 CLI format with --session and --tab addressing.

use std::net::TcpListener;

use crate::harness::{
    assert_failure, assert_success, headless, headless_json, skip, stdout_str, SessionGuard,
};

// ---------------------------------------------------------------------------
// 1. lifecycle_open_and_close
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_open_and_close() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start a headless browser session
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // Status should show session info
    let out = headless(&["browser", "status", "-s", "local-1"], 10);
    assert_success(&out, "status");
    let status = stdout_str(&out);
    assert!(
        status.contains("local-1") || status.contains("running"),
        "status should show session info, got: {}",
        status
    );

    // Close the session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

    // Start headless — should succeed
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start headless");

    // Cleanup
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

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
            "local-1",
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
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // Status should contain session info
    let out = headless(&["browser", "status", "-s", "local-1"], 10);
    assert_success(&out, "status");
    let status = stdout_str(&out);
    assert!(
        status.contains("local-1") || status.contains("running") || status.contains("local"),
        "status should show session info, got: {}",
        status
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // list-sessions should show s0
    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    assert!(
        stdout_str(&out).contains("local-1"),
        "list-sessions should contain s0, got: {}",
        stdout_str(&out)
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // Restart the session
    let out = headless(&["browser", "restart", "-s", "local-1"], 30);
    assert_success(&out, "restart");

    // Status should still show session info after restart
    let out = headless(&["browser", "status", "-s", "local-1"], 10);
    assert_success(&out, "status after restart");
    let status = stdout_str(&out);
    assert!(
        status.contains("local-1") || status.contains("running"),
        "status after restart should show session info, got: {}",
        status
    );

    // Cleanup
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

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
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto");

    // Snapshot
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "snapshot");

    // Close should still succeed after operations
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

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
    let out = headless(
        &["browser", "open", "https://example.com", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open second tab");

    // Close the session — should close everything (both tabs)
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
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
    let _guard = SessionGuard::new();

    // Start session
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");

    // First close should succeed
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "first close");

    // Second close should fail — session no longer exists
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_failure(&out, "second close should fail");
}

// ---------------------------------------------------------------------------
// 10. lifecycle_set_session_id_explicit
// ---------------------------------------------------------------------------

/// Start with --set-session-id → session uses the explicit ID → close
#[test]
fn lifecycle_set_session_id_explicit() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session with explicit ID
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            "research-google",
        ],
        30,
    );
    assert_success(&out, "start with explicit session id");

    // Status should work with the explicit ID
    let out = headless(&["browser", "status", "-s", "research-google"], 10);
    assert_success(&out, "status with explicit id");

    // list-sessions should show the explicit ID
    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    assert!(
        stdout_str(&out).contains("research-google"),
        "list-sessions should contain research-google, got: {}",
        stdout_str(&out)
    );

    // Close with explicit ID
    let out = headless(&["browser", "close", "-s", "research-google"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 11. lifecycle_set_session_id_invalid
// ---------------------------------------------------------------------------

/// Start with invalid --set-session-id → should fail
#[test]
fn lifecycle_set_session_id_invalid() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Invalid: starts with number
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            "1invalid",
        ],
        30,
    );
    assert_failure(&out, "start with invalid session id (starts with number)");

    // Invalid: single character (min 2 chars)
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            "a",
        ],
        30,
    );
    assert_failure(&out, "start with invalid session id (single char)");

    // Invalid: uppercase
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            "MySession",
        ],
        30,
    );
    assert_failure(&out, "start with invalid session id (uppercase)");
}

// ---------------------------------------------------------------------------
// 12. lifecycle_set_session_id_conflict
// ---------------------------------------------------------------------------

/// Start two sessions with same --set-session-id → second should fail
#[test]
fn lifecycle_set_session_id_conflict() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start first session with explicit ID
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            "my-session",
        ],
        30,
    );
    assert_success(&out, "start first session");

    // Start second session with same ID — should fail (conflict)
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            "my-session",
        ],
        30,
    );
    assert_failure(&out, "start second session with same id should fail");

    // Close
    let out = headless(&["browser", "close", "-s", "my-session"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 13. lifecycle_profile_based_auto_id
// ---------------------------------------------------------------------------

/// Start without --set-session-id → auto-generates from profile name
#[test]
fn lifecycle_profile_based_auto_id() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start with default profile (no --set-session-id)
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start with default profile");

    // list-sessions should show "local-1" as the auto-generated session ID
    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    assert!(
        stdout_str(&out).contains("local-1"),
        "auto-generated session id should be 'local-1', got: {}",
        stdout_str(&out)
    );

    // Close
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 14. lifecycle_port_fallback_when_9222_occupied
// ---------------------------------------------------------------------------

/// When the default CDP port (9222) is occupied by another process,
/// `browser start` should automatically pick a free port and succeed.
#[test]
fn lifecycle_port_fallback_when_9222_occupied() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Hold port 9222 to simulate another process (e.g. agent-browser) occupying it
    let listener = TcpListener::bind(("127.0.0.1", 9222));
    if listener.is_err() {
        // Port 9222 is already occupied by something else — the test condition
        // is naturally satisfied; proceed without our own listener.
    }
    // Keep `listener` alive for the duration of the test (dropped at end of scope)

    // Start headless — should succeed via port fallback
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 45);
    assert_success(&out, "start with port 9222 occupied");

    // Verify session is functional: goto + snapshot
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto with fallback port");

    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "snapshot with fallback port");

    // Cleanup
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");

    drop(listener);
}
