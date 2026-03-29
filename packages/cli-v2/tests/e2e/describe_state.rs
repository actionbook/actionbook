//! E2E tests for `browser describe` / `browser state`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

const TARGET_SELECTOR: &str = "#target";

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

/// Inject a button fixture with accessible name, role, and a labelled input.
fn inject_fixture(sid: &str, tid: &str) {
    let js = r#"document.body.style.margin = '0';
document.body.innerHTML = '<ul><li id="item">John Smith<button id="target" type="button" aria-label="Edit">Edit</button></li></ul>';
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

// ---------------------------------------------------------------------------
// browser describe — happy path
// ---------------------------------------------------------------------------

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
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.describe");
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["target"]["selector"], TARGET_SELECTOR);

    // summary is a non-empty string
    assert!(
        v["data"]["summary"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "summary must be a non-empty string"
    );
    // role and tag are present
    assert!(v["data"]["role"].is_string());
    assert!(v["data"]["tag"].is_string());
    // name is present (button has aria-label="Edit")
    assert!(
        v["data"]["name"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "name must be present for labelled button"
    );
    // state object with visible and enabled
    assert!(v["data"]["state"]["visible"].is_boolean());
    assert!(v["data"]["state"]["enabled"].is_boolean());
    // nearby is null when --nearby not passed
    assert!(v["data"]["nearby"].is_null());
}

#[test]
fn describe_text_happy_path() {
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
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe text");
    let text = stdout_str(&out);

    assert!(
        text.lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    // summary line is the second line of output
    assert!(text.lines().count() >= 2, "expected at least 2 lines");
    // summary contains the element role or name
    assert!(
        text.contains("button") || text.contains("Edit"),
        "text output must contain role or name: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// browser describe --nearby
// ---------------------------------------------------------------------------

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
            TARGET_SELECTOR,
            "--nearby",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe --nearby json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.describe");
    assert_eq!(v["data"]["target"]["selector"], TARGET_SELECTOR);

    // nearby must be an object (not null) when --nearby is passed
    assert!(
        v["data"]["nearby"].is_object(),
        "nearby must be an object when --nearby flag is set"
    );

    let nearby = &v["data"]["nearby"];
    // spec: at most 1 parent, 1 prev sibling, 1 next sibling, up to 3 children
    assert!(nearby["parent"].is_string() || nearby["parent"].is_null());
    assert!(nearby["previous_sibling"].is_string() || nearby["previous_sibling"].is_null());
    assert!(nearby["next_sibling"].is_string() || nearby["next_sibling"].is_null());
    assert!(nearby["children"].is_array());
    assert!(
        nearby["children"].as_array().unwrap().len() <= 3,
        "children must be at most 3"
    );

    // our fixture: button is inside <li>, so parent should be the listitem
    assert!(
        nearby["parent"].as_str().is_some(),
        "button inside li should have a parent in nearby"
    );
}

#[test]
fn describe_nearby_text_happy_path() {
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
            TARGET_SELECTOR,
            "--nearby",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "describe --nearby text");
    let text = stdout_str(&out);

    assert!(
        text.lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    // with --nearby, output should include at least one nearby field label
    assert!(
        text.contains("parent:") || text.contains("previous_sibling:") || text.contains("children:"),
        "nearby text output must include nearby field labels: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// browser state — happy path
// ---------------------------------------------------------------------------

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
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.state");
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["target"]["selector"], TARGET_SELECTOR);

    // all 6 boolean state fields must be present per spec
    let state = &v["data"]["state"];
    assert!(state["visible"].is_boolean(), "visible must be boolean");
    assert!(state["enabled"].is_boolean(), "enabled must be boolean");
    assert!(state["checked"].is_boolean(), "checked must be boolean");
    assert!(state["focused"].is_boolean(), "focused must be boolean");
    assert!(state["editable"].is_boolean(), "editable must be boolean");
    assert!(state["selected"].is_boolean(), "selected must be boolean");

    // button is visible and enabled by default
    assert_eq!(state["visible"], true);
    assert_eq!(state["enabled"], true);
}

#[test]
fn state_text_happy_path() {
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
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "state text");
    let text = stdout_str(&out);

    assert!(
        text.lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    // all 6 fields must appear in text output
    assert!(text.contains("visible:"), "missing visible field");
    assert!(text.contains("enabled:"), "missing enabled field");
    assert!(text.contains("checked:"), "missing checked field");
    assert!(text.contains("focused:"), "missing focused field");
    assert!(text.contains("editable:"), "missing editable field");
    assert!(text.contains("selected:"), "missing selected field");
}

// ---------------------------------------------------------------------------
// Error cases — SESSION_NOT_FOUND / TAB_NOT_FOUND / ELEMENT_NOT_FOUND
// ---------------------------------------------------------------------------

#[test]
fn describe_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "describe",
            TARGET_SELECTOR,
            "--session",
            "missing-session",
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "describe nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
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
            TARGET_SELECTOR,
            "--session",
            "missing-session",
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "state nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn describe_tab_not_found_json() {
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
            TARGET_SELECTOR,
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
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
}

#[test]
fn state_tab_not_found_json() {
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
            TARGET_SELECTOR,
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
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
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
    assert_failure(&out, "describe missing selector");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
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
    assert_failure(&out, "state missing selector");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["error"]["details"]["selector"], "#missing");
}

// ---------------------------------------------------------------------------
// JS_EXCEPTION — monkeypatch pattern (P1 regression gate for Batch 3)
// ---------------------------------------------------------------------------

/// P1 regression: `browser describe` must surface `JS_EXCEPTION` when the
/// underlying JS summary computation throws.
#[test]
fn describe_js_exception_propagated() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // Monkeypatch Element.prototype.getAttribute to throw — summary generation reads attributes.
    let patch = "Element.prototype.getAttribute = function() { throw new Error('injected describe exception'); }; void(0)";
    let patch_out = headless_json(
        &["browser", "eval", patch, "--session", &sid, "--tab", &tid],
        5,
    );
    assert_success(&patch_out, "patch getAttribute");

    let out = headless_json(
        &[
            "browser",
            "describe",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "describe with throwing getAttribute");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.describe");
    assert_error_envelope(&v, "JS_EXCEPTION");
}

/// P1 regression: `browser state` must surface `JS_EXCEPTION` when the
/// underlying JS state computation throws.
#[test]
fn state_js_exception_propagated() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // Monkeypatch ownerDocument to throw — state computation reads document.
    let patch = "Object.defineProperty(document.querySelector('#target'), 'ownerDocument', { get() { throw new Error('injected state exception'); } }); void(0)";
    let patch_out = headless_json(
        &["browser", "eval", patch, "--session", &sid, "--tab", &tid],
        5,
    );
    assert_success(&patch_out, "patch ownerDocument");

    let out = headless_json(
        &[
            "browser",
            "state",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "state with throwing ownerDocument");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.state");
    assert_error_envelope(&v, "JS_EXCEPTION");
}
