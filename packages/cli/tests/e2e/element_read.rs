//! E2E tests for `browser html` / `text` / `value` / `attr`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, wait_page_ready,
};

const READER_SELECTOR: &str = "#reader";
const EMAIL_SELECTOR: &str = "#email";

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
document.body.innerHTML = '<main id="reader"><h1>Story Title</h1><p>Primary article copy.</p><input id="email" value="user@example.com" aria-label="Email field" data-testid="email-input"><div id="plain">Plain block</div></main>';
document.title = 'Read Contract Fixture';
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
fn html_json_selector_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "html",
            READER_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "html selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser html");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Read Contract Fixture");
    assert_eq!(v["data"]["target"]["selector"], READER_SELECTOR);
    let html = v["data"]["value"].as_str().unwrap_or("");
    assert!(html.contains("<main id=\"reader\">"));
    assert!(html.contains("Story Title"));
    assert!(html.contains("user@example.com"));
}

#[test]
fn html_json_xpath_selector_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "html",
            "//*[@id='reader']",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "html xpath json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser html");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["target"]["selector"], "//*[@id='reader']");
    let html = v["data"]["value"].as_str().unwrap_or("");
    assert!(html.contains("<main id=\"reader\">"));
    assert!(html.contains("Story Title"));
    assert!(html.contains("user@example.com"));
}

#[test]
fn html_json_full_page_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(&["browser", "html", "--session", &sid, "--tab", &tid], 10);
    assert_success(&out, "html page json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser html");
    assert!(v["data"]["target"]["selector"].is_null());
    let html = v["data"]["value"].as_str().unwrap_or("");
    assert!(html.contains("<html"));
    assert!(html.contains("<main id=\"reader\">"));
}

#[test]
fn html_text_selector_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "html",
            READER_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "html selector text");
    let text = stdout_str(&out);

    assert!(
        text.lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    assert!(text.contains("<main id=\"reader\">"));
    assert!(text.contains("Story Title"));
}

#[test]
fn text_json_selector_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "text",
            READER_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "text selector json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser text");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["data"]["target"]["selector"], READER_SELECTOR);
    let text = v["data"]["value"].as_str().unwrap_or("");
    assert!(text.contains("Story Title"));
    assert!(text.contains("Primary article copy."));
}

#[test]
fn text_json_full_page_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(&["browser", "text", "--session", &sid, "--tab", &tid], 10);
    assert_success(&out, "text page json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser text");
    assert!(v["data"]["target"]["selector"].is_null());
    let text = v["data"]["value"].as_str().unwrap_or("");
    assert!(text.contains("Story Title"));
    assert!(text.contains("Primary article copy."));
}

#[test]
fn value_and_attr_json_and_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let value_out = headless_json(
        &[
            "browser",
            "value",
            EMAIL_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&value_out, "value json");
    let value_json = parse_json(&value_out);
    assert_eq!(value_json["command"], "browser value");
    assert_eq!(value_json["data"]["target"]["selector"], EMAIL_SELECTOR);
    assert_eq!(value_json["data"]["value"], "user@example.com");
    assert_eq!(value_json["context"]["title"], "Read Contract Fixture");

    let value_text_out = headless(
        &[
            "browser",
            "value",
            EMAIL_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&value_text_out, "value text");
    let value_text = stdout_str(&value_text_out);
    assert!(
        value_text
            .lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    assert!(value_text.contains("user@example.com"));

    let attr_out = headless_json(
        &[
            "browser",
            "attr",
            EMAIL_SELECTOR,
            "aria-label",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&attr_out, "attr json");
    let attr_json = parse_json(&attr_out);
    assert_eq!(attr_json["command"], "browser attr");
    assert_eq!(attr_json["data"]["target"]["selector"], EMAIL_SELECTOR);
    assert_eq!(attr_json["data"]["value"], "Email field");
    assert!(attr_json["data"].get("attribute").is_none());

    let attr_text_out = headless(
        &[
            "browser",
            "attr",
            EMAIL_SELECTOR,
            "aria-label",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&attr_text_out, "attr text");
    let attr_text = stdout_str(&attr_text_out);
    assert!(
        attr_text
            .lines()
            .next()
            .unwrap_or("")
            .starts_with(&format!("[{sid} {tid}]"))
    );
    assert!(attr_text.contains("Email field"));
}

#[test]
fn value_and_attr_return_null_when_property_or_attribute_absent() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let value_out = headless_json(
        &[
            "browser",
            "value",
            "#plain",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&value_out, "value null json");
    let value_json = parse_json(&value_out);
    assert_eq!(value_json["command"], "browser value");
    assert_eq!(value_json["data"]["target"]["selector"], "#plain");
    assert!(value_json["data"]["value"].is_null());

    let value_text_out = headless(
        &[
            "browser",
            "value",
            "#plain",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&value_text_out, "value null text");
    let value_text = stdout_str(&value_text_out);
    assert!(value_text.contains("null"));

    let attr_out = headless_json(
        &[
            "browser",
            "attr",
            EMAIL_SELECTOR,
            "placeholder",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&attr_out, "attr null json");
    let attr_json = parse_json(&attr_out);
    assert_eq!(attr_json["command"], "browser attr");
    assert_eq!(attr_json["data"]["target"]["selector"], EMAIL_SELECTOR);
    assert!(attr_json["data"]["value"].is_null());

    let attr_text_out = headless(
        &[
            "browser",
            "attr",
            EMAIL_SELECTOR,
            "placeholder",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&attr_text_out, "attr null text");
    let attr_text = stdout_str(&attr_text_out);
    assert!(attr_text.contains("null"));
}

#[test]
fn value_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "value",
            EMAIL_SELECTOR,
            "--session",
            "missing-session",
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "value missing session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser value");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn attr_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "attr",
            EMAIL_SELECTOR,
            "aria-label",
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "attr missing tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser attr");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
}

#[test]
fn text_selector_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "text",
            "#missing",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "text missing selector");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser text");
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["error"]["details"]["selector"], "#missing");
}
