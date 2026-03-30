//! E2E tests for `browser attrs` / `box` / `styles`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, wait_page_ready,
};

const TARGET_SELECTOR: &str = "#target";

fn start_session() -> (String, String) {
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

    wait_page_ready(&sid, &tid);
    (sid, tid)
}

fn inject_fixture(sid: &str, tid: &str) {
    let js = r#"document.body.style.margin = '0';
document.body.innerHTML = '<div id="target" class="hero-card" data-testid="hero-card" aria-label="Styled Card" title="Card Title" style="box-sizing:border-box;position:absolute;left:80px;top:120px;width:160px;height:48px;display:flex;visibility:visible;opacity:0.5;color:rgb(255, 0, 0);background-color:rgb(0, 128, 0);font-size:18px;font-weight:700;font-family:monospace;margin:0;padding:6px;border:2px solid rgb(0, 0, 255);z-index:9;overflow:hidden;cursor:pointer;">Hello</div>';
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

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn attrs_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "attrs",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "attrs json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.attrs");
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["target"]["selector"], TARGET_SELECTOR);

    let attrs = v["data"]["value"].as_object().expect("attrs object");
    assert_eq!(attrs["id"], "target");
    assert_eq!(attrs["class"], "hero-card");
    assert_eq!(attrs["data-testid"], "hero-card");
    assert_eq!(attrs["aria-label"], "Styled Card");
    assert_eq!(attrs["title"], "Card Title");
    assert!(attrs.contains_key("style"));
}

#[test]
fn attrs_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "attrs",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "attrs text");
    let text = stdout_str(&out);

    assert!(
        text.lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    assert!(text.contains("target: #target"));
    assert!(text.contains("aria-label: Styled Card"));
    assert!(text.contains("data-testid: hero-card"));
    assert!(text.contains("id: target"));
}

#[test]
fn box_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "box",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "box json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.box");
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["data"]["target"]["selector"], TARGET_SELECTOR);

    let rect = &v["data"]["value"];
    assert_close(rect["x"].as_f64().unwrap_or_default(), 80.0);
    assert_close(rect["y"].as_f64().unwrap_or_default(), 120.0);
    assert_close(rect["width"].as_f64().unwrap_or_default(), 160.0);
    assert_close(rect["height"].as_f64().unwrap_or_default(), 48.0);
    assert_close(rect["right"].as_f64().unwrap_or_default(), 240.0);
    assert_close(rect["bottom"].as_f64().unwrap_or_default(), 168.0);
}

#[test]
fn box_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "box",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "box text");
    let text = stdout_str(&out);

    assert!(
        text.lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    assert!(text.contains("target: #target"));
    assert!(text.contains("x: 80"));
    assert!(text.contains("y: 120"));
    assert!(text.contains("width: 160"));
    assert!(text.contains("height: 48"));
    assert!(text.contains("right: 240"));
    assert!(text.contains("bottom: 168"));
}

#[test]
fn styles_json_default_props_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "styles",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "styles json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.styles");
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["data"]["target"]["selector"], TARGET_SELECTOR);

    let styles = v["data"]["value"].as_object().expect("styles object");
    assert_eq!(styles["display"], "flex");
    assert_eq!(styles["visibility"], "visible");
    assert_eq!(styles["opacity"], "0.5");
    assert_eq!(styles["fontSize"], "18px");
    assert_eq!(styles["fontWeight"], "700");
    assert_eq!(styles["position"], "absolute");
    assert_eq!(styles["zIndex"], "9");
    assert_eq!(styles["overflow"], "hidden");
    assert_eq!(styles["cursor"], "pointer");
    assert_eq!(styles["width"], "160px");
    assert_eq!(styles["height"], "48px");
    assert!(
        styles["color"].as_str().unwrap_or("").contains("255, 0, 0"),
        "unexpected color {:?}",
        styles["color"]
    );
    assert!(
        styles["backgroundColor"]
            .as_str()
            .unwrap_or("")
            .contains("0, 128, 0"),
        "unexpected backgroundColor {:?}",
        styles["backgroundColor"]
    );
}

