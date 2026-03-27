//! Contract E2E tests for Phase B2b interaction / wait / eval commands.
//!
//! This file starts with the command shapes that are already stable enough to
//! pin on `release/1.0.0`. Coordinate-based click/drag coverage will be added
//! after the corresponding CLI/action parity work lands.

use crate::harness::{
    append_body_html_js, assert_success, headless, headless_json, set_body_html_js, skip,
    stdout_str, SessionGuard,
};
use serde_json::Value;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn parse_envelope(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

fn assert_envelope(v: &Value, expected_command: &str) {
    assert_eq!(v["ok"], true, "ok should be true, got: {}", v);
    assert_eq!(
        v["command"], expected_command,
        "command should be {expected_command}, got: {}",
        v["command"]
    );
    assert!(v["error"].is_null(), "error should be null, got: {}", v);
    assert!(
        v["meta"]["duration_ms"].as_u64().is_some(),
        "meta.duration_ms should be present, got: {}",
        v["meta"]
    );
}

fn start_session() -> (String, String) {
    let session_id = format!("b2b-{}", SESSION_COUNTER.fetch_add(1, Ordering::Relaxed));
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            &session_id,
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&out, "start session for b2b contract test");
    let out = headless_json(&["browser", "list-tabs", "-s", &session_id], 15);
    assert_success(&out, "list-tabs after start");
    let json = parse_envelope(&out);
    let tab_id = json["data"]["tabs"][0]["tab_id"]
        .as_str()
        .expect("tab_id in list-tabs data")
        .to_string();
    (session_id, tab_id)
}

fn close_session(session_id: &str) {
    let _ = headless(&["browser", "close", "-s", session_id], 15);
}

#[test]
fn contract_b2b_click_selector_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js =
        set_body_html_js(r#"<button id="btn" onclick="window.__clicked = true">Click</button>"#);
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject clickable button");

    let out = headless_json(&["browser", "click", "#btn", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "click json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.click");
    assert_eq!(json["data"]["action"], "click");
    assert_eq!(json["data"]["target"]["selector"], "#btn");
    assert!(
        json["data"]["changed"]["focus_changed"].is_boolean(),
        "click changed.focus_changed should be boolean, got: {}",
        json["data"]
    );

    let out = headless(&["browser", "click", "#btn", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "click text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.click"), "got: {text}");
    assert!(text.contains("target: #btn"), "got: {text}");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.__clicked === true",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify click effect");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    close_session(&sid);
}

#[test]
fn contract_b2b_type_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = set_body_html_js(r#"<input id="msg" value="" />"#);
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject input");

    let out = headless_json(
        &[
            "browser",
            "type",
            "hello world",
            "-s",
            &sid,
            "-t",
            &tid,
            "#msg",
        ],
        15,
    );
    assert_success(&out, "type json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.type");
    assert_eq!(json["data"]["action"], "type");
    assert_eq!(json["data"]["target"]["selector"], "#msg");
    assert_eq!(json["data"]["value_summary"]["text_length"], 11);

    let out = headless(
        &[
            "browser",
            "type",
            "hello world",
            "-s",
            &sid,
            "-t",
            &tid,
            "#msg",
        ],
        15,
    );
    assert_success(&out, "type text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.type"), "got: {text}");
    assert!(text.contains("target: #msg"), "got: {text}");
    assert!(text.contains("text_length: 11"), "got: {text}");

    let out = headless(&["browser", "value", "#msg", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "verify typed value");
    assert!(
        stdout_str(&out).contains("hello world"),
        "got: {}",
        stdout_str(&out)
    );

    close_session(&sid);
}

#[test]
fn contract_b2b_select_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = set_body_html_js(
        r#"<select id="sel"><option value="a">A</option><option value="b">B</option></select>"#,
    );
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject select");

    let out = headless_json(
        &[
            "browser",
            "select",
            "b",
            "--selector",
            "#sel",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "select json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.select");
    assert_eq!(json["data"]["action"], "select");
    assert_eq!(json["data"]["target"]["selector"], "#sel");
    assert_eq!(json["data"]["value_summary"]["value"], "b");

    let out = headless(
        &[
            "browser",
            "select",
            "b",
            "--selector",
            "#sel",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "select text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.select"), "got: {text}");
    assert!(text.contains("target: #sel"), "got: {text}");
    assert!(text.contains("value: b"), "got: {text}");

    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#sel').value",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify selected value");
    assert!(stdout_str(&out).contains("b"), "got: {}", stdout_str(&out));

    close_session(&sid);
}

