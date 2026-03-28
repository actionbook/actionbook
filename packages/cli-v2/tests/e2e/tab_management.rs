//! Browser tab management E2E tests: list-tabs, new-tab, close-tab.
//!
//! Each test is self-contained: start → operate → assert → close.
//! Covers BOTH JSON (§2.4 envelope) and text (§2.5 protocol) output.
//! All assertions strictly follow api-reference.md §8.

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
        "meta.pagination must be null for tab commands"
    );
    assert!(
        v["meta"]["truncated"].is_boolean(),
        "meta.truncated must be a boolean"
    );
}

/// Assert full error envelope per §3.1 (including meta).
fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
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
        "error.details must be object or null per §3.1"
    );
    // §2.4: meta is part of the envelope for both success and error responses
    assert_meta(v);
}

/// Assert context is a non-null object per §2.4.
fn assert_context_object(v: &serde_json::Value) {
    assert!(
        v["context"].is_object(),
        "context must be an object for session/tab commands per §2.4"
    );
}

/// Assert native_tab_id key is present with valid type per §8.
fn assert_native_tab_id(tab: &serde_json::Value) {
    assert!(
        tab.as_object()
            .is_some_and(|o| o.contains_key("native_tab_id")),
        "native_tab_id key must be present in tab object"
    );
    let ntid = &tab["native_tab_id"];
    assert!(
        ntid.is_string() || ntid.is_number() || ntid.is_null(),
        "native_tab_id must be string, number, or null"
    );
}

/// Start a headless session, return session_id.
fn start_session(url: &str) -> String {
    let out = headless(
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
    "local-1".to_string()
}

/// Start a named headless session with profile.
fn start_named_session(session_id: &str, profile: &str, url: &str) {
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            profile,
            "--set-session-id",
            session_id,
            "--open-url",
            url,
        ],
        30,
    );
    assert_success(&out, &format!("start {session_id}"));
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

// ===========================================================================
// Group 1: list-tabs — Basic (§8.1)
// ===========================================================================

// 1. list-tabs JSON — §8.1
#[test]
fn tab_list_tabs_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs json");
    let v = parse_json(&out);

    // Envelope per §2.4
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.list-tabs");
    assert!(v["error"].is_null(), "error must be null on success");

    // Context: session-level per §8.1 — session_id present, no tab_id
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null()
            || !v["context"]
                .as_object()
                .is_some_and(|o| o.contains_key("tab_id")),
        "list-tabs: context must NOT have tab_id (session-level)"
    );

    // Data per §8.1
    assert!(
        v["data"]["total_tabs"].is_number(),
        "total_tabs must be a number"
    );
    assert!(
        v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 1,
        "total_tabs must be >= 1"
    );
    let tabs = v["data"]["tabs"]
        .as_array()
        .expect("tabs must be an array");
    assert!(!tabs.is_empty(), "tabs array must not be empty");

    // Each tab object per §8.1: tab_id, url, title, native_tab_id
    let tab = &tabs[0];
    assert!(tab["tab_id"].is_string(), "tab.tab_id must be a string");
    assert!(tab["url"].is_string(), "tab.url must be a string");
    assert!(tab["title"].is_string(), "tab.title must be a string");
    assert_native_tab_id(tab);

    // Meta per §2.4
    assert_meta(&v);

    close_session(&sid);
}

// 2. list-tabs text — §8.1 + §2.5
#[test]
fn tab_list_tabs_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs text");
    let text = stdout_str(&out);

    // Session-level header: [SID]
    assert!(
        text.contains(&format!("[{sid}]")),
        "list-tabs text: should contain [{sid}]"
    );
    // Tab count line
    assert!(
        text.contains("tab"),
        "list-tabs text: should contain 'tab' count line"
    );
    // Tab entry: [t1] followed by title text per §8.1
    assert!(
        text.contains("[t1]"),
        "list-tabs text: should contain [t1] tab entry"
    );
    // URL on next line per §8.1 text format
    assert!(
        text.contains("actionbook.dev"),
        "list-tabs text: should contain URL (actionbook.dev) per §8.1"
    );

    close_session(&sid);
}

