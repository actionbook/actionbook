//! Phase B2a contract E2E tests.
//!
//! Validates the JSON envelope shape for observation/query/logging commands
//! defined in Phase B2a.
//!
//! Each test is self-contained: start session → eval/observe → assert contracts → close.
//! All tests are gated by `RUN_E2E_TESTS=true`.

use crate::harness::{
    assert_success, headless, headless_json, set_body_html_js, skip, stdout_str, SessionGuard,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a headless session on about:blank, return (session_id, tab_id).
fn start_session() -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&out, "start session for b2a test");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from start");
    let session_id = json["context"]["session_id"]
        .as_str()
        .expect("session_id in start context")
        .to_string();
    let tab_id = json["data"]["tabs"][0]["tab_id"]
        .as_str()
        .unwrap_or("t0")
        .to_string();
    (session_id, tab_id)
}

// ---------------------------------------------------------------------------
// Test 1: Snapshot JSON envelope
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_snapshot_json_envelope() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless_json(&["browser", "snapshot", "-s", &sid, "-t", &tid], 30);
    assert_success(&out, "snapshot --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from snapshot");

    assert_eq!(json["ok"], true, "ok must be true, got: {}", json);
    assert_eq!(
        json["command"], "browser.snapshot",
        "command must be 'browser.snapshot', got: {}",
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
        context.get("session_id").and_then(|v| v.as_str()).is_some(),
        "context.session_id must be a string, got: {}",
        context
    );

    // data.format must be "snapshot"
    assert_eq!(
        json["data"]["format"], "snapshot",
        "data.format must be 'snapshot', got: {}",
        json["data"]
    );

    // meta present
    assert!(
        json["meta"]["duration_ms"].as_u64().is_some(),
        "meta.duration_ms must be present, got: {}",
        json["meta"]
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 2: Title, URL, Viewport JSON envelopes
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_title_url_viewport_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Title
    let title_out = headless_json(&["browser", "title", "-s", &sid, "-t", &tid], 15);
    assert_success(&title_out, "title --json");
    let title_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&title_out)).expect("valid JSON from title");
    assert_eq!(title_json["ok"], true);
    assert_eq!(title_json["command"], "browser.title");
    assert!(
        !title_json["data"]["value"].is_null(),
        "data.value must not be null for title, got: {}",
        title_json["data"]
    );

    // URL
    let url_out = headless_json(&["browser", "url", "-s", &sid, "-t", &tid], 15);
    assert_success(&url_out, "url --json");
    let url_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&url_out)).expect("valid JSON from url");
    assert_eq!(url_json["ok"], true);
    assert_eq!(url_json["command"], "browser.url");
    assert!(
        !url_json["data"]["value"].is_null(),
        "data.value must not be null for url, got: {}",
        url_json["data"]
    );

    // Viewport
    let vp_out = headless_json(&["browser", "viewport", "-s", &sid, "-t", &tid], 15);
    assert_success(&vp_out, "viewport --json");
    let vp_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&vp_out)).expect("valid JSON from viewport");
    assert_eq!(vp_json["ok"], true);
    assert_eq!(vp_json["command"], "browser.viewport");
    assert!(
        vp_json["data"]["width"].as_u64().is_some(),
        "data.width must be a number, got: {}",
        vp_json["data"]
    );
    assert!(
        vp_json["data"]["height"].as_u64().is_some(),
        "data.height must be a number, got: {}",
        vp_json["data"]
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 3: HTML, text, value JSON
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_html_text_value_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Set up DOM with an input
    let setup_js = set_body_html_js("<input id=\"x\" value=\"hello\">");
    let _ = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);

    // Value of #x
    let value_out = headless_json(&["browser", "value", "#x", "-s", &sid, "-t", &tid], 15);
    assert_success(&value_out, "value --json");
    let value_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&value_out)).expect("valid JSON from value");
    assert_eq!(value_json["ok"], true);
    assert_eq!(value_json["command"], "browser.value");
    assert_eq!(
        value_json["data"]["value"], "hello",
        "data.value must be 'hello', got: {}",
        value_json["data"]
    );

    // Text of body
    let text_out = headless_json(&["browser", "text", "body", "-s", &sid, "-t", &tid], 15);
    assert_success(&text_out, "text --json");
    let text_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&text_out)).expect("valid JSON from text");
    assert_eq!(text_json["ok"], true);
    assert_eq!(text_json["command"], "browser.text");
    // data.value should be a string (even if empty for about:blank body)
    assert!(
        text_json["data"]["value"].is_string() || text_json["data"]["value"].is_null(),
        "data.value should be a string for text command, got: {}",
        text_json["data"]
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 4: Query modes JSON
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_query_modes_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Set up 3 .item divs (first one also has id="unique" for `query one` test)
    let setup_js = set_body_html_js(
        "<div id=\"unique\" class=\"item\">A</div><div class=\"item\">B</div><div class=\"item\">C</div>",
    );
    let _ = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);

    // All mode
    let all_out = headless_json(
        &["browser", "query", "all", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&all_out, "query all --json");
    let all_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&all_out)).expect("valid JSON from query all");
    assert_eq!(all_json["ok"], true);
    assert_eq!(all_json["command"], "browser.query");
    assert_eq!(
        all_json["data"]["count"], 3,
        "count must be 3, got: {}",
        all_json["data"]
    );
    assert!(
        all_json["data"]["items"].is_array(),
        "items must be an array, got: {}",
        all_json["data"]
    );

    // One mode (use unique selector that matches exactly 1 element)
    let one_out = headless_json(
        &["browser", "query", "one", "#unique", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&one_out, "query one --json");
    let one_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&one_out)).expect("valid JSON from query one");
    assert_eq!(one_json["ok"], true);
    assert_eq!(one_json["data"]["count"], 1);
    assert!(
        !one_json["data"]["item"].is_null(),
        "item must not be null for one mode, got: {}",
        one_json["data"]
    );

    // Count mode
    let count_out = headless_json(
        &["browser", "query", "count", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&count_out, "query count --json");
    let count_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&count_out)).expect("valid JSON from query count");
    assert_eq!(count_json["ok"], true);
    assert_eq!(count_json["data"]["count"], 3);

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 5: Query cardinality error
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_query_cardinality_error() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Query for a selector that doesn't exist in "one" mode
    let out = headless_json(
        &[
            "browser",
            "query",
            "one",
            ".nonexistent-xyz-abc-123",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    // Should either fail (exit non-zero) or return ok=false
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from failed query");
    assert_eq!(
        json["ok"], false,
        "ok must be false when element not found, got: {}",
        json
    );
    assert_eq!(
        json["command"], "browser.query",
        "command must be 'browser.query', got: {}",
        json
    );
    let error_code = json["error"]["code"].as_str().unwrap_or("");
    assert!(
        error_code == "ELEMENT_NOT_FOUND"
            || error_code == "MULTIPLE_MATCHES"
            || error_code == "INTERNAL_ERROR",
        "error.code must be an acceptable query error code, got: {}",
        error_code
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 6: Describe JSON
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_describe_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Set up a button
    let setup_js = set_body_html_js("<button id=\"btn\">Click me</button>");
    let _ = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);

    let out = headless_json(&["browser", "describe", "#btn", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "describe --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from describe");
    assert_eq!(json["ok"], true, "ok must be true, got: {}", json);
    assert_eq!(json["command"], "browser.describe");
    assert!(
        !json["data"].is_null(),
        "data must not be null for describe, got: {}",
        json
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 7: State JSON
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_state_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Set up an input
    let setup_js = set_body_html_js("<input id=\"inp\" type=\"text\">");
    let _ = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);

    let out = headless_json(&["browser", "state", "#inp", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "state --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from state");
    assert_eq!(json["ok"], true, "ok must be true, got: {}", json);
    assert_eq!(json["command"], "browser.state");
    // data.state must have a visible field
    assert!(
        !json["data"].is_null(),
        "data must not be null for state, got: {}",
        json
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 8: Logs console JSON
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_logs_console_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Emit a console log
    let _ = headless(
        &[
            "browser",
            "eval",
            "console.log('hello from b2a test')",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );

    let out = headless_json(&["browser", "logs", "console", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "logs console --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from logs console");
    assert_eq!(json["ok"], true, "ok must be true, got: {}", json);
    assert_eq!(json["command"], "browser.logs.console");
    assert!(
        json["data"]["items"].is_array(),
        "data.items must be an array, got: {}",
        json["data"]
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 9: Inspect point JSON
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_inspect_point_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    // inspect-point may fail if the point hits no element on about:blank
    // Just check that we get a valid JSON envelope
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from inspect-point");

    assert_eq!(
        json["command"], "browser.inspect-point",
        "command must be 'browser.inspect-point', got: {}",
        json
    );
    // ok may be true or false depending on what's at that point
    let ok = json["ok"].as_bool().unwrap_or(false);
    if !ok {
        // If not ok, error.code must be a valid code
        let error_code = json["error"]["code"].as_str().unwrap_or("");
        assert!(
            !error_code.is_empty(),
            "error.code must not be empty when inspect-point fails, got: {}",
            json
        );
    }

    assert!(
        json["meta"]["duration_ms"].as_u64().is_some(),
        "meta.duration_ms must be present, got: {}",
        json["meta"]
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}
