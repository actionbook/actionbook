//! E2E tests for `actionbook browser send` command.
//!
//! Validates that fetch-based HTTP requests execute correctly through the
//! full CLI → daemon → CDP pipeline.

use crate::harness::{
    SessionGuard, assert_failure, assert_meta, assert_success, headless, headless_json, parse_json,
    skip, start_session, stdout_str, url_a, url_echo,
};

/// Local server URL for `/api/data` endpoint used in send tests.
fn api_data_url() -> String {
    format!(
        "http://127.0.0.1:{}/api/data?source=send-test",
        crate::harness::local_server().port
    )
}

/// Helper: parse the echo endpoint's JSON body from a send response.
/// The echo endpoint returns the request method, headers, and body as JSON.
fn parse_echo_body(v: &serde_json::Value) -> serde_json::Value {
    let body_str = v["data"]["body"].as_str().expect("body must be a string");
    serde_json::from_str(body_str).expect("echo body should be valid JSON")
}

#[test]
fn send_get_returns_json_response() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "send",
            &api_data_url(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "send GET");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser send");
    assert!(v["error"].is_null());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["status"], 200);
    assert_eq!(v["data"]["statusText"], "OK");

    // Verify response body contains the expected JSON from the fixture
    let body_str = v["data"]["body"].as_str().expect("body must be a string");
    let body: serde_json::Value =
        serde_json::from_str(body_str).expect("body should be valid JSON");
    assert_eq!(body["ok"], true);
    assert_eq!(body["source"], "send-test");

    assert_meta(&v);
}

#[test]
fn send_post_with_body_verified() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "send",
            &url_echo(),
            "-X",
            "POST",
            "-d",
            r#"{"key":"value"}"#,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "send POST echo");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], 200);

    // Verify the echo confirms POST method and body were forwarded
    let echo = parse_echo_body(&v);
    assert_eq!(echo["method"], "POST", "method should be POST");
    let echoed_body = echo["body"].as_str().unwrap_or("");
    assert!(
        echoed_body.contains(r#"{"key":"value"}"#),
        "request body should be forwarded: {echoed_body}"
    );
    assert_meta(&v);
}

#[test]
fn send_with_custom_headers_verified() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "send",
            &url_echo(),
            "-H",
            "X-Custom-Header: test-value",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "send with headers echo");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], 200);

    // Verify the echo confirms the custom header was forwarded
    let echo = parse_echo_body(&v);
    assert_eq!(
        echo["headers"]["x-custom-header"], "test-value",
        "custom header should be forwarded to the server"
    );
    assert_meta(&v);
}

#[test]
fn send_text_mode_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "send",
            &api_data_url(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "send text mode");
    let text = stdout_str(&out);

    // Text mode output should contain the response status and body
    assert!(
        text.contains("200") || text.contains("ok"),
        "text output should show status or body content: {text}"
    );
}

#[test]
fn send_invalid_session_fails() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "send",
            "https://example.com",
            "--session",
            "nonexistent-session",
            "--tab",
            "t0",
        ],
        15,
    );
    assert_failure(&out, "send invalid session");
    let v = parse_json(&out);

    assert_eq!(v["ok"], false);
    assert!(v["error"]["code"].as_str().is_some());
    assert_meta(&v);
}

#[test]
fn send_explicit_method_override_verified() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    // Use PUT method explicitly — echo endpoint confirms it
    let out = headless_json(
        &[
            "browser",
            "send",
            &url_echo(),
            "-X",
            "PUT",
            "-d",
            r#"{"update":"data"}"#,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "send PUT echo");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], 200);

    // Verify the echo confirms PUT method and body
    let echo = parse_echo_body(&v);
    assert_eq!(echo["method"], "PUT", "method should be PUT");
    let echoed_body = echo["body"].as_str().unwrap_or("");
    assert!(
        echoed_body.contains(r#"{"update":"data"}"#),
        "request body should be forwarded: {echoed_body}"
    );
    assert_meta(&v);
}

#[test]
fn send_response_includes_headers() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "send",
            &api_data_url(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "send response headers");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    // The /api/data fixture returns X-Ab-Fixture: api-data header
    let headers = &v["data"]["headers"];
    assert!(headers.is_object(), "response headers must be an object");
    assert_eq!(
        headers["x-ab-fixture"], "api-data",
        "fixture header should be present"
    );
    assert_meta(&v);
}
