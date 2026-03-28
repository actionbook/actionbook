//! Browser tab management E2E tests: list-tabs, new-tab, close-tab.
//!
//! Each test is self-contained: start → operate → assert → close.
//! Covers BOTH JSON (§2.4 envelope) and text (§2.5 protocol) output.
//!
//! tab_id is Chrome's native target ID (opaque string), not a fixed tN.
//! Tests dynamically extract tab_ids from command responses.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

const TEST_URL_1: &str = "https://actionbook.dev";
const TEST_URL_2: &str = "https://example.com";
const TEST_URL_3: &str = "https://example.org";

// ── Helpers ──────────────────────────────────────────────────────────

/// Assert full meta structure per §2.4.
fn assert_meta(v: &serde_json::Value) {
    assert!(v["meta"]["duration_ms"].is_number(), "meta.duration_ms must be a number");
    assert!(v["meta"]["warnings"].is_array(), "meta.warnings must be an array");
    assert!(v["meta"]["pagination"].is_null(), "meta.pagination must be null");
    assert!(v["meta"]["truncated"].is_boolean(), "meta.truncated must be a boolean");
}

/// Assert full error envelope per §3.1 (including meta).
fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert!(v["data"].is_null(), "data must be null on failure");
    assert_eq!(v["error"]["code"], expected_code);
    assert!(v["error"]["message"].is_string(), "error.message must be a string");
    assert!(v["error"]["retryable"].is_boolean(), "error.retryable must be a boolean");
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details must be object or null"
    );
    assert_meta(v);
}

/// Assert context is a non-null object.
fn assert_context_object(v: &serde_json::Value) {
    assert!(v["context"].is_object(), "context must be an object");
}

/// Assert a tab_id is a non-empty string (native Chrome target ID).
fn assert_tab_id(tab_id: &serde_json::Value) {
    assert!(tab_id.is_string(), "tab_id must be a string");
    assert!(!tab_id.as_str().unwrap().is_empty(), "tab_id must not be empty");
}

/// Start a headless session via JSON, return (session_id, first_tab_id).
fn start_session_json(url: &str) -> (String, String) {
    let out = headless_json(
        &["browser", "start", "--mode", "local", "--headless", "--open-url", url],
        30,
    );
    assert_success(&out, "start session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"].as_str().unwrap().to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    (sid, tid)
}

/// Start a named headless session, return first_tab_id.
fn start_named_session_json(session_id: &str, profile: &str, url: &str) -> String {
    let out = headless_json(
        &[
            "browser", "start", "--mode", "local", "--headless",
            "--profile", profile, "--set-session-id", session_id,
            "--open-url", url,
        ],
        30,
    );
    assert_success(&out, &format!("start {session_id}"));
    let v = parse_json(&out);
    v["data"]["tab"]["tab_id"].as_str().unwrap().to_string()
}

/// Open a new tab via JSON, return tab_id.
fn new_tab_json(session_id: &str, url: &str) -> String {
    let out = headless_json(
        &["browser", "new-tab", url, "--session", session_id],
        30,
    );
    assert_success(&out, "new-tab");
    let v = parse_json(&out);
    v["data"]["tab"]["tab_id"].as_str().unwrap().to_string()
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

// ===========================================================================
// Group 1: list-tabs — Basic (§8.1)
// ===========================================================================

#[test]
fn tab_list_tabs_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.list-tabs");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);

    assert!(v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 1);
    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let tab = &tabs[0];
    assert_tab_id(&tab["tab_id"]);
    assert!(tab["url"].is_string());
    assert!(tab["title"].is_string());
    // native_tab_id removed — should NOT be present
    assert!(
        !tab.as_object().unwrap().contains_key("native_tab_id"),
        "native_tab_id should not be present"
    );
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn tab_list_tabs_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}]")));
    assert!(text.contains("tab"));
    assert!(text.contains("actionbook.dev"));

    close_session(&sid);
}

#[test]
fn tab_list_tabs_after_new_tab_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, t1) = start_session_json(TEST_URL_1);
    let t2 = new_tab_json(&sid, TEST_URL_2);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after new-tab");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(2));
    let tabs = v["data"]["tabs"].as_array().unwrap();
    let ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(ids.contains(&t1.as_str()), "should have t1");
    assert!(ids.contains(&t2.as_str()), "should have t2");

    for tab in tabs {
        assert_tab_id(&tab["tab_id"]);
        assert!(tab["url"].is_string());
        assert!(tab["title"].is_string());
    }

    close_session(&sid);
}

#[test]
fn tab_list_tabs_after_new_tab_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);
    let _t2 = new_tab_json(&sid, TEST_URL_2);

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after new-tab text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}]")));
    assert!(text.contains("2"));

    close_session(&sid);
}

// ===========================================================================
// Group 2: new-tab — Basic (§8.2)
// ===========================================================================

