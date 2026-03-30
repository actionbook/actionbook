//! Contract E2E tests for Phase B2b interaction / wait / eval commands.

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
    assert!(
        v["context"]["session_id"].as_str().is_some(),
        "context.session_id should be present, got: {}",
        v["context"]
    );
    assert!(
        v["context"]["tab_id"].as_str().is_some(),
        "context.tab_id should be present, got: {}",
        v["context"]
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
fn contract_b2b_click_coordinates_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = set_body_html_js(
        r#"<button id="coord-btn" style="position:absolute; left:40px; top:50px; width:120px; height:80px;" onclick="window.__coordClicked = true">Click</button>"#,
    );
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject coordinate click target");

    let out = headless_json(&["browser", "click", "80,90", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "click coordinates json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.click");
    assert_eq!(json["data"]["action"], "click");
    assert_eq!(json["data"]["target"]["selector"], "80,90");

    let out = headless(&["browser", "click", "80,90", "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "click coordinates text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.click"), "got: {text}");
    assert!(text.contains("target: 80,90"), "got: {text}");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.__coordClicked === true",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify coordinate click effect");
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
            "#msg",
            "hello world",
            "-s",
            &sid,
            "-t",
            &tid,
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
            "#msg",
            "hello world",
            "-s",
            &sid,
            "-t",
            &tid,
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
        &["browser", "select", "#sel", "b", "-s", &sid, "-t", &tid],
        15,
    );
    assert_success(&out, "select json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.select");
    assert_eq!(json["data"]["action"], "select");
    assert_eq!(json["data"]["target"]["selector"], "#sel");
    assert_eq!(json["data"]["value_summary"]["value"], "b");

    let out = headless(
        &["browser", "select", "#sel", "b", "-s", &sid, "-t", &tid],
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
            "browser", "upload", "#upload", &path_a, &path_b, "-s", &sid, "-t", &tid,
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
            "browser", "upload", "#upload", &path_a, &path_b, "-s", &sid, "-t", &tid,
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

    let setup_js = set_body_html_js(
        r#"
        <div id="scrollbox" style="width: 180px; height: 180px; overflow: auto; border: 1px solid #999;">
          <div style="width: 1400px; height: 1400px; position: relative;">
            <div id="anchor" style="position: absolute; left: 1100px; top: 1100px; width: 40px; height: 40px;">Anchor</div>
          </div>
        </div>
        "#,
    );
    let out = headless(&["browser", "eval", &setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject scroll fixtures");

    let out = headless_json(
        &[
            "browser",
            "scroll",
            "down",
            "240",
            "--container",
            "#scrollbox",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll down json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.scroll");
    assert_eq!(json["data"]["action"], "scroll");
    assert_eq!(json["data"]["direction"], "down");
    assert_eq!(json["data"]["amount"], 240);
    assert_eq!(json["data"]["changed"]["scroll_changed"], true);

    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#scrollbox').scrollTop >= 240",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll down");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    let out = headless(
        &[
            "browser",
            "scroll",
            "right",
            "180",
            "--container",
            "#scrollbox",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll right");

    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#scrollbox').scrollLeft >= 180",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll right");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    let out = headless(
        &[
            "browser",
            "scroll",
            "left",
            "60",
            "--container",
            "#scrollbox",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll left");

    let out = headless(
        &[
            "browser",
            "eval",
            "(() => { const el = document.querySelector('#scrollbox'); return el.scrollLeft > 0 && el.scrollLeft < 180; })()",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll left");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    let out = headless(
        &[
            "browser",
            "scroll",
            "up",
            "40",
            "--container",
            "#scrollbox",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll up");

    let out = headless(
        &[
            "browser",
            "eval",
            "(() => { const el = document.querySelector('#scrollbox'); return el.scrollTop > 0 && el.scrollTop < 240; })()",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll up");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    let out = headless(
        &[
            "browser",
            "scroll",
            "bottom",
            "--container",
            "#scrollbox",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll bottom");

    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#scrollbox').scrollTop > 1000",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll bottom");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    let out = headless(
        &[
            "browser",
            "scroll",
            "top",
            "--container",
            "#scrollbox",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "scroll top");

    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#scrollbox').scrollTop === 0",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll top");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

    let out = headless(
        &[
            "browser",
            "scroll",
            "into-view",
            "#anchor",
            "--align",
            "center",
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

    let out = headless(
        &[
            "browser",
            "eval",
            "(() => { const el = document.querySelector('#scrollbox'); return el.scrollTop > 0 && el.scrollLeft > 0; })()",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify scroll into-view");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

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
            "browser", "wait", "element", "#ready", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    assert_success(&out, "wait element json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.wait.element");
    assert_eq!(json["data"]["kind"], "element");
    assert_eq!(json["data"]["satisfied"], true);
    assert!(
        json["data"]["elapsed_ms"].as_u64().is_some(),
        "wait element elapsed_ms should be present, got: {}",
        json["data"]
    );
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
        json["data"]["elapsed_ms"].as_u64().is_some(),
        "wait navigation elapsed_ms should be present, got: {}",
        json["data"]
    );
    assert!(
        json["data"]["observed_value"]["url"].as_str().is_some(),
        "navigation observed_value.url should be a string, got: {}",
        json["data"]
    );
    assert!(
        json["data"]["observed_value"]["ready_state"]
            .as_str()
            .is_some(),
        "navigation observed_value.ready_state should be a string, got: {}",
        json["data"]
    );

    let out = headless(
        &["browser", "wait", "network-idle", "-s", &sid, "-t", &tid],
        20,
    );
    assert_success(&out, "wait network-idle text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.wait.network-idle"), "got: {text}");
    assert!(text.contains("elapsed_ms:"), "got: {text}");

    close_session(&sid);
}

#[test]
fn contract_b2b_drag_coordinates_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session();

    let setup_js = r#"(() => {
        document.body.innerHTML = '';
        const source = document.createElement('div');
        source.id = 'source';
        source.style.cssText = 'position:absolute; left:20px; top:30px; width:60px; height:60px; background:#f66;';
        document.body.appendChild(source);
        window.__dragStarted = false;
        window.__dragEnd = null;
        source.addEventListener('mousedown', () => {
            window.__dragStarted = true;
        });
        document.addEventListener('mouseup', (event) => {
            window.__dragEnd = { x: event.clientX, y: event.clientY };
        });
        return 'ready';
    })()"#;
    let out = headless(&["browser", "eval", setup_js, "-s", &sid, "-t", &tid], 15);
    assert_success(&out, "inject drag fixtures");

    let out = headless_json(
        &[
            "browser", "drag", "#source", "300,400", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    assert_success(&out, "drag coordinates json");
    let json = parse_envelope(&out);
    assert_envelope(&json, "browser.drag");
    assert_eq!(json["data"]["action"], "drag");
    assert_eq!(json["data"]["target"]["from"]["selector"], "#source");
    assert_eq!(json["data"]["target"]["to"]["selector"], "300,400");

    let out = headless(
        &[
            "browser", "drag", "#source", "300,400", "-s", &sid, "-t", &tid,
        ],
        15,
    );
    assert_success(&out, "drag coordinates text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.drag"), "got: {text}");
    assert!(text.contains("from: #source"), "got: {text}");
    assert!(text.contains("to: 300,400"), "got: {text}");

    let out = headless(
        &[
            "browser",
            "eval",
            "window.__dragStarted === true && window.__dragEnd !== null && Math.abs(window.__dragEnd.x - 300) <= 5 && Math.abs(window.__dragEnd.y - 400) <= 5",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        15,
    );
    assert_success(&out, "verify drag coordinates effect");
    assert!(
        stdout_str(&out).contains("true"),
        "got: {}",
        stdout_str(&out)
    );

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
