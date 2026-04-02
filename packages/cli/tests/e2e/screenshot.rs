//! E2E tests for `browser screenshot <path>` command (§10.2).
//!
//! Uses about:blank with injected DOM fixtures to avoid external network dependency.
//! The fixture uses hardcoded safe HTML — no user-supplied content is involved.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, wait_page_ready,
};

// ── Helpers ───────────────────────────────────────────────────────────

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
    wait_page_ready(&sid, &tid);
    (sid, tid)
}

/// Inject a simple DOM fixture with an h1 and a button.
/// Uses hardcoded safe HTML only — no user-supplied content.
fn inject_fixture(sid: &str, tid: &str) {
    // Safe: all content is hardcoded test data, not user input
    let js = r#"document.body.textContent = ''; var h = document.createElement('h1'); h.style.cssText = 'margin:0;padding:20px'; h.textContent = 'Hello World'; document.body.appendChild(h); var b = document.createElement('button'); b.id = 'btn'; b.style.margin = '20px'; b.textContent = 'Click Me'; document.body.appendChild(b); void(0)"#;
    let out = headless_json(&["browser", "eval", js, "--session", sid, "--tab", tid], 10);
    assert_success(&out, "inject fixture");
}

fn close_session(session_id: &str) {
    let _ = headless(&["browser", "close", "--session", session_id], 30);
}

fn assert_meta(v: &serde_json::Value) {
    assert!(v["meta"]["duration_ms"].is_number());
    assert!(v["meta"]["warnings"].is_array());
    assert!(v["meta"]["pagination"].is_null());
    assert!(v["meta"]["truncated"].is_boolean());
}

fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false, "ok must be false on error");
    assert!(v["data"].is_null(), "data must be null on failure");
    assert_eq!(v["error"]["code"], expected_code);
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert_meta(v);
}

// ===========================================================================
// Group 1: Happy path
// ===========================================================================

#[test]
fn screenshot_json_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    // Remove the temp file so screenshot writes to a clean path
    drop(tmp);
    let path_png = format!("{path}.png");

    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path_png,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "screenshot json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser screenshot");
    assert!(v["error"].is_null());
    assert_meta(&v);

    // context
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);

    // data.artifact
    let artifact = &v["data"]["artifact"];
    assert!(artifact.is_object(), "data.artifact must be object");
    assert!(
        artifact["path"].as_str().unwrap().contains(&path),
        "artifact.path must contain the output path"
    );
    assert_eq!(artifact["mime_type"], "image/png");
    let bytes = artifact["bytes"].as_u64().unwrap();
    assert!(bytes > 0, "artifact.bytes must be > 0");

    // File must exist on disk with matching size
    let file_meta = std::fs::metadata(artifact["path"].as_str().unwrap());
    assert!(file_meta.is_ok(), "screenshot file must exist on disk");
    assert_eq!(
        file_meta.unwrap().len(),
        bytes,
        "file size must match artifact.bytes"
    );

    // Cleanup
    let _ = std::fs::remove_file(artifact["path"].as_str().unwrap());
    close_session(&sid);
}

#[test]
fn screenshot_text_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = format!("{}.png", tmp.path().to_string_lossy());
    drop(tmp);

    let out = headless(
        &[
            "browser",
            "screenshot",
            &path,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "screenshot text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    // Header: [sid tid] url
    assert!(
        lines[0].starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: got {}",
        lines[0]
    );

    // "ok browser screenshot" line
    assert!(
        text.contains("ok browser screenshot"),
        "text must contain 'ok browser screenshot': got {text:.400}"
    );

    // "path: ..." line
    assert!(
        text.contains("path: "),
        "text must contain 'path: ': got {text:.400}"
    );

    // Cleanup
    let _ = std::fs::remove_file(&path);
    close_session(&sid);
}

#[test]
fn screenshot_jpeg_format() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = format!("{}.jpg", tmp.path().to_string_lossy());
    drop(tmp);

    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--screenshot-format",
            "jpeg",
        ],
        15,
    );
    assert_success(&out, "screenshot jpeg");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["artifact"]["mime_type"], "image/jpeg");
    assert!(v["data"]["artifact"]["bytes"].as_u64().unwrap() > 0);

    let _ = std::fs::remove_file(v["data"]["artifact"]["path"].as_str().unwrap());
    close_session(&sid);
}

#[test]
fn screenshot_full_page() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = format!("{}.png", tmp.path().to_string_lossy());
    drop(tmp);

    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--full",
        ],
        15,
    );
    assert_success(&out, "screenshot full");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["data"]["artifact"]["bytes"].as_u64().unwrap() > 0);

    let _ = std::fs::remove_file(v["data"]["artifact"]["path"].as_str().unwrap());
    close_session(&sid);
}

