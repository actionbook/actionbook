//! E2E tests for `browser inspect-point` command (§10.11).
//!
//! Uses a deterministic DOM fixture injected via `browser eval` on `about:blank`
//! to avoid dependence on external URLs or page layout.
//!
//! **Expected to FAIL until implementation lands:**
//! - `inspect_point_json_happy_path`
//! - `inspect_point_text_happy_path`
//! - `inspect_point_with_parent_depth`
//! - `inspect_point_no_element`
//!
//! **Expected to PASS against stub (error paths handled before command logic):**
//! - `inspect_point_session_not_found_json` / `_text`
//! - `inspect_point_tab_not_found_json`
//! - `inspect_point_missing_session_arg` / `_missing_tab_arg`
//! - `inspect_point_invalid_coords`

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

// ── Helpers ───────────────────────────────────────────────────────────

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
    assert_success(&out, "start session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    // Ensure about:blank is loaded
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

/// Inject a deterministic DOM fixture with known positions and parent chain.
///
/// Structure (aria-labels on all containers for verifiable parent ordering):
/// ```html
/// <div id="outer" aria-label="Outer Container"
///      style="position:fixed;top:0;left:0;width:400px;height:400px">
///   <div id="inner" aria-label="Inner Container"
///        style="position:absolute;top:50px;left:50px;width:200px;height:200px">
///     <button id="target-btn" aria-label="Test Button"
///             style="position:absolute;top:20px;left:20px;width:100px;height:40px">
///       Click Me
///     </button>
///   </div>
/// </div>
/// ```
///
/// The button at (90, 90) — inside the fixed-position tree:
///   outer(0,0) → inner(50,50) → button(70,70 to 170,110)
///
/// Parent ordering with --parent-depth 2:
///   parents[0] = Inner Container (nearest), parents[1] = Outer Container
fn inject_fixture(sid: &str, tid: &str) {
    let js = r#"document.body.innerHTML = '<div id=\"outer\" aria-label=\"Outer Container\" style=\"position:fixed;top:0;left:0;width:400px;height:400px\"><div id=\"inner\" aria-label=\"Inner Container\" style=\"position:absolute;top:50px;left:50px;width:200px;height:200px\"><button id=\"target-btn\" aria-label=\"Test Button\" style=\"position:absolute;top:20px;left:20px;width:100px;height:40px\">Click Me</button></div></div>'; void(0)"#;
    let out = headless_json(&["browser", "eval", js, "--session", sid, "--tab", tid], 10);
    assert_success(&out, "inject fixture");
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

/// Assert full §2.4 meta structure.
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

/// Assert full §3.1 error envelope.
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

// ===========================================================================
// Group 1: Happy path — deterministic fixture
// ===========================================================================

/// Coordinates (90, 90) hit the button inside the fixture:
///   outer(0,0) → inner(50,50) → button(70,70..170,110)
const HIT_X: &str = "90";
const HIT_Y: &str = "90";
const HIT_COORDS: &str = "90,90";

#[test]
fn inspect_point_json_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            HIT_COORDS,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "inspect-point json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.inspect-point");
    assert!(v["error"].is_null());
    assert_meta(&v);

    // context — tab-level, including url per §2.5
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be a string (even for about:blank)"
    );

    // §10.11 data.point — must echo back the requested coordinates
    assert!(v["data"]["point"].is_object(), "data.point must be object");
    assert_eq!(v["data"]["point"]["x"], 90.0, "point.x must be 90");
    assert_eq!(v["data"]["point"]["y"], 90.0, "point.y must be 90");

    // §10.11 data.element — deterministic fixture guarantees a button
    let element = &v["data"]["element"];
    assert!(element.is_object(), "data.element must be object");
    assert_eq!(
        element["role"].as_str().unwrap_or(""),
        "button",
        "element.role must be 'button' for the fixture button"
    );
    assert_eq!(
        element["name"].as_str().unwrap_or(""),
        "Test Button",
        "element.name must be 'Test Button' (from aria-label)"
    );
    // selector is a ref (e.g. "e1", "e2") per mcfeng's direction
    let selector = element["selector"].as_str().unwrap_or("");
    assert!(
        !selector.is_empty(),
        "element.selector must be a non-empty ref"
    );

    // §10.11 data.parents — without --parent-depth, should be empty
    assert!(
        v["data"]["parents"].is_array(),
        "data.parents must be an array"
    );
    let parents = v["data"]["parents"].as_array().unwrap();
    assert_eq!(
        parents.len(),
        0,
        "parents must be empty without --parent-depth"
    );

    // No screenshot_path per mcfeng's direction
    assert!(
        v["data"].get("screenshot_path").is_none() || v["data"]["screenshot_path"].is_null(),
        "screenshot_path should not be present"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_text_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "inspect-point",
            HIT_COORDS,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "inspect-point text");
    let text = stdout_str(&out);

    // §2.5: header is `[sid tid] <url>`
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.len() >= 4,
        "text must have header + role + selector + point lines: got {text:.400}"
    );

    let header = lines[0];
    assert!(
        header.starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: got {header}"
    );
    // §2.5: header must include URL (even about:blank)
    assert!(
        header.contains("about:blank"),
        "header must contain URL per §2.5: got {header}"
    );

    // §10.11: line 2 = role "name" — must contain the button role and aria-label
    assert!(
        lines[1].contains("button") && lines[1].contains("Test Button"),
        "line 2 must contain role and name: got '{}'",
        lines[1]
    );

    // §10.11: line 3 = selector: <ref>
    assert!(
        lines[2].starts_with("selector: "),
        "line 3 must start with 'selector: ': got '{}'",
        lines[2]
    );

    // §10.11: last line = point: x,y
    let point_line = lines.iter().find(|l| l.starts_with("point: "));
    assert!(
        point_line.is_some(),
        "must contain 'point: x,y' line: got {text:.400}"
    );
    assert_eq!(
        point_line.unwrap(),
        &format!("point: {HIT_X},{HIT_Y}"),
        "point line must echo coordinates"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_with_parent_depth() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            HIT_COORDS,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--parent-depth",
            "2",
        ],
        10,
    );
    assert_success(&out, "inspect-point with parent-depth");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);

    // Element must still be the button
    assert_eq!(
        v["data"]["element"]["role"].as_str().unwrap_or(""),
        "button"
    );

    // Parents: with --parent-depth 2, fixture has button → #inner → #outer
    // So we expect exactly 2 parents in order: inner first, outer second
    let parents = v["data"]["parents"]
        .as_array()
        .expect("parents must be array");
    assert_eq!(
        parents.len(),
        2,
        "with --parent-depth 2, fixture should yield exactly 2 parents: got {}",
        parents.len()
    );

    // Each parent must have role, name, selector (ref)
    for (i, parent) in parents.iter().enumerate() {
        assert!(
            parent["role"].is_string(),
            "parents[{i}].role must be a string"
        );
        assert!(
            parent["name"].is_string(),
            "parents[{i}].name must be a string"
        );
        let sel = parent["selector"].as_str().unwrap_or("");
        assert!(
            !sel.is_empty(),
            "parents[{i}].selector must be a non-empty ref"
        );
    }

    // Parent order: nearest parent first (inner div), then outer div.
    // The fixture gives both containers distinct aria-labels so we can pin order exactly.
    assert_eq!(
        parents[0]["name"].as_str().unwrap_or(""),
        "Inner Container",
        "parents[0] must be the inner div (nearest parent)"
    );
    assert_eq!(
        parents[1]["name"].as_str().unwrap_or(""),
        "Outer Container",
        "parents[1] must be the outer div"
    );
    // Refs must be distinct
    assert_ne!(
        parents[0]["selector"], parents[1]["selector"],
        "parent refs must be distinct"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_no_element() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    // Don't inject fixture — about:blank with no content

    // Use coordinates far outside any content — should return null element
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "-9999,-9999",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "inspect-point no element");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["point"]["x"], -9999.0);
    assert_eq!(v["data"]["point"]["y"], -9999.0);
    // element should be null when no element at coordinates
    assert!(
        v["data"]["element"].is_null(),
        "element must be null when no element at coordinates: got {:?}",
        v["data"]["element"]
    );

    close_session(&sid);
}

