//! Browser data E2E tests: cookies and storage operations.
//!
//! Covers api-reference §13 (Cookies) and §14 (Storage).
//! Each test is self-contained: start → operate → close.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str};

fn parse_json_output(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_str(&stdout_str(out)).expect("valid JSON output")
}

// ── Cookies ────────────────────────────────────────────────────────

#[test]
fn cookies_set_get_delete() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Set cookie
    let out = headless_json(
        &["browser", "cookies", "set", "test", "val", "-s", "s0"],
        10,
    );
    assert_success(&out, "cookies set");
    let json = parse_json_output(&out);
    assert_eq!(json["data"]["action"], "set");
    assert_eq!(json["data"]["affected"], 1);

    // Get cookie — should contain "val"
    let out = headless_json(&["browser", "cookies", "get", "test", "-s", "s0"], 10);
    assert_success(&out, "cookies get after set");
    let json = parse_json_output(&out);
    assert_eq!(json["data"]["item"]["name"], "test");
    assert_eq!(json["data"]["item"]["value"], "val");

    // Delete cookie
    let out = headless_json(&["browser", "cookies", "delete", "test", "-s", "s0"], 10);
    assert_success(&out, "cookies delete");
    let json = parse_json_output(&out);
    assert_eq!(json["data"]["action"], "delete");
    assert_eq!(json["data"]["affected"], 1);

    // Get cookie after delete — should not contain "val"
    let out = headless_json(&["browser", "cookies", "get", "test", "-s", "s0"], 10);
    let json = parse_json_output(&out);
    assert!(json["data"]["item"].is_null(), "cookie should be deleted");

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn cookies_list_and_clear() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Set a cookie
    let out = headless(
        &[
            "browser",
            "cookies",
            "set",
            "x",
            "y",
            "-s",
            "s0",
            "--domain",
            "example.com",
        ],
        10,
    );
    assert_success(&out, "cookies set");

    // List cookies by domain — should contain our cookie
    let out = headless_json(
        &[
            "browser",
            "cookies",
            "list",
            "-s",
            "s0",
            "--domain",
            "example.com",
        ],
        10,
    );
    assert_success(&out, "cookies list");
    let json = parse_json_output(&out);
    let list_output = json["data"]["items"].to_string();
    assert!(
        list_output.contains("x"),
        "cookies list should contain 'x', got: {}",
        list_output
    );

    // Clear filtered cookies
    let out = headless_json(
        &[
            "browser",
            "cookies",
            "clear",
            "-s",
            "s0",
            "--domain",
            "example.com",
        ],
        10,
    );
    assert_success(&out, "cookies clear");
    let json = parse_json_output(&out);
    assert_eq!(json["data"]["action"], "clear");
    assert!(json["data"]["affected"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(json["data"]["domain"], "example.com");

    // List after clear — should be empty (no cookie names)
    let out = headless_json(
        &[
            "browser",
            "cookies",
            "list",
            "-s",
            "s0",
            "--domain",
            "example.com",
        ],
        10,
    );
    assert_success(&out, "cookies list after clear");
    let json = parse_json_output(&out);
    let list_output = json["data"]["items"].to_string();
    assert!(
        !list_output.contains("x"),
        "cookies list after clear should not contain 'x', got: {}",
        list_output
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn cookies_s1t2_shared() {
    if skip() {
        return;
    }

    // Start session with first tab
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

    // Create a second tab by opening a URL on s0
    let out = headless(&["browser", "open", "https://example.com", "-s", "s0"], 30);
    assert_success(&out, "create second tab");

    // Navigate second tab to a different page
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com/page2",
            "-s",
            "s0",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "open second tab");

    // Set cookie on the session
    let out = headless_json(
        &[
            "browser",
            "cookies",
            "set",
            "shared",
            "cookieval",
            "-s",
            "s0",
        ],
        10,
    );
    assert_success(&out, "cookies set");
    let json = parse_json_output(&out);
    assert_eq!(json["data"]["action"], "set");

    // Get cookie — cookies are session-level so should be visible
    let out = headless_json(&["browser", "cookies", "get", "shared", "-s", "s0"], 10);
    assert_success(&out, "cookies get");
    let json = parse_json_output(&out);
    assert_eq!(json["data"]["item"]["value"], "cookieval");

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Local Storage ──────────────────────────────────────────────────

#[test]
fn storage_local_set_get() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Set local storage key
    let out = headless(
        &[
            "browser",
            "local-storage",
            "set",
            "mykey",
            "myval",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage set");

    // Get local storage key
    let out = headless(
        &[
            "browser",
            "local-storage",
            "get",
            "mykey",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage get");
    assert!(
        stdout_str(&out).contains("myval"),
        "local-storage get should contain 'myval', got: {}",
        stdout_str(&out)
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn storage_local_list_delete_clear() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Set a key
    let out = headless(
        &[
            "browser",
            "local-storage",
            "set",
            "lskey",
            "lsval",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage set");

    // List — should contain our key
    let out = headless(
        &["browser", "local-storage", "list", "-s", "s0", "-t", "t0"],
        10,
    );
    assert_success(&out, "local-storage list");
    assert!(
        stdout_str(&out).contains("lskey"),
        "local-storage list should contain 'lskey', got: {}",
        stdout_str(&out)
    );

    // Delete the key
    let out = headless(
        &[
            "browser",
            "local-storage",
            "delete",
            "lskey",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage delete");

    // List after delete — should not contain the key
    let out = headless(
        &["browser", "local-storage", "list", "-s", "s0", "-t", "t0"],
        10,
    );
    assert_success(&out, "local-storage list after delete");
    assert!(
        !stdout_str(&out).contains("lskey"),
        "local-storage list after delete should not contain 'lskey', got: {}",
        stdout_str(&out)
    );

    // Set an extra key to test clear
    let out = headless(
        &[
            "browser",
            "local-storage",
            "set",
            "extra",
            "val",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage set extra");

    // Clear local storage
    let out = headless(
        &[
            "browser",
            "local-storage",
            "clear",
            "extra",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage clear");

    // Verify clear worked — list should not contain "extra"
    let out = headless(
        &["browser", "local-storage", "list", "-s", "s0", "-t", "t0"],
        10,
    );
    assert_success(&out, "local-storage list after clear");
    assert!(
        !stdout_str(&out).contains("extra"),
        "local-storage list after clear should not contain 'extra', got: {}",
        stdout_str(&out)
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Session Storage ────────────────────────────────────────────────

#[test]
fn storage_session_roundtrip() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Set session storage key
    let out = headless(
        &[
            "browser",
            "session-storage",
            "set",
            "sk",
            "sv",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "session-storage set");

    // Get session storage key
    let out = headless(
        &[
            "browser",
            "session-storage",
            "get",
            "sk",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "session-storage get");
    assert!(
        stdout_str(&out).contains("sv"),
        "session-storage get should contain 'sv', got: {}",
        stdout_str(&out)
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn storage_session_list_delete_clear() {
    if skip() {
        return;
    }

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
    assert_success(&out, "start");

    // Set a session storage key
    let out = headless(
        &[
            "browser",
            "session-storage",
            "set",
            "sskey",
            "ssval",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "session-storage set");

    // List — should contain our key
    let out = headless(
        &["browser", "session-storage", "list", "-s", "s0", "-t", "t0"],
        10,
    );
    assert_success(&out, "session-storage list");
    assert!(
        stdout_str(&out).contains("sskey"),
        "session-storage list should contain 'sskey', got: {}",
        stdout_str(&out)
    );

    // Delete the key
    let out = headless(
        &[
            "browser",
            "session-storage",
            "delete",
            "sskey",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "session-storage delete");

    // List after delete — should not contain the key
    let out = headless(
        &["browser", "session-storage", "list", "-s", "s0", "-t", "t0"],
        10,
    );
    assert_success(&out, "session-storage list after delete");
    assert!(
        !stdout_str(&out).contains("sskey"),
        "session-storage list after delete should not contain 'sskey', got: {}",
        stdout_str(&out)
    );

    // Set another key and clear
    let out = headless(
        &[
            "browser",
            "session-storage",
            "set",
            "ssextra",
            "ssval2",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "session-storage set extra");

    // Clear session storage
    let out = headless(
        &[
            "browser",
            "session-storage",
            "clear",
            "extra",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "session-storage clear");

    // Verify clear worked
    let out = headless(
        &["browser", "session-storage", "list", "-s", "s0", "-t", "t0"],
        10,
    );
    assert_success(&out, "session-storage list after clear");
    assert!(
        !stdout_str(&out).contains("extra"),
        "session-storage list after clear should not contain 'extra', got: {}",
        stdout_str(&out)
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

// ── Cross-tab Storage ──────────────────────────────────────────────

#[test]
fn storage_s1t2_isolation() {
    if skip() {
        return;
    }

    // Start session with first tab
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

    // Create a second tab by opening a URL on s0
    let out = headless(&["browser", "open", "https://example.com", "-s", "s0"], 30);
    assert_success(&out, "create second tab");

    // Navigate second tab to a different page
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com/page2",
            "-s",
            "s0",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "open second tab");

    // Set local-storage on t0
    let out = headless(
        &[
            "browser",
            "local-storage",
            "set",
            "crosskey",
            "crossval",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "local-storage set on t0");

    // Get local-storage on t1 — both tabs are same origin (example.com),
    // so localStorage is shared. The value set on t0 should be visible on t1.
    let out = headless(
        &[
            "browser",
            "local-storage",
            "get",
            "crosskey",
            "-s",
            "s0",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "local-storage get on t1");
    let t1_output = stdout_str(&out);
    assert!(
        t1_output.contains("crossval"),
        "same-origin tabs should share localStorage — t1 should see 'crossval', got: {}",
        t1_output
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}
