//! Browser interaction E2E tests: browser click, type, fill, select.
//!
//! This file groups interaction commands together, similar to navigation.rs.
//! The current coverage here is for `browser click`, `browser type`,
//! `browser fill`, and `browser select`,
//! per api-reference.md §11.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, wait_page_ready,
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
    assert_eq!(v["command"], "browser click");
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
    assert_eq!(v["command"], "browser type");
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

fn assert_type_success_coordinates(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_coordinates: &str,
    expected_text_length: u64,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser type");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "type");
    assert_eq!(data["target"]["coordinates"], expected_coordinates);
    assert_eq!(data["value_summary"]["text_length"], expected_text_length);

    assert_meta(v);
}

fn assert_type_success_no_selector(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_text_length: u64,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser type");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "type");
    assert_eq!(data["value_summary"]["text_length"], expected_text_length);
    assert!(
        data.pointer("/target/selector").is_none(),
        "selector-less type should not report a selector target"
    );

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
    assert_eq!(v["command"], "browser fill");
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

fn assert_fill_success_coordinates(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_coordinates: &str,
    expected_text_length: u64,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser fill");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "fill");
    assert_eq!(data["target"]["coordinates"], expected_coordinates);
    assert_eq!(data["value_summary"]["text_length"], expected_text_length);

    assert_meta(v);
}

fn assert_fill_success_no_selector(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_text_length: u64,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser fill");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "fill");
    assert_eq!(data["value_summary"]["text_length"], expected_text_length);
    assert!(
        data.pointer("/target/selector").is_none(),
        "selector-less fill should not report a selector target"
    );

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
    assert_select_success_full(
        v,
        session_id,
        tab_id,
        expected_selector,
        expected_value,
        expected_by_text,
        false,
    );
}

fn assert_select_success_full(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
    expected_value: &str,
    expected_by_text: bool,
    expected_by_ref: bool,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser select");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "select");
    assert_eq!(data["target"]["selector"], expected_selector);
    assert_eq!(data["value_summary"]["value"], expected_value);
    assert_eq!(data["value_summary"]["by_text"], expected_by_text);
    assert_eq!(data["value_summary"]["by_ref"], expected_by_ref);

    assert_meta(v);
}

fn assert_hover_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser hover");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "hover");
    assert_eq!(data["target"]["selector"], expected_selector);
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

fn assert_focus_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser focus");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "focus");
    assert_eq!(data["target"]["selector"], expected_selector);
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

fn assert_press_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_keys: &str,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser press");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "press");
    assert_eq!(data["keys"], expected_keys);
    assert!(
        data.get("target").is_none() || data["target"].is_null(),
        "press should not require a target in the response when keys are used"
    );
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

fn assert_drag_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_source: &str,
    expected_destination_selector: Option<&str>,
    expected_destination_coordinates: Option<&str>,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser drag");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "drag");
    assert_eq!(data["target"]["selector"], expected_source);
    if let Some(selector) = expected_destination_selector {
        assert_eq!(data["destination"]["selector"], selector);
    }
    if let Some(coords) = expected_destination_coordinates {
        assert_eq!(data["destination"]["coordinates"], coords);
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

fn assert_mouse_move_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_coordinates: &str,
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser mouse-move");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "mouse-move");
    assert_eq!(data["target"]["coordinates"], expected_coordinates);
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

fn assert_upload_success(
    v: &serde_json::Value,
    session_id: &str,
    tab_id: &str,
    expected_selector: &str,
    expected_files: &[String],
) {
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser upload");
    assert!(v["error"].is_null(), "error must be null on success");

    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], session_id);
    assert_eq!(v["context"]["tab_id"], tab_id);

    let data = &v["data"];
    assert_eq!(data["action"], "upload");
    assert_eq!(data["target"]["selector"], expected_selector);
    assert_eq!(data["value_summary"]["count"], expected_files.len());

    let files = data["value_summary"]["files"]
        .as_array()
        .expect("value_summary.files must be an array");
    let actual: Vec<String> = files
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(actual, expected_files);

    assert_meta(v);
}

fn start_session(url: &str) -> (String, String) {
    let (sid, profile) = unique_session("s");
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            &sid,
            "--profile",
            &profile,
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
    wait_page_ready(&sid, &tid);
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

fn install_hover_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-hover-fixture');
  if (existing) existing.remove();

  window.__ab_hover_enter_count = 0;
  window.__ab_hover_over_count = 0;
  window.__ab_hover_move_count = 0;
  window.__ab_hover_last_target = '';

  const root = document.createElement('div');
  root.id = 'ab-hover-fixture';
  root.innerHTML = `
    <style>
      #ab-hover-target {
        position: fixed;
        top: 540px;
        left: 40px;
        width: 220px;
        height: 44px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: #d1fae5;
        color: #111827;
        z-index: 2147483647;
      }
    </style>
    <div id="ab-hover-target">Hover target</div>
  `;
  document.body.appendChild(root);

  const target = document.getElementById('ab-hover-target');
  target.addEventListener('mouseenter', () => {
    window.__ab_hover_enter_count += 1;
    window.__ab_hover_last_target = target.id;
    document.body.setAttribute('data-hover-enter', String(window.__ab_hover_enter_count));
  });
  target.addEventListener('mouseover', () => {
    window.__ab_hover_over_count += 1;
    window.__ab_hover_last_target = target.id;
    document.body.setAttribute('data-hover-over', String(window.__ab_hover_over_count));
  });
  target.addEventListener('mousemove', () => {
    window.__ab_hover_move_count += 1;
    window.__ab_hover_last_target = target.id;
    document.body.setAttribute('data-hover-move', String(window.__ab_hover_move_count));
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "hover fixture should install successfully");
}

fn install_focus_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-focus-fixture');
  if (existing) existing.remove();

  window.__ab_focus_target_count = 0;
  window.__ab_focus_other_count = 0;
  window.__ab_blur_other_count = 0;
  window.__ab_last_focused = '';

  const root = document.createElement('div');
  root.id = 'ab-focus-fixture';
  root.innerHTML = `
    <style>
      #ab-focus-other, #ab-focus-target {
        position: fixed;
        left: 40px;
        width: 220px;
        height: 40px;
        z-index: 2147483647;
      }
      #ab-focus-other {
        top: 600px;
      }
      #ab-focus-target {
        top: 650px;
      }
    </style>
    <input id="ab-focus-other" type="text" value="other" />
    <button id="ab-focus-target" type="button">Focus target</button>
  `;
  document.body.appendChild(root);

  const other = document.getElementById('ab-focus-other');
  const target = document.getElementById('ab-focus-target');

  other.addEventListener('focus', () => {
    window.__ab_focus_other_count += 1;
    window.__ab_last_focused = other.id;
  });
  other.addEventListener('blur', () => {
    window.__ab_blur_other_count += 1;
  });
  target.addEventListener('focus', () => {
    window.__ab_focus_target_count += 1;
    window.__ab_last_focused = target.id;
  });

  other.focus();
  return document.activeElement && document.activeElement.id;
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(
        value, "ab-focus-other",
        "focus fixture should install successfully"
    );
}

