//! Phase B1: tab management + navigation field-level PRD contract E2E (#t31).
//!
//! Verifies exact PRD data shapes and text output for:
//!   - browser close-tab  (§8.3)
//!   - browser goto       (§9.1)
//!   - browser back       (§9.2)
//!   - browser forward    (§9.3)
//!   - browser reload     (§9.4)
//!
//! All tests are data: URL based for deterministic title/DOM assertions.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};
use serde_json::{json, Value};

fn parse_envelope(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

// ---------------------------------------------------------------------------
// Helper: start a headless local session at about:blank
// ---------------------------------------------------------------------------
fn start_blank_session() {
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
    assert_success(&out, "start blank session");
}

// ===========================================================================
// §8.3  browser close-tab
// ===========================================================================

/// PRD 8.3: data.closed_tab_id == closed tab's tab_id; context.tab_id set,
/// context.url null.
#[test]
fn contract_b1_close_tab_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    // Open a second tab so we have something to close.
    let out = headless(&["browser", "new-tab", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "new-tab");

    let out = headless_json(&["browser", "close-tab", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "close-tab json");

    let v = parse_envelope(&out);

    // JSON envelope fields
    assert_eq!(v["ok"], json!(true), "ok should be true");
    assert_eq!(v["command"], "browser.close-tab", "command name");

    // context: tab_id is the closed tab, url/title null
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(
        v["context"]["tab_id"], "t1",
        "context.tab_id must be the closed tab"
    );
    assert_eq!(
        v["context"]["url"],
        json!(null),
        "context.url must be null for close-tab"
    );
    assert_eq!(
        v["context"]["title"],
        json!(null),
        "context.title must be null for close-tab"
    );

    // data: closed_tab_id matches
    assert_eq!(
        v["data"]["closed_tab_id"], "t1",
        "data.closed_tab_id must be 't1', got: {}",
        v["data"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// PRD 8.3 text output:
///   [session tab_id]
///   ok browser.close-tab
#[test]
fn contract_b1_close_tab_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let out = headless(&["browser", "new-tab", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "new-tab");

    let out = headless(&["browser", "close-tab", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "close-tab text");

    let text = stdout_str(&out);
    assert_eq!(
        text.trim(),
        "[local-1 t1]\nok browser.close-tab",
        "close-tab text output should be '[local-1 t1]\\nok browser.close-tab', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// §9.1  browser goto
// ===========================================================================

/// PRD 9.1: data.kind="goto", data.requested_url, data.from_url, data.to_url,
/// data.title; context.url == data.to_url.
#[test]
fn contract_b1_goto_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let target_url = "data:text/html,<title>GotoContract</title><h1>hello</h1>";
    let out = headless_json(
        &["browser", "goto", target_url, "-s", "local-1", "-t", "t0"],
        30,
    );
    assert_success(&out, "goto json");

    let v = parse_envelope(&out);

    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["command"], "browser.goto");

    // context
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(v["context"]["tab_id"], "t0");
    let ctx_url = v["context"]["url"].as_str().unwrap_or("");
    assert!(!ctx_url.is_empty(), "context.url must be non-empty");
    assert_eq!(
        v["context"]["title"], "GotoContract",
        "context.title must match page title"
    );

    // data fields per PRD §9.1
    assert_eq!(v["data"]["kind"], "goto", "data.kind must be 'goto'");
    assert_eq!(
        v["data"]["requested_url"], target_url,
        "data.requested_url must be the URL passed to goto"
    );
    assert!(
        v["data"]["from_url"].is_string(),
        "data.from_url must be a string, got: {}",
        v["data"]["from_url"]
    );
    let to_url = v["data"]["to_url"].as_str().unwrap_or("");
    assert!(!to_url.is_empty(), "data.to_url must be non-empty");
    assert_eq!(
        v["data"]["to_url"], v["context"]["url"],
        "data.to_url must match context.url"
    );
    assert_eq!(
        v["data"]["title"], "GotoContract",
        "data.title must be the page title"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// PRD 9.1 text output (exact PRD shape, no extra lines):
///   [session tab] url
///   ok browser.goto
///   title: <title>
#[test]
fn contract_b1_goto_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let target_url = "data:text/html,<title>GotoText</title><h1>hi</h1>";
    let out = headless(
        &["browser", "goto", target_url, "-s", "local-1", "-t", "t0"],
        30,
    );
    assert_success(&out, "goto text");

    let text = stdout_str(&out);
    // PRD §9.1 text: [session tab] url\nok browser.goto\ntitle: <title>
    // Must NOT contain an extra "from_url → to_url" arrow line.
    let expected = format!("[local-1 t0] {target_url}\nok browser.goto\ntitle: GotoText");
    assert_eq!(
        text.trim(),
        expected,
        "goto text output must match PRD §9.1 format exactly"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// §9.2  browser back
// ===========================================================================

/// PRD 9.2: same structure as goto with kind="back".
/// data.from_url and data.to_url differ; context.url == data.to_url.
#[test]
fn contract_b1_back_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    // Build history: page1 → page2
    let page1 = "data:text/html,<title>BackPage1</title><h1>one</h1>";
    let page2 = "data:text/html,<title>BackPage2</title><h1>two</h1>";

    let out = headless(&["browser", "goto", page1, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page1");
    let out = headless(&["browser", "goto", page2, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page2");

    let out = headless_json(&["browser", "back", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "back json");

    let v = parse_envelope(&out);

    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["command"], "browser.back");

    // context
    assert_eq!(v["context"]["session_id"], "local-1");
    assert_eq!(v["context"]["tab_id"], "t0");
    let ctx_url = v["context"]["url"].as_str().unwrap_or("");
    assert!(
        !ctx_url.is_empty(),
        "context.url must be non-empty after back"
    );

    // data per PRD §9.2 (same fields as goto except kind)
    assert_eq!(v["data"]["kind"], "back", "data.kind must be 'back'");
    assert!(
        v["data"]["from_url"].is_string(),
        "data.from_url must be a string"
    );
    let to_url = v["data"]["to_url"].as_str().unwrap_or("");
    assert!(!to_url.is_empty(), "data.to_url must be non-empty");
    assert_ne!(
        v["data"]["from_url"], v["data"]["to_url"],
        "data.from_url and data.to_url must differ after back"
    );
    assert_eq!(
        v["data"]["to_url"], v["context"]["url"],
        "data.to_url must match context.url"
    );
    assert_eq!(
        v["data"]["title"], "BackPage1",
        "data.title must be the page title after back"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// PRD 9.2 text: [session tab] url\nok browser.back\ntitle: <title>
/// No extra arrow line.
#[test]
fn contract_b1_back_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let page1 = "data:text/html,<title>BackTextPage1</title><h1>one</h1>";
    let page2 = "data:text/html,<title>BackTextPage2</title><h1>two</h1>";

    let out = headless(&["browser", "goto", page1, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page1");
    let out = headless(&["browser", "goto", page2, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page2");

    let out = headless(&["browser", "back", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "back text");

    let text = stdout_str(&out);
    // Header contains [local-1 t0] and the page1 URL; title line is present.
    assert!(
        text.trim().starts_with("[local-1 t0]"),
        "back text must start with [local-1 t0], got: {text}"
    );
    assert!(
        text.contains("ok browser.back"),
        "back text must contain 'ok browser.back', got: {text}"
    );
    assert!(
        text.contains("title: BackTextPage1"),
        "back text must contain 'title: BackTextPage1', got: {text}"
    );
    // Must NOT contain the extra arrow line.
    assert!(
        !text.contains('\u{2192}'),
        "back text must not contain the extra arrow line '→', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// §9.3  browser forward
// ===========================================================================

/// PRD 9.3: same structure as goto with kind="forward".
#[test]
fn contract_b1_forward_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let page1 = "data:text/html,<title>FwdPage1</title><h1>one</h1>";
    let page2 = "data:text/html,<title>FwdPage2</title><h1>two</h1>";

    let out = headless(&["browser", "goto", page1, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page1");
    let out = headless(&["browser", "goto", page2, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page2");
    let out = headless(&["browser", "back", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "back to page1");

    let out = headless_json(&["browser", "forward", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "forward json");

    let v = parse_envelope(&out);

    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["command"], "browser.forward");
    assert_eq!(v["context"]["tab_id"], "t0");

    assert_eq!(v["data"]["kind"], "forward", "data.kind must be 'forward'");
    assert!(
        v["data"]["from_url"].is_string(),
        "data.from_url must be a string"
    );
    assert!(
        !v["data"]["to_url"].as_str().unwrap_or("").is_empty(),
        "data.to_url must be non-empty"
    );
    assert_ne!(
        v["data"]["from_url"], v["data"]["to_url"],
        "from_url and to_url must differ"
    );
    assert_eq!(
        v["data"]["to_url"], v["context"]["url"],
        "data.to_url must match context.url"
    );
    assert_eq!(
        v["data"]["title"], "FwdPage2",
        "data.title must be 'FwdPage2'"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// PRD 9.3 text: [session tab] url\nok browser.forward\ntitle: <title>
/// No extra arrow line.
#[test]
fn contract_b1_forward_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let page1 = "data:text/html,<title>FwdTextPage1</title><h1>one</h1>";
    let page2 = "data:text/html,<title>FwdTextPage2</title><h1>two</h1>";

    let out = headless(&["browser", "goto", page1, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page1");
    let out = headless(&["browser", "goto", page2, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "goto page2");
    let out = headless(&["browser", "back", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "back to page1");

    let out = headless(&["browser", "forward", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "forward text");

    let text = stdout_str(&out);
    assert!(
        text.trim().starts_with("[local-1 t0]"),
        "forward text must start with [local-1 t0], got: {text}"
    );
    assert!(
        text.contains("ok browser.forward"),
        "forward text must contain 'ok browser.forward', got: {text}"
    );
    assert!(
        text.contains("title: FwdTextPage2"),
        "forward text must contain 'title: FwdTextPage2', got: {text}"
    );
    // Must NOT contain the extra arrow line.
    assert!(
        !text.contains('\u{2192}'),
        "forward text must not contain the extra arrow line '→', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// §9.4  browser reload
// ===========================================================================

/// PRD 9.4: same structure as goto with kind="reload".
/// Reload keeps the same URL: data.from_url == data.to_url.
#[test]
fn contract_b1_reload_prd_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let target = "data:text/html,<title>ReloadContract</title><h1>r</h1>";
    let out = headless(
        &["browser", "goto", target, "-s", "local-1", "-t", "t0"],
        30,
    );
    assert_success(&out, "goto");

    let out = headless_json(&["browser", "reload", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "reload json");

    let v = parse_envelope(&out);

    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["command"], "browser.reload");
    assert_eq!(v["context"]["tab_id"], "t0");

    assert_eq!(v["data"]["kind"], "reload", "data.kind must be 'reload'");
    assert!(
        v["data"]["from_url"].is_string(),
        "data.from_url must be a string"
    );
    assert!(
        !v["data"]["to_url"].as_str().unwrap_or("").is_empty(),
        "data.to_url must be non-empty"
    );
    // Reload keeps the same URL
    assert_eq!(
        v["data"]["from_url"], v["data"]["to_url"],
        "reload: data.from_url must equal data.to_url (same page)"
    );
    assert_eq!(
        v["data"]["title"], "ReloadContract",
        "data.title must be preserved after reload"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

/// PRD 9.4 text: [session tab] url\nok browser.reload\ntitle: <title>
/// No extra arrow line.
#[test]
fn contract_b1_reload_prd_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    start_blank_session();

    let target = "data:text/html,<title>ReloadText</title><h1>r</h1>";
    let out = headless(
        &["browser", "goto", target, "-s", "local-1", "-t", "t0"],
        30,
    );
    assert_success(&out, "goto");

    let out = headless(&["browser", "reload", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "reload text");

    let text = stdout_str(&out);
    assert!(
        text.trim().starts_with("[local-1 t0]"),
        "reload text must start with [local-1 t0]"
    );
    assert!(
        text.contains("ok browser.reload"),
        "reload text must contain 'ok browser.reload'"
    );
    assert!(
        text.contains("title: ReloadText"),
        "reload text must contain 'title: ReloadText'"
    );
    assert!(
        !text.contains('\u{2192}'),
        "reload text must not contain arrow line '→', got: {text}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