// ===========================================================================
// Group 2: Error paths
// ===========================================================================

#[test]
fn inspect_point_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            "nonexistent",
            "--tab",
            "t0",
        ],
        10,
    );
    assert_failure(&out, "inspect-point session not found");
    let v = parse_json(&out);
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null on SESSION_NOT_FOUND"
    );
}

#[test]
fn inspect_point_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            "nonexistent",
            "--tab",
            "t0",
        ],
        10,
    );
    assert_failure(&out, "inspect-point session not found text");
    let text = stdout_str(&out);
    assert!(
        text.contains("SESSION_NOT_FOUND"),
        "text must contain SESSION_NOT_FOUND: got {text:.200}"
    );
}

#[test]
fn inspect_point_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab",
        ],
        10,
    );
    assert_failure(&out, "inspect-point tab not found");
    let v = parse_json(&out);
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "tab_id must be null on TAB_NOT_FOUND"
    );

    close_session(&sid);
}

#[test]
fn inspect_point_missing_session_arg() {
    if skip() {
        return;
    }
    let out = headless_json(&["browser", "inspect-point", "100,100", "--tab", "t0"], 10);
    assert_failure(&out, "inspect-point missing --session");
}

#[test]
fn inspect_point_missing_tab_arg() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "--session",
            "any-sid",
        ],
        10,
    );
    assert_failure(&out, "inspect-point missing --tab");
}

#[test]
fn inspect_point_invalid_coords() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    // Invalid coordinate format — must fail with INVALID_ARGUMENT
    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "not-valid",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "inspect-point invalid coords");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["code"].as_str().unwrap_or(""),
        "INVALID_ARGUMENT",
        "invalid coords must return INVALID_ARGUMENT error code"
    );

    close_session(&sid);
}
