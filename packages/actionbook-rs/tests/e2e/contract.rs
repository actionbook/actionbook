//! Phase A contract E2E tests.
//!
//! Validates the JSON envelope shape, error code mapping, and session ID rules
//! defined in the Phase A contracts.
//!
//! Each test is self-contained: start session(s) → assert contracts → close.
//! All tests are gated by `RUN_E2E_TESTS=true`.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};

// ---------------------------------------------------------------------------
// Group 1: JSON envelope shape
// ---------------------------------------------------------------------------

/// Verify that `browser start --json` produces the correct Phase A envelope:
/// ok=true, command="browser.start", context.session_id present, error=null,
/// meta.duration_ms present.
#[test]
fn contract_lifecycle_start_json_envelope() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from browser start");

    // Top-level shape
    assert_eq!(
        json["ok"], true,
        "ok must be true on success, got: {}",
        json
    );
    assert_eq!(
        json["command"], "browser.start",
        "command must be 'browser.start', got: {}",
        json
    );
    assert!(
        json["error"].is_null(),
        "error must be null on success, got: {}",
        json["error"]
    );

    // context must carry session_id
    let context = &json["context"];
    assert!(
        !context.is_null(),
        "context must not be null for browser.start, got: {}",
        json
    );
    assert!(
        context.get("session_id").and_then(|v| v.as_str()).is_some(),
        "context.session_id must be a string, got context: {}",
        context
    );

    // data must be present (session info)
    assert!(
        !json["data"].is_null(),
        "data must not be null for browser.start, got: {}",
        json
    );

    // meta must have duration_ms as a non-negative integer
    let meta = &json["meta"];
    assert!(
        !meta.is_null(),
        "meta must not be null, got: {}",
        json
    );
    assert!(
        meta.get("duration_ms").and_then(|v| v.as_u64()).is_some(),
        "meta.duration_ms must be a non-negative integer, got meta: {}",
        meta
    );

    // Cleanup: extract session_id and close
    let session_id = context["session_id"].as_str().unwrap();
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that `browser list-sessions --json` produces the correct envelope:
/// ok=true, command="browser.list-sessions", data.sessions is array, error=null,
/// meta present.
#[test]
fn contract_lifecycle_list_sessions_json_envelope() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start a session so there is at least one to list
    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start session");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON from start");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id in start context");

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from list-sessions");

    // Top-level shape
    assert_eq!(
        json["ok"], true,
        "ok must be true on success, got: {}",
        json
    );
    assert_eq!(
        json["command"], "browser.list-sessions",
        "command must be 'browser.list-sessions', got: {}",
        json
    );
    assert!(
        json["error"].is_null(),
        "error must be null on success, got: {}",
        json["error"]
    );

    // data.sessions must be an array
    let sessions = json["data"]["sessions"]
        .as_array()
        .expect("data.sessions must be an array");
    assert!(
        !sessions.is_empty(),
        "data.sessions must contain the started session, got: {}",
        json["data"]
    );

    // meta present with duration_ms
    let meta = &json["meta"];
    assert!(
        !meta.is_null(),
        "meta must not be null, got: {}",
        json
    );
    assert!(
        meta.get("duration_ms").and_then(|v| v.as_u64()).is_some(),
        "meta.duration_ms must be a non-negative integer, got meta: {}",
        meta
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that running a browser command against a non-existent session in
/// --json mode yields the correct error envelope:
/// ok=false, command field present, error.code="SESSION_NOT_FOUND", meta present.
#[test]
fn contract_non_lifecycle_error_json_envelope() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Run goto against a session ID that does not exist
    let out = headless_json(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "definitely-does-not-exist-xyz",
            "-t",
            "t0",
        ],
        10,
    );

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from failed goto");

    // Top-level shape
    assert_eq!(
        json["ok"], false,
        "ok must be false on error, got: {}",
        json
    );

    // command field must be present and non-empty
    assert!(
        json.get("command")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "command field must be present and non-empty, got: {}",
        json
    );

    // error.code must be SESSION_NOT_FOUND
    let error_code = json["error"]["code"]
        .as_str()
        .expect("error.code must be a string");
    assert_eq!(
        error_code, "SESSION_NOT_FOUND",
        "error.code must be SESSION_NOT_FOUND, got: {}",
        error_code
    );

    // meta present with duration_ms
    let meta = &json["meta"];
    assert!(
        !meta.is_null(),
        "meta must not be null, got: {}",
        json
    );
    assert!(
        meta.get("duration_ms").and_then(|v| v.as_u64()).is_some(),
        "meta.duration_ms must be a non-negative integer, got meta: {}",
        meta
    );
}

// ---------------------------------------------------------------------------
// Group 2: Error code mapping
// ---------------------------------------------------------------------------

/// Verify that closing/checking status of a non-existent session in --json mode
/// produces error.code == "SESSION_NOT_FOUND".
#[test]
fn contract_error_session_not_found() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // browser close on a non-existent session ID
    let out = headless_json(&["browser", "close", "-s", "no-such-session-abc123"], 10);

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from failed close");

    assert_eq!(
        json["ok"], false,
        "ok must be false for missing session, got: {}",
        json
    );
    let error_code = json["error"]["code"]
        .as_str()
        .expect("error.code must be a string");
    assert_eq!(
        error_code, "SESSION_NOT_FOUND",
        "error.code must be SESSION_NOT_FOUND, got: {}",
        error_code
    );

    // Also verify with browser status command
    let out2 = headless_json(&["browser", "status", "-s", "no-such-session-abc123"], 10);
    let json2: serde_json::Value =
        serde_json::from_str(&stdout_str(&out2)).expect("valid JSON from failed status");

    assert_eq!(
        json2["ok"], false,
        "ok must be false for missing session status, got: {}",
        json2
    );
    let error_code2 = json2["error"]["code"]
        .as_str()
        .expect("error.code must be a string in status response");
    assert_eq!(
        error_code2, "SESSION_NOT_FOUND",
        "error.code must be SESSION_NOT_FOUND for status on missing session, got: {}",
        error_code2
    );
}