// 3. list-tabs after new-tab JSON — §8.1
#[test]
fn tab_list_tabs_after_new_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    // Open second tab
    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab");

    // list-tabs
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after new-tab");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);

    // Data: 2 tabs
    assert_eq!(
        v["data"]["total_tabs"],
        serde_json::json!(2),
        "total_tabs should be 2"
    );
    let tabs = v["data"]["tabs"]
        .as_array()
        .expect("tabs must be an array");
    assert_eq!(tabs.len(), 2, "tabs array should have 2 entries");

    // Each tab has all 4 fields per §8.1
    let tab_ids: Vec<&str> = tabs
        .iter()
        .filter_map(|t| t["tab_id"].as_str())
        .collect();
    assert!(tab_ids.contains(&"t1"), "should have t1");
    assert!(tab_ids.contains(&"t2"), "should have t2");

    for tab in tabs {
        assert!(tab["tab_id"].is_string());
        assert!(tab["url"].is_string());
        assert!(tab["title"].is_string());
        assert_native_tab_id(tab);
    }

    // Verify URLs: t1 = actionbook.dev, t2 = example.com
    let t1 = tabs.iter().find(|t| t["tab_id"] == "t1").unwrap();
    let t2 = tabs.iter().find(|t| t["tab_id"] == "t2").unwrap();
    assert!(
        t1["url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "t1 url should contain actionbook.dev"
    );
    assert!(
        t2["url"].as_str().unwrap_or("").contains("example.com"),
        "t2 url should contain example.com"
    );

    close_session(&sid);
}

// 4. list-tabs after new-tab text — §8.1 + §2.5
#[test]
fn tab_list_tabs_after_new_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab");

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after new-tab text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid}]")),
        "should contain [{sid}]"
    );
    assert!(text.contains("2"), "should contain '2' in count line");
    assert!(text.contains("[t1]"), "should contain [t1]");
    assert!(text.contains("[t2]"), "should contain [t2]");

    close_session(&sid);
}

// ===========================================================================
// Group 2: new-tab — Basic (§8.2)
// ===========================================================================

// 5. new-tab JSON — §8.2
#[test]
fn tab_new_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless_json(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab json");
    let v = parse_json(&out);

    // Envelope per §2.4
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.new-tab");
    assert!(v["error"].is_null(), "error must be null on success");

    // Context: session_id present; tab_id present (special case like browser start per §2.4)
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_string(),
        "new-tab: context.tab_id should be present (special case)"
    );

    // Data per §8.2: tab object
    let tab = &v["data"]["tab"];
    assert_eq!(tab["tab_id"], "t2", "new tab should be t2");
    assert!(tab["url"].is_string(), "tab.url must be a string");
    assert!(tab["title"].is_string(), "tab.title must be a string");
    assert_native_tab_id(tab);

    // Data per §8.2: created, new_window
    assert_eq!(
        v["data"]["created"], true,
        "created must be true for new tab"
    );
    assert_eq!(
        v["data"]["new_window"], false,
        "new_window must be false by default"
    );

    // Meta per §2.4
    assert_meta(&v);

    close_session(&sid);
}

// 6. new-tab text — §8.2 + §2.5
#[test]
fn tab_new_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab text");
    let text = stdout_str(&out);

    // Header: [SID t2] <url> — tab-level format per §8.2
    assert!(
        text.contains(&format!("[{sid} t2]")),
        "new-tab text: should contain [{sid} t2], got: {text}"
    );
    // URL should appear in the header line per §8.2
    assert!(
        text.contains("example.com"),
        "new-tab text: header should contain URL (example.com) per §8.2"
    );
    // Body: ok browser.new-tab
    assert!(
        text.contains("ok browser.new-tab"),
        "new-tab text: should contain 'ok browser.new-tab'"
    );
    // Body: title: <title>
    assert!(
        text.contains("title:"),
        "new-tab text: should contain 'title:' per §8.2"
    );

    close_session(&sid);
}