fn install_press_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-press-fixture');
  if (existing) existing.remove();

  window.__ab_press_keydown_count = 0;
  window.__ab_press_keyup_count = 0;
  window.__ab_press_events = [];
  window.__ab_press_key_defs = [];

  const root = document.createElement('div');
  root.id = 'ab-press-fixture';
  root.innerHTML = `
    <style>
      #ab-press-input {
        position: fixed;
        top: 710px;
        left: 40px;
        width: 260px;
        height: 40px;
        z-index: 2147483647;
      }
    </style>
    <input id="ab-press-input" type="text" value="seed" />
  `;
  document.body.appendChild(root);

  const input = document.getElementById('ab-press-input');
  input.addEventListener('keydown', (event) => {
    window.__ab_press_keydown_count += 1;
    window.__ab_press_events.push(
      'keydown:' + event.key + ':' + event.ctrlKey + ':' + event.shiftKey
    );
    window.__ab_press_key_defs.push({
      type: 'keydown', key: event.key, code: event.code, keyCode: event.keyCode
    });
  });
  input.addEventListener('keyup', (event) => {
    window.__ab_press_keyup_count += 1;
    window.__ab_press_events.push(
      'keyup:' + event.key + ':' + event.ctrlKey + ':' + event.shiftKey
    );
  });

  input.focus();
  return document.activeElement && document.activeElement.id;
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(
        value, "ab-press-input",
        "press fixture should install successfully"
    );
}

fn install_drag_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-drag-fixture');
  if (existing) existing.remove();

  window.__ab_drag_mousemove_count = 0;
  window.__ab_drag_drop_count = 0;
  window.__ab_drag_last_drop = '';
  window.__ab_drag_state = 'idle';

  const root = document.createElement('div');
  root.id = 'ab-drag-fixture';
  root.innerHTML = `
    <style>
      #ab-drag-source, #ab-drag-target {
        position: fixed;
        top: 780px;
        width: 110px;
        height: 48px;
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 2147483647;
        user-select: none;
      }
      #ab-drag-source {
        left: 40px;
        background: #bfdbfe;
      }
      #ab-drag-target {
        left: 260px;
        background: #bbf7d0;
      }
    </style>
    <div id="ab-drag-source">Drag source</div>
    <div id="ab-drag-target">Drop target</div>
  `;
  document.body.appendChild(root);

  const source = document.getElementById('ab-drag-source');
  const target = document.getElementById('ab-drag-target');

  const finishDrop = (label) => {
    if (window.__ab_drag_state !== 'dragging') return;
    window.__ab_drag_drop_count += 1;
    window.__ab_drag_last_drop = label;
    window.__ab_drag_state = 'idle';
  };

  source.addEventListener('mousedown', () => {
    window.__ab_drag_state = 'dragging';
  });

  document.addEventListener('mousemove', () => {
    if (window.__ab_drag_state === 'dragging') {
      window.__ab_drag_mousemove_count += 1;
    }
  }, true);

  target.addEventListener('mouseup', () => {
    finishDrop('ab-drag-target');
  });

  document.addEventListener('mouseup', (event) => {
    if (window.__ab_drag_state !== 'dragging') return;
    const el = document.elementFromPoint(event.clientX, event.clientY);
    if (el && el.id === 'ab-drag-target') {
      finishDrop('ab-drag-target');
    } else {
      window.__ab_drag_state = 'idle';
    }
  }, true);

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "drag fixture should install successfully");
}

fn install_upload_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-upload-fixture');
  if (existing) existing.remove();

  window.__ab_upload_change_count = 0;
  window.__ab_upload_last_count = 0;
  window.__ab_upload_last_names = '';

  const root = document.createElement('div');
  root.id = 'ab-upload-fixture';
  root.innerHTML = `
    <style>
      #ab-upload-input {
        position: fixed;
        top: 840px;
        left: 40px;
        width: 260px;
        z-index: 2147483647;
      }
    </style>
    <input id="ab-upload-input" type="file" multiple />
  `;
  document.body.appendChild(root);

  const input = document.getElementById('ab-upload-input');
  input.addEventListener('change', () => {
    window.__ab_upload_change_count += 1;
    window.__ab_upload_last_count = input.files.length;
    window.__ab_upload_last_names = Array.from(input.files).map(file => file.name).join(',');
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "upload fixture should install successfully");
}

