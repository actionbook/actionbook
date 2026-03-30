//! Browser lifecycle E2E tests: start, status, list, restart, close.
//!
//! Each test is self-contained: start → operate → assert → close.
//! Uses daemon v2 CLI format with --session and --tab addressing.

use std::net::TcpListener;

use serde_json::Value;

use crate::harness::{
    assert_failure, assert_success, headless, headless_json, skip, stdout_str, SessionGuard,
};

const TEST_URL: &str = "https://actionbook.dev/";

// ---------------------------------------------------------------------------
// Helper: parse JSON envelope from a command output
// ---------------------------------------------------------------------------

fn parse_json(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw stdout: {text}");
    })
}

// ---------------------------------------------------------------------------
// 1. lifecycle_open_and_close
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_open_and_close() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start a headless browser session — verify JSON response
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "start: ok should be true");
    assert_eq!(v["command"], "browser.start", "start: command field");
    assert_eq!(
        v["data"]["session"]["session_id"], "local-1",
        "start: session_id should be local-1"
    );
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "start: status should be running"
    );
    assert_eq!(
        v["data"]["session"]["mode"], "local",
        "start: mode should be local"
    );
    assert!(
        v["data"]["tab"]["tab_id"].as_str().is_some(),
        "start: tab_id should be present, got: {}",
        v["data"]["tab"]
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "start: context.session_id should be local-1"
    );

    // Status should show session info — verify JSON response
    let out = headless_json(&["browser", "status", "-s", "local-1"], 10);
    assert_success(&out, "status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "status: ok should be true");
    assert_eq!(v["command"], "browser.status", "status: command field");
    assert_eq!(
        v["data"]["session"]["session_id"], "local-1",
        "status: session_id should be local-1"
    );
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "status: status should be running"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "status: context.session_id should be local-1"
    );

    // Close the session — verify JSON response
    let out = headless_json(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "close: ok should be true");
    assert_eq!(v["command"], "browser.close", "close: command field");
    assert_eq!(
        v["data"]["session_id"], "local-1",
        "close: session_id should be local-1"
    );
    assert_eq!(
        v["data"]["status"], "closed",
        "close: status should be closed"
    );
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

    // Start headless — verify headless field in JSON response
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start headless");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "start: ok should be true");
    assert_eq!(v["command"], "browser.start", "start: command field");
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "start: status should be running"
    );
    assert_eq!(
        v["data"]["session"]["headless"], true,
        "start: headless should be true"
    );

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

    // Start session with a URL — verify tab URL in JSON response
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start with url");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "start: ok should be true");
    assert_eq!(v["command"], "browser.start", "start: command field");
    assert!(
        v["data"]["tab"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "start: tab.url should contain actionbook.dev, got: {}",
        v["data"]["tab"]["url"]
    );

    // Eval location.href — verify data.value in JSON response
    let out = headless_json(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "eval location");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "eval: ok should be true");
    assert_eq!(v["command"], "browser.eval", "eval: command field");
    assert!(
        v["data"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "eval: data.value should contain actionbook.dev, got: {}",
        v["data"]["value"]
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

    // Status — verify JSON response fields
    let out = headless_json(&["browser", "status", "-s", "local-1"], 10);
    assert_success(&out, "status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "status: ok should be true");
    assert_eq!(v["command"], "browser.status", "status: command field");
    assert_eq!(
        v["data"]["session"]["session_id"], "local-1",
        "status: session_id should be local-1"
    );
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "status: status should be running"
    );
    assert_eq!(
        v["data"]["session"]["mode"], "local",
        "status: mode should be local"
    );
    assert!(
        v["data"]["tabs"].as_array().is_some(),
        "status: tabs array should be present"
    );
    assert!(
        v["data"]["capabilities"].is_object(),
        "status: capabilities object should be present"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "status: context.session_id should be local-1"
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

    // list-sessions — verify JSON response
    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "list-sessions: ok should be true");
    assert_eq!(
        v["command"], "browser.list-sessions",
        "list-sessions: command field"
    );
    // list-sessions is a Global command — must NOT return context
    assert!(
        v["context"].is_null(),
        "list-sessions: context should be null for global commands, got: {}",
        v["context"]
    );
    assert!(
        v["data"]["total_sessions"].as_u64().unwrap_or(0) >= 1,
        "list-sessions: total_sessions should be >= 1"
    );
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(
        ids.contains(&"local-1"),
        "list-sessions: sessions should contain local-1, got: {:?}",
        ids
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

    // Restart — verify JSON response
    let out = headless_json(&["browser", "restart", "-s", "local-1"], 30);
    assert_success(&out, "restart");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "restart: ok should be true");
    assert_eq!(v["command"], "browser.restart", "restart: command field");
    assert_eq!(
        v["data"]["session"]["session_id"], "local-1",
        "restart: session_id should be local-1"
    );
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "restart: status should be running after restart"
    );
    assert_eq!(
        v["data"]["reopened"], true,
        "restart: reopened should be true"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "restart: context.session_id should be local-1"
    );

    // Status should still work after restart
    let out = headless_json(&["browser", "status", "-s", "local-1"], 10);
    assert_success(&out, "status after restart");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "status after restart: should still be running"
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
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start session");

    // Goto actionbook.dev — verify JSON response
    let out = headless_json(
        &["browser", "goto", TEST_URL, "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "goto");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "goto: ok should be true");
    assert_eq!(v["command"], "browser.goto", "goto: command field");
    assert_eq!(v["data"]["kind"], "goto", "goto: data.kind should be goto");
    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "goto: to_url should contain actionbook.dev, got: {}",
        v["data"]["to_url"]
    );

    // Snapshot — verify JSON envelope
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "snapshot");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "snapshot: ok should be true");
    assert_eq!(v["command"], "browser.snapshot", "snapshot: command field");
    assert!(!v["data"].is_null(), "snapshot: data should not be null");

    // Close — verify closed_tabs in JSON response
    let out = headless_json(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close after operations");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "close: ok should be true");
    assert_eq!(v["command"], "browser.close", "close: command field");
    assert_eq!(
        v["data"]["status"], "closed",
        "close: status should be closed"
    );
    assert!(
        v["data"]["closed_tabs"].as_u64().unwrap_or(0) >= 1,
        "close: closed_tabs should be >= 1"
    );
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
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start session");

    // Open a second tab in the same session
    let out = headless(&["browser", "open", TEST_URL, "-s", "local-1"], 30);
    assert_success(&out, "open second tab");

    // Close the session — verify closed_tabs == 2 in JSON response
    let out = headless_json(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close session with multiple tabs");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "close: ok should be true");
    assert_eq!(v["command"], "browser.close", "close: command field");
    assert_eq!(
        v["data"]["status"], "closed",
        "close: status should be closed"
    );
    assert_eq!(
        v["data"]["closed_tabs"],
        serde_json::json!(2),
        "close: closed_tabs should be 2 (both tabs closed)"
    );
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

    // Second close should fail — verify JSON error response
    let out = headless_json(&["browser", "close", "-s", "local-1"], 30);
    assert_failure(&out, "second close should fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false, "second close: ok should be false");
    assert_eq!(v["command"], "browser.close", "second close: command field");
    assert!(
        !v["error"].is_null(),
        "second close: error should not be null"
    );
    assert_eq!(
        v["error"]["code"], "SESSION_NOT_FOUND",
        "second close: error code should be SESSION_NOT_FOUND"
    );
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

    // Start session with explicit ID — verify session_id in JSON response
    let out = headless_json(
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
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "start: ok should be true");
    assert_eq!(v["command"], "browser.start", "start: command field");
    assert_eq!(
        v["data"]["session"]["session_id"], "research-google",
        "start: session_id should match --set-session-id"
    );
    assert_eq!(
        v["context"]["session_id"], "research-google",
        "start: context.session_id should match --set-session-id"
    );

    // Status should work with the explicit ID
    let out = headless_json(&["browser", "status", "-s", "research-google"], 10);
    assert_success(&out, "status with explicit id");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["session"]["session_id"], "research-google",
        "status: session_id should be research-google"
    );

    // list-sessions should show the explicit ID
    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(
        ids.contains(&"research-google"),
        "list-sessions: sessions should contain research-google, got: {:?}",
        ids
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

    // Invalid: starts with number — verify JSON error response
    let out = headless_json(
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
    let v = parse_json(&out);
    assert_eq!(v["ok"], false, "invalid id: ok should be false");
    assert!(!v["error"].is_null(), "invalid id: error should be present");

    // Invalid: single character (min 2 chars)
    let out = headless_json(
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
    let v = parse_json(&out);
    assert_eq!(v["ok"], false, "single char id: ok should be false");
    assert!(
        !v["error"].is_null(),
        "single char id: error should be present"
    );

    // Invalid: uppercase
    let out = headless_json(
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
    let v = parse_json(&out);
    assert_eq!(v["ok"], false, "uppercase id: ok should be false");
    assert!(
        !v["error"].is_null(),
        "uppercase id: error should be present"
    );
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

    // Start second session with same ID — verify JSON error response
    let out = headless_json(
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
    let v = parse_json(&out);
    assert_eq!(v["ok"], false, "conflict: ok should be false");
    assert_eq!(v["command"], "browser.start", "conflict: command field");
    assert!(!v["error"].is_null(), "conflict: error should not be null");

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

    // Start with default profile — verify auto-generated session_id in JSON
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start with default profile");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "start: ok should be true");
    let session_id = v["data"]["session"]["session_id"]
        .as_str()
        .expect("session_id should be a string");
    assert_eq!(
        session_id, "local-1",
        "start: auto-generated session_id should be local-1, got: {session_id}"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "start: context.session_id should be local-1"
    );

    // list-sessions should show "local-1" as the auto-generated session ID
    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(
        ids.contains(&"local-1"),
        "list-sessions: sessions should contain local-1, got: {:?}",
        ids
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

    // Start headless — verify JSON response (port fallback succeeds)
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 45);
    assert_success(&out, "start with port 9222 occupied");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "start: ok should be true");
    assert_eq!(v["command"], "browser.start", "start: command field");
    assert_eq!(
        v["data"]["session"]["status"], "running",
        "start: status should be running even when port 9222 is occupied"
    );

    // Verify session is functional: goto
    let out = headless_json(
        &["browser", "goto", TEST_URL, "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "goto with fallback port");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "goto: ok should be true");
    assert_eq!(v["data"]["kind"], "goto", "goto: data.kind should be goto");

    // Snapshot
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "snapshot with fallback port");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "snapshot: ok should be true");

    // Cleanup
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");

    drop(listener);
}
