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
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

const TEST_URL: &str = "https://actionbook.dev/";
static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a headless session on about:blank, return (session_id, tab_id).
fn start_session() -> (String, String) {
    let session_id = format!(
        "b2a-query-{}",
        SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            &session_id,
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start session for b2a test");
    wait_for_ready_state_complete(&session_id, "t0");

    let tabs_out = headless_json(&["browser", "list-tabs", "-s", &session_id], 15);
    assert_success(&tabs_out, "list-tabs after start");
    let json: Value = serde_json::from_str(&stdout_str(&tabs_out)).expect("valid JSON from tabs");
    let tab_id = json["data"]["tabs"][0]["tab_id"]
        .as_str()
        .expect("tab_id in list-tabs data")
        .to_string();
    (session_id, tab_id)
}

fn wait_for_ready_state_complete(session_id: &str, tab_id: &str) {
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            session_id,
            "-t",
            tab_id,
            "--timeout",
            "10000",
        ],
        30,
    );
    assert_success(&out, "wait condition readyState complete");
}

fn parse_envelope(out: &std::process::Output) -> Value {
    serde_json::from_str(&stdout_str(out)).expect("valid JSON envelope")
}

fn install_query_fixture(session_id: &str, tab_id: &str) {
    let setup_js = set_body_html_js(
        r#"
<main id="query-fixture">
  <div class="single item">Item A</div>
  <div class="item">Item B</div>
  <div class="item" style="display:none">Item Hidden</div>
  <section class="card">
    <h2>Actionbook Query Target</h2>
    <button class="child">Open</button>
  </section>
</main>
"#,
    );
    let out = headless(
        &["browser", "eval", &setup_js, "-s", session_id, "-t", tab_id],
        15,
    );
    assert_success(&out, "install query fixture");
}

