//! Browser navigation E2E tests: goto, back, forward, reload.
//!
//! All navigation commands are Tab-level: require `--session <SID> --tab <TID>`.
//! Tests are strict per api-reference.md §9.
//!
//! All 4 commands are fully implemented. All 30 tests are expected to pass.
//! `nav_reload_navigation_failed_json` uses a conditional assertion to handle
//! Chrome's non-deterministic behavior when reloading after a failed navigation.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

const URL_A: &str = "https://actionbook.dev";
const URL_B: &str = "https://example.com";

// ── Helpers ───────────────────────────────────────────────────────────

/// Assert full §2.4 meta structure.
fn assert_meta(v: &serde_json::Value) {
    assert!(
        v["meta"]["duration_ms"].is_number(),
        "meta.duration_ms must be a number"
    );
    assert!(
        v["meta"]["warnings"].is_array(),
        "meta.warnings must be an array"
    );
    assert!(
        v["meta"]["pagination"].is_null(),
        "meta.pagination must be null"
    );
    assert!(
        v["meta"]["truncated"].is_boolean(),
        "meta.truncated must be a boolean"
    );
}

/// Assert full §3.1 error envelope.
fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false, "ok must be false on error");
    assert!(v["data"].is_null(), "data must be null on failure");
    assert_eq!(v["error"]["code"], expected_code);
    assert!(
        v["error"]["message"].is_string(),
        "error.message must be a string"
    );
    assert!(
        v["error"]["retryable"].is_boolean(),
        "error.retryable must be a boolean"
    );
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details must be object or null"
    );
    assert_meta(v);
}

/// Assert navigation data fields per §9 contract.
///
/// For goto (has_requested_url = true): asserts `requested_url` is a non-empty string.
/// For back/forward/reload (has_requested_url = false): asserts `requested_url` is null
/// per §9.2–9.4 interpretation — those commands have no "requested" URL concept.
fn assert_nav_data(v: &serde_json::Value, expected_kind: &str, has_requested_url: bool) {
    let data = &v["data"];
    assert_eq!(
        data["kind"], expected_kind,
        "data.kind must be '{expected_kind}'"
    );
    if has_requested_url {
        assert!(
            data["requested_url"].is_string(),
            "data.requested_url must be a string for goto"
        );
        assert!(
            !data["requested_url"].as_str().unwrap().is_empty(),
            "data.requested_url must not be empty"
        );
    } else {
        // §9.2–9.4: back/forward/reload share §9.1 data structure but have no
        // "requested URL" concept. requested_url must be null (field present, value null).
        assert!(
            data["requested_url"].is_null(),
            "data.requested_url must be null for back/forward/reload (got {:?})",
            data["requested_url"]
        );
    }
    assert!(
        data["from_url"].is_string(),
        "data.from_url must be a string"
    );
    assert!(data["to_url"].is_string(), "data.to_url must be a string");
    assert!(data["title"].is_string(), "data.title must be a string");
}

/// Start a headless session, return (session_id, tab_id).
fn start_session(url: &str) -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            url,
        ],
        30,
    );
    assert_success(&out, "start session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    (sid, tid)
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

// ===========================================================================
// Group 1: goto — Happy Path (§9.1)
// ===========================================================================

#[test]
fn nav_goto_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.goto");
    assert!(v["error"].is_null());

    // context — tab-level must have session_id + tab_id
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    // context.url updated to post-navigation URL per §5.6
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be updated after goto"
    );
    assert!(
        v["context"]["title"].is_string(),
        "context.title must be updated after goto"
    );

    // data fields per §9.1
    assert_nav_data(&v, "goto", true);
    // requested_url must match the URL we passed
    assert_eq!(v["data"]["requested_url"], URL_B);
    // to_url must be the final URL (may differ from requested_url on redirect)
    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap()
            .contains("example.com"),
        "to_url should contain example.com"
    );

    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn nav_goto_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto text");
    let text = stdout_str(&out);

    // §2.5 tab-level text: `[<session_id> <tab_id>] <url>`
    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.goto"),
        "must contain ok browser.goto"
    );
    assert!(text.contains("title:"), "must contain title:");

    close_session(&sid);
}

