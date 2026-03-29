//! Browser interaction E2E tests: browser click, type, fill, select.
//!
//! This file groups interaction commands together, similar to navigation.rs.
//! The current coverage here is for `browser click`, `browser type`,
//! `browser fill`, and `browser select`,
//! per api-reference.md §11.

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

fn assert_type_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
    expected_text_length: u64,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.type");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "type");
    assert_eq!(data["target"]["selector"], expected_selector);
    assert_eq!(data["value_summary"]["text_length"], expected_text_length);

    assert_meta(v);
}

fn assert_fill_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
    expected_text_length: u64,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.fill");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "fill");
    assert_eq!(data["target"]["selector"], expected_selector);
    assert_eq!(data["value_summary"]["text_length"], expected_text_length);

    assert_meta(v);
}

fn assert_select_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
    expected_value: &str,
    expected_by_text: bool,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.select");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "select");
    assert_eq!(data["target"]["selector"], expected_selector);
    assert_eq!(data["value_summary"]["value"], expected_value);
    assert_eq!(data["value_summary"]["by_text"], expected_by_text);

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
  window.__ab_middle_clicks = 0;
  window.__ab_right_clicks = 0;
  window.__ab_last_click_button = -1;

  const root = document.createElement('div');
  root.id = 'ab-click-fixture';
  root.innerHTML = `
    <style>
      #ab-click-btn, #ab-link, #ab-right-target, #ab-middle-target {
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
      #ab-middle-target {
        top: 220px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: #dbeafe;
      }
    </style>
    <button id="ab-click-btn" type="button">Click target</button>
    <a id="ab-link" href="https://example.org/#ab-click-target">Open link</a>
    <div id="ab-right-target" tabindex="0">Right click target</div>
    <div id="ab-middle-target" tabindex="0">Middle click target</div>
  `;
  document.body.appendChild(root);

  const btn = document.getElementById('ab-click-btn');
  btn.addEventListener('click', (event) => {
    window.__ab_clicks += 1;
    window.__ab_last_click_button = event.button;
    document.body.setAttribute('data-clicks', String(window.__ab_clicks));
    document.body.setAttribute('data-last-click-button', String(window.__ab_last_click_button));
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

  const middleTarget = document.getElementById('ab-middle-target');
  middleTarget.addEventListener('auxclick', (event) => {
    if (event.button === 1) {
      window.__ab_middle_clicks += 1;
      document.body.setAttribute('data-middle-clicks', String(window.__ab_middle_clicks));
    }
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "fixture should install successfully");
}

fn install_type_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-type-fixture');
  if (existing) existing.remove();

  window.__ab_type_keydown_count = 0;
  window.__ab_type_input_count = 0;
  window.__ab_type_keyup_count = 0;
  window.__ab_type_events = [];

  const root = document.createElement('div');
  root.id = 'ab-type-fixture';
  root.innerHTML = `
    <style>
      #ab-type-input {
        position: fixed;
        top: 280px;
        left: 40px;
        width: 240px;
        height: 36px;
        z-index: 2147483647;
      }
    </style>
    <input id="ab-type-input" type="text" value="seed-" />
  `;
  document.body.appendChild(root);

  const input = document.getElementById('ab-type-input');
  input.addEventListener('keydown', (event) => {
    window.__ab_type_keydown_count += 1;
    window.__ab_type_events.push('keydown:' + event.key);
  });
  input.addEventListener('input', () => {
    window.__ab_type_input_count += 1;
    window.__ab_type_events.push('input:' + input.value);
  });
  input.addEventListener('keyup', (event) => {
    window.__ab_type_keyup_count += 1;
    window.__ab_type_events.push('keyup:' + event.key);
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "type fixture should install successfully");
}

fn install_fill_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-fill-fixture');
  if (existing) existing.remove();

  window.__ab_fill_keydown_count = 0;
  window.__ab_fill_input_count = 0;
  window.__ab_fill_keyup_count = 0;
  window.__ab_fill_change_count = 0;
  window.__ab_fill_events = [];

  const root = document.createElement('div');
  root.id = 'ab-fill-fixture';
  root.innerHTML = `
    <style>
      #ab-fill-input, #ab-fill-textarea {
        position: fixed;
        left: 40px;
        width: 240px;
        height: 36px;
        z-index: 2147483647;
      }
      #ab-fill-input {
        top: 340px;
      }
      #ab-fill-textarea {
        top: 390px;
        height: 72px;
      }
    </style>
    <input id="ab-fill-input" type="text" value="seed-" />
    <textarea id="ab-fill-textarea">seed-area</textarea>
  `;
  document.body.appendChild(root);

  const attachFillListeners = (el, label) => {
    el.addEventListener('keydown', (event) => {
      window.__ab_fill_keydown_count += 1;
      window.__ab_fill_events.push(label + ':keydown:' + event.key);
    });
    el.addEventListener('input', () => {
      window.__ab_fill_input_count += 1;
      window.__ab_fill_events.push(label + ':input:' + el.value);
    });
    el.addEventListener('keyup', (event) => {
      window.__ab_fill_keyup_count += 1;
      window.__ab_fill_events.push(label + ':keyup:' + event.key);
    });
    el.addEventListener('change', () => {
      window.__ab_fill_change_count += 1;
      window.__ab_fill_events.push(label + ':change:' + el.value);
    });
  };

  attachFillListeners(document.getElementById('ab-fill-input'), 'input');
  attachFillListeners(document.getElementById('ab-fill-textarea'), 'textarea');

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "fill fixture should install successfully");
}