fn install_mouse_move_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-mouse-move-fixture');
  if (existing) existing.remove();

  window.__ab_mouse_move_count = 0;
  window.__ab_mouse_move_last_target = '';
  window.__ab_mouse_move_last_coords = '';

  const root = document.createElement('div');
  root.id = 'ab-mouse-move-fixture';
  root.innerHTML = `
    <style>
      #ab-mouse-move-target {
        position: fixed;
        top: 120px;
        left: 90px;
        width: 180px;
        height: 60px;
        background: #fde68a;
        z-index: 2147483647;
      }
    </style>
    <div id="ab-mouse-move-target">Move target</div>
  `;
  document.body.appendChild(root);

  const target = document.getElementById('ab-mouse-move-target');
  target.addEventListener('mousemove', (event) => {
    window.__ab_mouse_move_count += 1;
    window.__ab_mouse_move_last_target = target.id;
    window.__ab_mouse_move_last_coords = `${event.clientX},${event.clientY}`;
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(
        value, "ok",
        "mouse-move fixture should install successfully"
    );
}

fn install_scroll_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-scroll-fixture');
  if (existing) existing.remove();

  window.scrollTo(0, 0);
  document.body.innerHTML = '';
  document.documentElement.style.height = '';
  document.body.style.height = '';
  document.body.style.margin = '0';

  const root = document.createElement('div');
  root.id = 'ab-scroll-fixture';
  root.innerHTML = `
    <style>
      body {
        min-height: 2600px;
      }
      #ab-scroll-target {
        margin-top: 1400px;
        height: 60px;
        background: #bfdbfe;
      }
      #ab-scroll-container {
        position: fixed;
        top: 120px;
        left: 40px;
        width: 260px;
        height: 120px;
        overflow: auto;
        border: 1px solid #111827;
        background: #f8fafc;
        z-index: 2147483647;
      }
      #ab-scroll-container-inner {
        width: 600px;
        height: 700px;
        position: relative;
      }
      #ab-scroll-container-target {
        position: absolute;
        top: 560px;
        left: 20px;
        width: 180px;
        height: 40px;
        background: #bbf7d0;
      }
    </style>
    <div id="ab-scroll-container">
      <div id="ab-scroll-container-inner">
        <div id="ab-scroll-container-target">Container target</div>
      </div>
    </div>
    <div id="ab-scroll-target">Page target</div>
  `;
  document.body.appendChild(root);
  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "scroll fixture should install successfully");
}

fn create_upload_files(names: &[&str]) -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().expect("create upload temp dir");
    let mut paths = Vec::new();
    for name in names {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("fixture for {name}\n")).expect("write upload fixture file");
        paths.push(path.to_string_lossy().to_string());
    }
    (dir, paths)
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
        text.contains("ok browser click"),
        "must contain ok browser click"
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
        text.contains("ok browser click"),
        "must contain ok browser click"
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

    assert_eq!(v["command"], "browser click");
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

    assert_eq!(v["command"], "browser click");
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

    assert_eq!(v["command"], "browser click");
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

        assert_eq!(v["command"], "browser click");
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
        text.contains("ok browser type"),
        "must contain ok browser type"
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

#[test]
fn type_coordinates_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_type_fixture(&sid, &tid);
    let coords = "60,290";

    let out = headless_json(
        &[
            "browser",
            "type",
            coords,
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "type coordinates json");
    let v = parse_json(&out);

    assert_type_success_coordinates(&v, &sid, &tid, coords, 3);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-type-input').value"),
        "seed-abc"
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
fn type_no_selector_uses_active_element_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_type_fixture(&sid, &tid);

    let click_out = headless_json(
        &[
            "browser",
            "click",
            "60,290",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&click_out, "focus type input by coordinates");
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.activeElement && document.activeElement.id"
        ),
        "ab-type-input"
    );

    let out = headless_json(
        &["browser", "type", "abc", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "type with activeElement json");
    let v = parse_json(&out);

    assert_type_success_no_selector(&v, &sid, &tid, 3);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-type-input').value"),
        "seed-abc"
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

    assert_eq!(v["command"], "browser type");
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

    assert_eq!(v["command"], "browser type");
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

    assert_eq!(v["command"], "browser type");
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
        text.contains("ok browser fill"),
        "must contain ok browser fill"
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

#[test]
fn fill_coordinates_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_fill_fixture(&sid, &tid);
    let coords = "60,350";

    let out = headless_json(
        &[
            "browser",
            "fill",
            coords,
            "abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "fill coordinates json");
    let v = parse_json(&out);

    assert_fill_success_coordinates(&v, &sid, &tid, coords, 3);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-fill-input').value"),
        "abc"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.activeElement && document.activeElement.id"
        ),
        "ab-fill-input"
    );

    close_session(&sid);
}

#[test]
fn fill_no_selector_uses_active_element_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_fill_fixture(&sid, &tid);

    let click_out = headless_json(
        &[
            "browser",
            "click",
            "60,350",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&click_out, "focus fill input by coordinates");
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.activeElement && document.activeElement.id"
        ),
        "ab-fill-input"
    );

    let out = headless_json(
        &["browser", "fill", "abc", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "fill with activeElement json");
    let v = parse_json(&out);

    assert_fill_success_no_selector(&v, &sid, &tid, 3);
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-fill-input').value"),
        "abc"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "document.activeElement && document.activeElement.id"
        ),
        "ab-fill-input"
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

    assert_eq!(v["command"], "browser fill");
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

    assert_eq!(v["command"], "browser fill");
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

    assert_eq!(v["command"], "browser fill");
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
        text.contains("ok browser select"),
        "must contain ok browser select"
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

/// Run snapshot and find the ref for an option with the given name.
fn snapshot_option_ref(session_id: &str, tab_id: &str, option_name: &str) -> String {
    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            session_id,
            "--tab",
            tab_id,
        ],
        15,
    );
    assert_success(&out, "snapshot for option ref");
    let v = parse_json(&out);
    let nodes = v["data"]["nodes"]
        .as_array()
        .expect("snapshot nodes must be an array");
    for node in nodes {
        let name = node["name"].as_str().unwrap_or("");
        let role = node["role"].as_str().unwrap_or("");
        if role == "option" && name == option_name {
            let r = node["ref"].as_str().unwrap();
            return format!("@{r}");
        }
    }
    panic!("option ref not found for name='{option_name}' in snapshot nodes");
}

#[test]
fn select_by_ref_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_select_fixture(&sid, &tid);

    // Run snapshot to populate RefCache, then find the ref for "Banana" option
    let banana_ref = snapshot_option_ref(&sid, &tid, "Banana");

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-select",
            &banana_ref,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--by-ref",
        ],
        15,
    );
    assert_success(&out, "select by-ref json");
    let v = parse_json(&out);

    assert_select_success_full(&v, &sid, &tid, "#ab-select", &banana_ref, false, true);
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
fn select_by_ref_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_select_fixture(&sid, &tid);

    let citrus_ref = snapshot_option_ref(&sid, &tid, "Citrus Fruit");

    let out = headless(
        &[
            "browser",
            "select",
            "#ab-select",
            &citrus_ref,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--by-ref",
        ],
        15,
    );
    assert_success(&out, "select by-ref text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser select"),
        "must contain ok browser select"
    );
    assert!(
        text.contains("target: #ab-select"),
        "must contain target line with selector"
    );
    assert!(
        text.contains(&format!("value: {citrus_ref}")),
        "must contain ref value"
    );
    assert!(text.contains("by_ref: true"), "must contain by_ref: true");

    // Verify the option was actually selected
    assert_eq!(
        eval_value(&sid, &tid, "document.querySelector('#ab-select').value"),
        "citrus"
    );

    close_session(&sid);
}

