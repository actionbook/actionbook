//! E2E tests for `browser logs console` / `browser logs errors`.

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

fn seed_title(sid: &str, tid: &str) {
    let out = headless_json(
        &[
            "browser",
            "eval",
            "document.title = 'Logs Fixture'; void(0)",
            "--session",
            sid,
            "--tab",
            tid,
        ],
        10,
    );
    assert_success(&out, "set title");
}

fn emit_console(sid: &str, tid: &str, script: &str) {
    let out = headless_json(
        &["browser", "eval", script, "--session", sid, "--tab", tid],
        10,
    );
    assert_success(&out, "emit console logs");
}

fn emit_errors(sid: &str, tid: &str, script: &str) {
    let out = headless_json(
        &["browser", "eval", script, "--session", sid, "--tab", tid],
        10,
    );
    assert_success(&out, "emit error logs");
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

fn assert_log_item_shape(item: &serde_json::Value) {
    assert!(item["id"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(item["level"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(item["text"].as_str().is_some());
    assert!(item["source"].as_str().is_some());
    assert!(item["timestamp_ms"].is_number());
}

#[test]
fn logs_console_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_console(&sid, &tid, "console.info('console-info-marker'); void(0)");

    let out = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs console json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.logs.console");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Logs Fixture");
    assert_eq!(v["data"]["cleared"], false);
    let items = v["data"]["items"].as_array().expect("items must be array");
    assert_eq!(items.len(), 1);
    assert_log_item_shape(&items[0]);
    assert_eq!(items[0]["level"], "info");
    assert_eq!(items[0]["text"], "console-info-marker");
}

#[test]
fn logs_console_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_console(&sid, &tid, "console.info('console-text-marker'); void(0)");

    let out = headless(
        &[
            "browser",
            "logs",
            "console",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs console text");
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
    assert_eq!(lines.get(1), Some(&"1 log"));
    let log_line = lines.get(2).copied().unwrap_or_default();
    let parts: Vec<&str> = log_line.splitn(4, ' ').collect();
    assert_eq!(
        parts.len(),
        4,
        "log line must be level timestamp source text"
    );
    assert_eq!(parts[0], "info");
    assert!(parts[1].chars().all(|c| c.is_ascii_digit()));
    assert!(!parts[2].is_empty());
    assert_eq!(parts[3], "console-text-marker");
}

#[test]
fn logs_console_level_filter() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_console(
        &sid,
        &tid,
        "console.info('level-info'); console.warn('level-warn'); console.error('level-error'); void(0)",
    );

    let out = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--level",
            "warn,error",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs console level filter");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["level"], "warn");
    assert_eq!(items[0]["text"], "level-warn");
    assert_eq!(items[1]["level"], "error");
    assert_eq!(items[1]["text"], "level-error");
}

#[test]
fn logs_console_tail() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_console(
        &sid,
        &tid,
        "console.info('tail-1'); console.info('tail-2'); console.info('tail-3'); void(0)",
    );

    let out = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--tail",
            "1",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs console tail");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["text"], "tail-3");
}

#[test]
fn logs_console_since() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_console(
        &sid,
        &tid,
        "console.info('since-1'); console.info('since-2'); console.info('since-3'); void(0)",
    );

    let initial = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&initial, "logs console initial for since");
    let initial_v = parse_json(&initial);
    let items = initial_v["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    let since_id = items[0]["id"].as_str().expect("item id must be string");

    let out = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--since",
            since_id,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs console since");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["text"], "since-2");
    assert_eq!(items[1]["text"], "since-3");
}

#[test]
fn logs_console_clear() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_console(&sid, &tid, "console.info('clear-me'); void(0)");

    let out = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--clear",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs console clear");
    let v = parse_json(&out);

    assert_eq!(v["data"]["cleared"], true);
    assert_eq!(v["data"]["items"].as_array().unwrap().len(), 1);

    let after = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&after, "logs console after clear");
    let after_v = parse_json(&after);
    assert_eq!(after_v["data"]["cleared"], false);
    assert_eq!(after_v["data"]["items"], serde_json::json!([]));
}

#[test]
fn logs_errors_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_errors(
        &sid,
        &tid,
        "window.dispatchEvent(new ErrorEvent('error', { message: 'error-marker', filename: 'app.js' })); void(0)",
    );

    let out = headless_json(
        &[
            "browser",
            "logs",
            "errors",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs errors json");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();

    assert_eq!(v["command"], "browser.logs.errors");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["data"]["cleared"], false);
    assert_eq!(items.len(), 1);
    assert_log_item_shape(&items[0]);
    assert_eq!(items[0]["level"], "error");
    assert_eq!(items[0]["text"], "error-marker");
    assert_eq!(items[0]["source"], "app.js");
}

#[test]
fn logs_errors_source_filter() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_errors(
        &sid,
        &tid,
        "window.dispatchEvent(new ErrorEvent('error', { message: 'alpha-error', filename: 'alpha.js' })); window.dispatchEvent(new ErrorEvent('error', { message: 'beta-error', filename: 'beta.js' })); void(0)",
    );

    let out = headless_json(
        &[
            "browser",
            "logs",
            "errors",
            "--source",
            "beta.js",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs errors source filter");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source"], "beta.js");
    assert_eq!(items[0]["text"], "beta-error");
}

#[test]
fn logs_errors_tail() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_errors(
        &sid,
        &tid,
        "window.dispatchEvent(new ErrorEvent('error', { message: 'tail-alpha', filename: 'alpha.js' })); window.dispatchEvent(new ErrorEvent('error', { message: 'tail-beta', filename: 'beta.js' })); void(0)",
    );

    let out = headless_json(
        &[
            "browser",
            "logs",
            "errors",
            "--tail",
            "1",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs errors tail");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["text"], "tail-beta");
    assert_eq!(items[0]["source"], "beta.js");
}

#[test]
fn logs_errors_clear() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    seed_title(&sid, &tid);
    emit_errors(
        &sid,
        &tid,
        "window.dispatchEvent(new ErrorEvent('error', { message: 'clear-error', filename: 'clear.js' })); void(0)",
    );

    let out = headless_json(
        &[
            "browser",
            "logs",
            "errors",
            "--clear",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "logs errors clear");
    let v = parse_json(&out);

    assert_eq!(v["data"]["cleared"], true);
    assert_eq!(v["data"]["items"].as_array().unwrap().len(), 1);

    let after = headless_json(
        &[
            "browser",
            "logs",
            "errors",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&after, "logs errors after clear");
    let after_v = parse_json(&after);
    assert_eq!(after_v["data"]["cleared"], false);
    assert_eq!(after_v["data"]["items"], serde_json::json!([]));
}

#[test]
fn logs_console_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "logs",
            "console",
            "--session",
            "missing-session",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "logs console missing session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.logs.console");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn logs_errors_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "logs",
            "errors",
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "logs errors missing tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.logs.errors");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}