/// Verify that `browser wait element` with a very short timeout on a selector
/// that won't exist yields error.code == "ELEMENT_NOT_FOUND".
#[test]
fn contract_error_element_not_found() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start a real session on example.com
    let start_out = headless_json(
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
    assert_success(&start_out, "start session for element_not_found test");

    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON from start");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id in start context");

    // Navigate so the page is fully loaded
    let goto_out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            session_id,
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&goto_out, "goto example.com");

    // Wait for an element that definitely does not exist, with a very short timeout
    let out = headless_json(
        &[
            "browser",
            "wait",
            "element",
            "#nonexistent-element-xyz",
            "-s",
            session_id,
            "-t",
            "t0",
            "--timeout",
            "500",
        ],
        15,
    );

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from failed wait-element");

    assert_eq!(
        json["ok"], false,
        "ok must be false when element not found, got: {}",
        json
    );
    let error_code = json["error"]["code"]
        .as_str()
        .expect("error.code must be a string");
    assert!(
        error_code == "ELEMENT_NOT_FOUND" || error_code == "TIMEOUT",
        "error.code must be ELEMENT_NOT_FOUND or TIMEOUT (timeout may fire first), got: {}",
        error_code
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

// ---------------------------------------------------------------------------
// Group 3: Session ID rules
// ---------------------------------------------------------------------------

/// Verify that `--set-session-id mytest-id` assigns exactly that ID.
#[test]
fn contract_session_id_explicit_set_session_id() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let explicit_id = "mytest-id";

    let start_out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            explicit_id,
        ],
        30,
    );
    assert_success(&start_out, "start with --set-session-id");

    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON from start");

    // The context should already carry the explicit session_id
    let context_id = start_json["context"]["session_id"]
        .as_str()
        .expect("context.session_id must be present");
    assert_eq!(
        context_id, explicit_id,
        "context.session_id must equal the explicit ID '{}', got: {}",
        explicit_id, context_id
    );

    // Confirm via list-sessions
    let list_out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&list_out, "list-sessions after explicit-id start");

    let list_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&list_out)).expect("valid JSON from list-sessions");
    let sessions = list_json["data"]["sessions"]
        .as_array()
        .expect("data.sessions must be array");

    let found = sessions.iter().any(|s| {
        s.get("session_id")
            .and_then(|v| v.as_str())
            .map(|id| id == explicit_id)
            .unwrap_or(false)
    });
    assert!(
        found,
        "session '{}' must appear in list-sessions, got sessions: {}",
        explicit_id,
        list_json["data"]["sessions"]
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", explicit_id], 30);
}

/// Verify that auto-generated session IDs start with "local-" (not "s0").
#[test]
fn contract_session_id_auto_gen_sequential() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start with auto-gen ID");

    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON from start");

    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("context.session_id must be present");

    assert!(
        session_id.starts_with("local-"),
        "auto-gen session ID must start with 'local-', got: {}",
        session_id
    );
    assert_ne!(
        session_id, "s0",
        "auto-gen session ID must not be the old 's0' format, got: {}",
        session_id
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that two consecutively started sessions receive distinct auto-gen IDs
/// following the "local-1", "local-2" pattern.
#[test]
fn contract_session_id_collision_skip() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start first session
    let start1 = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start1, "start first session");
    let json1: serde_json::Value =
        serde_json::from_str(&stdout_str(&start1)).expect("valid JSON from first start");
    let id1 = json1["context"]["session_id"]
        .as_str()
        .expect("context.session_id for first session")
        .to_string();

    // Start second session
    let start2 = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start2, "start second session");
    let json2: serde_json::Value =
        serde_json::from_str(&stdout_str(&start2)).expect("valid JSON from second start");
    let id2 = json2["context"]["session_id"]
        .as_str()
        .expect("context.session_id for second session")
        .to_string();

    // Both IDs must be distinct
    assert_ne!(
        id1, id2,
        "two auto-gen sessions must have distinct IDs, both got: {}",
        id1
    );

    // Both IDs must follow the "local-N" pattern
    assert!(
        id1.starts_with("local-"),
        "first auto-gen ID must start with 'local-', got: {}",
        id1
    );
    assert!(
        id2.starts_with("local-"),
        "second auto-gen ID must start with 'local-', got: {}",
        id2
    );

    // Confirm both appear in list-sessions
    let list_out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&list_out, "list-sessions with two sessions");
    let list_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&list_out)).expect("valid JSON from list-sessions");
    let sessions = list_json["data"]["sessions"]
        .as_array()
        .expect("data.sessions must be array");

    let found_ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s.get("session_id").and_then(|v| v.as_str()))
        .collect();

    assert!(
        found_ids.contains(&id1.as_str()),
        "first session '{}' must appear in list-sessions, found: {:?}",
        id1,
        found_ids
    );
    assert!(
        found_ids.contains(&id2.as_str()),
        "second session '{}' must appear in list-sessions, found: {:?}",
        id2,
        found_ids
    );

    // Cleanup both sessions
    let _ = headless(&["browser", "close", "-s", &id1], 30);
    let _ = headless(&["browser", "close", "-s", &id2], 30);
}
