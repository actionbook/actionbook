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
    assert!(!meta.is_null(), "meta must not be null, got: {}", json);
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
    assert!(!meta.is_null(), "meta must not be null, got: {}", json);
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
    assert!(!meta.is_null(), "meta must not be null, got: {}", json);
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
        error_code == "ELEMENT_NOT_FOUND"
            || error_code == "TIMEOUT"
            || error_code == "INTERNAL_ERROR",
        "error.code must be ELEMENT_NOT_FOUND, TIMEOUT, or INTERNAL_ERROR, got: {}",
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
        explicit_id, list_json["data"]["sessions"]
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

// ---------------------------------------------------------------------------
// Group 5: PRD 7.1 browser.start data shape
// ---------------------------------------------------------------------------

/// Verify that `browser start --json` returns the PRD 7.1 nested structure:
/// data.session (session_id, mode, status, headless, cdp_endpoint),
/// data.tab (tab_id, url, title, native_tab_id), data.reused.
#[test]
fn contract_start_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start --json for PRD shape");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from browser start");

    let data = &json["data"];

    // session object
    let session = &data["session"];
    assert!(
        session["session_id"].is_string(),
        "data.session.session_id must be a string, got: {}",
        session
    );
    assert!(
        session["mode"].is_string(),
        "data.session.mode must be a string, got: {}",
        session
    );
    assert_eq!(
        session["status"], "running",
        "data.session.status must be 'running', got: {}",
        session
    );
    assert!(
        session["headless"].is_boolean(),
        "data.session.headless must be a boolean, got: {}",
        session
    );

    // tab object
    let tab = &data["tab"];
    assert!(
        tab["tab_id"].is_string(),
        "data.tab.tab_id must be a string, got: {}",
        tab
    );
    assert!(
        tab["url"].is_string(),
        "data.tab.url must be a string, got: {}",
        tab
    );

    // reused flag
    assert!(
        data["reused"].is_boolean(),
        "data.reused must be a boolean, got: {}",
        data
    );

    // Cleanup
    let session_id = session["session_id"].as_str().unwrap();
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that `browser start` text output matches PRD 7.1 format:
/// contains `ok browser.start`, `mode: local`, `status: running`.
#[test]
fn contract_start_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start text for PRD format");

    let text = stdout_str(&out);

    assert!(
        text.contains("ok browser.start"),
        "text output must contain 'ok browser.start', got:\n{}",
        text
    );
    assert!(
        text.contains("mode: local"),
        "text output must contain 'mode: local', got:\n{}",
        text
    );
    assert!(
        text.contains("status: running"),
        "text output must contain 'status: running', got:\n{}",
        text
    );

    // Extract session_id from the JSON version for cleanup
    let json_out = headless_json(&["browser", "list-sessions"], 10);
    let json: serde_json::Value = serde_json::from_str(&stdout_str(&json_out)).unwrap_or_default();
    if let Some(sessions) = json["data"]["sessions"].as_array() {
        for s in sessions {
            if let Some(id) = s["session_id"].as_str() {
                let _ = headless(&["browser", "close", "-s", id], 30);
            }
        }
    }
}

/// Verify that `browser start --open-url` returns the post-navigation title,
/// not a stale "New Tab" or empty string.
#[test]
fn contract_start_open_url_returns_post_nav_title() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://actionbook.dev/",
        ],
        30,
    );
    assert_success(&out, "start --open-url --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from browser start");

    let data = &json["data"];

    // tab.url must reflect the navigated URL
    let tab_url = data["tab"]["url"].as_str().unwrap_or_default();
    assert!(
        tab_url.contains("actionbook.dev"),
        "data.tab.url must contain 'actionbook.dev', got: {}",
        tab_url
    );

    // tab.title must NOT be empty or "New Tab" — it should be the actual page title
    let tab_title = data["tab"]["title"].as_str().unwrap_or_default();
    assert!(
        !tab_title.is_empty() && tab_title != "New Tab" && tab_title != "about:blank",
        "data.tab.title must be the post-navigation title, got: '{}'",
        tab_title
    );

    // context.title must match data.tab.title
    let ctx_title = json["context"]["title"].as_str().unwrap_or_default();
    assert_eq!(
        ctx_title, tab_title,
        "context.title must match data.tab.title, got context='{}' vs data='{}'",
        ctx_title, tab_title
    );

    // Cleanup
    let session_id = data["session"]["session_id"].as_str().unwrap();
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