#[test]
fn screenshot_selector() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = format!("{}.png", tmp.path().to_string_lossy());
    drop(tmp);

    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--selector",
            "h1",
        ],
        15,
    );
    assert_success(&out, "screenshot selector");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["data"]["artifact"]["bytes"].as_u64().unwrap() > 0);

    let _ = std::fs::remove_file(v["data"]["artifact"]["path"].as_str().unwrap());
    close_session(&sid);
}

#[test]
fn screenshot_annotate() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    // First, run snapshot to populate RefCache with refs
    let snap_out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&snap_out, "snapshot before annotate");

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = format!("{}.png", tmp.path().to_string_lossy());
    drop(tmp);

    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--annotate",
        ],
        15,
    );
    assert_success(&out, "screenshot annotate");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["data"]["artifact"]["bytes"].as_u64().unwrap() > 0);

    // Annotations must be present and non-empty (fixture has interactive elements)
    let annotations = v["data"]["annotations"].as_array();
    assert!(
        annotations.is_some(),
        "data.annotations must be an array when --annotate is used"
    );
    let annotations = annotations.unwrap();
    assert!(
        !annotations.is_empty(),
        "annotations must not be empty for fixture with interactive elements"
    );

    // Each annotation must have ref, number, role, name, box
    for (i, ann) in annotations.iter().enumerate() {
        assert!(
            ann["ref"].is_string(),
            "annotations[{i}].ref must be a string"
        );
        assert!(
            ann["number"].is_number(),
            "annotations[{i}].number must be a number"
        );
        assert!(
            ann["role"].is_string(),
            "annotations[{i}].role must be a string"
        );
        assert!(
            ann["name"].is_string(),
            "annotations[{i}].name must be a string"
        );
        assert!(
            ann["box"].is_object(),
            "annotations[{i}].box must be an object"
        );
        assert!(
            ann["box"]["x"].is_number(),
            "annotations[{i}].box.x must be a number"
        );
        assert!(
            ann["box"]["y"].is_number(),
            "annotations[{i}].box.y must be a number"
        );
        assert!(
            ann["box"]["width"].is_number(),
            "annotations[{i}].box.width must be a number"
        );
        assert!(
            ann["box"]["height"].is_number(),
            "annotations[{i}].box.height must be a number"
        );
    }

    let _ = std::fs::remove_file(v["data"]["artifact"]["path"].as_str().unwrap());
    close_session(&sid);
}

// ===========================================================================
// Group 2: Error paths
// ===========================================================================

#[test]
fn screenshot_invalid_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    // Non-existent directory
    let out = headless_json(
        &[
            "browser",
            "screenshot",
            "/nonexistent/dir/test.png",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_failure(&out, "screenshot invalid path");
    let v = parse_json(&out);
    assert_error_envelope(&v, "ARTIFACT_WRITE_FAILED");

    close_session(&sid);
}

#[test]
fn screenshot_session_not_found_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "screenshot",
            "/tmp/test.png",
            "--session",
            "nonexistent",
            "--tab",
            "t0",
        ],
        10,
    );
    assert_failure(&out, "screenshot session not found");
    let v = parse_json(&out);
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null on SESSION_NOT_FOUND"
    );
}

#[test]
fn screenshot_session_not_found_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "screenshot",
            "/tmp/test.png",
            "--session",
            "nonexistent",
            "--tab",
            "t0",
        ],
        10,
    );
    assert_failure(&out, "screenshot session not found text");
    let text = stdout_str(&out);
    assert!(
        text.contains("SESSION_NOT_FOUND"),
        "text must contain SESSION_NOT_FOUND: got {text:.200}"
    );
}

#[test]
fn screenshot_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "screenshot",
            "/tmp/test.png",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab",
        ],
        10,
    );
    assert_failure(&out, "screenshot tab not found");
    let v = parse_json(&out);
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "tab_id must be null on TAB_NOT_FOUND"
    );

    close_session(&sid);
}

#[test]
fn screenshot_missing_session_arg() {
    if skip() {
        return;
    }
    let out = headless_json(
        &["browser", "screenshot", "/tmp/test.png", "--tab", "t0"],
        10,
    );
    assert_failure(&out, "screenshot missing --session");
}

#[test]
fn screenshot_missing_tab_arg() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "screenshot",
            "/tmp/test.png",
            "--session",
            "any-sid",
        ],
        10,
    );
    assert_failure(&out, "screenshot missing --tab");
}