fn install_select_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-select-fixture');
  if (existing) existing.remove();

  window.__ab_select_input_count = 0;
  window.__ab_select_change_count = 0;
  window.__ab_select_events = [];

  const root = document.createElement('div');
  root.id = 'ab-select-fixture';
  root.innerHTML = `
    <style>
      #ab-select {
        position: fixed;
        top: 480px;
        left: 40px;
        width: 240px;
        height: 36px;
        z-index: 2147483647;
      }
    </style>
    <select id="ab-select">
      <option value="apple" selected>Apple</option>
      <option value="banana">Banana</option>
      <option value="citrus">Citrus Fruit</option>
    </select>
  `;
  document.body.appendChild(root);

  const select = document.getElementById('ab-select');
  select.addEventListener('input', () => {
    window.__ab_select_input_count += 1;
    window.__ab_select_events.push('input:' + select.value);
  });
  select.addEventListener('change', () => {
    window.__ab_select_change_count += 1;
    window.__ab_select_events.push('change:' + select.value);
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "select fixture should install successfully");
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
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
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
        ],
        15,
    );
    assert_success(&out, "click selector json");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-click-btn"));
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_clicks)"), "1");
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_last_click_button)"),
        "0"
    );

    close_session(&sid);
}

#[test]
fn click_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_click_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "click",
            "#ab-click-btn",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
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
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "60,60",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "click coordinates json");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, None);
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_clicks)"), "1");

    close_session(&sid);
}

#[test]
fn click_coordinates_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_click_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "click",
            "60,60",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "click coordinates text");
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
        text.contains("target: 60,60"),
        "must contain target line with coordinates"
    );

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
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
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
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
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
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_right_clicks)"),
        "1"
    );

    close_session(&sid);
}

#[test]
fn click_middle_button_dispatches_auxclick() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-middle-target",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--button",
            "middle",
        ],
        15,
    );
    assert_success(&out, "click button=middle");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, Some("#ab-middle-target"));
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_middle_clicks)"),
        "1"
    );

    close_session(&sid);
}

#[test]
fn click_new_tab_opens_link_in_new_tab() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
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