#[test]
fn select_by_ref_and_by_text_mutually_exclusive() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-select",
            "@e1",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--by-ref",
            "--by-text",
        ],
        15,
    );
    assert_failure(&out, "select by-ref + by-text json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser select");
    assert_error_envelope(&v, "INVALID_ARGUMENT");

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

    assert_eq!(v["command"], "browser select");
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

    assert_eq!(v["command"], "browser select");
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

    assert_eq!(v["command"], "browser select");
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

// ========================================================================
// Group 13: hover — command wiring, success path, and error path
// ========================================================================

#[test]
fn hover_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_hover_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "hover",
            "#ab-hover-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "hover json");
    let v = parse_json(&out);

    assert_hover_success(&v, &sid, &tid, "#ab-hover-target");
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_hover_enter_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_hover_over_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_hover_move_count !== 0)"),
        "true"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_hover_last_target"),
        "ab-hover-target"
    );

    close_session(&sid);
}

#[test]
fn hover_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_hover_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "hover",
            "#ab-hover-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "hover text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser hover"),
        "must contain ok browser hover"
    );
    assert!(
        text.contains("target: #ab-hover-target"),
        "must contain target line with selector"
    );

    close_session(&sid);
}

#[test]
fn hover_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "hover",
            "#ab-hover-target",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "hover nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser hover");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn hover_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "hover",
            "#ab-hover-target",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "hover nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn hover_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "hover",
            "#ab-hover-target",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "hover nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser hover");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn hover_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "hover",
            "#ab-hover-target",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "hover nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn hover_missing_selector_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "hover",
            "#definitely-missing-element",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "hover missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser hover");
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
fn hover_missing_selector_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "hover",
            "#definitely-missing-element",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "hover missing selector text");
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
// Group 14: focus — command wiring, success path, and error path
// ========================================================================

#[test]
fn focus_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_focus_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "focus",
            "#ab-focus-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "focus json");
    let v = parse_json(&out);

    assert_focus_success(&v, &sid, &tid, "#ab-focus-target");
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    // focus_changed=true is the authoritative signal: the pre/post activeElement
    // reference comparison runs inside the CDP sequence where focus is still held.
    // Checking document.activeElement.id after the fact is unreliable in headless
    // Chrome — the page reverts to document.body once the CDP command completes
    // without a real OS-level window focus event.
    assert_eq!(v["data"]["changed"]["focus_changed"], true);

    close_session(&sid);
}

#[test]
fn focus_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_focus_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "focus",
            "#ab-focus-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "focus text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser focus"),
        "must contain ok browser focus"
    );
    assert!(
        text.contains("target: #ab-focus-target"),
        "must contain target line with selector"
    );

    close_session(&sid);
}

#[test]
fn focus_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "focus",
            "#ab-focus-target",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "focus nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser focus");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn focus_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "focus",
            "#ab-focus-target",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "focus nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn focus_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "focus",
            "#ab-focus-target",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "focus nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser focus");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn focus_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "focus",
            "#ab-focus-target",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "focus nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn focus_missing_selector_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "focus",
            "#definitely-missing-element",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "focus missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser focus");
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
fn focus_missing_selector_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "focus",
            "#definitely-missing-element",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "focus missing selector text");
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
// Group 15: press — command wiring, success path, and error path
// ========================================================================

#[test]
fn press_json_single_key() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_press_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "press json single key");
    let v = parse_json(&out);

    assert_press_success(&v, &sid, &tid, "Enter");
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_press_keydown_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_press_keyup_count)"),
        "1"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "window.__ab_press_events.includes('keydown:Enter:false:false') ? 'yes' : 'no'"
        ),
        "yes"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "window.__ab_press_events.includes('keyup:Enter:false:false') ? 'yes' : 'no'"
        ),
        "yes"
    );

    close_session(&sid);
}

#[test]
fn press_text_chord() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_press_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "press",
            "Control+A",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "press text chord");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser press"),
        "must contain ok browser press"
    );
    assert!(
        text.contains("keys: Control+A"),
        "must contain keys line with the chord"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "window.__ab_press_events.includes('keydown:a:true:false') ? 'yes' : 'no'"
        ),
        "yes"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "window.__ab_press_events.includes('keyup:a:true:false') ? 'yes' : 'no'"
        ),
        "yes"
    );

    close_session(&sid);
}

/// Verify that press Enter sends correct CDP key definitions (code, keyCode)
/// so that native browser actions (form submit) are triggered.
#[test]
fn press_enter_sends_cdp_key_definitions() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_press_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "press enter key defs");
    let v = parse_json(&out);
    assert_press_success(&v, &sid, &tid, "Enter");

    // Verify the keydown event carries correct code and keyCode
    let def_json = eval_value(&sid, &tid, "JSON.stringify(window.__ab_press_key_defs[0])");
    let def: serde_json::Value = serde_json::from_str(&def_json).expect("valid JSON");
    assert_eq!(def["type"], "keydown");
    assert_eq!(def["key"], "Enter");
    assert_eq!(def["code"], "Enter", "code must be 'Enter', not empty");
    assert_eq!(def["keyCode"], 13, "keyCode must be 13, not 0");

    close_session(&sid);
}

/// Verify that press Enter triggers form submission (native browser action).
#[test]
fn press_enter_submits_form() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    // Install a minimal form with a text input and a submit handler
    let fixture = r#"
