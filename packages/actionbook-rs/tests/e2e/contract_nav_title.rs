//! Contract E2E tests for navigation title/context reliability (#t68).
//!
//! Verifies that goto/back/forward/reload reliably populate:
//! - data.title (non-empty for pages with <title>)
//! - context.title (matches data.title)
//! - context.url (post-navigation URL)
//! - text output includes "title: ..." line
//!
//! Uses data: URLs so title assertions are deterministic.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};
use serde_json::Value;

fn parse_envelope(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

// ---------------------------------------------------------------------------
// 1. contract_nav_goto_title_in_data
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_goto_title_in_data() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless_json(
        &[
            "browser",
            "goto",
            "data:text/html,<title>GotoTitle</title><h1>hello</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto json");

    let v = parse_envelope(&out);
    assert_eq!(
        v["data"]["title"], "GotoTitle",
        "data.title should be 'GotoTitle', got: {}",
        v["data"]["title"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 2. contract_nav_goto_context_title_and_url
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_goto_context_title_and_url() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless_json(
        &[
            "browser",
            "goto",
            "data:text/html,<title>ContextTitle</title><h1>hi</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto json");

    let v = parse_envelope(&out);
    assert_eq!(
        v["context"]["title"], "ContextTitle",
        "context.title should be 'ContextTitle', got: {}",
        v["context"]["title"]
    );
    assert!(
        v["context"]["url"].is_string() && !v["context"]["url"].as_str().unwrap_or("").is_empty(),
        "context.url should be a non-empty string, got: {}",
        v["context"]["url"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 3. contract_nav_goto_text_has_title_line
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_goto_text_has_title_line() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>GotoTextTitle</title><h1>hi</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto text");

    let text = stdout_str(&out);
    assert!(
        text.contains("title: GotoTextTitle"),
        "text output should contain 'title: GotoTextTitle', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 4. contract_nav_back_title_in_data
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_back_title_in_data() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>PageOne</title><h1>one</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page1");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>PageTwo</title><h1>two</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page2");

    let out = headless_json(&["browser", "back", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "back json");

    let v = parse_envelope(&out);
    assert_eq!(
        v["data"]["title"], "PageOne",
        "data.title should be 'PageOne' after back, got: {}",
        v["data"]["title"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 5. contract_nav_back_context_url_changes
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_back_context_url_changes() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>BackPage1</title><h1>one</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page1");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>BackPage2</title><h1>two</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page2");

    let out = headless_json(&["browser", "back", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "back json");

    let v = parse_envelope(&out);
    assert!(
        v["context"]["url"].is_string() && !v["context"]["url"].as_str().unwrap_or("").is_empty(),
        "context.url should be a non-empty string after back, got: {}",
        v["context"]["url"]
    );
    assert_ne!(
        v["data"]["from_url"], v["data"]["to_url"],
        "data.from_url and data.to_url should differ after back, got from={} to={}",
        v["data"]["from_url"], v["data"]["to_url"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 6. contract_nav_forward_title_in_data
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_forward_title_in_data() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>FwdPageOne</title><h1>one</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page1");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>FwdPageTwo</title><h1>two</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page2");

    let out = headless(&["browser", "back", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "back");

    let out = headless_json(&["browser", "forward", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "forward json");

    let v = parse_envelope(&out);
    assert_eq!(
        v["data"]["title"], "FwdPageTwo",
        "data.title should be 'FwdPageTwo' after forward, got: {}",
        v["data"]["title"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 7. contract_nav_reload_title_preserved
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_reload_title_preserved() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>ReloadTitle</title><h1>r</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto");

    let out = headless_json(&["browser", "reload", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "reload json");

    let v = parse_envelope(&out);
    assert_eq!(
        v["data"]["title"], "ReloadTitle",
        "data.title should be 'ReloadTitle' after reload, got: {}",
        v["data"]["title"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 8. contract_nav_back_text_has_title_line
// ---------------------------------------------------------------------------

#[test]
fn contract_nav_back_text_has_title_line() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>BackTextTitle</title><h1>one</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page1");

    let out = headless(
        &[
            "browser",
            "goto",
            "data:text/html,<title>BackTextPage2</title><h1>two</h1>",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "goto page2");

    let out = headless(&["browser", "back", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "back text");

    let text = stdout_str(&out);
    assert!(
        text.contains("title: BackTextTitle"),
        "text output should contain 'title: BackTextTitle', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