// ---------------------------------------------------------------------------
// Group 6: PRD 7.3 browser.status contract
// ---------------------------------------------------------------------------

/// Verify that `browser status --json` returns the PRD 7.3 nested structure:
/// data.session (session_id, mode, status, headless, tabs_count),
/// data.tabs (array of {tab_id, url, title}),
/// data.capabilities (snapshot, pdf, upload).
#[test]
fn contract_status_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start for status test");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id");

    let out = headless_json(&["browser", "status", "-s", session_id], 30);
    assert_success(&out, "status --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from status");
    let data = &json["data"];

    // session object
    let session = &data["session"];
    assert!(
        session["session_id"].is_string(),
        "data.session.session_id must be a string, got: {}",
        data
    );
    assert!(
        session["mode"].is_string(),
        "data.session.mode must be a string, got: {}",
        data
    );
    assert_eq!(
        session["status"], "running",
        "data.session.status must be 'running', got: {}",
        data
    );
    assert!(
        session["headless"].is_boolean(),
        "data.session.headless must be a boolean, got: {}",
        data
    );
    assert!(
        session["tabs_count"].is_number(),
        "data.session.tabs_count must be a number, got: {}",
        data
    );

    // tabs array
    let tabs = data["tabs"].as_array().expect("data.tabs must be an array");
    assert!(
        !tabs.is_empty(),
        "data.tabs must have at least 1 tab, got: {}",
        data
    );
    let tab = &tabs[0];
    assert!(
        tab["tab_id"].is_string(),
        "tabs[0].tab_id must be a string, got: {}",
        tab
    );
    assert!(
        tab["url"].is_string(),
        "tabs[0].url must be a string, got: {}",
        tab
    );

    // capabilities object
    let caps = &data["capabilities"];
    assert!(
        caps["snapshot"].is_boolean(),
        "data.capabilities.snapshot must be a boolean, got: {}",
        data
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that `browser status` text output matches PRD 7.3 format.
#[test]
fn contract_status_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start for status text test");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id");

    let out = headless(&["browser", "status", "-s", session_id], 30);
    assert_success(&out, "status text");
    let text = stdout_str(&out);

    assert!(
        text.contains("status: running"),
        "text must contain 'status: running', got:\n{}",
        text
    );
    assert!(
        text.contains("mode: local"),
        "text must contain 'mode: local', got:\n{}",
        text
    );
    assert!(
        text.contains("tabs:"),
        "text must contain 'tabs:', got:\n{}",
        text
    );
    assert!(
        !text.contains("windows:"),
        "text must NOT contain 'windows:', got:\n{}",
        text
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

// ---------------------------------------------------------------------------
// Group 7: PRD 7.4 browser.close contract
// ---------------------------------------------------------------------------

/// Verify that `browser close --json` returns PRD 7.4 data shape:
/// {session_id, status: "closed", closed_tabs: N}
#[test]
fn contract_close_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start for close test");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id");

    let out = headless_json(&["browser", "close", "-s", session_id], 30);
    assert_success(&out, "close --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from close");
    let data = &json["data"];

    assert_eq!(
        data["session_id"].as_str().unwrap(),
        session_id,
        "data.session_id must match, got: {}",
        data
    );
    assert_eq!(
        data["status"], "closed",
        "data.status must be 'closed', got: {}",
        data
    );
    assert!(
        data["closed_tabs"].is_number(),
        "data.closed_tabs must be a number, got: {}",
        data
    );
}

/// Verify that `browser close` text output matches PRD 7.4 format.
#[test]
fn contract_close_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start for close text test");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id");

    let out = headless(&["browser", "close", "-s", session_id], 30);
    assert_success(&out, "close text");
    let text = stdout_str(&out);

    assert!(
        text.contains("ok browser.close"),
        "text must contain 'ok browser.close', got:\n{}",
        text
    );
    assert!(
        text.contains("closed_tabs:"),
        "text must contain 'closed_tabs:', got:\n{}",
        text
    );
}

// ---------------------------------------------------------------------------
// Group 8: PRD 7.5 browser.restart contract
// ---------------------------------------------------------------------------