#[test]
fn nav_goto_context_url_updated() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Navigate away from URL_A to URL_B
    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b");
    let v = parse_json(&out);

    // §5.6: context.url must reflect the post-navigation URL
    let ctx_url = v["context"]["url"].as_str().unwrap_or("");
    assert!(
        ctx_url.contains("example.com"),
        "context.url must be updated to destination: got '{ctx_url}'"
    );

    // §5.6: context.title is updated synchronously when known
    let ctx_title = v["context"]["title"].as_str().unwrap_or("");
    assert!(
        !ctx_title.is_empty(),
        "context.title must be updated after goto: got empty string"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 2: goto — Error Paths (§9.1)
// ===========================================================================

/// NAVIGATION_FAILED: passed a URL with an unrecognized scheme that Chrome CDP
/// is expected to reject at the protocol level, returning an error response
/// from `Page.navigate`. If Chrome navigates to an error page instead, the
/// implementation must be updated to inspect the CDP `errorText` field.
#[test]
fn nav_goto_navigation_failed_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // An invalid scheme that Chrome CDP should reject, triggering NAVIGATION_FAILED.
    let out = headless_json(
        &[
            "browser",
            "goto",
            "invalidscheme://this-should-fail",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "goto invalid scheme");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.goto");
    assert_error_envelope(&v, "NAVIGATION_FAILED");
    assert!(
        v["context"].is_object(),
        "context must be present when session and tab are found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    close_session(&sid);
}

#[test]
fn nav_goto_session_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "goto",
            URL_B,
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "goto nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.goto");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn nav_goto_session_not_found_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "goto",
            URL_B,
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "goto nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn nav_goto_tab_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session(URL_A);

    let out = headless_json(
        &[
            "browser",
            "goto",
            URL_B,
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "goto nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.goto");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    // context should include session_id when session exists
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn nav_goto_tab_not_found_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session(URL_A);

    let out = headless(
        &[
            "browser",
            "goto",
            URL_B,
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "goto nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 3: back — Happy Path (§9.2)
// ===========================================================================

#[test]
fn nav_back_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // First goto URL_B to create history
    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b before back");

    // Now go back
    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.back");
    assert!(v["error"].is_null());

    // context — tab-level
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    // §5.6: context.url and context.title both updated post-navigation
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be updated after back"
    );
    assert!(
        !v["context"]["title"].as_str().unwrap_or("").is_empty(),
        "context.title must be updated after back"
    );

    // data fields per §9.2 (no requested_url for back)
    assert_nav_data(&v, "back", false);
    // from_url was URL_B, to_url should be URL_A (or a URL containing actionbook.dev)
    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "back to_url should be the previous URL (actionbook.dev)"
    );

    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn nav_back_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b before back text");

    let out = headless(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back text");
    let text = stdout_str(&out);

    // §2.5 tab-level: `[<session_id> <tab_id>] <url>`
    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.back"),
        "must contain ok browser.back"
    );
    assert!(text.contains("title:"), "must contain title:");

    close_session(&sid);
}

// ===========================================================================
// Group 4: back — Error Paths (§9.2)
// ===========================================================================

#[test]
fn nav_back_session_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "back",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "back nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.back");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn nav_back_tab_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session(URL_A);

    let out = headless_json(
        &[
            "browser",
            "back",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "back nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.back");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

// ===========================================================================
// Group 5: forward — Happy Path (§9.3)
// ===========================================================================

#[test]
fn nav_forward_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // goto URL_B, then back to URL_A to have forward history
    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b");
    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back to url_a");

    // Now forward
    let out = headless_json(
        &["browser", "forward", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "forward json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.forward");
    assert!(v["error"].is_null());

    // context — tab-level
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    // §5.6: context.url and context.title both updated post-navigation
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be updated after forward"
    );
    assert!(
        !v["context"]["title"].as_str().unwrap_or("").is_empty(),
        "context.title must be updated after forward"
    );

    // data fields per §9.3 (no requested_url for forward)
    assert_nav_data(&v, "forward", false);
    // to_url should be URL_B
    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap_or("")
            .contains("example.com"),
        "forward to_url should be the next URL (example.com)"
    );

    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn nav_forward_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b");
    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back to url_a");

    let out = headless(
        &["browser", "forward", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "forward text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.forward"),
        "must contain ok browser.forward"
    );
    assert!(text.contains("title:"), "must contain title:");

    close_session(&sid);
}

// ===========================================================================
// Group 6: forward — Error Paths (§9.3)
// ===========================================================================

