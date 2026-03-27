//! Phase B2b contract E2E tests.
//!
//! Validates the JSON envelope shape for interaction/wait/eval commands
//! defined in Phase B2b.
//!
//! Each test is self-contained: start session → interact → assert contracts → close.
//! All tests are gated by `RUN_E2E_TESTS=true`.

use crate::harness::{
    assert_success, headless, headless_json, set_body_html_js, skip, stdout_str, SessionGuard,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a headless session on about:blank, return (session_id, tab_id).
fn start_session() -> (String, String) {
    let out = headless_json(
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
    assert_success(&out, "start session for b2b test");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from start");
    let session_id = json["context"]["session_id"]
        .as_str()
        .expect("session_id in start context")
        .to_string();
    let tab_id = json["data"]["tabs"][0]["tab_id"]
        .as_str()
        .unwrap_or("t0")
        .to_string();
    (session_id, tab_id)
}

/// Inject HTML into the current page via eval.
fn inject_html(sid: &str, tid: &str, html: &str) {
    let js = set_body_html_js(html);
    let out = headless(&["browser", "eval", "-s", sid, "-t", tid, &js], 15);
    assert_success(&out, "inject HTML");
}

// ---------------------------------------------------------------------------
// Test 1: Click JSON envelope
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_click_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(&sid, &tid, r#"<button id="btn">Click Me</button>"#);

    let out = headless_json(&["browser", "click", "-s", &sid, "-t", &tid, "#btn"], 15);
    assert_success(&out, "click --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from click");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.click");
    assert!(json["context"]["session_id"].as_str().is_some());
    assert!(json["data"]["target"]["selector"].as_str().is_some());
    assert!(json["meta"]["duration_ms"].as_u64().is_some());
    assert!(json["error"].is_null());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 2: Type + Fill JSON envelopes
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_type_fill_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(&sid, &tid, r#"<input id="name" type="text" />"#);

    // Type: CLI shape is `type <TEXT> [SELECTOR] -s -t`
    let type_out = headless_json(
        &["browser", "type", "-s", &sid, "-t", &tid, "hello", "#name"],
        15,
    );
    assert_success(&type_out, "type --json");
    let type_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&type_out)).expect("valid JSON from type");
    assert_eq!(type_json["ok"], true);
    assert_eq!(type_json["command"], "browser.type");
    assert_eq!(type_json["data"]["text"], "hello");
    assert_eq!(type_json["data"]["target"]["selector"], "#name");

    // Fill: CLI shape is `fill <SELECTOR> <TEXT> -s -t`
    let fill_out = headless_json(
        &["browser", "fill", "-s", &sid, "-t", &tid, "#name", "world"],
        15,
    );
    assert_success(&fill_out, "fill --json");
    let fill_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&fill_out)).expect("valid JSON from fill");
    assert_eq!(fill_json["ok"], true);
    assert_eq!(fill_json["command"], "browser.fill");
    assert_eq!(fill_json["data"]["value"], "world");

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 3: Hover, Focus, Press JSON envelopes
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_hover_focus_press_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(
        &sid,
        &tid,
        r#"<button id="btn">Test</button><input id="inp" />"#,
    );

    // Hover
    let hover_out = headless_json(&["browser", "hover", "-s", &sid, "-t", &tid, "#btn"], 15);
    assert_success(&hover_out, "hover --json");
    let hover_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&hover_out)).expect("valid JSON from hover");
    assert_eq!(hover_json["ok"], true);
    assert_eq!(hover_json["command"], "browser.hover");
    assert!(hover_json["data"]["target"]["selector"].as_str().is_some());

    // Focus
    let focus_out = headless_json(&["browser", "focus", "-s", &sid, "-t", &tid, "#inp"], 15);
    assert_success(&focus_out, "focus --json");
    let focus_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&focus_out)).expect("valid JSON from focus");
    assert_eq!(focus_json["ok"], true);
    assert_eq!(focus_json["command"], "browser.focus");

    // Press
    let press_out = headless_json(&["browser", "press", "-s", &sid, "-t", &tid, "Enter"], 15);
    assert_success(&press_out, "press --json");
    let press_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&press_out)).expect("valid JSON from press");
    assert_eq!(press_json["ok"], true);
    assert_eq!(press_json["command"], "browser.press");
    assert_eq!(press_json["data"]["key"], "Enter");

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 4: Scroll JSON envelope
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_scroll_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // CLI shape: `scroll down [AMOUNT] -s -t` (subcommand)
    let out = headless_json(
        &["browser", "scroll", "down", "-s", &sid, "-t", &tid, "300"],
        15,
    );
    assert_success(&out, "scroll --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from scroll");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.scroll");
    assert_eq!(json["data"]["direction"], "down");
    assert!(json["meta"]["duration_ms"].as_u64().is_some());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 5: Eval JSON envelope
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_eval_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless_json(&["browser", "eval", "-s", &sid, "-t", &tid, "1 + 1"], 15);
    assert_success(&out, "eval --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from eval");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.eval");
    assert_eq!(json["data"]["expression"], "1 + 1");
    assert_eq!(json["data"]["value"], 2);
    assert!(json["meta"]["duration_ms"].as_u64().is_some());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 6: Mouse-move + Cursor-position JSON envelopes
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_mouse_move_cursor_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    // Mouse-move: CLI shape is `mouse-move <COORDS> -s -t` where COORDS = "x,y"
    let move_out = headless_json(
        &["browser", "mouse-move", "-s", &sid, "-t", &tid, "100,200"],
        15,
    );
    assert_success(&move_out, "mouse-move --json");
    let move_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&move_out)).expect("valid JSON from mouse-move");
    assert_eq!(move_json["ok"], true);
    assert_eq!(move_json["command"], "browser.mouse-move");
    assert!(move_json["data"]["x"].as_f64().is_some());
    assert!(move_json["data"]["y"].as_f64().is_some());

    // Cursor-position
    let cursor_out = headless_json(&["browser", "cursor-position", "-s", &sid, "-t", &tid], 15);
    assert_success(&cursor_out, "cursor-position --json");
    let cursor_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&cursor_out)).expect("valid JSON from cursor-position");
    assert_eq!(cursor_json["ok"], true);
    assert_eq!(cursor_json["command"], "browser.cursor-position");
    assert!(cursor_json["data"]["x"].as_f64().is_some());
    assert!(cursor_json["data"]["y"].as_f64().is_some());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 7: Wait-element JSON envelope
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_wait_element_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(&sid, &tid, r#"<div id="target">Exists</div>"#);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "element",
            "-s",
            &sid,
            "-t",
            &tid,
            "#target",
            "--timeout",
            "5000",
        ],
        15,
    );
    assert_success(&out, "wait element --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from wait element");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.wait.element");
    assert_eq!(json["data"]["found"], true);
    assert_eq!(json["data"]["target"]["selector"], "#target");
    assert!(json["meta"]["duration_ms"].as_u64().is_some());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 8: Wait-condition JSON envelope
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_wait_condition_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless_json(
        &[
            "browser",
            "wait",
            "condition",
            "-s",
            &sid,
            "-t",
            &tid,
            "true",
            "--timeout",
            "5000",
        ],
        15,
    );
    assert_success(&out, "wait condition --json");

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from wait condition");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.wait.condition");
    assert_eq!(json["data"]["met"], true);
    assert!(json["meta"]["duration_ms"].as_u64().is_some());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 9: Click text contract
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_click_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(&sid, &tid, r#"<button id="btn">Click Me</button>"#);

    let out = headless(&["browser", "click", "-s", &sid, "-t", &tid, "#btn"], 15);
    assert_success(&out, "click text");

    let text = stdout_str(&out);
    assert!(
        text.contains("ok browser.click"),
        "text must contain 'ok browser.click', got: {text}"
    );
    assert!(
        text.contains("target: #btn"),
        "text must contain 'target: #btn', got: {text}"
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 10: Type text contract
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_type_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(&sid, &tid, r#"<input id="name" type="text" />"#);

    let out = headless(
        &["browser", "type", "-s", &sid, "-t", &tid, "hello", "#name"],
        15,
    );
    assert_success(&out, "type text");

    let text = stdout_str(&out);
    assert!(
        text.contains("ok browser.type"),
        "text must contain 'ok browser.type', got: {text}"
    );
    assert!(
        text.contains("target: #name"),
        "text must contain 'target: #name', got: {text}"
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 11: Eval text contract
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_eval_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless(&["browser", "eval", "-s", &sid, "-t", &tid, "1 + 1"], 15);
    assert_success(&out, "eval text");

    let text = stdout_str(&out);
    assert!(
        text.contains('2'),
        "eval text must contain result value '2', got: {text}"
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 12: Wait-element text contract
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_wait_element_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();
    inject_html(&sid, &tid, r#"<div id="target">Exists</div>"#);

    let out = headless(
        &[
            "browser",
            "wait",
            "element",
            "-s",
            &sid,
            "-t",
            &tid,
            "#target",
            "--timeout",
            "5000",
        ],
        15,
    );
    assert_success(&out, "wait element text");

    let text = stdout_str(&out);
    assert!(
        text.contains("ok browser.wait.element"),
        "text must contain 'ok browser.wait.element', got: {text}"
    );
    assert!(
        text.contains("target: #target"),
        "text must contain 'target: #target', got: {text}"
    );

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}

// ---------------------------------------------------------------------------
// Test 13: Interaction error contract
// ---------------------------------------------------------------------------

#[test]
fn contract_b2b_click_error_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless_json(
        &[
            "browser",
            "click",
            "-s",
            &sid,
            "-t",
            &tid,
            "#nonexistent-element-xyz",
        ],
        15,
    );

    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from click error");
    assert_eq!(json["ok"], false);
    assert_eq!(json["command"], "browser.click");
    assert!(json["error"]["code"].as_str().is_some());
    assert!(json["error"]["message"].as_str().is_some());
    assert!(json["data"].is_null());

    let _ = headless(&["browser", "close", "-s", &sid], 15);
}