(() => {
  const f = document.createElement('form');
  f.id = 'ab-form';
  f.addEventListener('submit', (e) => {
    e.preventDefault();
    window.__ab_form_submitted = true;
  });
  const inp = document.createElement('input');
  inp.id = 'ab-form-input';
  inp.type = 'text';
  inp.value = 'test';
  f.appendChild(inp);
  document.body.appendChild(f);
  inp.focus();
  window.__ab_form_submitted = false;
  return document.activeElement && document.activeElement.id;
})()
"#;
    let value = eval_value(&sid, &tid, fixture);
    assert_eq!(value, "ab-form-input", "form fixture should install");

    // Press Enter — should trigger form submit
    let out = headless_json(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "press enter submits form");

    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_form_submitted)"),
        "true",
        "Enter must trigger form submission"
    );

    close_session(&sid);
}

/// Verify that Escape sends correct code and keyCode.
#[test]
fn press_escape_sends_cdp_key_definitions() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_press_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "press",
            "Escape",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "press escape key defs");

    let def_json = eval_value(&sid, &tid, "JSON.stringify(window.__ab_press_key_defs[0])");
    let def: serde_json::Value = serde_json::from_str(&def_json).expect("valid JSON");
    assert_eq!(def["key"], "Escape");
    assert_eq!(def["code"], "Escape", "code must be 'Escape'");
    assert_eq!(def["keyCode"], 27, "keyCode must be 27");

    close_session(&sid);
}

/// Verify that Tab sends correct code and keyCode.
#[test]
fn press_tab_sends_cdp_key_definitions() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_press_fixture(&sid, &tid);

    let out = headless_json(
        &["browser", "press", "Tab", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "press tab key defs");

    let def_json = eval_value(&sid, &tid, "JSON.stringify(window.__ab_press_key_defs[0])");
    let def: serde_json::Value = serde_json::from_str(&def_json).expect("valid JSON");
    assert_eq!(def["key"], "Tab");
    assert_eq!(def["code"], "Tab", "code must be 'Tab'");
    assert_eq!(def["keyCode"], 9, "keyCode must be 9");

    close_session(&sid);
}

#[test]
fn press_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "press nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser press");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn press_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "press nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn press_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "press nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser press");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn press_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "press",
            "Enter",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "press nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn press_invalid_chord_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "press",
            "Control+",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "press invalid chord json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser press");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "INVALID_ARGUMENT");

    close_session(&sid);
}

#[test]
fn press_invalid_chord_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "press",
            "Control+",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "press invalid chord text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error INVALID_ARGUMENT:"),
        "text must contain error INVALID_ARGUMENT: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 16: drag — command wiring, success path, and error path
// ========================================================================

#[test]
fn drag_json_to_selector() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_drag_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "#ab-drag-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "drag json to selector");
    let v = parse_json(&out);

    assert_drag_success(
        &v,
        &sid,
        &tid,
        "#ab-drag-source",
        Some("#ab-drag-target"),
        None,
    );
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_drag_mousemove_count !== 0)"),
        "true"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_drag_drop_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_drag_last_drop"),
        "ab-drag-target"
    );

    close_session(&sid);
}

#[test]
fn drag_text_to_coordinates() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_drag_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "315,804",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "drag text to coordinates");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser drag"),
        "must contain ok browser drag"
    );
    assert!(
        text.contains("target: #ab-drag-source"),
        "must contain source target line"
    );
    assert!(
        text.contains("destination: 315,804"),
        "must contain destination line with coordinates"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_drag_drop_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_drag_last_drop"),
        "ab-drag-target"
    );

    close_session(&sid);
}

#[test]
fn drag_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "#ab-drag-target",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "drag nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser drag");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn drag_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "#ab-drag-target",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "drag nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn drag_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "#ab-drag-target",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "drag nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser drag");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn drag_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "#ab-drag-target",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "drag nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn drag_missing_source_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "drag",
            "#definitely-missing-source",
            "#ab-drag-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "drag missing source json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser drag");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(
        v["error"]["details"]["selector"],
        "#definitely-missing-source"
    );

    close_session(&sid);
}

#[test]
fn drag_missing_source_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "drag",
            "#definitely-missing-source",
            "#ab-drag-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "drag missing source text");
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
fn drag_invalid_destination_coordinates_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_drag_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "315,abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "drag invalid destination coordinates json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser drag");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "INVALID_ARGUMENT");

    close_session(&sid);
}

#[test]
fn drag_invalid_destination_coordinates_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_drag_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "drag",
            "#ab-drag-source",
            "315,abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "drag invalid destination coordinates text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error INVALID_ARGUMENT:"),
        "text must contain error INVALID_ARGUMENT: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 17: upload — command wiring, success path, and error path
// ========================================================================

#[test]
fn upload_json_single_file() {
    if skip() {
        return;
    }
    let (_tmp, files) = create_upload_files(&["upload-a.txt"]);
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_upload_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            &files[0],
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "upload json single file");
    let v = parse_json(&out);

    assert_upload_success(&v, &sid, &tid, "#ab-upload-input", &files);
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_upload_change_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_upload_last_count)"),
        "1"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_upload_last_names"),
        "upload-a.txt"
    );

    close_session(&sid);
}

#[test]
fn upload_text_multiple_files() {
    if skip() {
        return;
    }
    let (_tmp, files) = create_upload_files(&["upload-a.txt", "upload-b.txt"]);
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_upload_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            &files[0],
            &files[1],
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "upload text multiple files");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser upload"),
        "must contain ok browser upload"
    );
    assert!(
        text.contains("target: #ab-upload-input"),
        "must contain target line with selector"
    );
    assert!(text.contains("count: 2"), "must contain file count line");
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_upload_last_count)"),
        "2"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_upload_last_names"),
        "upload-a.txt,upload-b.txt"
    );

    close_session(&sid);
}

#[test]
fn upload_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            "/tmp/example.txt",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "upload nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser upload");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn upload_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            "/tmp/example.txt",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "upload nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn upload_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            "/tmp/example.txt",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "upload nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser upload");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn upload_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            "/tmp/example.txt",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "upload nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn upload_missing_selector_json() {
    if skip() {
        return;
    }
    let (_tmp, files) = create_upload_files(&["upload-a.txt"]);
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "upload",
            "#definitely-missing-element",
            &files[0],
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "upload missing selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser upload");
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
fn upload_missing_selector_text() {
    if skip() {
        return;
    }
    let (_tmp, files) = create_upload_files(&["upload-a.txt"]);
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "upload",
            "#definitely-missing-element",
            &files[0],
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "upload missing selector text");
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
fn upload_relative_path_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_upload_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            "relative.txt",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "upload relative path json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser upload");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "INVALID_ARGUMENT");

    close_session(&sid);
}