#[test]
fn tab_new_tab_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);

    let out = headless_json(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.new-tab");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_string(), "context.tab_id should be present");

    let tab = &v["data"]["tab"];
    assert_tab_id(&tab["tab_id"]);
    assert!(tab["url"].is_string());
    assert!(tab["title"].is_string());
    assert!(!tab.as_object().unwrap().contains_key("native_tab_id"));
    assert_eq!(v["data"]["created"], true);
    assert_eq!(v["data"]["new_window"], false);
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn tab_new_tab_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);

    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}")), "header should contain session_id");
    assert!(text.contains("ok browser.new-tab"));
    assert!(text.contains("title:"));

    close_session(&sid);
}

#[test]
fn tab_new_tab_sequential_ids_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, t1) = start_session_json(TEST_URL_1);

    let t2 = new_tab_json(&sid, TEST_URL_2);
    let t3 = new_tab_json(&sid, TEST_URL_3);

    // All tab_ids must be unique non-empty strings
    assert!(!t1.is_empty() && !t2.is_empty() && !t3.is_empty());
    assert!(t1 != t2 && t2 != t3 && t1 != t3, "all tab_ids must be unique");

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs 3 tabs");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(3));

    close_session(&sid);
}

#[test]
fn tab_new_tab_alias_open_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);

    let out = headless_json(
        &["browser", "open", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "browser open alias");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.new-tab");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);

    let tab = &v["data"]["tab"];
    assert_tab_id(&tab["tab_id"]);
    assert_eq!(v["data"]["created"], true);
    assert_eq!(v["data"]["new_window"], false);
    assert_meta(&v);

    close_session(&sid);
}

// ===========================================================================
// Group 3: close-tab — Basic (§8.3)
// ===========================================================================

#[test]
fn tab_close_tab_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);
    let t2 = new_tab_json(&sid, TEST_URL_2);

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_success(&out, "close-tab json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.close-tab");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], t2);
    assert_eq!(v["data"]["closed_tab_id"], t2);
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn tab_close_tab_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);
    let t2 = new_tab_json(&sid, TEST_URL_2);

    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_success(&out, "close-tab text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}")));
    assert!(text.contains("ok browser.close-tab"));

    close_session(&sid);
}

#[test]
fn tab_close_tab_then_list_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, t1) = start_session_json(TEST_URL_1);
    let t2 = new_tab_json(&sid, TEST_URL_2);
    let t3 = new_tab_json(&sid, TEST_URL_3);

    // Close t2
    let out = headless(&["browser", "close-tab", "--session", &sid, "--tab", &t2], 30);
    assert_success(&out, "close t2");

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after close");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(2));
    let tabs = v["data"]["tabs"].as_array().unwrap();
    let ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(ids.contains(&t1.as_str()), "t1 should remain");
    assert!(ids.contains(&t3.as_str()), "t3 should remain");
    assert!(!ids.contains(&t2.as_str()), "t2 should be closed");

    close_session(&sid);
}

// ===========================================================================
// Group 4: Error Cases
// ===========================================================================

