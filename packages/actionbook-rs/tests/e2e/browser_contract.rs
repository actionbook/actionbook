//! Contract E2E tests for Phase B1: tab management + navigation CLI output.
//!
//! Validates JSON envelope shape (`ok`, `command`, `context`, `data`) and text
//! output format for: `new-tab`, `list-tabs`, `close-tab`, `goto`, `back`,
//! `forward`, `reload`.
//!
//! Uses `about:blank` and `data:` URLs to avoid external dependencies.

use crate::harness::{
    assert_failure, assert_success, headless, headless_json, skip, stdout_str, SessionGuard,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Helper: parse JSON envelope and assert common fields
// ---------------------------------------------------------------------------

fn parse_envelope(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

fn assert_envelope(v: &Value, expected_ok: bool, expected_command: &str) {
    assert_eq!(
        v["ok"], expected_ok,
        "ok should be {expected_ok}, got: {}",
        v["ok"]
    );
    assert_eq!(
        v["command"], expected_command,
        "command should be {expected_command}, got: {}",
        v["command"]
    );
    assert!(
        v.get("context").is_some(),
        "envelope should contain 'context'"
    );
}

// ---------------------------------------------------------------------------
// 1. new-tab JSON contract
// ---------------------------------------------------------------------------

#[test]
fn contract_new_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless_json(&["browser", "new-tab", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "new-tab json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.new-tab");

    // context should have session_id, tab_id
    assert_eq!(v["context"]["session_id"], "local-1");
    assert!(
        v["context"]["tab_id"].is_string(),
        "context.tab_id should be a string"
    );

    // data should have tab object with tab_id and url
    assert!(
        v["data"]["tab"]["tab_id"].is_string(),
        "data.tab.tab_id should be a string"
    );
    assert!(v["data"]["created"] == true, "data.created should be true");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 2. new-tab text contract
// ---------------------------------------------------------------------------

#[test]
fn contract_new_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(&["browser", "new-tab", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "new-tab text");

    let text = stdout_str(&out);
    // new-tab text format: [sid tid] <url>\nok browser.new-tab
    assert!(
        text.contains("[local-1 t"),
        "text output should contain [local-1 tN] prefix, got: {text}"
    );
    assert!(
        text.contains("ok browser.new-tab"),
        "text output should contain 'ok browser.new-tab', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 3. new-tab alias "open" still works
// ---------------------------------------------------------------------------

#[test]
fn contract_open_alias_works() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // Use the "open" alias
    let out = headless_json(&["browser", "open", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "open alias");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.new-tab");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 4. list-tabs JSON contract
// ---------------------------------------------------------------------------

#[test]
fn contract_list_tabs_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
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
    assert_success(&out, "start");

    // Open a second tab
    let out = headless(&["browser", "new-tab", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "new-tab");

    let out = headless_json(&["browser", "list-tabs", "-s", "local-1"], 10);
    assert_success(&out, "list-tabs json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.list-tabs");
    assert_eq!(v["context"]["session_id"], "local-1");

    // data should have tabs array
    let tabs = &v["data"]["tabs"];
    assert!(tabs.is_array(), "data.tabs should be an array");
    assert!(
        tabs.as_array().unwrap().len() >= 2,
        "should have at least 2 tabs"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 5. close-tab JSON contract
// ---------------------------------------------------------------------------

#[test]
fn contract_close_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
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
    assert_success(&out, "start");

    // Open a second tab so we can close it
    let out = headless(&["browser", "new-tab", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "new-tab");

    let out = headless_json(&["browser", "close-tab", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "close-tab json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.close-tab");
    assert_eq!(v["context"]["session_id"], "local-1");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 6. goto JSON contract
// ---------------------------------------------------------------------------

#[test]
fn contract_goto_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
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
    assert_success(&out, "start");

    let out = headless_json(
        &[
            "browser",
            "goto",
            "data:text/html,<title>Test Title</title><h1>hello</h1>",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.goto");
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(v["context"]["tab_id"], "t0");

    // data should have navigation fields
    assert!(
        v["data"]["kind"].is_string(),
        "data.kind should be a string"
    );
    assert!(
        v["data"]["to_url"].is_string(),
        "data.to_url should be a string"
    );
    assert!(
        v["data"]["title"].is_string(),
        "data.title should be a string"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 7. goto text contract
// ---------------------------------------------------------------------------

#[test]
fn contract_goto_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
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
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<h1>hello</h1>",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto text");

    let text = stdout_str(&out);
    assert!(
        text.contains("[local-1 t0]"),
        "text output should contain [local-1 t0], got: {text}"
    );
    assert!(
        text.contains("ok browser.goto"),
        "text output should contain 'ok browser.goto', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 8. back/forward JSON contract
// ---------------------------------------------------------------------------

#[test]
fn contract_back_forward_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
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
    assert_success(&out, "start");

    // Navigate to create history
    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<h1>page1</h1>",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto page1");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<h1>page2</h1>",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto page2");

    // Back
    let out = headless_json(&["browser", "back", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "back json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.back");
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(v["context"]["tab_id"], "t0");
    assert!(
        v["data"]["kind"].is_string(),
        "data.kind should be a string"
    );

    // Forward
    let out = headless_json(&["browser", "forward", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "forward json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.forward");
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(v["context"]["tab_id"], "t0");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 9. reload JSON contract
// ---------------------------------------------------------------------------

#[test]
fn contract_reload_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
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
    assert_success(&out, "start");

    let out = headless_json(&["browser", "reload", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "reload json");

    let v = parse_envelope(&out);
    assert_envelope(&v, true, "browser.reload");
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(v["context"]["tab_id"], "t0");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 10. new-tab --window flag mutual exclusivity
// ---------------------------------------------------------------------------

#[test]
fn contract_new_tab_window_flags_conflict() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // Using both --new-window and --window should fail (clap conflicts_with)
    // clap catches this at parse time and writes to stderr — no JSON produced
    let out = headless(
        &[
            "browser",
            "new-tab",
            "about:blank",
            "-s",
            "local-1",
            "--new-window",
            "--window",
            "w0",
        ],
        30,
    );
    assert_failure(&out, "using both --new-window and --window should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--new-window")
            || stderr.contains("--window")
            || stderr.contains("conflict")
            || stderr.contains("cannot be used with"),
        "error should mention conflicting flags, got stderr: {stderr}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