#[test]
fn upload_relative_path_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_upload_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "upload",
            "#ab-upload-input",
            "relative.txt",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "upload relative path text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error INVALID_ARGUMENT:"),
        "text must contain error INVALID_ARGUMENT: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 18: eval — command wiring, success path, and error path
// ========================================================================

#[test]
fn eval_json_number() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "eval", "2 + 2", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "eval json number");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser eval");
    assert!(v["error"].is_null(), "error must be null on success");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"]
            .as_str()
            .is_some_and(|url| url.starts_with(TEST_URL)),
        "context.url should include the current page URL"
    );
    assert_eq!(v["context"]["title"], "Example Domain");
    assert_eq!(v["data"]["value"], serde_json::json!(4));
    assert_eq!(v["data"]["type"], "number");
    assert_eq!(v["data"]["preview"], "4");
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn eval_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &["browser", "eval", "2 + 2", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "eval text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains(&format!("[{sid} {tid}] {TEST_URL}")),
        "header must include the current page URL: got {text}"
    );
    assert!(
        text.trim_end().ends_with('4'),
        "eval text should end with the evaluated value: {text}"
    );

    close_session(&sid);
}

#[test]
fn eval_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "eval",
            "2 + 2",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "eval nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser eval");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn eval_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "eval",
            "2 + 2",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "eval nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn eval_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "eval",
            "2 + 2",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "eval nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser eval");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn eval_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "eval",
            "2 + 2",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "eval nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn eval_exception_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "eval",
            "(() => { throw new Error('boom-eval'); })()",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "eval exception json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser eval");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "EVAL_FAILED");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("boom-eval")
            || v["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("Error"),
        "eval exception should surface an expression error message"
    );

    close_session(&sid);
}

#[test]
fn eval_exception_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "eval",
            "(() => { throw new Error('boom-eval'); })()",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "eval exception text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error EVAL_FAILED:"),
        "text must contain error EVAL_FAILED: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 19: mouse-move — command wiring, success path, and error path
// ========================================================================

#[test]
fn mouse_move_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_mouse_move_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "mouse-move json");
    let v = parse_json(&out);

    assert_mouse_move_success(&v, &sid, &tid, "120,140");
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(
        eval_value(&sid, &tid, "String(window.__ab_mouse_move_count !== 0)"),
        "true"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_mouse_move_last_target"),
        "ab-mouse-move-target"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_mouse_move_last_coords"),
        "120,140"
    );

    close_session(&sid);
}

#[test]
fn mouse_move_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_mouse_move_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "mouse-move text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser mouse-move"),
        "must contain ok browser mouse-move"
    );
    assert!(
        text.contains("target: 120,140"),
        "must contain target line with coordinates"
    );
    assert_eq!(
        eval_value(&sid, &tid, "window.__ab_mouse_move_last_target"),
        "ab-mouse-move-target"
    );

    close_session(&sid);
}

#[test]
fn mouse_move_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "mouse-move nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser mouse-move");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn mouse_move_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "mouse-move nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn mouse_move_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "mouse-move nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser mouse-move");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn mouse_move_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "mouse-move nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn mouse_move_invalid_coordinates_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "mouse-move",
            "120,abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "mouse-move invalid coordinates json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser mouse-move");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "INVALID_ARGUMENT");

    close_session(&sid);
}

#[test]
fn mouse_move_invalid_coordinates_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "mouse-move",
            "120,abc",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "mouse-move invalid coordinates text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("error INVALID_ARGUMENT:"),
        "text must contain error INVALID_ARGUMENT: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 20: cursor-position — command wiring, success path, and error path
// ========================================================================

#[test]
fn cursor_position_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let move_out = headless(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&move_out, "cursor-position setup mouse-move");

    let out = headless_json(
        &[
            "browser",
            "cursor-position",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "cursor-position json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser cursor-position");
    assert!(v["error"].is_null(), "error must be null on success");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["x"], 120);
    assert_eq!(v["data"]["y"], 140);
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn cursor_position_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let move_out = headless(
        &[
            "browser",
            "mouse-move",
            "120,140",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&move_out, "cursor-position setup mouse-move");

    let out = headless(
        &[
            "browser",
            "cursor-position",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "cursor-position text");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser cursor-position"),
        "must contain ok browser cursor-position"
    );
    assert!(text.contains("x: 120"), "must contain x line: {text}");
    assert!(text.contains("y: 140"), "must contain y line: {text}");

    close_session(&sid);
}

#[test]
fn cursor_position_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "cursor-position",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "cursor-position nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser cursor-position");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn cursor_position_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "cursor-position",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "cursor-position nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn cursor_position_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "cursor-position",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "cursor-position nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser cursor-position");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn cursor_position_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "cursor-position",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "cursor-position nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

// ========================================================================
// Group 20b: cursor-position regression — non-mouse-move actions update position
// ========================================================================

#[test]
fn cursor_position_after_click_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    // Click at coordinates — this should update cursor position
    let click_out = headless_json(
        &[
            "browser",
            "click",
            "200,250",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&click_out, "click setup for cursor-position");

    // Now cursor-position should reflect the click coordinates
    let out = headless_json(
        &[
            "browser",
            "cursor-position",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser cursor-position");
    assert_eq!(v["data"]["x"], 200, "x should match click coordinate");
    assert_eq!(v["data"]["y"], 250, "y should match click coordinate");

    close_session(&sid);
}

// ========================================================================
// Group 21: scroll — command wiring, success path, and error path
// ========================================================================

#[test]
fn scroll_json_down_page() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_scroll_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "scroll",
            "down",
            "180",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll json down page");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser scroll");
    assert!(v["error"].is_null(), "error must be null on success");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["action"], "scroll");
    assert_eq!(v["data"]["direction"], "down");
    assert_eq!(v["data"]["pixels"], 180);
    assert_eq!(v["data"]["changed"]["scroll_changed"], true);
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(eval_value(&sid, &tid, "String(window.scrollY)"), "180");
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn scroll_text_bottom_container() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_scroll_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "scroll",
            "bottom",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--container",
            "#ab-scroll-container",
        ],
        15,
    );
    assert_success(&out, "scroll text bottom container");
    let text = stdout_str(&out);

    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "header must contain [session_id tab_id]: got {text}"
    );
    assert!(
        text.contains("ok browser scroll"),
        "must contain ok browser scroll"
    );
    assert!(
        text.contains("direction: bottom"),
        "must contain direction line"
    );
    assert!(
        text.contains("container: #ab-scroll-container"),
        "must contain container line"
    );
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "(() => { const el = document.getElementById('ab-scroll-container'); return String(el.scrollTop === el.scrollHeight - el.clientHeight); })()"
        ),
        "true"
    );

    close_session(&sid);
}