#[test]
fn click_new_tab_coordinates_without_href_does_not_open_new_tab() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_click_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "60,60",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--new-tab",
        ],
        15,
    );
    assert_success(&out, "click new-tab coordinates without href");
    let v = parse_json(&out);

    assert_click_success(&v, &sid, &tid, None);
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(eval_value(&sid, &tid, "String(window.__ab_clicks)"), "1");

    let tabs = list_tabs(&sid);
    assert_eq!(tabs["data"]["total_tabs"], serde_json::json!(1));

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
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
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
        ],
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
fn click_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-click-btn",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "click nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.click");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn click_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "click",
            "#ab-click-btn",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "click nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn click_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-click-btn",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "click nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.click");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn click_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "click",
            "#ab-click-btn",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "click nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn click_missing_selector_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

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

#[test]
fn click_missing_selector_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
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
    assert_failure(&out, "click missing selector text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error ELEMENT_NOT_FOUND:"),
        "text must contain error ELEMENT_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn click_invalid_coordinates_json() {
    if skip() {
        return;
    }

    // These cases are intentionally coordinate-like but malformed, so they
    // should be rejected as invalid coordinates rather than treated as selectors.
    for target in ["10,", ",10", "10,abc", ",,,"] {
        let (sid, tid) = start_session(TEST_URL);
        let out = headless_json(
            &["browser", "click", target, "--session", &sid, "--tab", &tid],
            10,
        );
        assert_failure(&out, &format!("click invalid coordinates json: {target}"));
        let v = parse_json(&out);

        assert_eq!(v["command"], "browser.click");
        assert_error_envelope(&v, "INVALID_ARGUMENT");
        assert!(
            v["context"].is_object(),
            "context must be present when session and tab are valid"
        );
        assert_eq!(v["context"]["session_id"], sid);
        assert_eq!(v["context"]["tab_id"], tid);

        close_session(&sid);
    }
}

// ========================================================================
// Group 5: type — basic success path
// ========================================================================

#[test]
fn type_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_type_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "type",
            "#ab-type-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "type json");
    let v = parse_json(&out);

    assert_type_success(&v, &sid, &tid, "#ab-type-input", 3);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-type-input').value"),
        "seed-abc"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_type_keydown_count)"),
        "3"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_type_input_count)"),
        "3"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_type_keyup_count)"),
        "3"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.activeElement && document.activeElement.id"
        ),
        "ab-type-input"
    );

    close_session(&sid);
}

#[test]
fn type_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_type_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "type",
            "#ab-type-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "type text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.type"),
        "must contain ok browser.type"
    );
    assert!(
        text.contains("target: #ab-type-input"),
        "must contain target line with selector"
    );
    assert!(
        text.contains("text_length: 3"),
        "must contain text_length: 3"
    );

    close_session(&sid);
}

#[test]
fn type_with_spaces_and_punctuation_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_type_fixture(&sid, &tid);
    let typed_text = "Hello, world!";

    let out = headless_json(
        &[
            "browser",
            "type",
            "#ab-type-input",
            typed_text,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "type with spaces json");
    let v = parse_json(&out);

    assert_type_success(&v, &sid, &tid, "#ab-type-input", typed_text.len() as u64);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-type-input').value"),
        format!("seed-{typed_text}")
    );

    close_session(&sid);
}

// ========================================================================
// Group 6: type — error paths
// ========================================================================

#[test]
fn type_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "type",
            "#ab-type-input",
            "abc",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "type nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.type");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn type_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "type",
            "#ab-type-input",
            "abc",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "type nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn type_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "type",
            "#ab-type-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "type nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.type");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn type_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "type",
            "#ab-type-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "type nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn type_missing_selector_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "type",
            "#definitely-missing-element",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "type missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.type");
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

#[test]
fn type_missing_selector_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "type",
            "#definitely-missing-element",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "type missing selector text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error ELEMENT_NOT_FOUND:"),
        "text must contain error ELEMENT_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 7: fill — basic success path
// ========================================================================

#[test]
fn fill_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_fill_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "fill json");
    let v = parse_json(&out);

    assert_fill_success(&v, &sid, &tid, "#ab-fill-input", 3);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-fill-input').value"),
        "abc"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_input_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_keydown_count)"),
        "0"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_keyup_count)"),
        "0"
    );

    close_session(&sid);
}