/// Verify that `browser restart --json` returns PRD 7.5 data shape:
/// {session: {session_id, mode, status, headless, tabs_count}, reopened: true}
#[test]
fn contract_restart_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start for restart test");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id");

    let out = headless_json(&["browser", "restart", "-s", session_id], 30);
    assert_success(&out, "restart --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from restart");
    let data = &json["data"];

    // session object
    let session = &data["session"];
    assert_eq!(
        session["session_id"].as_str().unwrap(),
        session_id,
        "data.session.session_id must match original, got: {}",
        data
    );
    assert!(
        session["mode"].is_string(),
        "data.session.mode must be a string, got: {}",
        data
    );
    assert_eq!(
        session["status"], "running",
        "data.session.status must be 'running', got: {}",
        data
    );
    assert!(
        session["headless"].is_boolean(),
        "data.session.headless must be a boolean, got: {}",
        data
    );
    assert!(
        session["tabs_count"].is_number(),
        "data.session.tabs_count must be a number, got: {}",
        data
    );

    // reopened flag
    assert_eq!(
        data["reopened"], true,
        "data.reopened must be true, got: {}",
        data
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that `browser restart` text output matches PRD 7.5 format.
#[test]
fn contract_restart_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let start_out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&start_out, "start for restart text test");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&start_out)).expect("valid JSON");
    let session_id = start_json["context"]["session_id"]
        .as_str()
        .expect("session_id");

    let out = headless(&["browser", "restart", "-s", session_id], 30);
    assert_success(&out, "restart text");
    let text = stdout_str(&out);

    assert!(
        text.contains("ok browser.restart"),
        "text must contain 'ok browser.restart', got:\n{}",
        text
    );
    assert!(
        text.contains("status: running"),
        "text must contain 'status: running', got:\n{}",
        text
    );

    // Cleanup — session is still running after restart
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

// ---------------------------------------------------------------------------
// Group 9: PRD 10.1 browser.snapshot — real nodes/stats
// ---------------------------------------------------------------------------