#[test]
fn scroll_into_view_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_scroll_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "scroll",
            "into-view",
            "#ab-scroll-target",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--align",
            "center",
        ],
        15,
    );
    assert_success(&out, "scroll into-view json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser scroll");
    assert!(v["error"].is_null(), "error must be null on success");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["action"], "scroll");
    assert_eq!(v["data"]["target"]["selector"], "#ab-scroll-target");
    assert_eq!(v["data"]["align"], "center");
    assert_eq!(v["data"]["changed"]["scroll_changed"], true);
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "(() => { const rect = document.getElementById('ab-scroll-target').getBoundingClientRect(); return String(rect.top >= 0 && rect.bottom <= window.innerHeight); })()"
        ),
        "true"
    );
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn scroll_into_view_xpath_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_scroll_fixture(&sid, &tid);

    let xpath = "//div[@id='ab-scroll-target']";
    let out = headless_json(
        &[
            "browser",
            "scroll",
            "into-view",
            xpath,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--align",
            "center",
        ],
        15,
    );
    assert_success(&out, "scroll into-view xpath json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser scroll");
    assert!(v["error"].is_null(), "error must be null on success");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["action"], "scroll");
    assert_eq!(v["data"]["target"]["selector"], xpath);
    assert_eq!(v["data"]["align"], "center");
    assert_eq!(v["data"]["changed"]["scroll_changed"], true);
    assert_eq!(v["data"]["changed"]["url_changed"], false);
    assert_eq!(v["data"]["changed"]["focus_changed"], false);
    assert_eq!(
        eval_value(
            &sid,
            &tid,
            "(() => { const rect = document.getElementById('ab-scroll-target').getBoundingClientRect(); return String(rect.top >= 0 && rect.bottom <= window.innerHeight); })()"
        ),
        "true"
    );
    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn scroll_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "scroll",
            "down",
            "180",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "scroll nonexistent session json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser scroll");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn scroll_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "scroll",
            "down",
            "180",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "scroll nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn scroll_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "scroll",
            "down",
            "180",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "scroll nonexistent tab json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser scroll");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);

    close_session(&sid);
}

#[test]
fn scroll_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "scroll",
            "down",
            "180",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "scroll nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

#[test]
fn scroll_missing_container_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "scroll",
            "bottom",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--container",
            "#definitely-missing-container",
        ],
        10,
    );
    assert_failure(&out, "scroll missing container json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser scroll");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(
        v["error"]["details"]["selector"],
        "#definitely-missing-container"
    );

    close_session(&sid);
}

#[test]
fn scroll_missing_container_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "scroll",
            "bottom",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--container",
            "#definitely-missing-container",
        ],
        10,
    );
    assert_failure(&out, "scroll missing container text");
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
fn scroll_into_view_missing_target_json() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_scroll_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "scroll",
            "into-view",
            "#definitely-missing-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "scroll into-view missing target json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser scroll");
    assert!(v["context"].is_object(), "context must be present on error");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(
        v["error"]["details"]["selector"],
        "#definitely-missing-target"
    );

    close_session(&sid);
}

#[test]
fn scroll_into_view_missing_target_text() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_scroll_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "scroll",
            "into-view",
            "#definitely-missing-target",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "scroll into-view missing target text");
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
// Group: scroll-to-center — off-screen element operations
// ========================================================================

/// Inject a page with a sticky header, 2000px spacer, and interactive
/// elements far below the fold. This tests that commands auto-scroll
/// elements to viewport center before operating.
fn install_offscreen_fixture(session_id: &str, tab_id: &str) {
    let expression = r#"
(() => {
  const existing = document.getElementById('ab-offscreen-fixture');
  if (existing) existing.remove();

  window.scrollTo(0, 0);
  document.body.innerHTML = '';
  document.body.style.margin = '0';

  window.__ab_offscreen_clicked = false;
  window.__ab_offscreen_hovered = false;

  const root = document.createElement('div');
  root.id = 'ab-offscreen-fixture';
  root.innerHTML = `
    <style>
      #ab-sticky-header {
        position: sticky;
        top: 0;
        height: 50px;
        background: #333;
        color: #fff;
        z-index: 100;
        display: flex;
        align-items: center;
        padding: 0 16px;
        font-size: 14px;
      }
      #ab-spacer {
        height: 2000px;
      }
      #ab-offscreen-section {
        padding: 20px;
      }
      #ab-offscreen-btn {
        display: block;
        width: 200px;
        height: 40px;
        margin-bottom: 12px;
      }
      #ab-offscreen-input {
        display: block;
        width: 200px;
        height: 36px;
        margin-bottom: 12px;
      }
      #ab-offscreen-select {
        display: block;
        width: 200px;
        height: 36px;
        margin-bottom: 12px;
      }
      #ab-offscreen-hover {
        width: 200px;
        height: 40px;
        background: #f5f5f5;
        display: flex;
        align-items: center;
        justify-content: center;
        margin-bottom: 12px;
      }
      #ab-trailing-spacer {
        height: 500px;
      }
    </style>
    <div id="ab-sticky-header">Sticky Header</div>
    <div id="ab-spacer"></div>
    <div id="ab-offscreen-section">
      <button id="ab-offscreen-btn" type="button">Off-Screen Button</button>
      <input id="ab-offscreen-input" type="text" placeholder="off-screen input" />
      <select id="ab-offscreen-select">
        <option value="">--</option>
        <option value="a">Option A</option>
        <option value="b">Option B</option>
      </select>
      <div id="ab-offscreen-hover" tabindex="0">Hover target</div>
    </div>
    <div id="ab-trailing-spacer"></div>
  `;
  document.body.appendChild(root);

  document.getElementById('ab-offscreen-btn').addEventListener('click', () => {
    window.__ab_offscreen_clicked = true;
  });
  document.getElementById('ab-offscreen-hover').addEventListener('mouseenter', () => {
    window.__ab_offscreen_hovered = true;
  });

  return 'ok';
})()
"#;

    let value = eval_value(session_id, tab_id, expression);
    assert_eq!(value, "ok", "offscreen fixture should install successfully");
}

