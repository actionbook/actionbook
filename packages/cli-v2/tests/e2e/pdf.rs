//! E2E tests for `browser pdf`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};

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
    let js = r#"document.body.innerHTML = `
  <main>
    <h1>PDF Contract Fixture</h1>
    <p>Export this page to PDF.</p>
  </main>
`;
document.title = 'PDF Fixture';
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
fn pdf_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("fixture.pdf");
    let path_str = path.to_string_lossy().to_string();

    let out = headless_json(
        &[
            "browser",
            "pdf",
            &path_str,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "pdf json");
    let v = parse_json(&out);
    let metadata = std::fs::metadata(&path).expect("pdf file should exist");

    assert_eq!(v["command"], "browser.pdf");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "PDF Fixture");
    assert_eq!(v["data"]["artifact"]["path"], path_str);
    assert_eq!(v["data"]["artifact"]["mime_type"], "application/pdf");
    assert_eq!(v["data"]["artifact"]["bytes"], metadata.len());
    assert!(
        metadata.len() > 0,
        "pdf file should be non-empty: {}",
        path.display()
    );
}

#[test]
fn pdf_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("fixture.pdf");
    let path_str = path.to_string_lossy().to_string();

    let out = headless(
        &[
            "browser",
            "pdf",
            &path_str,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&out, "pdf text");
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
    assert_eq!(lines.get(1), Some(&"ok browser.pdf"));
    assert_eq!(lines.get(2), Some(&format!("path: {path_str}").as_str()));
}

#[test]
fn pdf_artifact_write_failed_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let tmp = tempfile::tempdir().expect("create temp dir");
    let bad_path = tmp.path().to_string_lossy().to_string();

    let out = headless_json(
        &[
            "browser",
            "pdf",
            &bad_path,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_failure(&out, "pdf bad path");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.pdf");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ARTIFACT_WRITE_FAILED");
}

#[test]
fn pdf_session_not_found_json() {
    if skip() {
        return;
    }

    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("missing-session.pdf");
    let path_str = path.to_string_lossy().to_string();

    let out = headless_json(
        &[
            "browser",
            "pdf",
            &path_str,
            "--session",
            "missing-session",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "pdf missing session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.pdf");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn pdf_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("missing-tab.pdf");
    let path_str = path.to_string_lossy().to_string();

    let out = headless_json(
        &[
            "browser",
            "pdf",
            &path_str,
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "pdf missing tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.pdf");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}