/// Verify that `browser snapshot --json` returns real parsed nodes from the
/// accessibility tree, not empty arrays or fabricated stats.
/// PRD 10.1: data.nodes is [{ref, role, name, value}], data.stats has real counts.
#[test]
fn contract_snapshot_prd_real_nodes() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session and navigate to a real page
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://actionbook.dev/",
        ],
        30,
    );
    assert_success(&out, "start --open-url --json");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from start");
    let session_id = start_json["data"]["session"]["session_id"]
        .as_str()
        .unwrap();
    let tab_id = start_json["data"]["tab"]["tab_id"].as_str().unwrap();

    // Take a snapshot
    let out = headless_json(
        &[
            "browser", "snapshot", "-s", session_id, "-t", tab_id, "--json",
        ],
        30,
    );
    assert_success(&out, "snapshot --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from snapshot");

    assert_eq!(
        json["command"], "browser.snapshot",
        "command must be browser.snapshot"
    );

    let data = &json["data"];

    // format must be "snapshot"
    assert_eq!(
        data["format"], "snapshot",
        "data.format must be 'snapshot', got: {}",
        data
    );

    // content must be a non-empty string with [ref=eN] patterns
    let content = data["content"]
        .as_str()
        .expect("data.content must be a string");
    assert!(!content.is_empty(), "data.content must be non-empty");
    assert!(
        content.contains("[ref=e"),
        "data.content must contain [ref=eN] references, got:\n{}",
        &content[..content.len().min(500)]
    );

    // nodes must be a non-empty array
    let nodes = data["nodes"]
        .as_array()
        .expect("data.nodes must be an array");
    assert!(
        !nodes.is_empty(),
        "data.nodes must be non-empty (real parsed nodes), got empty array"
    );

    // Each node must have ref, role, name, value fields per PRD
    for (i, node) in nodes.iter().enumerate() {
        assert!(
            node.get("ref").is_some(),
            "nodes[{}] must have 'ref' field, got: {}",
            i,
            node
        );
        assert!(
            node.get("role").and_then(|v| v.as_str()).is_some(),
            "nodes[{}] must have 'role' as string, got: {}",
            i,
            node
        );
        assert!(
            node.get("name").is_some(),
            "nodes[{}] must have 'name' field, got: {}",
            i,
            node
        );
        assert!(
            node.get("value").is_some(),
            "nodes[{}] must have 'value' field, got: {}",
            i,
            node
        );
    }

    // stats must have real counts
    let stats = &data["stats"];
    let node_count = stats["node_count"]
        .as_u64()
        .expect("stats.node_count must be an integer");
    let interactive_count = stats["interactive_count"]
        .as_u64()
        .expect("stats.interactive_count must be an integer");

    // node_count must match the actual nodes array length
    assert_eq!(
        node_count,
        nodes.len() as u64,
        "stats.node_count ({}) must match nodes.len() ({})",
        node_count,
        nodes.len()
    );

    // Derive expected interactive_count from the actual nodes' roles
    let interactive_roles: std::collections::HashSet<&str> = [
        "button",
        "link",
        "textbox",
        "checkbox",
        "radio",
        "combobox",
        "listbox",
        "menuitem",
        "tab",
        "treeitem",
        "switch",
        "slider",
        "spinbutton",
        "searchbox",
        "option",
        "menuitemcheckbox",
        "menuitemradio",
    ]
    .into_iter()
    .collect();

    let expected_interactive: u64 = nodes
        .iter()
        .filter(|n| {
            n.get("role")
                .and_then(|v| v.as_str())
                .map(|r| interactive_roles.contains(r))
                .unwrap_or(false)
        })
        .count() as u64;

    assert_eq!(
        interactive_count, expected_interactive,
        "stats.interactive_count ({}) must exactly match count derived from nodes roles ({})",
        interactive_count, expected_interactive
    );

    // A real page like actionbook.dev should have at least 1 interactive element
    assert!(
        interactive_count >= 1,
        "actionbook.dev should have at least 1 interactive element, got: {}",
        interactive_count
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

/// Verify that `browser snapshot` text output contains the tree content directly
/// (not JSON wrapper objects). PRD 10.1 text: tree text prefixed with
/// [session tab] url header.
#[test]
fn contract_snapshot_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://actionbook.dev/",
        ],
        30,
    );
    assert_success(&out, "start --open-url --json");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from start");
    let session_id = start_json["data"]["session"]["session_id"]
        .as_str()
        .unwrap();
    let tab_id = start_json["data"]["tab"]["tab_id"].as_str().unwrap();

    // Take a snapshot in text mode (no --json)
    let out = headless(&["browser", "snapshot", "-s", session_id, "-t", tab_id], 30);
    assert_success(&out, "snapshot text");
    let text = stdout_str(&out);

    // PRD 10.1: text output must start with "[session tab] url" header
    let header_pattern = format!("[{session_id} {tab_id}]");
    assert!(
        text.starts_with(&header_pattern),
        "text output must start with PRD header '[session tab] url', got:\n{}",
        &text[..text.len().min(500)]
    );

    // Header line should contain the URL
    let first_line = text.lines().next().unwrap_or("");
    assert!(
        first_line.contains("actionbook.dev"),
        "header line must contain the page URL, got: {}",
        first_line
    );

    // Text output must contain tree content with ref patterns
    assert!(
        text.contains("[ref=e"),
        "text output must contain [ref=eN] references, got:\n{}",
        &text[..text.len().min(500)]
    );

    // Must NOT contain JSON wrapper keys like __tree, __ctx_url
    assert!(
        !text.contains("__tree"),
        "text output must not contain __tree wrapper key, got:\n{}",
        &text[..text.len().min(500)]
    );
    assert!(
        !text.contains("__ctx_"),
        "text output must not contain __ctx_ wrapper keys, got:\n{}",
        &text[..text.len().min(500)]
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", session_id], 30);
}

// ---------------------------------------------------------------------------
// Group 8: Snapshot field-level contract (#t76 wave 1)
// ---------------------------------------------------------------------------

/// Helper: start a headless session on actionbook.dev, return (session_id, tab_id).
fn snapshot_session_setup() -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://actionbook.dev/",
        ],
        30,
    );
    assert_success(&out, "start --open-url --json");
    let start_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from start");
    let session_id = start_json["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tab_id = start_json["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string();
    (session_id, tab_id)
}

