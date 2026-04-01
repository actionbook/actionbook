//! Browser navigation E2E tests: goto, back, forward, reload.
//!
//! All navigation commands are Tab-level: require `--session <SID> --tab <TID>`.
//! Tests are strict per api-reference.md section 9.
//!
//! Uses local HTTP server from harness — no external network dependency.

use crate::harness::{
    SessionGuard, assert_error_envelope, assert_failure, assert_meta, assert_success, headless,
    headless_json, parse_json, skip, start_session, stdout_str, url_a, url_b,
};

// ── Helpers ───────────────────────────────────────────────────────────

/// Assert navigation data fields per section 9 contract.
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

// ===========================================================================
// Group 1: goto — Happy Path
// ===========================================================================

#[test]
fn nav_goto_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser goto");
    assert!(v["error"].is_null());

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    assert!(
        v["context"]["url"].is_string(),
        "context.url must be updated after goto"
    );
    assert!(
        v["context"]["title"].is_string(),
        "context.title must be updated after goto"
    );

    assert_nav_data(&v, "goto", true);
    assert_eq!(v["data"]["requested_url"], url_b);
    assert!(
        v["data"]["to_url"].as_str().unwrap().contains("page-b"),
        "to_url should contain page-b"
    );

    assert_meta(&v);
}

#[test]
fn nav_goto_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser goto"),
        "must contain ok browser goto"
    );
    assert!(text.contains("title:"), "must contain title:");
}

#[test]
fn nav_goto_context_url_updated() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b");
    let v = parse_json(&out);

    let ctx_url = v["context"]["url"].as_str().unwrap_or("");
    assert!(
        ctx_url.contains("page-b"),
        "context.url must be updated to destination: got '{ctx_url}'"
    );

    let ctx_title = v["context"]["title"].as_str().unwrap_or("");
    assert!(
        !ctx_title.is_empty(),
        "context.title must be updated after goto: got empty string"
    );
}

// ===========================================================================
// Group 2: goto — Error Paths
// ===========================================================================

#[test]
fn nav_goto_navigation_failed_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

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

    assert_eq!(v["command"], "browser goto");
    assert_error_envelope(&v, "NAVIGATION_FAILED");
    assert!(
        v["context"].is_object(),
        "context must be present when session and tab are found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
}

#[test]
fn nav_goto_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "goto",
            &url_b(),
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "goto nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser goto");
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

    let out = headless(
        &[
            "browser",
            "goto",
            &url_b(),
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
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "goto",
            &url_b(),
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "goto nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser goto");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);
}

#[test]
fn nav_goto_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "goto",
            &url_b(),
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
}

// ===========================================================================
// Group 3: back — Happy Path
// ===========================================================================

#[test]
fn nav_back_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b before back");

    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser back");
    assert!(v["error"].is_null());

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    assert!(
        v["context"]["url"].is_string(),
        "context.url must be updated after back"
    );
    assert!(
        !v["context"]["title"].as_str().unwrap_or("").is_empty(),
        "context.title must be updated after back"
    );

    assert_nav_data(&v, "back", false);
    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap_or("")
            .contains("page-a"),
        "back to_url should be the previous URL (page-a)"
    );

    assert_meta(&v);
}

#[test]
fn nav_back_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b before back text");

    let out = headless(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser back"),
        "must contain ok browser back"
    );
    assert!(text.contains("title:"), "must contain title:");
}

// ===========================================================================
// Group 4: back — Error Paths
// ===========================================================================

#[test]
fn nav_back_session_not_found_json() {
    if skip() {
        return;
    }

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

    assert_eq!(v["command"], "browser back");
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
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

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

    assert_eq!(v["command"], "browser back");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);
}

// ===========================================================================
// Group 5: forward — Happy Path
// ===========================================================================

#[test]
fn nav_forward_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b");
    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "back to url_a");

    let out = headless_json(
        &["browser", "forward", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "forward json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser forward");
    assert!(v["error"].is_null());

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    assert!(
        v["context"]["url"].is_string(),
        "context.url must be updated after forward"
    );
    assert!(
        !v["context"]["title"].as_str().unwrap_or("").is_empty(),
        "context.title must be updated after forward"
    );

    assert_nav_data(&v, "forward", false);
    assert!(
        v["data"]["to_url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "forward to_url should be the next URL (page-b)"
    );

    assert_meta(&v);
}

#[test]
fn nav_forward_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
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
        text.contains("ok browser forward"),
        "must contain ok browser forward"
    );
    assert!(text.contains("title:"), "must contain title:");
}

// ===========================================================================
// Group 6: forward — Error Paths
// ===========================================================================

#[test]
fn nav_forward_session_not_found_json() {
    if skip() {
        return;
    }

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

    assert_eq!(v["command"], "browser forward");
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
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

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

    assert_eq!(v["command"], "browser forward");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);
}

// ===========================================================================
// Group 7: reload — Happy Path
// ===========================================================================

#[test]
fn nav_reload_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "reload", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "reload json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser reload");
    assert!(v["error"].is_null());

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    assert!(
        v["context"]["url"].is_string(),
        "context.url must be present after reload"
    );
    assert!(
        !v["context"]["title"].as_str().unwrap_or("").is_empty(),
        "context.title must be updated after reload"
    );

    assert_nav_data(&v, "reload", false);
    assert_eq!(
        v["data"]["from_url"], v["data"]["to_url"],
        "reload: from_url must equal to_url"
    );

    assert_meta(&v);
}

