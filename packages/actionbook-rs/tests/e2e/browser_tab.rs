//! Browser tab management E2E tests.
//!
//! Covers opening, switching, closing, and listing tabs across sessions.
//! Uses daemon v2 CLI format with --session and --tab addressing.

use serde_json::Value;

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};

fn parse_json(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nraw stdout: {text}");
    })
}

/// S1T2: start → open second URL → list-tabs → shows 2 tabs → close
#[test]
fn tab_open_creates_new_tab() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start session");

    // Open a second tab — verify browser.new-tab JSON response
    let out = headless_json(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open second tab");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "open: ok should be true");
    assert_eq!(v["command"], "browser.new-tab", "open: command field");
    assert_eq!(
        v["data"]["tab"]["tab_id"], "t2",
        "open: second tab should be t2, got: {}",
        v["data"]["tab"]["tab_id"]
    );
    assert_eq!(v["data"]["created"], true, "open: created should be true");
    assert_eq!(
        v["data"]["new_window"], false,
        "open: new_window should be false"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "open: context.session_id should be local-1"
    );
    assert_eq!(
        v["context"]["tab_id"], "t2",
        "open: context.tab_id should be t2"
    );

    // List tabs — verify browser.list-tabs JSON response
    let out = headless_json(&["browser", "list-tabs", "-s", "local-1"], 10);
    assert_success(&out, "list-tabs");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "list-tabs: ok should be true");
    assert_eq!(
        v["command"], "browser.list-tabs",
        "list-tabs: command field"
    );
    assert_eq!(
        v["data"]["total_tabs"], 2,
        "list-tabs: total_tabs should be 2"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "list-tabs: context.session_id should be local-1"
    );
    assert!(
        v["context"]["tab_id"].is_null(),
        "list-tabs: context.tab_id should be absent (session-level), got: {}",
        v["context"]["tab_id"]
    );
    let tabs = v["data"]["tabs"]
        .as_array()
        .expect("tabs should be an array");
    assert_eq!(
        tabs.len(),
        2,
        "list-tabs: tabs array should have 2 elements"
    );
    let tab_ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(
        tab_ids.contains(&"t1"),
        "list-tabs: should contain t1, got: {:?}",
        tab_ids
    );
    assert!(
        tab_ids.contains(&"t2"),
        "list-tabs: should contain t2, got: {:?}",
        tab_ids
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// S1T1: start with --open-url → eval location.href on t1 → correct URL → close
#[test]
fn tab_open_navigates_to_url() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session opening example.com
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
    assert_success(&out, "start session");

    // Eval location on t1
    let out = headless(
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
    let location = stdout_str(&out);
    assert!(
        location.contains("example.com"),
        "t1 should be on example.com, got: {}",
        location
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second URL → eval on t1 → eval on t2 → different URLs → close
#[test]
fn tab_switch_changes_active() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session with example.com
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
    assert_success(&out, "start session");

    // Open second tab with example.org
    let out = headless(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open second tab");

    // Explicitly navigate t2 to example.org (Chrome may open internal URLs)
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "-s",
            "local-1",
            "-t",
            "t2",
        ],
        30,
    );
    assert_success(&out, "goto example.org on t2");

    // Wait for t2 to finish loading
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t2",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for t2 load");

    // Eval on t1 — should be example.com
    let out = headless(
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
    assert_success(&out, "eval t1");
    let loc_t1 = stdout_str(&out);
    assert!(
        loc_t1.contains("example.com"),
        "t1 should be on example.com, got: {}",
        loc_t1
    );

    // Eval on t2 — should be example.org
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "local-1",
            "-t",
            "t2",
        ],
        30,
    );
    assert_success(&out, "eval t2");
    let loc_t2 = stdout_str(&out);
    assert!(
        loc_t2.contains("example.org"),
        "t2 should be on example.org, got: {}",
        loc_t2
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second tab → close-tab t2 → list-tabs → only 1 tab → close
#[test]
fn tab_close_removes_tab() {
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
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Open second tab
    let out = headless(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open second tab");

    // Close tab t2 — verify browser.close-tab JSON response
    let out = headless_json(&["browser", "close-tab", "-s", "local-1", "-t", "t2"], 30);
    assert_success(&out, "close-tab t2");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true, "close-tab: ok should be true");
    assert_eq!(
        v["command"], "browser.close-tab",
        "close-tab: command field"
    );
    assert_eq!(
        v["data"]["closed_tab_id"], "t2",
        "close-tab: closed_tab_id should be t2"
    );
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "close-tab: context.session_id should be local-1"
    );
    assert_eq!(
        v["context"]["tab_id"], "t2",
        "close-tab: context.tab_id should be t2"
    );

    // List tabs — verify only t1 remains
    let out = headless_json(&["browser", "list-tabs", "-s", "local-1"], 10);
    assert_success(&out, "list-tabs after close");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["total_tabs"], 1,
        "list-tabs: total_tabs should be 1 after close"
    );
    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let tab_ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(
        tab_ids.contains(&"t1"),
        "list-tabs: should still contain t1, got: {:?}",
        tab_ids
    );
    assert!(
        !tab_ids.contains(&"t2"),
        "list-tabs: should NOT contain t2 after close, got: {:?}",
        tab_ids
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second tab → close-tab t2 → eval on t1 → still works → close
#[test]
fn tab_close_preserves_other() {
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
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Open second tab
    let out = headless(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open second tab");

    // Close tab t2
    let out = headless(&["browser", "close-tab", "-s", "local-1", "-t", "t2"], 30);
    assert_success(&out, "close-tab t2");

    // Eval on t1 should still work
    let out = headless(
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
    assert_success(&out, "eval t1 after closing t2");
    let location = stdout_str(&out);
    assert!(
        location.contains("example.com"),
        "t1 should still be on example.com, got: {}",
        location
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second tab → list-tabs → output contains both URLs → close
#[test]
fn tab_pages_lists_all() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session with example.com
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
    assert_success(&out, "start session");

    // Open second tab with example.org
    let out = headless(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open second tab");

    // List tabs — verify JSON contains both URLs
    let out = headless_json(&["browser", "list-tabs", "-s", "local-1"], 10);
    assert_success(&out, "list-tabs");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["total_tabs"], 2,
        "list-tabs: total_tabs should be 2"
    );
    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let urls: Vec<&str> = tabs.iter().filter_map(|t| t["url"].as_str()).collect();
    assert!(
        urls.iter().any(|u| u.contains("example.com")),
        "list-tabs: should contain example.com URL, got: {:?}",
        urls
    );
    assert!(
        urls.iter().any(|u| u.contains("example.org")),
        "list-tabs: should contain example.org URL, got: {:?}",
        urls
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open tab2 → open tab3 → list-tabs → shows t1, t2, t3 → close
#[test]
fn tab_open_sequential_ids() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session with first tab (t1)
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
    assert_success(&out, "start session");

    // Open second tab (t2)
    let out = headless_json(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open tab t2");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["tab"]["tab_id"], "t2",
        "second open: tab_id should be t2, got: {}",
        v["data"]["tab"]["tab_id"]
    );

    // Open third tab (t3)
    let out = headless_json(
        &[
            "browser",
            "open",
            "https://example.com/page2",
            "-s",
            "local-1",
        ],
        30,
    );
    assert_success(&out, "open tab t3");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["tab"]["tab_id"], "t3",
        "third open: tab_id should be t3, got: {}",
        v["data"]["tab"]["tab_id"]
    );

    // List tabs — verify JSON shows t1, t2, t3
    let out = headless_json(&["browser", "list-tabs", "-s", "local-1"], 10);
    assert_success(&out, "list-tabs");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["total_tabs"], 3,
        "list-tabs: total_tabs should be 3"
    );
    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let tab_ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(
        tab_ids.contains(&"t1"),
        "list-tabs: should contain t1, got: {:?}",
        tab_ids
    );
    assert!(
        tab_ids.contains(&"t2"),
        "list-tabs: should contain t2, got: {:?}",
        tab_ids
    );
    assert!(
        tab_ids.contains(&"t3"),
        "list-tabs: should contain t3, got: {:?}",
        tab_ids
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
