//! E2E tests for `browser describe` / `state`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

const DESCRIBE_SELECTOR: &str = "#describe-target";
const STATE_SELECTOR: &str = "#state-target";
const STATE_CHECKED_SELECTOR: &str = "#state-checked";
const STATE_DISABLED_SELECTOR: &str = "#state-disabled";
const STATE_HIDDEN_SELECTOR: &str = "#state-hidden";

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
    assert_success(&out, "start session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            "about:blank",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&goto_out, "goto about:blank");

    (sid, tid)
}

fn inject_fixture(sid: &str, tid: &str) {
    let js = r#"document.body.style.margin = '0';
document.body.innerHTML = `
  <ul>
    <li id="person-row" aria-label="John Smith">
      <span id="person-name">John Smith</span>
      <button id="describe-target" type="button">Edit</button>
    </li>
  </ul>
  <input id="state-target" type="text" value="query" placeholder="Search">
  <input id="state-checked" type="checkbox" checked>
  <input id="state-disabled" type="text" value="locked" disabled>
  <button id="state-hidden" style="display:none">Hidden Action</button>
`;
document.title = 'Describe State Fixture';
document.querySelector('#state-target').focus();
void(0)"#;
    let out = headless_json(&["browser", "eval", js, "--session", sid, "--tab", tid], 10);
    assert_success(&out, "inject fixture");
}

fn assert_meta(v: &serde_json::Value) {
    assert!(v["meta"]["duration_ms"].is_number());
    assert!(v["meta"]["warnings"].is_array());
    assert!(v["meta"]["pagination"].is_null());
    assert!(v["meta"]["truncated"].is_boolean());
}

fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false);
    assert!(v["data"].is_null());
    assert_eq!(v["error"]["code"], expected_code);
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert!(v["error"]["details"].is_object() || v["error"]["details"].is_null());
    assert_meta(v);
}

#[test]
fn describe_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Describe State Fixture");
    assert_eq!(v["data"]["target"]["selector"], DESCRIBE_SELECTOR);
    assert_eq!(v["data"]["summary"], "button \"Edit\"");
    assert_eq!(v["data"]["role"], "button");
    assert_eq!(v["data"]["name"], "Edit");
    assert_eq!(v["data"]["tag"], "button");
    assert_eq!(v["data"]["attributes"]["type"], "button");
    assert_eq!(v["data"]["state"]["visible"], true);
    assert_eq!(v["data"]["state"]["enabled"], true);
    assert!(v["data"]["nearby"].is_null());
}

#[test]
fn describe_nearby_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--nearby",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe nearby json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_eq!(v["ok"], true);
    let nearby = &v["data"]["nearby"];
    assert_eq!(nearby["parent"], "listitem \"John Smith\"");
    assert_eq!(nearby["previous_sibling"], "text \"John Smith\"");
    assert!(nearby["next_sibling"].is_null());
    assert!(nearby["children"].as_array().unwrap().is_empty());
}

#[test]
fn describe_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert!(
        lines
            .first()
            .unwrap_or(&"")
            .starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: {text}"
    );
    assert!(
        lines.first().unwrap_or(&"").contains("about:blank"),
        "header must contain current URL: {text}"
    );
    assert_eq!(lines.get(1), Some(&"button \"Edit\""));
}

#[test]
fn describe_nearby_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--nearby",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe nearby text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert!(
        lines
            .first()
            .unwrap_or(&"")
            .starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: {text}"
    );
    assert_eq!(lines.get(1), Some(&"button \"Edit\""));
    assert_eq!(lines.get(2), Some(&"parent: listitem \"John Smith\""));
    assert_eq!(lines.get(3), Some(&"previous_sibling: text \"John Smith\""));
}