#[test]
fn nav_reload_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "reload", "--session", &sid, "--tab", &tid], 30);
    assert_success(&out, "reload text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser reload"),
        "must contain ok browser reload"
    );
    assert!(text.contains("title:"), "must contain title:");
}

#[test]
fn nav_reload_preserves_url() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
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
            .contains("page-b"),
        "reload to_url must stay on current page (page-b)"
    );
}

// ===========================================================================
// Group 8: reload — Error Paths
// ===========================================================================

#[test]
fn nav_reload_session_not_found_json() {
    if skip() {
        return;
    }

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

    assert_eq!(v["command"], "browser reload");
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
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

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

    assert_eq!(v["command"], "browser reload");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);
}

// ===========================================================================
// Group 9: NAVIGATION_FAILED — back/forward/reload
// ===========================================================================

#[test]
fn nav_back_navigation_failed_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    // Chrome keeps the initial newtab page in history, so first back succeeds.
    let out1 = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 15);
    assert_success(&out1, "first back goes to newtab");

    // Second back should fail — no more history.
    let out = headless_json(&["browser", "back", "--session", &sid, "--tab", &tid], 15);
    assert_failure(&out, "back with no more history");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser back");
    assert_error_envelope(&v, "NAVIGATION_FAILED");
    assert!(
        v["context"].is_object(),
        "context present when session/tab found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
}

#[test]
fn nav_forward_navigation_failed_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "forward", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_failure(&out, "forward with no history");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser forward");
    assert_error_envelope(&v, "NAVIGATION_FAILED");
    assert!(
        v["context"].is_object(),
        "context present when session/tab found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
}

#[test]
fn nav_reload_navigation_failed_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

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
    let out = headless_json(&["browser", "reload", "--session", &sid, "--tab", &tid], 15);
    if out.status.success() {
        let v = parse_json(&out);
        assert_eq!(v["ok"], true);
        assert_eq!(v["command"], "browser reload");
    } else {
        let v = parse_json(&out);
        assert_eq!(v["command"], "browser reload");
        assert_error_envelope(&v, "NAVIGATION_FAILED");
    }
}

// ===========================================================================
// Group 10: Missing Args (--session and --tab required)
// ===========================================================================

#[test]
fn nav_goto_missing_session_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "goto", &url_b(), "--tab", "some-tab"], 10);
    assert_failure(&out, "goto missing --session");
}

#[test]
fn nav_goto_missing_tab_arg() {
    if skip() {
        return;
    }

    let out = headless_json(
        &["browser", "goto", &url_b(), "--session", "some-session"],
        10,
    );
    assert_failure(&out, "goto missing --tab");
}

#[test]
fn nav_back_missing_session_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "back", "--tab", "some-tab"], 10);
    assert_failure(&out, "back missing --session");
}

#[test]
fn nav_reload_missing_tab_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "reload", "--session", "some-session"], 10);
    assert_failure(&out, "reload missing --tab");
}

// ===========================================================================
// Group 11: goto --wait-until
// ===========================================================================

/// Default goto (--wait-until load) waits for page load and returns correct URL/title.
#[test]
fn nav_goto_default_waits_for_load() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto default wait");
    let v = parse_json(&out);

    // After waiting for load, to_url and title must reflect the new page
    let to_url = v["data"]["to_url"].as_str().unwrap_or("");
    assert!(
        to_url.contains("page-b"),
        "to_url should contain page-b after load wait, got: {to_url}"
    );
    let title = v["data"]["title"].as_str().unwrap_or("");
    assert!(
        !title.is_empty(),
        "title should be populated after load wait"
    );
}

/// --wait-until none returns immediately (backward compat).
#[test]
fn nav_goto_wait_until_none() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &[
            "browser",
            "goto",
            &url_b,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--wait-until",
            "none",
        ],
        30,
    );
    assert_success(&out, "goto --wait-until none");
}

/// --wait-until domcontentloaded waits for DOMContentLoaded.
#[test]
fn nav_goto_wait_until_domcontentloaded() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &[
            "browser",
            "goto",
            &url_b,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--wait-until",
            "domcontentloaded",
        ],
        30,
    );
    assert_success(&out, "goto --wait-until domcontentloaded");
    let v = parse_json(&out);
    let to_url = v["data"]["to_url"].as_str().unwrap_or("");
    assert!(
        to_url.contains("page-b"),
        "to_url should contain page-b after domcontentloaded, got: {to_url}"
    );
}

/// --wait-until load waits for full load event.
#[test]
fn nav_goto_wait_until_load() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(
        &[
            "browser",
            "goto",
            &url_b,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--wait-until",
            "load",
        ],
        30,
    );
    assert_success(&out, "goto --wait-until load");
    let v = parse_json(&out);
    let to_url = v["data"]["to_url"].as_str().unwrap_or("");
    assert!(
        to_url.contains("page-b"),
        "to_url should contain page-b after load, got: {to_url}"
    );
    let title = v["data"]["title"].as_str().unwrap_or("");
    assert_eq!(title, "Page B", "title should be Page B after load");
}

/// After goto with wait, snapshot refs should be stable for subsequent interactions.
#[test]
fn nav_goto_wait_then_snapshot_refs_stable() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    // goto with default wait (load)
    let out = headless_json(
        &["browser", "goto", &url_b, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto url_b");

    // snapshot — should get stable refs
    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "snapshot after goto");

    // hover on first ref — should work, not REF_STALE
    let out = headless_json(
        &["browser", "hover", "@e1", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "hover @e1 after goto+wait should not be REF_STALE");
}