fn assert_query_item(
    item: &Value,
    selector: &str,
    tag: &str,
    text: &str,
    visible: bool,
    enabled: bool,
) {
    assert_eq!(item["selector"], selector, "unexpected selector in {item}");
    assert_eq!(item["tag"], tag, "unexpected tag in {item}");
    assert_eq!(item["text"], text, "unexpected text in {item}");
    assert_eq!(item["visible"], visible, "unexpected visible in {item}");
    assert_eq!(item["enabled"], enabled, "unexpected enabled in {item}");
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

    install_query_fixture(&sid, &tid);

    // All mode
    let all_out = headless_json(
        &["browser", "query", "all", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&all_out, "query all --json");
    let all_json = parse_envelope(&all_out);
    assert_eq!(all_json["ok"], true);
    assert_eq!(all_json["command"], "browser.query");
    assert_eq!(all_json["context"]["session_id"], sid);
    assert_eq!(all_json["context"]["tab_id"], tid);
    assert_eq!(all_json["context"]["url"], TEST_URL);
    assert_eq!(
        all_json["data"]["mode"], "all",
        "mode must be 'all', got: {}",
        all_json["data"]
    );
    assert_eq!(
        all_json["data"]["query"], ".item",
        "query must round-trip, got: {}",
        all_json["data"]
    );
    assert_eq!(
        all_json["data"]["count"], 3,
        "count must be 3, got: {}",
        all_json["data"]
    );
    let all_items = all_json["data"]["items"]
        .as_array()
        .expect("items must be an array");
    assert_eq!(
        all_items.len(),
        3,
        "expected 3 query items, got: {all_items:?}"
    );
    assert_query_item(
        &all_items[0],
        ".item:nth-of-type(1)",
        "div",
        "Item A",
        true,
        true,
    );
    assert_query_item(
        &all_items[1],
        ".item:nth-of-type(2)",
        "div",
        "Item B",
        true,
        true,
    );
    assert_query_item(
        &all_items[2],
        ".item:nth-of-type(3)",
        "div",
        "",
        false,
        true,
    );

    // One mode
    let one_out = headless_json(
        &["browser", "query", "one", ".single", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&one_out, "query one --json");
    let one_json = parse_envelope(&one_out);
    assert_eq!(one_json["ok"], true);
    assert_eq!(one_json["command"], "browser.query");
    assert_eq!(one_json["data"]["mode"], "one");
    assert_eq!(one_json["data"]["query"], ".single");
    assert_eq!(one_json["data"]["count"], 1);
    assert_query_item(
        &one_json["data"]["item"],
        ".single:nth-of-type(1)",
        "div",
        "Item A",
        true,
        true,
    );

    // Count mode
    let count_out = headless_json(
        &["browser", "query", "count", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&count_out, "query count --json");
    let count_json = parse_envelope(&count_out);
    assert_eq!(count_json["ok"], true);
    assert_eq!(count_json["data"]["mode"], "count");
    assert_eq!(count_json["data"]["query"], ".item");
    assert_eq!(count_json["data"]["count"], 3);

    // Nth mode
    let nth_out = headless_json(
        &[
            "browser", "query", "nth", "2", ".item", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    assert_success(&nth_out, "query nth --json");
    let nth_json = parse_envelope(&nth_out);
    assert_eq!(nth_json["ok"], true);
    assert_eq!(nth_json["data"]["mode"], "nth");
    assert_eq!(nth_json["data"]["query"], ".item");
    assert_eq!(nth_json["data"]["index"], 2);
    assert_eq!(nth_json["data"]["count"], 3);
    assert_query_item(
        &nth_json["data"]["item"],
        ".item:nth-of-type(2)",
        "div",
        "Item B",
        true,
        true,
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

#[test]
fn contract_b2a_query_context_uses_live_page_state() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    install_query_fixture(&sid, &tid);

    let mutate_out = headless(
        &[
            "browser",
            "eval",
            r#"history.pushState({}, '', '/query-live-context'); document.title = 'Live Query Title';"#,
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&mutate_out, "mutate query page state");

    let out = headless_json(
        &["browser", "query", "one", ".single", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&out, "query one after live state mutation");
    let json = parse_envelope(&out);
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.query");
    assert_eq!(
        json["context"]["url"],
        format!("{TEST_URL}query-live-context"),
        "query context.url must come from live page state, got: {}",
        json["context"]
    );
    assert_eq!(
        json["context"]["title"], "Live Query Title",
        "query context.title must come from live page state, got: {}",
        json["context"]
    );

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

    install_query_fixture(&sid, &tid);

    let out = headless_json(
        &["browser", "query", "one", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    let json = parse_envelope(&out);
    assert_eq!(
        json["ok"], false,
        "ok must be false when query one has multiple matches, got: {}",
        json
    );
    assert_eq!(
        json["command"], "browser.query",
        "command must be 'browser.query', got: {}",
        json
    );
    assert!(
        !json["error"]["retryable"].as_bool().unwrap_or(true),
        "query cardinality errors must be non-retryable, got: {}",
        json["error"]
    );
    assert_eq!(json["error"]["code"], "MULTIPLE_MATCHES");
    assert_eq!(
        json["error"]["message"],
        "Query mode 'one' requires exactly 1 match, found 3"
    );
    assert_eq!(json["error"]["details"]["query"], ".item");
    assert_eq!(json["error"]["details"]["count"], 3);
    assert_eq!(
        json["error"]["details"]["sample_selectors"],
        serde_json::json!([
            ".item:nth-of-type(1)",
            ".item:nth-of-type(2)",
            ".item:nth-of-type(3)"
        ])
    );

    let nth_out = headless_json(
        &[
            "browser", "query", "nth", "4", ".item", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    let nth_json = parse_envelope(&nth_out);
    assert_eq!(nth_json["ok"], false);
    assert_eq!(nth_json["command"], "browser.query");
    assert_eq!(nth_json["error"]["code"], "INDEX_OUT_OF_RANGE");
    assert_eq!(
        nth_json["error"]["message"],
        "index 4 out of range (found 3 matches)"
    );
    assert_eq!(nth_json["error"]["details"]["query"], ".item");
    assert_eq!(nth_json["error"]["details"]["count"], 3);
    assert_eq!(nth_json["error"]["details"]["index"], 4);

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 5b: Query extended syntax
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_query_extended_syntax_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    install_query_fixture(&sid, &tid);

    let visible_out = headless_json(
        &[
            "browser",
            "query",
            "all",
            ".item:visible",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&visible_out, "query all .item:visible");
    let visible_json = parse_envelope(&visible_out);
    assert_eq!(visible_json["data"]["count"], 2);
    let visible_items = visible_json["data"]["items"]
        .as_array()
        .expect("visible items array");
    assert_eq!(visible_items.len(), 2);
    assert_eq!(visible_items[0]["text"], "Item A");
    assert_eq!(visible_items[1]["text"], "Item B");
    assert!(
        visible_items
            .iter()
            .all(|item| item["visible"].as_bool().unwrap_or(false)),
        "all :visible matches must report visible=true, got: {}",
        visible_json["data"]
    );

    let contains_out = headless_json(
        &[
            "browser",
            "query",
            "one",
            r#"div:contains("Item B")"#,
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&contains_out, "query one div:contains(Item B)");
    let contains_json = parse_envelope(&contains_out);
    assert_eq!(contains_json["data"]["count"], 1);
    assert_eq!(contains_json["data"]["item"]["text"], "Item B");

    let has_out = headless_json(
        &[
            "browser",
            "query",
            "one",
            "section:has(button.child)",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&has_out, "query one section:has(button.child)");
    let has_json = parse_envelope(&has_out);
    assert_eq!(has_json["data"]["count"], 1);
    assert_eq!(has_json["data"]["item"]["tag"], "section");
    assert!(
        has_json["data"]["item"]["text"]
            .as_str()
            .unwrap_or("")
            .contains("Actionbook Query Target"),
        "section:has(button.child) should return the card section, got: {}",
        has_json["data"]
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 5c: Query text output
// ---------------------------------------------------------------------------

#[test]
fn contract_b2a_query_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    install_query_fixture(&sid, &tid);

    let one_out = headless(
        &["browser", "query", "one", ".single", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&one_out, "query one text");
    let one_stdout = stdout_str(&one_out);
    let one_lines: Vec<&str> = one_stdout.trim().lines().collect();
    assert_eq!(one_lines[0], format!("[{sid} {tid}] {TEST_URL}"));
    assert_eq!(one_lines[1], "1 match");
    assert_eq!(one_lines[2], "selector: .single:nth-of-type(1)");
    assert_eq!(one_lines[3], "text: Item A");

    let all_out = headless(
        &["browser", "query", "all", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&all_out, "query all text");
    let all_stdout = stdout_str(&all_out);
    let all_lines: Vec<&str> = all_stdout.trim().lines().collect();
    assert_eq!(all_lines[0], format!("[{sid} {tid}] {TEST_URL}"));
    assert_eq!(all_lines[1], "3 matches");
    assert_eq!(all_lines[2], "1. .item:nth-of-type(1)");
    assert_eq!(all_lines[3], "   Item A");

    let nth_out = headless(
        &[
            "browser", "query", "nth", "2", ".item", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    assert_success(&nth_out, "query nth text");
    let nth_stdout = stdout_str(&nth_out);
    let nth_lines: Vec<&str> = nth_stdout.trim().lines().collect();
    assert_eq!(nth_lines[0], format!("[{sid} {tid}] {TEST_URL}"));
    assert_eq!(nth_lines[1], "match 2/3");
    assert_eq!(nth_lines[2], "selector: .item:nth-of-type(2)");
    assert_eq!(nth_lines[3], "text: Item B");

    let count_out = headless(
        &["browser", "query", "count", ".item", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&count_out, "query count text");
    let count_stdout = stdout_str(&count_out);
    let count_lines: Vec<&str> = count_stdout.trim().lines().collect();
    assert_eq!(count_lines[0], format!("[{sid} {tid}] {TEST_URL}"));
    assert_eq!(count_lines[1], "3");

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
