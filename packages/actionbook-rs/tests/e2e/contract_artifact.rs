//! Wave 5 contract E2E tests for screenshot / pdf artifact commands.
//!
//! Covers PRD field-level contracts for:
//! - `browser screenshot`
//! - `browser pdf`
//! - artifact JSON fields (`path`, `mime_type`, `bytes`)
//! - text output shape
//! - screenshot flag paths (`--full`, `--annotate`, `--screenshot-quality`,
//!   `--screenshot-format`, `--selector`)

use crate::harness::{
    assert_success, headless, headless_json, set_body_html_js, skip, stdout_str, SessionGuard,
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
    assert_success(&out, "start artifact session");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from start");
    let session_id = json["data"]["session"]["session_id"]
        .as_str()
        .expect("data.session.session_id")
        .to_string();
    let tab_id = json["data"]["tab"]["tab_id"]
        .as_str()
        .expect("data.tab.tab_id")
        .to_string();
    (session_id, tab_id)
}

fn setup_fixture(session_id: &str, tab_id: &str) {
    let html = r#"
<main id="capture-root" style="min-height: 1800px; padding: 24px;">
  <h1>Artifact Contract Page</h1>
  <p>Screenshot and PDF fixture content.</p>
  <button id="cta">Primary Action</button>
  <section style="margin-top: 1400px;">Bottom marker</section>
</main>
"#;

    let set_title = headless(
        &[
            "browser",
            "eval",
            "document.title = 'Artifact Contract Page'",
            "-s",
            session_id,
            "-t",
            tab_id,
        ],
        15,
    );
    assert_success(&set_title, "set document.title");

    let setup_js = set_body_html_js(html);
    let setup_out = headless(
        &["browser", "eval", &setup_js, "-s", session_id, "-t", tab_id],
        15,
    );
    assert_success(&setup_out, "inject artifact fixture");
}

fn assert_context(json: &serde_json::Value, session_id: &str, tab_id: &str) {
    assert_eq!(json["context"]["session_id"], session_id);
    assert_eq!(json["context"]["tab_id"], tab_id);
    assert_eq!(json["context"]["url"], "about:blank");
    assert_eq!(json["context"]["title"], "Artifact Contract Page");
}

fn assert_prefixed_header(text: &str, session_id: &str, tab_id: &str) {
    let expected = format!("[{session_id} {tab_id}] about:blank");
    let first_line = text.lines().next().unwrap_or("");
    assert_eq!(
        first_line, expected,
        "text output must start with PRD header, got:\n{text}"
    );
}

fn make_temp_path(suffix: &str) -> String {
    let tmp = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("create temp file");
    let path = tmp.path().to_string_lossy().to_string();
    drop(tmp);
    path
}

fn assert_artifact(json: &serde_json::Value, expected_path: &str, expected_mime: &str) {
    assert_eq!(json["data"]["artifact"]["path"], expected_path);
    assert_eq!(json["data"]["artifact"]["mime_type"], expected_mime);
    let reported_bytes = json["data"]["artifact"]["bytes"]
        .as_u64()
        .expect("artifact.bytes");
    assert!(reported_bytes > 0, "artifact.bytes must be > 0");

    let metadata = std::fs::metadata(expected_path).expect("artifact file exists");
    assert_eq!(
        metadata.len(),
        reported_bytes,
        "artifact.bytes must match file size for {expected_path}"
    );
}

#[test]
fn contract_artifact_screenshot_json_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let path = make_temp_path(".png");
    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path,
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "browser screenshot --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from screenshot");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.screenshot");
    assert_context(&json, &session_id, &tab_id);
    assert_artifact(&json, &path, "image/png");

    let _ = std::fs::remove_file(&path);
    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_artifact_screenshot_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let path = make_temp_path(".png");
    let out = headless(
        &[
            "browser",
            "screenshot",
            &path,
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "browser screenshot text");
    let text = stdout_str(&out);
    assert_prefixed_header(&text, &session_id, &tab_id);
    assert!(text.contains("ok browser.screenshot"));
    assert!(text.contains(&format!("path: {path}")));

    let _ = std::fs::remove_file(&path);
    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_artifact_screenshot_flag_paths() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let cases = vec![
        (
            "full",
            vec!["--full".to_string()],
            ".png".to_string(),
            "image/png".to_string(),
        ),
        (
            "annotate",
            vec!["--annotate".to_string()],
            ".png".to_string(),
            "image/png".to_string(),
        ),
        (
            "selector",
            vec!["--selector".to_string(), "body".to_string()],
            ".png".to_string(),
            "image/png".to_string(),
        ),
        (
            "jpeg",
            vec![
                "--screenshot-format".to_string(),
                "jpeg".to_string(),
                "--screenshot-quality".to_string(),
                "60".to_string(),
            ],
            ".jpg".to_string(),
            "image/jpeg".to_string(),
        ),
    ];

    for (label, extra, suffix, expected_mime) in cases {
        let path = make_temp_path(&suffix);
        let mut args = vec![
            "browser".to_string(),
            "screenshot".to_string(),
            path.clone(),
            "-s".to_string(),
            session_id.clone(),
            "-t".to_string(),
            tab_id.clone(),
        ];
        args.extend(extra);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let out = headless_json(&arg_refs, 30);
        assert_success(&out, &format!("browser screenshot {label} --json"));
        let json: serde_json::Value =
            serde_json::from_str(&stdout_str(&out)).expect("valid JSON from screenshot flag case");
        assert_eq!(json["command"], "browser.screenshot");
        assert_context(&json, &session_id, &tab_id);
        assert_artifact(&json, &path, &expected_mime);
        let _ = std::fs::remove_file(&path);
    }

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_artifact_pdf_json_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let path = make_temp_path(".pdf");
    let out = headless_json(
        &["browser", "pdf", &path, "-s", &session_id, "-t", &tab_id],
        30,
    );
    assert_success(&out, "browser pdf --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&out)).expect("valid JSON from pdf");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.pdf");
    assert_context(&json, &session_id, &tab_id);
    assert_artifact(&json, &path, "application/pdf");

    let _ = std::fs::remove_file(&path);
    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_artifact_pdf_text_output() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let path = make_temp_path(".pdf");
    let out = headless(
        &["browser", "pdf", &path, "-s", &session_id, "-t", &tab_id],
        30,
    );
    assert_success(&out, "browser pdf text");
    let text = stdout_str(&out);
    assert_prefixed_header(&text, &session_id, &tab_id);
    assert!(text.contains("ok browser.pdf"));
    assert!(text.contains(&format!("path: {path}")));

    let _ = std::fs::remove_file(&path);
    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}