#[test]
fn contract_b2b_upload_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = set_body_html_js(r#"<input id="upload" type="file" multiple />"#);
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject file input");

    let mut file_a = tempfile::NamedTempFile::new().expect("temp file a");
    writeln!(file_a, "alpha").expect("write file a");
    let mut file_b = tempfile::NamedTempFile::new().expect("temp file b");
    writeln!(file_b, "beta").expect("write file b");
    let path_a = file_a.path().to_string_lossy().to_string();
    let path_b = file_b.path().to_string_lossy().to_string();

    let out = headless_json(
        &[
            "browser",
            "upload",
            &path_a,
            &path_b,
            "--selector",
            "#upload",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        20,
    );
    assert_success(&out, "upload json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.upload");
    assert_eq!(json["data"]["action"], "upload");
    assert_eq!(json["data"]["target"]["selector"], "#upload");
    assert_eq!(json["data"]["value_summary"]["count"], 2);

    let out = headless(
        &[
            "browser",
            "upload",
            &path_a,
            &path_b,
            "--selector",
            "#upload",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        20,
    );
    assert_success(&out, "upload text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.upload"), "got: {text}");
    assert!(text.contains("target: #upload"), "got: {text}");
    assert!(text.contains("count: 2"), "got: {text}");

    close_session(&sid);
}

#[test]
fn contract_b2b_scroll_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = set_body_html_js(r#"<div id="anchor" style="margin-top: 3000px">Anchor</div>"#);
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject tall page");

    let out = headless_json(
        &[
            "browser", "scroll", "down", "--amount", "240", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    assert_success(&out, "scroll down json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.scroll");
    assert_eq!(json["data"]["action"], "scroll");
    assert_eq!(json["data"]["direction"], "down");
    assert_eq!(json["data"]["amount"], 240);

    let out = headless(
        &[
            "browser",
            "scroll",
            "into-view",
            "#anchor",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll into-view text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.scroll"), "got: {text}");
    assert!(text.contains("direction: into-view"), "got: {text}");
    assert!(text.contains("target: #anchor"), "got: {text}");

    close_session(&sid);
}

#[test]
fn contract_b2b_eval_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless_json(&["browser", "eval", "1 + 1", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "eval json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.eval");
    assert_eq!(json["data"]["value"], 2);
    assert_eq!(json["data"]["type"], "number");
    assert_eq!(json["data"]["preview"], "2");

    let out = headless(&["browser", "eval", "1 + 1", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "eval text");
    let text = stdout_str(&out);
    assert_eq!(text.trim(), "2");

    close_session(&sid);
}

#[test]
fn contract_b2b_waits_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = set_body_html_js(
        r#"<div id="ready">Ready</div><a id="nav" href="data:text/html,<title>After</title><h1>After</h1>">Next</a>"#,
    );
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject wait fixtures");

    let out = headless_json(
        &[
            "browser",
            "wait",
            "element",
            "#ready",
            "-s",
            &sid,
            "-t",
            &tid,
            "--timeout",
            "5000",
        ],
        15,
    );
    assert_success(&out, "wait element json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.wait.element");
    assert_eq!(json["data"]["kind"], "element");
    assert_eq!(json["data"]["satisfied"], true);
    assert_eq!(json["data"]["observed_value"]["selector"], "#ready");

    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            &sid,
            "-t",
            &tid,
            "--timeout",
            "5000",
        ],
        15,
    );
    assert_success(&out, "wait condition text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.wait.condition"), "got: {text}");
    assert!(text.contains("elapsed_ms:"), "got: {text}");
    assert!(text.contains("observed_value: true"), "got: {text}");

    let out = headless(&["browser", "click", "#nav", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "click nav link");

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "-s",
            &sid,
            "-t",
            &tid,
            "--timeout",
            "10000",
        ],
        20,
    );
    assert_success(&out, "wait navigation json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.wait.navigation");
    assert_eq!(json["data"]["kind"], "navigation");
    assert_eq!(json["data"]["satisfied"], true);
    assert!(
        json["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or_default()
            .contains("data:text/html"),
        "navigation observed url should contain data URL, got: {}",
        json["data"]
    );

    let out = headless(
        &[
            "browser",
            "wait",
            "network-idle",
            "-s",
            &sid,
            "-t",
            &tid,
            "--timeout",
            "5000",
        ],
        20,
    );
    assert_success(&out, "wait network-idle text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.wait.network-idle"), "got: {text}");
    assert!(text.contains("elapsed_ms:"), "got: {text}");

    close_session(&sid);
}

#[test]
fn contract_b2b_append_html_helper_stays_trusted_types_safe() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let out = headless(
        &[
            "browser",
            "eval",
            &append_body_html_js(r#"<div id="tail">Tail</div>"#),
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "append html");

    let out = headless(&["browser", "text", "#tail", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "verify appended text");
    assert!(
        stdout_str(&out).contains("Tail"),
        "got: {}",
        stdout_str(&out)
    );

    close_session(&sid);
}