// 7. new-tab sequential IDs — §5.2
#[test]
fn tab_new_tab_sequential_ids_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    // new-tab #1 → t2
    let out = headless_json(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab t2");
    let v = parse_json(&out);
    assert_eq!(v["data"]["tab"]["tab_id"], "t2");

    // new-tab #2 → t3
    let out = headless_json(
        &["browser", "new-tab", TEST_URL_3, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab t3");
    let v = parse_json(&out);
    assert_eq!(v["data"]["tab"]["tab_id"], "t3");

    // new-tab #3 → t4
    let out = headless_json(
        &[
            "browser",
            "new-tab",
            "https://actionbook.dev/docs",
            "--session",
            &sid,
        ],
        30,
    );
    assert_success(&out, "new-tab t4");
    let v = parse_json(&out);
    assert_eq!(v["data"]["tab"]["tab_id"], "t4");

    // list-tabs → total_tabs = 4
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs 4 tabs");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(4));
    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let ids: Vec<&str> = tabs
        .iter()
        .filter_map(|t| t["tab_id"].as_str())
        .collect();
    assert!(ids.contains(&"t1"));
    assert!(ids.contains(&"t2"));
    assert!(ids.contains(&"t3"));
    assert!(ids.contains(&"t4"));

    close_session(&sid);
}

// 8. new-tab alias `browser open` — §8.2
#[test]
fn tab_new_tab_alias_open_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    // Use alias: browser open <url> --session <SID>
    let out = headless_json(
        &["browser", "open", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "browser open alias");
    let v = parse_json(&out);

    // Should resolve to browser.new-tab per §8.2
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.new-tab");
    assert!(v["error"].is_null());

    // Context: same as test 5 — session_id + tab_id per §2.4 special case
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_string(),
        "open alias: context.tab_id should be present"
    );

    // Data structure same as new-tab per §8.2
    let tab = &v["data"]["tab"];
    assert_eq!(tab["tab_id"], "t2");
    assert!(tab["url"].is_string());
    assert!(tab["title"].is_string());
    assert_native_tab_id(tab);
    assert_eq!(v["data"]["created"], true);
    assert_eq!(v["data"]["new_window"], false);

    assert_meta(&v);

    close_session(&sid);
}

// ===========================================================================
// Group 3: close-tab — Basic (§8.3)
// ===========================================================================

// 9. close-tab JSON — §8.3
#[test]
fn tab_close_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    // Open t2
    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab for close");

    // Close t2
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", "t2"],
        30,
    );
    assert_success(&out, "close-tab json");
    let v = parse_json(&out);

    // Envelope per §2.4
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.close-tab");
    assert!(v["error"].is_null(), "error must be null on success");

    // Context: tab-level per §8.3 — session_id + tab_id
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], "t2");

    // Data per §8.3
    assert_eq!(v["data"]["closed_tab_id"], "t2");

    // Meta per §2.4
    assert_meta(&v);

    close_session(&sid);
}

// 10. close-tab text — §8.3 + §2.5
#[test]
fn tab_close_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab for close");

    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", "t2"],
        30,
    );
    assert_success(&out, "close-tab text");
    let text = stdout_str(&out);

    // Header: [SID t2] — NO URL per §2.5 deviation note for close-tab
    assert!(
        text.contains(&format!("[{sid} t2]")),
        "close-tab text: should contain [{sid} t2], got: {text}"
    );
    // Body: ok browser.close-tab
    assert!(
        text.contains("ok browser.close-tab"),
        "close-tab text: should contain 'ok browser.close-tab'"
    );

    close_session(&sid);
}

// 11. close-tab then list — §8.3 + §8.1 + §5.2
#[test]
fn tab_close_tab_then_list_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    // Open t2 and t3
    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab t2");
    let out = headless(
        &["browser", "new-tab", TEST_URL_3, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab t3");

    // Close t2
    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", "t2"],
        30,
    );
    assert_success(&out, "close t2");

    // list-tabs: should have t1 and t3
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after close t2");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(
        v["data"]["total_tabs"],
        serde_json::json!(2),
        "total_tabs should be 2 after closing t2"
    );

    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let ids: Vec<&str> = tabs
        .iter()
        .filter_map(|t| t["tab_id"].as_str())
        .collect();
    assert!(ids.contains(&"t1"), "t1 should remain");
    assert!(ids.contains(&"t3"), "t3 should remain with original ID per §5.2");
    assert!(!ids.contains(&"t2"), "t2 should be closed");

    // All remaining tabs have full fields
    for tab in tabs {
        assert!(tab["tab_id"].is_string());
        assert!(tab["url"].is_string());
        assert!(tab["title"].is_string());
        assert_native_tab_id(tab);
    }

    close_session(&sid);
}

// ===========================================================================
// Group 4: Error Cases
// ===========================================================================

// 12. list-tabs nonexistent session JSON — §3
#[test]
fn tab_list_tabs_nonexistent_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "list-tabs", "--session", "nonexistent"], 10);
    assert_failure(&out, "list-tabs nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.list-tabs");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    // §4: context should be null/absent when session was NOT located
    assert!(
        v["context"].is_null(),
        "context should be null when session not found per §4"
    );
}

// 13. list-tabs nonexistent session text — §3
#[test]
fn tab_list_tabs_nonexistent_session_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "list-tabs", "--session", "nonexistent"], 10);
    assert_failure(&out, "list-tabs nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "should contain 'error SESSION_NOT_FOUND:', got: {text}"
    );
}