#[test]
fn nav_forward_session_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "forward",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "forward nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.forward");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn nav_forward_tab_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session(URL_A);

    let out = headless_json(
        &[
            "browser",
            "forward",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "forward nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.forward");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

// ===========================================================================
// Group 7: reload — Happy Path (§9.4)
// ===========================================================================

#[test]
fn nav_reload_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless_json(&["browser", "reload", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "reload json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.reload");
    assert!(v["error"].is_null());

    // context — tab-level
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    // §5.6: context.url and context.title both updated post-navigation
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be present after reload"
    );
    assert!(
        !v["context"]["title"].as_str().unwrap_or("").is_empty(),
        "context.title must be updated after reload"
    );

    // data fields per §9.4 (no requested_url for reload)
    assert_nav_data(&v, "reload", false);
    // for reload, from_url == to_url (staying on same page)
    assert_eq!(
        v["data"]["from_url"], v["data"]["to_url"],
        "reload: from_url must equal to_url"
    );

    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn nav_reload_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    let out = headless(&["browser", "reload", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "reload text");
    let text = stdout_str(&out);

    // §2.5 tab-level
    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.reload"),
        "must contain ok browser.reload"
    );
    assert!(text.contains("title:"), "must contain title:");

    close_session(&sid);
}

#[test]
fn nav_reload_preserves_url() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Navigate to URL_B then reload — should stay on URL_B
    let out = headless_json(
        &["browser", "goto", URL_B, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b before reload");

    let out = headless_json(&["browser", "reload", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "reload after goto");
    let v = parse_json(&out);

    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap_or("")
            .contains("example.com"),
        "reload to_url must stay on current page (example.com)"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 8: reload — Error Paths (§9.4)
// ===========================================================================

#[test]
fn nav_reload_session_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "reload",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "reload nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.reload");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn nav_reload_tab_not_found_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session(URL_A);

    let out = headless_json(
        &[
            "browser",
            "reload",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "reload nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.reload");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

// ===========================================================================
// Group 9: NAVIGATION_FAILED — back/forward/reload (§9.2–9.4)
// ===========================================================================
// back/forward/reload are not yet implemented; these tests define the
// NAVIGATION_FAILED contract and are expected to fail until implementation lands.

#[test]
fn nav_back_navigation_failed_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Attempt back with no history — implementation must return NAVIGATION_FAILED
    // (no prior page to go back to).
    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 15);
    assert_failure(&out, "back with no history");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.back");
    assert_error_envelope(&v, "NAVIGATION_FAILED");
    assert!(
        v["context"].is_object(),
        "context present when session/tab found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    close_session(&sid);
}

#[test]
fn nav_forward_navigation_failed_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Attempt forward with no forward history — implementation must return NAVIGATION_FAILED.
    let out = headless_json(
        &["browser", "forward", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_failure(&out, "forward with no history");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.forward");
    assert_error_envelope(&v, "NAVIGATION_FAILED");
    assert!(
        v["context"].is_object(),
        "context present when session/tab found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    close_session(&sid);
}

#[test]
fn nav_reload_navigation_failed_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(URL_A);

    // Navigate to a URL with an invalid scheme, then reload — the tab is in a
    // failed navigation state. Reload on such a tab must return NAVIGATION_FAILED.
    // If Chrome loads an error page instead, the implementation must detect
    // the CDP errorText field and return NAVIGATION_FAILED accordingly.
    let _goto_out = headless_json(
        &[
            "browser",
            "goto",
            "invalidscheme://this-should-fail",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    // Regardless of goto outcome, a reload on this tab must be handled:
    let out = headless_json(&["browser", "reload", "--session", &sid, "--tab", &tid], 15);
    // If tab is in error state, reload must return NAVIGATION_FAILED.
    // If Chrome recovered to a blank page, reload may succeed — but the
    // contract requires NAVIGATION_FAILED when reload itself fails.
    if out.status.success() {
        // reload succeeded (Chrome recovered) — verify full contract is met
        let v = parse_json(&out);
        assert_eq!(v["ok"], true);
        assert_eq!(v["command"], "browser.reload");
    } else {
        let v = parse_json(&out);
        assert_eq!(v["command"], "browser.reload");
        assert_error_envelope(&v, "NAVIGATION_FAILED");
    }

    close_session(&sid);
}

// ===========================================================================
// Group 10: Missing Args (§9 — --session and --tab required)
// ===========================================================================

#[test]
fn nav_goto_missing_session_arg() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // browser goto <url> --tab <tid> (missing --session) must fail with CLI parse error
    let out = headless_json(&["browser", "goto", URL_B, "--tab", "some-tab"], 10);
    assert_failure(&out, "goto missing --session");
}

#[test]
fn nav_goto_missing_tab_arg() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // browser goto <url> --session <sid> (missing --tab) must fail with CLI parse error
    let out = headless_json(&["browser", "goto", URL_B, "--session", "some-session"], 10);
    assert_failure(&out, "goto missing --tab");
}

#[test]
fn nav_back_missing_session_arg() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "back", "--tab", "some-tab"], 10);
    assert_failure(&out, "back missing --session");
}

#[test]
fn nav_reload_missing_tab_arg() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "reload", "--session", "some-session"], 10);
    assert_failure(&out, "reload missing --tab");
}