#[test]
fn state_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Describe State Fixture");
    assert_eq!(v["data"]["target"]["selector"], STATE_SELECTOR);
    assert_eq!(v["data"]["state"]["visible"], true);
    assert_eq!(v["data"]["state"]["enabled"], true);
    assert_eq!(v["data"]["state"]["checked"], false);
    assert_eq!(v["data"]["state"]["focused"], true);
    assert_eq!(v["data"]["state"]["editable"], true);
    assert_eq!(v["data"]["state"]["selected"], false);
}

#[test]
fn state_json_checked_true() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_CHECKED_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state checked json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["target"]["selector"], STATE_CHECKED_SELECTOR);
    assert_eq!(v["data"]["state"]["visible"], true);
    assert_eq!(v["data"]["state"]["enabled"], true);
    assert_eq!(v["data"]["state"]["checked"], true);
    assert_eq!(v["data"]["state"]["focused"], false);
    assert_eq!(v["data"]["state"]["editable"], false);
    assert_eq!(v["data"]["state"]["selected"], false);
}

#[test]
fn state_json_enabled_false() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_DISABLED_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state disabled json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["target"]["selector"], STATE_DISABLED_SELECTOR);
    assert_eq!(v["data"]["state"]["visible"], true);
    assert_eq!(v["data"]["state"]["enabled"], false);
    assert_eq!(v["data"]["state"]["checked"], false);
    assert_eq!(v["data"]["state"]["focused"], false);
    assert_eq!(v["data"]["state"]["editable"], false);
    assert_eq!(v["data"]["state"]["selected"], false);
}

#[test]
fn state_json_visible_false() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_HIDDEN_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state hidden json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["target"]["selector"], STATE_HIDDEN_SELECTOR);
    assert_eq!(v["data"]["state"]["visible"], false);
    assert_eq!(v["data"]["state"]["enabled"], true);
    assert_eq!(v["data"]["state"]["checked"], false);
    assert_eq!(v["data"]["state"]["focused"], false);
    assert_eq!(v["data"]["state"]["editable"], false);
    assert_eq!(v["data"]["state"]["selected"], false);
}

#[test]
fn state_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "state",
            STATE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert!(
        lines
            .first()
            .unwrap_or(&"")
            .starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: {text}"
    );
    assert!(
        lines.first().unwrap_or(&"").contains("about:blank"),
        "header must contain current URL: {text}"
    );
    assert_eq!(lines.get(1), Some(&"visible: true"));
    assert_eq!(lines.get(2), Some(&"enabled: true"));
    assert_eq!(lines.get(3), Some(&"checked: false"));
    assert_eq!(lines.get(4), Some(&"focused: true"));
    assert_eq!(lines.get(5), Some(&"editable: true"));
    assert_eq!(lines.get(6), Some(&"selected: false"));
}

#[test]
fn describe_element_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "describe",
            "#missing",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "describe missing element");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(v["error"]["details"]["selector"], "#missing");
}

#[test]
fn state_element_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "state",
            "#missing",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "state missing element");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(v["error"]["details"]["selector"], "#missing");
}

#[test]
fn describe_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "describe nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn state_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_SELECTOR,
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "state nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn describe_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "describe nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}

#[test]
fn state_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "state nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}

#[test]
fn describe_js_exception_returns_error() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // `describe` computes `state.visible`, which reaches `getComputedStyle`.
    let patch_out = headless_json(
        &[
            "browser",
            "eval",
            "window.getComputedStyle = function() { throw new Error('describe boom'); }; void(0)",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        5,
    );
    assert_success(&patch_out, "patch getComputedStyle for describe");

    let out = headless_json(
        &[
            "browser",
            "describe",
            DESCRIBE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "describe js exception");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_error_envelope(&v, "JS_EXCEPTION");
}

#[test]
fn state_js_exception_returns_error() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let patch_out = headless_json(
        &[
            "browser",
            "eval",
            "window.getComputedStyle = function() { throw new Error('state boom'); }; void(0)",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        5,
    );
    assert_success(&patch_out, "patch getComputedStyle for state");

    let out = headless_json(
        &[
            "browser",
            "state",
            STATE_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "state js exception");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_error_envelope(&v, "JS_EXCEPTION");
}