// 14. new-tab nonexistent session JSON — §3
#[test]
fn tab_new_tab_nonexistent_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "new-tab",
            TEST_URL_1,
            "--session",
            "nonexistent",
        ],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.new-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    // §4: context should be null/absent when session was NOT located
    assert!(
        v["context"].is_null(),
        "context should be null when session not found per §4"
    );
}

// 15. new-tab nonexistent session text — §3
#[test]
fn tab_new_tab_nonexistent_session_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "new-tab",
            TEST_URL_1,
            "--session",
            "nonexistent",
        ],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "should contain 'error SESSION_NOT_FOUND:', got: {text}"
    );
}

// 16. close-tab nonexistent session JSON — §3
#[test]
fn tab_close_tab_nonexistent_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "close-tab",
            "--session",
            "nonexistent",
            "--tab",
            "t1",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    // §4: context should be null/absent when session was NOT located
    assert!(
        v["context"].is_null(),
        "context should be null when session not found per §4"
    );
}

// 17. close-tab nonexistent session text — §3
#[test]
fn tab_close_tab_nonexistent_session_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "close-tab",
            "--session",
            "nonexistent",
            "--tab",
            "t1",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "should contain 'error SESSION_NOT_FOUND:', got: {text}"
    );
}

// 18. close-tab nonexistent tab JSON — §3
#[test]
fn tab_close_tab_nonexistent_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless_json(
        &[
            "browser",
            "close-tab",
            "--session",
            &sid,
            "--tab",
            "t999",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    // §4: session was found, so context.session_id must be returned
    assert!(
        v["context"].is_object(),
        "context should be present when session is found per §4"
    );
    assert_eq!(
        v["context"]["session_id"], sid,
        "context.session_id should be present per §4"
    );

    close_session(&sid);
}

// 19. close-tab nonexistent tab text — §3
#[test]
fn tab_close_tab_nonexistent_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless(
        &[
            "browser",
            "close-tab",
            "--session",
            &sid,
            "--tab",
            "t999",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "should contain 'error TAB_NOT_FOUND:', got: {text}"
    );

    close_session(&sid);
}

// 20. close-tab double close — §3
#[test]
fn tab_close_tab_double_close_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab for double close");

    // First close: success
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", "t2"],
        30,
    );
    assert_success(&out, "first close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.close-tab");
    assert_eq!(v["data"]["closed_tab_id"], "t2");

    // Second close: TAB_NOT_FOUND
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", "t2"],
        30,
    );
    assert_failure(&out, "second close should fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "TAB_NOT_FOUND");

    close_session(&sid);
}

// ===========================================================================
// Group 5: Concurrent — Same Session
// ===========================================================================

// 21. concurrent multi-tab same session — parallel list-tabs on same session
#[test]
fn tab_concurrent_multi_tab_same_session() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let sid = start_session(TEST_URL_1);

    // Open t2 and t3 at different URLs
    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab t2");
    let out = headless(
        &["browser", "new-tab", TEST_URL_3, "--session", &sid],
        30,
    );
    assert_success(&out, "new-tab t3");

    // Parallel list-tabs on the same session (3 concurrent requests)
    let sid1 = sid.clone();
    let sid2 = sid.clone();
    let sid3 = sid.clone();

    let t1 = std::thread::spawn(move || {
        headless_json(&["browser", "list-tabs", "--session", &sid1], 10)
    });
    let t2 = std::thread::spawn(move || {
        headless_json(&["browser", "list-tabs", "--session", &sid2], 10)
    });
    let t3 = std::thread::spawn(move || {
        headless_json(&["browser", "list-tabs", "--session", &sid3], 10)
    });

    let out1 = t1.join().expect("thread 1");
    let out2 = t2.join().expect("thread 2");
    let out3 = t3.join().expect("thread 3");

    assert_success(&out1, "list-tabs 1");
    assert_success(&out2, "list-tabs 2");
    assert_success(&out3, "list-tabs 3");

    // All return same result: 3 tabs with correct session context
    for (i, out) in [&out1, &out2, &out3].iter().enumerate() {
        let v = parse_json(out);
        assert_eq!(v["ok"], true);
        assert_eq!(v["context"]["session_id"], sid, "thread {i} session_id");
        assert_eq!(
            v["data"]["total_tabs"],
            serde_json::json!(3),
            "thread {i} total_tabs"
        );
        let tabs = v["data"]["tabs"].as_array().expect("tabs array");
        let ids: Vec<&str> = tabs
            .iter()
            .filter_map(|t| t["tab_id"].as_str())
            .collect();
        assert!(ids.contains(&"t1"), "thread {i} has t1");
        assert!(ids.contains(&"t2"), "thread {i} has t2");
        assert!(ids.contains(&"t3"), "thread {i} has t3");
    }

    close_session(&sid);
}

