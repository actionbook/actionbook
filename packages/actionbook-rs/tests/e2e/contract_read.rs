//! Wave 4 contract E2E tests for the browser read family.
//!
//! Covers PRD field-level contracts for:
//! - html / text / value / attr / attrs / box / styles / state
//! - text output headers
//! - text --mode raw|readability

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
    assert_success(&out, "start read-family session");
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
<article id="reader">
  <nav>Skip nav</nav>
  <h1>Story Title</h1>
  <p>Primary article copy.</p>
</article>
<input id="email" type="email" value="user@example.com" aria-label="Email field" data-kind="primary" />
<div id="box-target" style="position:absolute; left:10px; top:20px; width:120px; height:32px; color:rgb(255, 0, 0); font-size:18px;">Box text</div>
<textarea id="editor">Draft text</textarea>
"#;

    let set_title = headless(
        &[
            "browser",
            "eval",
            "document.title = 'Read Contract Page'",
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
    assert_success(&setup_out, "inject read-family fixture");

    let focus_out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#editor').focus()",
            "-s",
            session_id,
            "-t",
            tab_id,
        ],
        15,
    );
    assert_success(&focus_out, "focus editor");
}

fn assert_context(json: &serde_json::Value, session_id: &str, tab_id: &str) {
    assert_eq!(json["context"]["session_id"], session_id);
    assert_eq!(json["context"]["tab_id"], tab_id);
    assert_eq!(json["context"]["url"], "about:blank");
    assert_eq!(json["context"]["title"], "Read Contract Page");
}

fn assert_prefixed_header(text: &str, session_id: &str, tab_id: &str) {
    let expected = format!("[{session_id} {tab_id}] about:blank");
    let first_line = text.lines().next().unwrap_or("");
    assert_eq!(
        first_line, expected,
        "text output must start with PRD header, got:\n{text}"
    );
}