#[test]
fn tab_list_tabs_nonexistent_session_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let out = headless_json(&["browser", "list-tabs", "--session", "nonexistent"], 10);
    assert_failure(&out, "list-tabs nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.list-tabs");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn tab_list_tabs_nonexistent_session_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let out = headless(&["browser", "list-tabs", "--session", "nonexistent"], 10);
    assert_failure(&out, "list-tabs nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

#[test]
fn tab_new_tab_nonexistent_session_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let out = headless_json(&["browser", "new-tab", TEST_URL_1, "--session", "nonexistent"], 10);
    assert_failure(&out, "new-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.new-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn tab_new_tab_nonexistent_session_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let out = headless(&["browser", "new-tab", TEST_URL_1, "--session", "nonexistent"], 10);
    assert_failure(&out, "new-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

#[test]
fn tab_close_tab_nonexistent_session_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let out = headless_json(
        &["browser", "close-tab", "--session", "nonexistent", "--tab", "fake"],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn tab_close_tab_nonexistent_session_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let out = headless(
        &["browser", "close-tab", "--session", "nonexistent", "--tab", "fake"],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

#[test]
fn tab_close_tab_nonexistent_tab_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", "nonexistent-tab-id"],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object(), "context should be present when session found");
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn tab_close_tab_nonexistent_tab_text() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);
    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", "nonexistent-tab-id"],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab text");
    let text = stdout_str(&out);
    assert!(text.contains("error TAB_NOT_FOUND:"));

    close_session(&sid);
}

#[test]
fn tab_close_tab_double_close_json() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, _t1) = start_session_json(TEST_URL_1);
    let t2 = new_tab_json(&sid, TEST_URL_2);

    // First close: success
    let out = headless_json(&["browser", "close-tab", "--session", &sid, "--tab", &t2], 30);
    assert_success(&out, "first close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["closed_tab_id"], t2);

    // Second close: TAB_NOT_FOUND
    let out = headless_json(&["browser", "close-tab", "--session", &sid, "--tab", &t2], 30);
    assert_failure(&out, "second close should fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_error_envelope(&v, "TAB_NOT_FOUND");

    close_session(&sid);
}

// ===========================================================================
// Group 5: Concurrent — Same Session
// ===========================================================================

#[test]
fn tab_concurrent_multi_tab_same_session() {
    if skip() { return; }
    let _guard = SessionGuard::new();
    let (sid, t1) = start_session_json(TEST_URL_1);
    let t2 = new_tab_json(&sid, TEST_URL_2);
    let t3 = new_tab_json(&sid, TEST_URL_3);

    // Parallel eval on t1, t2, t3 — all through the same persistent CDP connection
    let sid1 = sid.clone(); let t1c = t1.clone();
    let sid2 = sid.clone(); let t2c = t2.clone();
    let sid3 = sid.clone(); let t3c = t3.clone();

    let h1 = std::thread::spawn(move || {
        headless_json(&["browser", "eval", "1+1", "--session", &sid1, "--tab", &t1c], 30)
    });
    let h2 = std::thread::spawn(move || {
        headless_json(&["browser", "eval", "1+1", "--session", &sid2, "--tab", &t2c], 30)
    });
    let h3 = std::thread::spawn(move || {
        headless_json(&["browser", "eval", "1+1", "--session", &sid3, "--tab", &t3c], 30)
    });

    let out1 = h1.join().expect("thread t1");
    let out2 = h2.join().expect("thread t2");
    let out3 = h3.join().expect("thread t3");
    assert_success(&out1, "eval t1");
    assert_success(&out2, "eval t2");
    assert_success(&out3, "eval t3");

    for out in [&out1, &out2, &out3] {
        let v = parse_json(out);
        assert_eq!(v["context"]["session_id"], sid);
    }

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs 3 tabs");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(3));

    close_session(&sid);
}

// ===========================================================================
// Group 6: Concurrent — Cross-Session
// ===========================================================================

#[test]
fn tab_concurrent_multi_tab_cross_session() {
    if skip() { return; }
    let _guard = SessionGuard::new();

    let _ta = start_named_session_json("session-a", "profile-a", TEST_URL_1);
    let _tb = start_named_session_json("session-b", "profile-b", TEST_URL_3);
    let _t2a = new_tab_json("session-a", TEST_URL_2);
    let _t2b = new_tab_json("session-b", TEST_URL_1);

    let ha = std::thread::spawn(|| {
        headless_json(&["browser", "list-tabs", "--session", "session-a"], 10)
    });
    let hb = std::thread::spawn(|| {
        headless_json(&["browser", "list-tabs", "--session", "session-b"], 10)
    });

    let out_a = ha.join().unwrap();
    let out_b = hb.join().unwrap();
    assert_success(&out_a, "list-tabs session-a");
    assert_success(&out_b, "list-tabs session-b");

    let va = parse_json(&out_a);
    let vb = parse_json(&out_b);
    assert_eq!(va["data"]["total_tabs"], serde_json::json!(2));
    assert_eq!(vb["data"]["total_tabs"], serde_json::json!(2));

    close_session("session-a");
    close_session("session-b");
}

#[test]
fn tab_concurrent_close_tabs_cross_session() {
    if skip() { return; }
    let _guard = SessionGuard::new();

    let _tx = start_named_session_json("session-x", "profile-x", TEST_URL_1);
    let _ty = start_named_session_json("session-y", "profile-y", TEST_URL_3);
    let t2x = new_tab_json("session-x", TEST_URL_2);
    let t2y = new_tab_json("session-y", TEST_URL_1);

    let tx_clone = t2x.clone();
    let ty_clone = t2y.clone();
    let hx = std::thread::spawn(move || {
        headless_json(&["browser", "close-tab", "--session", "session-x", "--tab", &tx_clone], 30)
    });
    let hy = std::thread::spawn(move || {
        headless_json(&["browser", "close-tab", "--session", "session-y", "--tab", &ty_clone], 30)
    });

    let out_x = hx.join().unwrap();
    let out_y = hy.join().unwrap();
    assert_success(&out_x, "close-tab session-x");
    assert_success(&out_y, "close-tab session-y");

    let vx = parse_json(&out_x);
    let vy = parse_json(&out_y);
    assert_eq!(vx["data"]["closed_tab_id"], t2x);
    assert_eq!(vy["data"]["closed_tab_id"], t2y);

    let out = headless_json(&["browser", "list-tabs", "--session", "session-x"], 10);
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(1));

    let out = headless_json(&["browser", "list-tabs", "--session", "session-y"], 10);
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(1));

    close_session("session-x");
    close_session("session-y");
}