/// Helper: get the element's bounding rect top relative to viewport.
fn get_element_viewport_top(session_id: &str, tab_id: &str, element_id: &str) -> f64 {
    let expr =
        format!("String(document.getElementById('{element_id}').getBoundingClientRect().top)");
    let val = eval_value(session_id, tab_id, &expr);
    val.parse::<f64>().unwrap_or(-1.0)
}

/// Helper: check that an element is roughly centered in the viewport.
/// We check that its top is between 20% and 70% of viewport height.
fn assert_element_near_center(session_id: &str, tab_id: &str, element_id: &str) {
    let top = get_element_viewport_top(session_id, tab_id, element_id);
    let vh: f64 = eval_value(session_id, tab_id, "String(window.innerHeight)")
        .parse()
        .unwrap_or(768.0);
    let ratio = top / vh;
    assert!(
        (0.2..=0.7).contains(&ratio),
        "element #{element_id} should be near viewport center: top={top}, vh={vh}, ratio={ratio:.2}"
    );
}

#[test]
fn click_offscreen_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    // Confirm element is off-screen before click
    let pre_scroll = eval_value(&sid, &tid, "String(window.scrollY)");
    assert_eq!(pre_scroll, "0", "page should start at top");

    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-offscreen-btn",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "click offscreen element");
    let v = parse_json(&out);
    assert_click_success(&v, &sid, &tid, Some("#ab-offscreen-btn"));

    // Verify click actually registered
    let clicked = eval_value(&sid, &tid, "String(window.__ab_offscreen_clicked)");
    assert_eq!(clicked, "true", "click handler must have fired");

    // Verify element was scrolled near viewport center
    assert_element_near_center(&sid, &tid, "ab-offscreen-btn");

    close_session(&sid);
}

#[test]
fn type_offscreen_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "type",
            "#ab-offscreen-input",
            "hello",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "type offscreen element");
    let v = parse_json(&out);
    assert_type_success(&v, &sid, &tid, "#ab-offscreen-input", 5);

    // Verify value was typed
    let val = eval_value(
        &sid,
        &tid,
        "document.getElementById('ab-offscreen-input').value",
    );
    assert_eq!(val, "hello", "input must contain typed text");

    // Verify element was scrolled near viewport center
    assert_element_near_center(&sid, &tid, "ab-offscreen-input");

    close_session(&sid);
}

#[test]
fn fill_offscreen_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "fill",
            "#ab-offscreen-input",
            "world",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "fill offscreen element");
    let v = parse_json(&out);
    assert_fill_success(&v, &sid, &tid, "#ab-offscreen-input", 5);

    // Verify value was filled
    let val = eval_value(
        &sid,
        &tid,
        "document.getElementById('ab-offscreen-input').value",
    );
    assert_eq!(val, "world", "input must contain filled text");

    // Verify element was scrolled near viewport center
    assert_element_near_center(&sid, &tid, "ab-offscreen-input");

    close_session(&sid);
}

#[test]
fn select_offscreen_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "select",
            "#ab-offscreen-select",
            "a",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "select offscreen element");
    let v = parse_json(&out);
    assert_select_success(&v, &sid, &tid, "#ab-offscreen-select", "a", false);

    // Verify value was selected
    let val = eval_value(
        &sid,
        &tid,
        "document.getElementById('ab-offscreen-select').value",
    );
    assert_eq!(val, "a", "select must have value 'a'");

    // Verify element was scrolled near viewport center
    assert_element_near_center(&sid, &tid, "ab-offscreen-select");

    close_session(&sid);
}

#[test]
fn focus_offscreen_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "focus",
            "#ab-offscreen-input",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "focus offscreen element");
    let v = parse_json(&out);
    assert_focus_success(&v, &sid, &tid, "#ab-offscreen-input");

    // Verify element is focused
    let focused_id = eval_value(&sid, &tid, "document.activeElement.id");
    assert_eq!(
        focused_id, "ab-offscreen-input",
        "activeElement must be the offscreen input"
    );

    // Verify element was scrolled near viewport center
    assert_element_near_center(&sid, &tid, "ab-offscreen-input");

    close_session(&sid);
}

#[test]
fn hover_offscreen_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "hover",
            "#ab-offscreen-hover",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "hover offscreen element");
    let v = parse_json(&out);
    assert_hover_success(&v, &sid, &tid, "#ab-offscreen-hover");

    // Verify hover event fired
    let hovered = eval_value(&sid, &tid, "String(window.__ab_offscreen_hovered)");
    assert_eq!(hovered, "true", "mouseenter handler must have fired");

    // Verify element was scrolled near viewport center
    assert_element_near_center(&sid, &tid, "ab-offscreen-hover");

    close_session(&sid);
}

#[test]
fn click_offscreen_avoids_sticky_header() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(TEST_URL);
    let _guard = SessionGuard::new(&sid);
    install_offscreen_fixture(&sid, &tid);

    // Click the offscreen button — it should scroll to center, away from sticky header
    let out = headless_json(
        &[
            "browser",
            "click",
            "#ab-offscreen-btn",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "click avoids sticky header");

    // The element's top should be well below the sticky header (50px)
    let top = get_element_viewport_top(&sid, &tid, "ab-offscreen-btn");
    assert!(
        top > 60.0,
        "element top ({top}) must be well below sticky header (50px) — center-scroll should place it in the middle"
    );

    // Confirm click went to the button, not the header
    let clicked = eval_value(&sid, &tid, "String(window.__ab_offscreen_clicked)");
    assert_eq!(
        clicked, "true",
        "click must hit the button, not the sticky header"
    );

    close_session(&sid);
}