// ===========================================================================
// Group 6: Concurrent — Cross-Session
// ===========================================================================

// 22. concurrent multi-tab cross-session — parallel list-tabs
#[test]
fn tab_concurrent_multi_tab_cross_session() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start two sessions with different profiles
    start_named_session("session-a", "profile-a", TEST_URL_1);
    start_named_session("session-b", "profile-b", TEST_URL_3);

    // Open t2 in each session
    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", "session-a"],
        30,
    );
    assert_success(&out, "new-tab session-a t2");
    let out = headless(
        &["browser", "new-tab", TEST_URL_1, "--session", "session-b"],
        30,
    );
    assert_success(&out, "new-tab session-b t2");

    // Parallel list-tabs on both sessions
    let ta = std::thread::spawn(|| {
        headless_json(&["browser", "list-tabs", "--session", "session-a"], 10)
    });
    let tb = std::thread::spawn(|| {
        headless_json(&["browser", "list-tabs", "--session", "session-b"], 10)
    });

    let out_a = ta.join().expect("thread session-a");
    let out_b = tb.join().expect("thread session-b");

    assert_success(&out_a, "list-tabs session-a");
    assert_success(&out_b, "list-tabs session-b");

    let va = parse_json(&out_a);
    let vb = parse_json(&out_b);

    // session-a: total_tabs=2, has t1+t2
    assert_eq!(va["data"]["total_tabs"], serde_json::json!(2));
    let tabs_a = va["data"]["tabs"].as_array().expect("tabs array a");
    let ids_a: Vec<&str> = tabs_a
        .iter()
        .filter_map(|t| t["tab_id"].as_str())
        .collect();
    assert!(ids_a.contains(&"t1"));
    assert!(ids_a.contains(&"t2"));

    // session-b: total_tabs=2, has t1+t2 (tab IDs are session-scoped per §5.2)
    assert_eq!(vb["data"]["total_tabs"], serde_json::json!(2));
    let tabs_b = vb["data"]["tabs"].as_array().expect("tabs array b");
    let ids_b: Vec<&str> = tabs_b
        .iter()
        .filter_map(|t| t["tab_id"].as_str())
        .collect();
    assert!(ids_b.contains(&"t1"));
    assert!(ids_b.contains(&"t2"));

    close_session("session-a");
    close_session("session-b");
}

// 23. concurrent close-tab cross-session — parallel close-tab
#[test]
fn tab_concurrent_close_tabs_cross_session() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    start_named_session("session-x", "profile-x", TEST_URL_1);
    start_named_session("session-y", "profile-y", TEST_URL_3);

    // Open t2 in each
    let out = headless(
        &["browser", "new-tab", TEST_URL_2, "--session", "session-x"],
        30,
    );
    assert_success(&out, "new-tab session-x t2");
    let out = headless(
        &["browser", "new-tab", TEST_URL_1, "--session", "session-y"],
        30,
    );
    assert_success(&out, "new-tab session-y t2");

    // Parallel close-tab t2 on both sessions
    let tx = std::thread::spawn(|| {
        headless_json(
            &[
                "browser",
                "close-tab",
                "--session",
                "session-x",
                "--tab",
                "t2",
            ],
            30,
        )
    });
    let ty = std::thread::spawn(|| {
        headless_json(
            &[
                "browser",
                "close-tab",
                "--session",
                "session-y",
                "--tab",
                "t2",
            ],
            30,
        )
    });

    let out_x = tx.join().expect("thread session-x");
    let out_y = ty.join().expect("thread session-y");

    assert_success(&out_x, "close-tab session-x");
    assert_success(&out_y, "close-tab session-y");

    let vx = parse_json(&out_x);
    let vy = parse_json(&out_y);

    // Both succeed with closed_tab_id = "t2"
    assert_eq!(vx["ok"], true);
    assert_eq!(vx["data"]["closed_tab_id"], "t2");
    assert_eq!(vy["ok"], true);
    assert_eq!(vy["data"]["closed_tab_id"], "t2");

    // list-tabs: each session has 1 tab remaining
    let out = headless_json(
        &["browser", "list-tabs", "--session", "session-x"],
        10,
    );
    assert_success(&out, "list-tabs session-x");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(1));

    let out = headless_json(
        &["browser", "list-tabs", "--session", "session-y"],
        10,
    );
    assert_success(&out, "list-tabs session-y");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(1));

    close_session("session-x");
    close_session("session-y");
}