#[test]
fn contract_read_html_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let json_out = headless_json(
        &[
            "browser",
            "html",
            "#reader",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&json_out, "browser html --json");
    let json: serde_json::Value =
        serde_json::from_str(&stdout_str(&json_out)).expect("valid JSON from browser html");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "browser.html");
    assert_context(&json, &session_id, &tab_id);
    assert_eq!(json["data"]["target"]["selector"], "#reader");
    let html = json["data"]["value"].as_str().expect("data.value string");
    assert!(html.contains("<article id=\"reader\">"));
    assert!(html.contains("Story Title"));

    let text_out = headless(
        &[
            "browser",
            "html",
            "#reader",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&text_out, "browser html text");
    let text = stdout_str(&text_out);
    assert_prefixed_header(&text, &session_id, &tab_id);
    assert!(
        text.contains("<article id=\"reader\">"),
        "html text output must contain the element outerHTML, got:\n{text}"
    );

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_read_text_raw_and_readability() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let raw_out = headless_json(
        &[
            "browser",
            "text",
            "#reader",
            "--mode",
            "raw",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&raw_out, "browser text raw --json");
    let raw_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&raw_out)).expect("valid JSON from text raw");
    assert_eq!(raw_json["command"], "browser.text");
    assert_context(&raw_json, &session_id, &tab_id);
    assert_eq!(raw_json["data"]["target"]["selector"], "#reader");
    let raw_value = raw_json["data"]["value"].as_str().expect("raw text string");
    assert!(raw_value.contains("Skip nav"));
    assert!(raw_value.contains("Primary article copy."));

    let readable_out = headless_json(
        &[
            "browser",
            "text",
            "#reader",
            "--mode",
            "readability",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&readable_out, "browser text readability --json");
    let readable_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&readable_out)).expect("valid JSON from text readability");
    assert_eq!(readable_json["command"], "browser.text");
    assert_context(&readable_json, &session_id, &tab_id);
    let readable_value = readable_json["data"]["value"]
        .as_str()
        .expect("readability text string");
    assert!(
        !readable_value.contains("Skip nav"),
        "readability mode must strip nav text, got:\n{readable_value}"
    );
    assert!(readable_value.contains("Story Title"));
    assert!(readable_value.contains("Primary article copy."));

    let readable_text_out = headless(
        &[
            "browser",
            "text",
            "#reader",
            "--mode",
            "readability",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&readable_text_out, "browser text readability");
    let readable_text = stdout_str(&readable_text_out);
    assert_prefixed_header(&readable_text, &session_id, &tab_id);
    assert!(readable_text.contains("Story Title"));
    assert!(!readable_text.contains("Skip nav"));

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_read_text_page_readability_without_selector() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let readable_out = headless_json(
        &[
            "browser",
            "text",
            "--mode",
            "readability",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&readable_out, "browser text page readability --json");
    let readable_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&readable_out)).expect("valid JSON from page readability");
    assert_eq!(readable_json["command"], "browser.text");
    assert_context(&readable_json, &session_id, &tab_id);
    assert!(
        readable_json["data"]["target"]["selector"].is_null(),
        "page-level readability must expose a null selector target, got: {}",
        readable_json["data"]
    );
    let readable_value = readable_json["data"]["value"]
        .as_str()
        .expect("page readability text string");
    assert!(
        !readable_value.contains("Skip nav"),
        "page-level readability must strip nav text, got:\n{readable_value}"
    );
    assert!(readable_value.contains("Story Title"));
    assert!(readable_value.contains("Primary article copy."));

    let readable_text_out = headless(
        &[
            "browser",
            "text",
            "--mode",
            "readability",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&readable_text_out, "browser text page readability");
    let readable_text = stdout_str(&readable_text_out);
    assert_prefixed_header(&readable_text, &session_id, &tab_id);
    assert!(readable_text.contains("Story Title"));
    assert!(!readable_text.contains("Skip nav"));

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_read_value_attr_attrs_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let value_out = headless_json(
        &[
            "browser",
            "value",
            "#email",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&value_out, "browser value --json");
    let value_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&value_out)).expect("valid JSON from value");
    assert_eq!(value_json["command"], "browser.value");
    assert_context(&value_json, &session_id, &tab_id);
    assert_eq!(value_json["data"]["target"]["selector"], "#email");
    assert_eq!(value_json["data"]["value"], "user@example.com");

    let value_text_out = headless(
        &[
            "browser",
            "value",
            "#email",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&value_text_out, "browser value text");
    let value_text = stdout_str(&value_text_out);
    assert_prefixed_header(&value_text, &session_id, &tab_id);
    assert!(value_text.contains("user@example.com"));

    let attr_out = headless_json(
        &[
            "browser",
            "attr",
            "#email",
            "aria-label",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&attr_out, "browser attr --json");
    let attr_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&attr_out)).expect("valid JSON from attr");
    assert_eq!(attr_json["command"], "browser.attr");
    assert_context(&attr_json, &session_id, &tab_id);
    assert_eq!(attr_json["data"]["target"]["selector"], "#email");
    assert_eq!(attr_json["data"]["value"], "Email field");
    assert!(
        attr_json["data"].get("attribute").is_none(),
        "PRD 10.8 attr JSON should not expose extra attribute field, got: {}",
        attr_json["data"]
    );

    let attr_text_out = headless(
        &[
            "browser",
            "attr",
            "#email",
            "aria-label",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&attr_text_out, "browser attr text");
    let attr_text = stdout_str(&attr_text_out);
    assert_prefixed_header(&attr_text, &session_id, &tab_id);
    assert!(attr_text.contains("Email field"));

    let attrs_out = headless_json(
        &[
            "browser",
            "attrs",
            "#email",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&attrs_out, "browser attrs --json");
    let attrs_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&attrs_out)).expect("valid JSON from attrs");
    assert_eq!(attrs_json["command"], "browser.attrs");
    assert_context(&attrs_json, &session_id, &tab_id);
    assert_eq!(attrs_json["data"]["target"]["selector"], "#email");
    let attrs = attrs_json["data"]["value"]
        .as_object()
        .expect("attrs value object");
    assert_eq!(attrs.get("id").and_then(|v| v.as_str()), Some("email"));
    assert_eq!(attrs.get("type").and_then(|v| v.as_str()), Some("email"));
    assert_eq!(
        attrs.get("data-kind").and_then(|v| v.as_str()),
        Some("primary")
    );

    let attrs_text_out = headless(
        &[
            "browser",
            "attrs",
            "#email",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&attrs_text_out, "browser attrs text");
    let attrs_text = stdout_str(&attrs_text_out);
    assert_prefixed_header(&attrs_text, &session_id, &tab_id);
    assert!(attrs_text.contains("id: email"));
    assert!(attrs_text.contains("type: email"));

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_read_box_styles_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let box_out = headless_json(
        &[
            "browser",
            "box",
            "#box-target",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&box_out, "browser box --json");
    let box_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&box_out)).expect("valid JSON from box");
    assert_eq!(box_json["command"], "browser.box");
    assert_context(&box_json, &session_id, &tab_id);
    assert_eq!(box_json["data"]["target"]["selector"], "#box-target");
    let box_value = &box_json["data"]["value"];
    let width = box_value["width"].as_f64().expect("width number");
    let height = box_value["height"].as_f64().expect("height number");
    assert_eq!(width, 120.0);
    assert_eq!(height, 32.0);
    assert!(box_value["x"].as_f64().is_some());
    assert!(box_value["y"].as_f64().is_some());
    assert!(box_value.get("right").is_none());
    assert!(box_value.get("bottom").is_none());

    let box_text_out = headless(
        &[
            "browser",
            "box",
            "#box-target",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&box_text_out, "browser box text");
    let box_text = stdout_str(&box_text_out);
    assert_prefixed_header(&box_text, &session_id, &tab_id);
    assert!(box_text.contains("x: "));
    assert!(box_text.contains("y: "));
    assert!(box_text.contains("width: 120"));
    assert!(box_text.contains("height: 32"));

    let styles_out = headless_json(
        &[
            "browser",
            "styles",
            "#box-target",
            "color",
            "font-size",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&styles_out, "browser styles --json");
    let styles_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&styles_out)).expect("valid JSON from styles");
    assert_eq!(styles_json["command"], "browser.styles");
    assert_context(&styles_json, &session_id, &tab_id);
    assert_eq!(styles_json["data"]["target"]["selector"], "#box-target");
    let styles = styles_json["data"]["value"]
        .as_object()
        .expect("styles value object");
    assert_eq!(
        styles.get("color").and_then(|v| v.as_str()),
        Some("rgb(255, 0, 0)")
    );
    assert_eq!(
        styles.get("font-size").and_then(|v| v.as_str()),
        Some("18px")
    );

    let styles_text_out = headless(
        &[
            "browser",
            "styles",
            "#box-target",
            "color",
            "font-size",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&styles_text_out, "browser styles text");
    let styles_text = stdout_str(&styles_text_out);
    assert_prefixed_header(&styles_text, &session_id, &tab_id);
    assert!(styles_text.contains("color: rgb(255, 0, 0)"));
    assert!(styles_text.contains("font-size: 18px"));

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}