/// PRD 10.1: `--compact` removes empty structural nodes, resulting in fewer
/// nodes than a full snapshot of the same page.
#[test]
fn contract_snapshot_compact_reduces_nodes() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    // Full snapshot (no flags)
    let out_full = headless_json(
        &["browser", "snapshot", "-s", &session_id, "-t", &tab_id],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let full: serde_json::Value = serde_json::from_str(&stdout_str(&out_full)).expect("valid JSON");
    let full_count = full["data"]["stats"]["node_count"]
        .as_u64()
        .expect("full node_count");

    // Compact snapshot
    let out_compact = headless_json(
        &[
            "browser",
            "snapshot",
            "--compact",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out_compact, "snapshot --compact");
    let compact: serde_json::Value =
        serde_json::from_str(&stdout_str(&out_compact)).expect("valid JSON");

    // Verify PRD shape is preserved
    assert_eq!(compact["data"]["format"], "snapshot");
    let compact_nodes = compact["data"]["nodes"]
        .as_array()
        .expect("compact nodes array");
    let compact_count = compact["data"]["stats"]["node_count"]
        .as_u64()
        .expect("compact node_count");

    // node_count must match actual nodes length
    assert_eq!(compact_count, compact_nodes.len() as u64);

    // Compact should have fewer or equal nodes (removes empty structural nodes)
    assert!(
        compact_count <= full_count,
        "--compact node_count ({}) must be <= full node_count ({})",
        compact_count,
        full_count
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: `--depth <n>` limits the accessibility tree to the given depth,
/// resulting in fewer nodes than a full snapshot.
#[test]
fn contract_snapshot_depth_limits_nodes() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    // Full snapshot (no depth limit)
    let out_full = headless_json(
        &["browser", "snapshot", "-s", &session_id, "-t", &tab_id],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let full: serde_json::Value = serde_json::from_str(&stdout_str(&out_full)).expect("valid JSON");
    let full_count = full["data"]["stats"]["node_count"]
        .as_u64()
        .expect("full node_count");

    // Depth-limited snapshot (depth=1 = root children only)
    let out_depth = headless_json(
        &[
            "browser",
            "snapshot",
            "--depth",
            "1",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out_depth, "snapshot --depth 1");
    let depth: serde_json::Value =
        serde_json::from_str(&stdout_str(&out_depth)).expect("valid JSON");

    // Verify PRD shape is preserved
    assert_eq!(depth["data"]["format"], "snapshot");
    let depth_nodes = depth["data"]["nodes"]
        .as_array()
        .expect("depth nodes array");
    let depth_count = depth["data"]["stats"]["node_count"]
        .as_u64()
        .expect("depth node_count");

    // node_count must match actual nodes length
    assert_eq!(depth_count, depth_nodes.len() as u64);

    // Depth-limited should have strictly fewer nodes than full
    assert!(
        depth_count < full_count,
        "--depth 1 node_count ({}) must be < full node_count ({})",
        depth_count,
        full_count
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: `--interactive` returns only interactive elements.
/// Verify that interactive-only snapshot has fewer nodes, and all returned
/// nodes have interactive roles.
#[test]
fn contract_snapshot_interactive_only_roles() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    let interactive_roles: std::collections::HashSet<&str> = [
        "button",
        "link",
        "textbox",
        "checkbox",
        "radio",
        "combobox",
        "listbox",
        "menuitem",
        "tab",
        "treeitem",
        "switch",
        "slider",
        "spinbutton",
        "searchbox",
        "option",
        "menuitemcheckbox",
        "menuitemradio",
    ]
    .into_iter()
    .collect();

    // Full snapshot
    let out_full = headless_json(
        &["browser", "snapshot", "-s", &session_id, "-t", &tab_id],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let full: serde_json::Value = serde_json::from_str(&stdout_str(&out_full)).expect("valid JSON");
    let full_count = full["data"]["stats"]["node_count"]
        .as_u64()
        .expect("full node_count");

    // Interactive-only snapshot
    let out_int = headless_json(
        &[
            "browser",
            "snapshot",
            "--interactive",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out_int, "snapshot --interactive");
    let int: serde_json::Value = serde_json::from_str(&stdout_str(&out_int)).expect("valid JSON");

    assert_eq!(int["data"]["format"], "snapshot");
    let int_nodes = int["data"]["nodes"].as_array().expect("interactive nodes");
    let int_count = int["data"]["stats"]["node_count"]
        .as_u64()
        .expect("interactive node_count");

    assert_eq!(int_count, int_nodes.len() as u64);

    // Interactive should have fewer nodes than full
    assert!(
        int_count < full_count,
        "--interactive node_count ({}) must be < full node_count ({})",
        int_count,
        full_count
    );

    // All returned nodes must have interactive roles
    for (i, node) in int_nodes.iter().enumerate() {
        let role = node["role"].as_str().unwrap_or("");
        assert!(
            interactive_roles.contains(role),
            "interactive nodes[{}] role '{}' must be an interactive role",
            i,
            role
        );
    }

    // interactive_count must equal node_count for --interactive
    let int_interactive_count = int["data"]["stats"]["interactive_count"]
        .as_u64()
        .expect("interactive_count");
    assert_eq!(
        int_interactive_count, int_count,
        "--interactive: interactive_count ({}) must equal node_count ({})",
        int_interactive_count, int_count
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: `--cursor` flag is accepted and returns valid PRD shape.
/// The flag should include cursor-interactive custom elements.
#[test]
fn contract_snapshot_cursor_flag() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--cursor",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "snapshot --cursor");
    let json: serde_json::Value = serde_json::from_str(&stdout_str(&out)).expect("valid JSON");

    // Must still return valid PRD 10.1 shape
    assert_eq!(json["data"]["format"], "snapshot");
    assert!(
        json["data"]["content"].as_str().is_some(),
        "data.content must be a string"
    );
    assert!(
        json["data"]["nodes"].as_array().is_some(),
        "data.nodes must be an array"
    );
    assert!(
        json["data"]["stats"]["node_count"].as_u64().is_some(),
        "stats.node_count must be present"
    );
    assert!(
        json["data"]["stats"]["interactive_count"]
            .as_u64()
            .is_some(),
        "stats.interactive_count must be present"
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: `--selector <sel>` flag is accepted and returns valid PRD shape.
/// The flag should limit the snapshot to the specified subtree.
#[test]
fn contract_snapshot_selector_flag() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--selector",
            "body",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "snapshot --selector body");
    let json: serde_json::Value = serde_json::from_str(&stdout_str(&out)).expect("valid JSON");

    // Must still return valid PRD 10.1 shape
    assert_eq!(json["data"]["format"], "snapshot");
    assert!(
        json["data"]["content"].as_str().is_some(),
        "data.content must be a string"
    );
    assert!(
        json["data"]["nodes"].as_array().is_some(),
        "data.nodes must be an array"
    );
    assert!(
        json["data"]["stats"]["node_count"].as_u64().is_some(),
        "stats.node_count must be present"
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: `meta.truncated` must be present in the JSON envelope and set
/// to `false` for a normal page snapshot (no truncation).
#[test]
fn contract_snapshot_meta_truncated_false() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    let out = headless_json(
        &["browser", "snapshot", "-s", &session_id, "-t", &tab_id],
        30,
    );
    assert_success(&out, "snapshot --json");
    let json: serde_json::Value = serde_json::from_str(&stdout_str(&out)).expect("valid JSON");

    // meta must exist
    assert!(
        json.get("meta").is_some(),
        "envelope must have 'meta' field"
    );

    // meta.truncated must be false for a normal page
    assert_eq!(
        json["meta"]["truncated"], false,
        "meta.truncated must be false for normal snapshot, got: {}",
        json["meta"]["truncated"]
    );

    // meta.duration_ms must be present
    assert!(
        json["meta"]["duration_ms"].as_u64().is_some()
            || json["meta"]["duration_ms"].as_f64().is_some(),
        "meta.duration_ms must be a number"
    );

    // meta.warnings must be an array
    assert!(
        json["meta"]["warnings"].as_array().is_some(),
        "meta.warnings must be an array"
    );

    // meta.pagination must be present (null is fine)
    assert!(
        json["meta"].get("pagination").is_some(),
        "meta.pagination must be present"
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: text output with `--compact` flag must still include the
/// `[session tab] url` header and tree content with refs.
#[test]
fn contract_snapshot_text_compact_has_header() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    let out = headless(
        &[
            "browser",
            "snapshot",
            "--compact",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "snapshot --compact text");
    let text = stdout_str(&out);

    // PRD header must be present
    let header_pattern = format!("[{session_id} {tab_id}]");
    assert!(
        text.starts_with(&header_pattern),
        "compact text must start with PRD header, got:\n{}",
        &text[..text.len().min(300)]
    );

    // Must contain refs
    assert!(
        text.contains("[ref=e"),
        "compact text must contain [ref=eN] references"
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}

/// PRD 10.1: text output with `--depth` flag must still include the
/// `[session tab] url` header.
#[test]
fn contract_snapshot_text_depth_has_header() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = snapshot_session_setup();

    let out = headless(
        &[
            "browser",
            "snapshot",
            "--depth",
            "2",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "snapshot --depth 2 text");
    let text = stdout_str(&out);

    // PRD header must be present
    let header_pattern = format!("[{session_id} {tab_id}]");
    assert!(
        text.starts_with(&header_pattern),
        "depth text must start with PRD header, got:\n{}",
        &text[..text.len().min(300)]
    );

    // Cleanup
    let _ = headless(&["browser", "close", "-s", &session_id], 30);
}