#[test]
fn styles_text_explicit_names_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "styles",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
            "color",
            "backgroundColor",
            "width",
            "height",
        ],
        10,
    );
    assert_success(&out, "styles text");
    let text = stdout_str(&out);

    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines
            .first()
            .unwrap_or(&"")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    assert!(lines.contains(&"target: #target"));
    assert!(text.contains("color: rgb(255, 0, 0)"));
    assert!(text.contains("backgroundColor: rgb(0, 128, 0)"));
    assert!(text.contains("width: 160px"));
    assert!(text.contains("height: 48px"));
}

#[test]
fn attrs_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "attrs",
            TARGET_SELECTOR,
            "--session",
            "missing-session",
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "attrs nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.attrs");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn box_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "box",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "box nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.box");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
}

#[test]
fn styles_selector_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "styles",
            "#missing",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "styles missing selector");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.styles");
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["error"]["details"]["selector"], "#missing");
}

#[test]
fn styles_js_exception_returns_error() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // Monkeypatch getComputedStyle to throw so callFunctionOn returns exceptionDetails
    let patch_out = headless_json(
        &[
            "browser",
            "eval",
            "window.getComputedStyle = function() { throw new Error('injected styles exception'); }; void(0)",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        5,
    );
    assert_success(&patch_out, "patch getComputedStyle");

    let out = headless_json(
        &[
            "browser",
            "styles",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "styles with throwing getComputedStyle");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.styles");
    assert_error_envelope(&v, "JS_EXCEPTION");
}

/// P2 regression: `browser box` must not scroll — verify off-screen element
/// returns coordinates without changing `window.scrollY`.
#[test]
fn box_offscreen_no_scroll_side_effect() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    // Inject a tall page with an element far below the fold.
    let js = r#"document.body.style.margin = '0';
document.body.innerHTML = '<div style="height:5000px"></div><div id="offscreen" style="position:absolute;left:10px;top:4500px;width:50px;height:50px;">Far</div>';
void(0)"#;
    let out = headless_json(
        &["browser", "eval", js, "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "inject offscreen fixture");

    // Read scroll position before.
    let scroll_before = headless_json(
        &[
            "browser",
            "eval",
            "window.scrollY",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&scroll_before, "scroll before");
    let before_y = parse_json(&scroll_before)["data"]["value"]
        .as_f64()
        .unwrap_or(-1.0);

    // Run browser box on offscreen element.
    let out = headless_json(
        &[
            "browser",
            "box",
            "#offscreen",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "box offscreen");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    // Element should report its absolute position.
    assert!(v["data"]["value"]["y"].as_f64().unwrap_or(0.0) > 4000.0);

    // Read scroll position after — must not have changed.
    let scroll_after = headless_json(
        &[
            "browser",
            "eval",
            "window.scrollY",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&scroll_after, "scroll after");
    let after_y = parse_json(&scroll_after)["data"]["value"]
        .as_f64()
        .unwrap_or(-1.0);

    assert_eq!(
        before_y, after_y,
        "browser box must not change scroll position (before={before_y}, after={after_y})"
    );
}

/// P1 regression: `browser box` must surface `JS_EXCEPTION` when
/// `getBoundingClientRect` throws instead of silently returning empty `{}`.
#[test]
fn box_js_exception_propagated() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // Monkeypatch getBoundingClientRect on the target element.
    let js = r#"document.querySelector('#target').getBoundingClientRect = () => { throw new Error("boom"); };
void(0)"#;
    let out = headless_json(
        &["browser", "eval", js, "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "monkeypatch getBoundingClientRect");

    let out = headless_json(
        &[
            "browser",
            "box",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "box js exception");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.box");
    assert_error_envelope(&v, "JS_EXCEPTION");
}

/// P1 regression: `browser attrs` must surface `JS_EXCEPTION` when
/// attribute iteration throws instead of silently returning empty `{}`.
#[test]
fn attrs_js_exception_propagated() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // Monkeypatch the target's attributes getter to throw.
    let js = r#"Object.defineProperty(document.querySelector('#target'), 'attributes', {
  get() { throw new Error("boom"); }
});
void(0)"#;
    let out = headless_json(
        &["browser", "eval", js, "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "monkeypatch attributes");

    let out = headless_json(
        &[
            "browser",
            "attrs",
            TARGET_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "attrs js exception");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.attrs");
    assert_error_envelope(&v, "JS_EXCEPTION");
}
