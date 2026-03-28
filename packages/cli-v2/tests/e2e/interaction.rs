//! Browser interaction E2E tests: browser click.
//!
//! This file groups interaction commands together, similar to navigation.rs.
//! The initial coverage here is for `browser click`, per api-reference.md §11.1.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

const TEST_URL: &str = "https://example.com";

// ── Helpers ───────────────────────────────────────────────────────────

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

fn assert_click_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: Option<&str>,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.click");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "click");
    assert!(data["target"].is_object(), "data.target must be an object");
    if let Some(selector) = expected_selector {
        assert_eq!(data["target"]["selector"], selector);
    }
    assert!(
        data["changed"]["url_changed"].is_boolean(),
        "data.changed.url_changed must be a boolean"
    );
    assert!(
        data["changed"]["focus_changed"].is_boolean(),
        "data.changed.focus_changed must be a boolean"
    );

    assert_meta(v);
}

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

fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

fn eval_value(session_id: &str, tab_id: &str, expression: &str) -> String {
    let out = headless_json(
        &[
            "browser",
            "eval",
            expression,
            "--session",
            session_id,
            "--tab",
            tab_id,
        ],
        15,
    );
    assert_success(&out, "eval");
    let v = parse_json(&out);
    v["data"]["value"].as_str().unwrap_or("").to_string()
}

fn install_click_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-click-fixture');
  if (existing) existing.remove();

  window.__ab_clicks = 0;
  window.__ab_dblclicks = 0;
  window.__ab_right_clicks = 0;

  const root = document.createElement('div');
  root.id = 'ab-click-fixture';
  root.innerHTML = `
    <style>
      #ab-click-btn, #ab-link, #ab-right-target {
        position: fixed;
        left: 40px;
        width: 180px;
        height: 36px;
        z-index: 2147483647;
      }
      #ab-click-btn { top: 40px; }
      #ab-link {
        top: 100px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: #ffedd5;
        color: #111827;
      }
      #ab-right-target {
        top: 160px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: #e5e7eb;
      }
    </style>
    <button id="ab-click-btn" type="button">Click target</button>
    <a id="ab-link" href="https://example.org/#ab-click-target">Open link</a>
    <div id="ab-right-target" tabindex="0">Right click target</div>
  `;
  document.body.appendChild(root);

  const btn = document.getElementById('ab-click-btn');
  btn.addEventListener('click', () => {
    window.__ab_clicks += 1;
    document.body.setAttribute('data-clicks', String(window.__ab_clicks));
  });
  btn.addEventListener('dblclick', () => {
    window.__ab_dblclicks += 1;
    document.body.setAttribute('data-dblclicks', String(window.__ab_dblclicks));
  });

  const rightTarget = document.getElementById('ab-right-target');
  rightTarget.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    window.__ab_right_clicks += 1;
    document.body.setAttribute('data-right-clicks', String(window.__ab_right_clicks));
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "fixture should install successfully");
}

fn list_tabs(session_id: &str) -> serde_json::Value {
    let out = headless_json(&["browser", "list-tabs", "--session", session_id], 15);
    assert_success(&out, "list-tabs");
    parse_json(&out)
}

// ========================================================================
// Group 1: click — basic success path
// ========================================================================

#[test]
fn click_selector_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &["browser", "click", "#ab-click-btn", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "click selector json");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-click-btn"));
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_clicks)"), "1");

    close_session(&sid);
}

#[test]
fn click_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless(
        &["browser", "click", "#ab-click-btn", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "click text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.click"),
        "must contain ok browser.click"
    );
    assert!(
        text.contains("target: #ab-click-btn"),
        "must contain target line with selector"
    );

    close_session(&sid);
}

#[test]
fn click_coordinates_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &["browser", "click", "60,60", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "click coordinates json");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, None);
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_clicks)"), "1");

    close_session(&sid);
}

// ========================================================================
// Group 2: click — option flags
// ========================================================================

#[test]
fn click_count_two_triggers_double_click() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-click-btn",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--count",
            "2",
        ],
        15,
    );
    assert_success(&out, "click count=2");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-click-btn"));
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_dblclicks)"), "1");

    close_session(&sid);
}

#[test]
fn click_right_button_dispatches_contextmenu() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-right-target",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--button",
            "right",
        ],
        15,
    );
    assert_success(&out, "click button=right");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-right-target"));
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_right_clicks)"), "1");

    close_session(&sid);
}

#[test]
fn click_new_tab_opens_link_in_new_tab() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-link",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--new-tab",
        ],
        30,
    );
    assert_success(&out, "click new-tab");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-link"));
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert!(
        eval_value(&sid, &tid, "location.href").contains("example.com"),
        "current tab should stay on the original page when --new-tab is used"
    );

    let tabs = list_tabs(&sid);
    assert!(
        tabs["data"]["total_tabs"].as_u64().unwrap_or(0) >= 2,
        "new-tab click should create another tab"
    );
    let tabs = tabs["data"]["tabs"].as_array().expect("tabs array");
    let any_new_tab = tabs
        .iter()
        .any(|tab| tab["url"].as_str().unwrap_or("").contains("example.org"));
    assert!(any_new_tab, "one tab should load the clicked link URL");

    close_session(&sid);
}

// ========================================================================
// Group 3: click — navigation semantics
// ========================================================================

#[test]
fn click_navigation_updates_context_url() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &["browser", "click", "#ab-link", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "click navigation");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-link"));
    assert_eq!(v["data"]["changed"]["url_changed"], true);
    assert!(
        v["context"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("example.org"),
        "context.url must update to the post-navigation URL"
    );
    assert!(
        v["context"]["title"].is_string(),
        "context.title should be returned after navigation when known"
    );

    close_session(&sid);
}

// ========================================================================
// Group 4: click — error path
// ========================================================================

#[test]
fn click_missing_selector_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session(TEST_URL);

    let out = headless_json(
        &[
            "browser",
            "click",
            "#definitely-missing-element",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "click missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.click");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(
        v["error"]["details"]["selector"],
        "#definitely-missing-element"
    );

    close_session(&sid);
}