#[test]
fn contract_read_state_json_and_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (session_id, tab_id) = start_session();
    setup_fixture(&session_id, &tab_id);

    let state_out = headless_json(
        &[
            "browser",
            "state",
            "#editor",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&state_out, "browser state --json");
    let state_json: serde_json::Value =
        serde_json::from_str(&stdout_str(&state_out)).expect("valid JSON from state");
    assert_eq!(state_json["command"], "browser.state");
    assert_context(&state_json, &session_id, &tab_id);
    assert_eq!(state_json["data"]["target"]["selector"], "#editor");
    let state = state_json["data"]["state"]
        .as_object()
        .expect("state object");
    assert_eq!(state.get("visible").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(state.get("enabled").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(state.get("checked").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(state.get("focused").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(state.get("editable").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(state.get("selected").and_then(|v| v.as_bool()), Some(false));
    assert!(
        state_json["data"].get("flags").is_none(),
        "PRD 10.10 state JSON should use data.state, got: {}",
        state_json["data"]
    );

    let state_text_out = headless(
        &[
            "browser",
            "state",
            "#editor",
            "-s",
            &session_id,
            "-t",
            &tab_id,
        ],
        15,
    );
    assert_success(&state_text_out, "browser state text");
    let state_text = stdout_str(&state_text_out);
    assert_prefixed_header(&state_text, &session_id, &tab_id);
    assert!(state_text.contains("visible: true"));
    assert!(state_text.contains("enabled: true"));
    assert!(state_text.contains("checked: false"));
    assert!(state_text.contains("focused: true"));
    assert!(state_text.contains("editable: true"));
    assert!(state_text.contains("selected: false"));

    let _ = headless(&["browser", "close", "-s", &session_id], 15);
}