#[test]
fn fill_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_fill_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "fill text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.fill"),
        "must contain ok browser.fill"
    );
    assert!(
        text.contains("target: #ab-fill-input"),
        "must contain target line with selector"
    );
    assert!(
        text.contains("text_length: 3"),
        "must contain text_length: 3"
    );

    close_session(&sid);
}

#[test]
fn fill_textarea_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_fill_fixture(&sid, &tid);
    let fill_text = "textarea value";

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#ab-fill-textarea",
            fill_text,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "fill textarea json");
    let v = parse_json(&out);

    assert_fill_success(&v, &sid, &tid, "#ab-fill-textarea", fill_text.len() as u64);
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.querySelector('#ab-fill-textarea').value"
        ),
        fill_text
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_input_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_keydown_count)"),
        "0"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_keyup_count)"),
        "0"
    );

    close_session(&sid);
}

#[test]
fn fill_replaces_existing_value_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_fill_fixture(&sid, &tid);
    let fill_text = "Hello, world!";

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            fill_text,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "fill replaces value json");
    let v = parse_json(&out);

    assert_fill_success(&v, &sid, &tid, "#ab-fill-input", fill_text.len() as u64);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-fill-input').value"),
        fill_text
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_fill_input_count)"),
        "1"
    );

    close_session(&sid);
}

// ========================================================================
// Group 8: fill — error paths
// ========================================================================

#[test]
fn fill_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            "abc",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "fill nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.fill");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn fill_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            "abc",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "fill nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn fill_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "fill nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.fill");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn fill_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "fill",
            "#ab-fill-input",
            "abc",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "fill nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn fill_missing_selector_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#definitely-missing-element",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "fill missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.fill");
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

#[test]
fn fill_missing_selector_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "fill",
            "#definitely-missing-element",
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "fill missing selector text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error ELEMENT_NOT_FOUND:"),
        "text must contain error ELEMENT_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 9: select — basic success path
// ========================================================================

#[test]
fn select_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_select_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-select",
            "banana",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "select json");
    let v = parse_json(&out);

    assert_select_success(&v, &sid, &tid, "#ab-select", "banana", false);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-select').value"),
        "banana"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.querySelector('#ab-select').selectedOptions[0].textContent.trim()"
        ),
        "Banana"
    );

    close_session(&sid);
}

#[test]
fn select_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_select_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "select",
            "#ab-select",
            "banana",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "select text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser.select"),
        "must contain ok browser.select"
    );
    assert!(
        text.contains("target: #ab-select"),
        "must contain target line with selector"
    );
    assert!(
        text.contains("value: banana"),
        "must contain selected value"
    );
    assert!(
        text.contains("by_text: false"),
        "must contain by_text: false"
    );

    close_session(&sid);
}

#[test]
fn select_by_text_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_select_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-select",
            "Citrus Fruit",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--by-text",
        ],
        15,
    );
    assert_success(&out, "select by-text json");
    let v = parse_json(&out);

    assert_select_success(&v, &sid, &tid, "#ab-select", "Citrus Fruit", true);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-select').value"),
        "citrus"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.querySelector('#ab-select').selectedOptions[0].textContent.trim()"
        ),
        "Citrus Fruit"
    );

    close_session(&sid);
}

// ========================================================================
// Group 10: select — error paths
// ========================================================================

#[test]
fn select_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-select",
            "banana",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "select nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.select");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn select_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "select",
            "#ab-select",
            "banana",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "select nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn select_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-select",
            "banana",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "select nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.select");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn select_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "select",
            "#ab-select",
            "banana",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "select nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn select_missing_selector_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "select",
            "#definitely-missing-element",
            "banana",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "select missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.select");
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

#[test]
fn select_missing_selector_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "select",
            "#definitely-missing-element",
            "banana",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "select missing selector text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error ELEMENT_NOT_FOUND:"),
        "text must contain error ELEMENT_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}
