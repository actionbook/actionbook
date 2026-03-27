//! Contract E2E tests for observation wave 3: logs/inspect/describe (#t78).
//!
//! Verifies field-level PRD compliance for:
//! - browser describe: target.selector, summary, role, name, tag, attributes, state, nearby
//! - browser inspect-point: point.{x,y}, element.{role,name,selector}, parents, screenshot_path
//! - browser logs console: items[].{id,level,text,source,timestamp_ms}, cleared, --tail/--since/--level/--clear flags
//! - browser logs errors: items[].{id,level,text,source,timestamp_ms}, cleared, --source/--clear flags
//!
//! Uses data: URLs for deterministic page content.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};
use serde_json::Value;

fn parse_envelope(out: &std::process::Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

/// Navigate to a data: URL and wait for page load.
fn goto(session: &str, tab: &str, url: &str) -> std::process::Output {
    headless(&["browser", "goto", url, "-s", session, "-t", tab], 30)
}

/// Eval JS on a tab.
fn eval_js(session: &str, tab: &str, js: &str) -> std::process::Output {
    headless(&["browser", "eval", js, "-s", session, "-t", tab], 15)
}

// ---------------------------------------------------------------------------
// describe: JSON field-level contract
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_describe_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // Navigate to a page with a button
    let out = goto(
        "local-1",
        "t0",
        "data:text/html,<button id=btn type=submit>Save</button>",
    );
    assert_success(&out, "goto");

    let out = headless_json(
        &["browser", "describe", "#btn", "-s", "local-1", "-t", "t0"],
        15,
    );
    assert_success(&out, "describe json");

    let v = parse_envelope(&out);
    assert_eq!(v["ok"], true, "ok should be true: {v}");
    assert_eq!(v["command"], "browser.describe", "command field: {v}");

    let d = &v["data"];
    // target.selector
    assert_eq!(
        d["target"]["selector"], "#btn",
        "data.target.selector should be '#btn': {d}"
    );
    // summary — must be a non-empty string
    assert!(
        d["summary"].is_string() && !d["summary"].as_str().unwrap_or("").is_empty(),
        "data.summary should be a non-empty string: {d}"
    );
    // role — must be a string
    assert!(d["role"].is_string(), "data.role should be a string: {d}");
    // name — must be a string (may be empty for unlabeled elements)
    assert!(d["name"].is_string(), "data.name should be a string: {d}");
    // tag — must be "button"
    assert_eq!(d["tag"], "button", "data.tag should be 'button': {d}");
    // attributes — must be an object
    assert!(
        d["attributes"].is_object(),
        "data.attributes should be an object: {d}"
    );
    // state.visible — must be bool
    assert!(
        d["state"]["visible"].is_boolean(),
        "data.state.visible should be bool: {d}"
    );
    // state.enabled — must be bool
    assert!(
        d["state"]["enabled"].is_boolean(),
        "data.state.enabled should be bool: {d}"
    );
    // nearby — null when --nearby not passed
    assert_eq!(
        d["nearby"],
        Value::Null,
        "data.nearby should be null without --nearby flag: {d}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// describe --nearby: JSON field contract
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_describe_nearby_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // Navigate to page with button inside a form with siblings
    let out = goto(
        "local-1",
        "t0",
        "data:text/html,<form><span>Label</span><button id=btn>Click</button><span>Help</span></form>",
    );
    assert_success(&out, "goto");

    let out = headless_json(
        &[
            "browser", "describe", "#btn", "-s", "local-1", "-t", "t0", "--nearby",
        ],
        15,
    );
    assert_success(&out, "describe nearby json");

    let v = parse_envelope(&out);
    assert_eq!(v["ok"], true, "ok should be true: {v}");

    let d = &v["data"];
    // nearby must be an object (not null)
    assert!(
        d["nearby"].is_object(),
        "data.nearby should be an object with --nearby flag: {d}"
    );
    let nearby = &d["nearby"];
    // children must be array
    assert!(
        nearby["children"].is_array(),
        "data.nearby.children should be array: {nearby}"
    );
    // parent, previous_sibling, next_sibling — string or null
    assert!(
        nearby["parent"].is_string() || nearby["parent"].is_null(),
        "data.nearby.parent should be string or null: {nearby}"
    );
    assert!(
        nearby["previous_sibling"].is_string() || nearby["previous_sibling"].is_null(),
        "data.nearby.previous_sibling should be string or null: {nearby}"
    );
    assert!(
        nearby["next_sibling"].is_string() || nearby["next_sibling"].is_null(),
        "data.nearby.next_sibling should be string or null: {nearby}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// describe: text output format
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_describe_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto(
        "local-1",
        "t0",
        "data:text/html,<button id=save>Save</button>",
    );
    assert_success(&out, "goto");

    let out = headless(
        &["browser", "describe", "#save", "-s", "local-1", "-t", "t0"],
        15,
    );
    assert_success(&out, "describe text");
    let text = stdout_str(&out);
    // First line: [session tab] url
    let first_line = text.lines().next().unwrap_or("");
    assert!(
        first_line.starts_with('['),
        "text first line should start with '[': {text:?}"
    );
    // Second line: summary string containing role and name
    let second_line = text.lines().nth(1).unwrap_or("");
    assert!(
        !second_line.is_empty(),
        "text second line (summary) should not be empty: {text:?}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// inspect-point: JSON field contract (success path)
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_inspect_point_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto(
        "local-1",
        "t0",
        "data:text/html,<body style='margin:0'><button style='position:absolute;top:50px;left:50px;width:100px;height:40px'>OK</button></body>",
    );
    assert_success(&out, "goto");

    let out = headless_json(
        &[
            "browser",
            "inspect-point",
            "100,70",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        15,
    );
    assert_success(&out, "inspect-point json");

    let v = parse_envelope(&out);
    assert_eq!(v["ok"], true, "ok should be true: {v}");
    assert_eq!(v["command"], "browser.inspect-point", "command field: {v}");

    let d = &v["data"];
    // point.x and point.y must be numbers
    assert!(
        d["point"]["x"].is_number(),
        "data.point.x should be a number: {d}"
    );
    assert!(
        d["point"]["y"].is_number(),
        "data.point.y should be a number: {d}"
    );
    // element.role, element.name, element.selector must be strings
    assert!(
        d["element"]["role"].is_string(),
        "data.element.role should be string: {d}"
    );
    assert!(
        d["element"]["name"].is_string(),
        "data.element.name should be string: {d}"
    );
    assert!(
        d["element"]["selector"].is_string(),
        "data.element.selector should be string: {d}"
    );
    // parents must be array
    assert!(d["parents"].is_array(), "data.parents should be array: {d}");
    // screenshot_path must be null (not yet implemented)
    assert_eq!(
        d["screenshot_path"],
        Value::Null,
        "data.screenshot_path should be null: {d}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// inspect-point: text output format
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_inspect_point_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto(
        "local-1",
        "t0",
        "data:text/html,<body style='margin:0'><button id=b1 style='position:absolute;top:50px;left:50px;width:100px;height:40px'>OK</button></body>",
    );
    assert_success(&out, "goto");

    let out = headless(
        &[
            "browser",
            "inspect-point",
            "100,70",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        15,
    );
    assert_success(&out, "inspect-point text");
    let text = stdout_str(&out);
    // First line: [session tab] url header
    let first_line = text.lines().next().unwrap_or("");
    assert!(
        first_line.starts_with('['),
        "text first line should be [session tab] url: {text:?}"
    );
    // Should contain "selector:" line
    assert!(
        text.contains("selector:"),
        "text output should contain 'selector:': {text:?}"
    );
    // Should contain "point:" line
    assert!(
        text.contains("point:"),
        "text output should contain 'point:': {text:?}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs console: items field shape
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_console_items() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    // Prime the capture hook (ENSURE_LOG_CAPTURE_JS runs on first logs call)
    let _ = headless_json(
        &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
        15,
    );
    // Fire the log after the hook is in place
    let out = eval_js("local-1", "t0", "console.log('hello-w3')");
    assert_success(&out, "eval log");

    // Poll for log to appear
    let mut v = Value::Null;
    for _ in 0..10 {
        let out = headless_json(
            &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if parsed["data"]["items"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
            {
                v = parsed;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    assert_ne!(v, Value::Null, "should have received a log response");

    let d = &v["data"];
    assert_eq!(v["ok"], true, "ok should be true: {v}");
    assert_eq!(v["command"], "browser.logs.console", "command: {v}");
    // items must be array
    assert!(d["items"].is_array(), "data.items should be array: {d}");
    // cleared must be bool
    assert!(
        d["cleared"].is_boolean(),
        "data.cleared should be bool: {d}"
    );
    assert_eq!(
        d["cleared"], false,
        "data.cleared should be false (no --clear flag): {d}"
    );

    // Find the 'hello-w3' log entry
    let items = d["items"].as_array().expect("items is array");
    let item = items
        .iter()
        .find(|i| {
            i["text"]
                .as_str()
                .map(|s| s.contains("hello-w3"))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("should find 'hello-w3' log item in {d}"));

    // id: must be a string like "log-N"
    assert!(
        item["id"].is_string() && item["id"].as_str().unwrap().starts_with("log-"),
        "item.id should be 'log-N': {item}"
    );
    // level: must be a string
    assert!(
        item["level"].is_string(),
        "item.level should be string: {item}"
    );
    // text: must contain the logged message
    assert_eq!(
        item["text"].as_str().unwrap_or(""),
        "hello-w3",
        "item.text should be 'hello-w3': {item}"
    );
    // source: must be a string
    assert!(
        item["source"].is_string(),
        "item.source should be string: {item}"
    );
    // timestamp_ms: must be a positive integer
    assert!(
        item["timestamp_ms"].is_u64() && item["timestamp_ms"].as_u64().unwrap() > 0,
        "item.timestamp_ms should be positive u64: {item}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs console --clear: cleared field becomes true
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_console_cleared() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    // Prime the capture hook, then fire log after hook is installed
    let _ = headless_json(
        &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
        15,
    );
    let out = eval_js("local-1", "t0", "console.log('to-clear')");
    assert_success(&out, "eval log");

    // Wait for log to appear
    for _ in 0..10 {
        let out = headless_json(
            &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if parsed["data"]["items"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
            {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    // Now call with --clear
    let out = headless_json(
        &[
            "browser", "logs", "console", "--clear", "-s", "local-1", "-t", "t0",
        ],
        15,
    );
    assert_success(&out, "logs console --clear");
    let v = parse_envelope(&out);
    assert_eq!(
        v["data"]["cleared"], true,
        "data.cleared should be true when --clear is passed: {}",
        v["data"]
    );

    // Subsequent call should return empty items
    let out = headless_json(
        &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
        15,
    );
    assert_success(&out, "logs after clear");
    let v2 = parse_envelope(&out);
    let items = v2["data"]["items"].as_array().expect("items array");
    assert!(
        items.is_empty(),
        "items should be empty after --clear: {}",
        v2["data"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs console --tail: limits number of items returned
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_console_tail() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    // Prime hook, then fire 5 logs after hook is installed
    let _ = headless_json(
        &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
        15,
    );
    let out = eval_js("local-1", "t0", "for(let i=0;i<5;i++) console.log('msg'+i)");
    assert_success(&out, "eval logs");

    // Wait for all 5 logs
    for _ in 0..15 {
        let out = headless_json(
            &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if parsed["data"]["items"]
                .as_array()
                .map(|a| a.len() >= 5)
                .unwrap_or(false)
            {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    // --tail 2 should return at most 2 items
    let out = headless_json(
        &[
            "browser", "logs", "console", "--tail", "2", "-s", "local-1", "-t", "t0",
        ],
        15,
    );
    assert_success(&out, "logs console --tail 2");
    let v = parse_envelope(&out);
    let items = v["data"]["items"].as_array().expect("items array");
    assert!(
        items.len() <= 2,
        "items.len() should be <= 2 with --tail 2, got {}: {}",
        items.len(),
        v["data"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs console --since: filters items after the given id
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_console_since() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    // Prime hook, then fire 3 logs after hook is installed
    let _ = headless_json(
        &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
        15,
    );
    let out = eval_js(
        "local-1",
        "t0",
        "console.log('first');console.log('second');console.log('third')",
    );
    assert_success(&out, "eval logs");

    // Wait for all 3 logs and capture them
    let mut all_items = vec![];
    for _ in 0..15 {
        let out = headless_json(
            &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if let Some(arr) = parsed["data"]["items"].as_array() {
                if arr.len() >= 3 {
                    all_items = arr.clone();
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    assert!(
        all_items.len() >= 3,
        "should have at least 3 log items, got {}",
        all_items.len()
    );

    // Get the id of the first item and use --since to get items after it
    let first_id = all_items[0]["id"]
        .as_str()
        .expect("first item id should be string");
    let out = headless_json(
        &[
            "browser", "logs", "console", "--since", first_id, "-s", "local-1", "-t", "t0",
        ],
        15,
    );
    assert_success(&out, "logs console --since");
    let v = parse_envelope(&out);
    let since_items = v["data"]["items"].as_array().expect("items array");
    // Should not include the first item
    assert!(
        since_items
            .iter()
            .all(|i| i["id"].as_str().unwrap_or("") != first_id),
        "--since should exclude the referenced id: {since_items:?}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs console --level: filters by log level
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_console_level() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    // Prime hook, then fire log + warn after hook is installed
    let _ = headless_json(
        &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
        15,
    );
    let out = eval_js(
        "local-1",
        "t0",
        "console.log('a-log');console.warn('a-warn')",
    );
    assert_success(&out, "eval logs");

    // Wait for both logs
    for _ in 0..15 {
        let out = headless_json(
            &["browser", "logs", "console", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if parsed["data"]["items"]
                .as_array()
                .map(|a| a.len() >= 2)
                .unwrap_or(false)
            {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    // --level warn should only return warn entries
    let out = headless_json(
        &[
            "browser", "logs", "console", "--level", "warn", "-s", "local-1", "-t", "t0",
        ],
        15,
    );
    assert_success(&out, "logs console --level warn");
    let v = parse_envelope(&out);
    let items = v["data"]["items"].as_array().expect("items array");
    assert!(
        !items.is_empty(),
        "should have at least one warn item: {}",
        v["data"]
    );
    for item in items {
        assert_eq!(
            item["level"].as_str().unwrap_or(""),
            "warn",
            "all items should have level 'warn' with --level warn: {item}"
        );
    }

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs errors: items field shape
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_errors_items() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // Navigate to blank page, prime the error capture hook, then fire error after hook is installed
    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    let _ = headless_json(
        &["browser", "logs", "errors", "-s", "local-1", "-t", "t0"],
        15,
    );
    let out = eval_js("local-1", "t0", "console.error('err-w3')");
    assert_success(&out, "eval error");

    // Poll for error log
    let mut v = Value::Null;
    for _ in 0..10 {
        let out = headless_json(
            &["browser", "logs", "errors", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if parsed["data"]["items"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
            {
                v = parsed;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    assert_ne!(v, Value::Null, "should have received an error log response");

    let d = &v["data"];
    assert_eq!(v["ok"], true, "ok should be true: {v}");
    assert_eq!(v["command"], "browser.logs.errors", "command: {v}");
    assert!(d["items"].is_array(), "data.items should be array: {d}");
    assert!(
        d["cleared"].is_boolean(),
        "data.cleared should be bool: {d}"
    );

    let items = d["items"].as_array().expect("items array");
    let item = items
        .iter()
        .find(|i| {
            i["text"]
                .as_str()
                .map(|s| s.contains("err-w3"))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("should find 'err-w3' error item in {d}"));

    // id: must be a string like "err-N"
    assert!(
        item["id"].is_string() && item["id"].as_str().unwrap().starts_with("err-"),
        "item.id should be 'err-N': {item}"
    );
    // level: must be "error"
    assert_eq!(
        item["level"].as_str().unwrap_or(""),
        "error",
        "item.level should be 'error': {item}"
    );
    // text: must contain message
    assert!(
        item["text"].as_str().unwrap_or("").contains("err-w3"),
        "item.text should contain 'err-w3': {item}"
    );
    // source: must be string
    assert!(
        item["source"].is_string(),
        "item.source should be string: {item}"
    );
    // timestamp_ms: positive u64
    assert!(
        item["timestamp_ms"].is_u64() && item["timestamp_ms"].as_u64().unwrap() > 0,
        "item.timestamp_ms should be positive u64: {item}"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// logs errors --clear: cleared field reflects clear
// ---------------------------------------------------------------------------

#[test]
fn contract_obs_w3_logs_errors_clear() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = goto("local-1", "t0", "data:text/html,<html>");
    assert_success(&out, "goto");

    // Prime hook, then fire error after hook is installed
    let _ = headless_json(
        &["browser", "logs", "errors", "-s", "local-1", "-t", "t0"],
        15,
    );
    let out = eval_js("local-1", "t0", "console.error('clear-test')");
    assert_success(&out, "eval error");

    // Wait for error to appear
    for _ in 0..10 {
        let out = headless_json(
            &["browser", "logs", "errors", "-s", "local-1", "-t", "t0"],
            15,
        );
        if out.status.success() {
            let parsed = parse_envelope(&out);
            if parsed["data"]["items"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
            {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    // Call with --clear
    let out = headless_json(
        &[
            "browser", "logs", "errors", "--clear", "-s", "local-1", "-t", "t0",
        ],
        15,
    );
    assert_success(&out, "logs errors --clear");
    let v = parse_envelope(&out);
    assert_eq!(
        v["data"]["cleared"], true,
        "data.cleared should be true when --clear is passed: {}",
        v["data"]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
