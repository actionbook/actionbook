//! Browser lifecycle E2E tests: start, list-sessions, status, close, restart.
//!
//! Each test is self-contained: start → operate → assert → close.
//! Covers BOTH JSON (§2.4 envelope) and text (§2.5 protocol) output.
//! All assertions strictly follow api-reference.md §7.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, config_path, headless, headless_json,
    headless_json_with_env, parse_json, skip, stdout_str,
};
use std::sync::{Arc, Barrier};

const TEST_URL: &str = "https://example.com";

fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, "close session");
}

// ===========================================================================
// 1. lifecycle_open_and_close — §7.1 + §7.4 (JSON)
// ===========================================================================

#[test]
fn lifecycle_open_and_close_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // start: §7.1 JSON
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.start");
    assert!(v["error"].is_null(), "start: error should be null");
    // data.session per §7.1
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    assert_eq!(v["data"]["session"]["mode"], "local");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["headless"].is_boolean());
    assert!(v["data"]["session"]["cdp_endpoint"].is_string());
    // data.tab
    assert!(
        v["data"]["tab"]["tab_id"].is_string(),
        "tab_id must be a string"
    );
    assert!(
        !v["data"]["tab"]["tab_id"].as_str().unwrap().is_empty(),
        "tab_id must not be empty"
    );
    assert!(v["data"]["tab"]["url"].is_string());
    assert!(v["data"]["tab"]["title"].is_string());
    // data.reused
    assert_eq!(v["data"]["reused"], false);
    // context (special: start returns context after session creation)
    assert_eq!(v["context"]["session_id"], "local-1");
    assert!(
        v["context"]["tab_id"].is_string(),
        "context.tab_id must be present"
    );
    // meta
    assert!(v["meta"]["duration_ms"].is_number());

    // status: §7.3 JSON
    let out = headless_json(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.status");
    assert_eq!(v["context"]["session_id"], "local-1");

    // close: §7.4 JSON
    let out = headless_json(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.close");
    assert_eq!(v["data"]["session_id"], "local-1");
    assert_eq!(v["data"]["status"], "closed");
    assert!(v["data"]["closed_tabs"].is_number());
    // §4: session commands must return context.session_id
    assert_eq!(v["context"]["session_id"], "local-1");
    assert!(v["meta"]["duration_ms"].is_number());
}

// ===========================================================================
// 2. lifecycle_open_and_close — §7.1 + §7.4 (Text)
// ===========================================================================

#[test]
fn lifecycle_open_and_close_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // start text: "[SID t1] url\nok browser.start\nmode: local\nstatus: running"
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start text");
    let text = stdout_str(&out);
    assert!(
        text.contains("[local-1"),
        "start text: should contain [local-1"
    );
    assert!(
        text.contains("ok browser.start"),
        "start text: should contain 'ok browser.start'"
    );
    assert!(
        text.contains("mode: local"),
        "start text: should contain 'mode: local'"
    );
    assert!(
        text.contains("status: running"),
        "start text: should contain 'status: running'"
    );
    assert!(
        text.contains("title:"),
        "start text: should contain 'title:' per §7.1"
    );

    // status text: "[SID]\nstatus: running\nmode: local\ntabs: N"
    let out = headless(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status text");
    let text = stdout_str(&out);
    assert!(
        text.contains("[local-1]"),
        "status text: should contain [local-1]"
    );
    assert!(
        text.contains("status: running"),
        "status text: should contain 'status: running'"
    );
    assert!(
        text.contains("mode: local"),
        "status text: should contain 'mode: local'"
    );

    // close text: "[SID]\nok browser.close\nclosed_tabs: N"
    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close text");
    let text = stdout_str(&out);
    assert!(
        text.contains("[local-1]"),
        "close text: should contain [local-1]"
    );
    assert!(
        text.contains("ok browser.close"),
        "close text: should contain 'ok browser.close'"
    );
    assert!(
        text.contains("closed_tabs:"),
        "close text: should contain 'closed_tabs:'"
    );
}

// ===========================================================================
// 3. lifecycle_open_headless — §7.1 JSON
// ===========================================================================

#[test]
fn lifecycle_open_headless_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start headless");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["session"]["headless"], true);

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// 4. lifecycle_open_with_url — §7.1 JSON + Text
// ===========================================================================

#[test]
fn lifecycle_open_with_url_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start with url");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(
        v["data"]["tab"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("example.com"),
        "tab.url should contain example.com, got: {}",
        v["data"]["tab"]["url"]
    );

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

#[test]
fn lifecycle_open_with_url_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start with url text");
    let text = stdout_str(&out);
    // text header should contain the URL
    assert!(
        text.contains("example.com"),
        "start text: should contain example.com URL"
    );

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// 5. lifecycle_status — §7.3 JSON + Text
// ===========================================================================

#[test]
fn lifecycle_status_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless_json(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.status");
    // data.session per §7.3
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert_eq!(v["data"]["session"]["mode"], "local");
    assert!(v["data"]["session"]["headless"].is_boolean());
    assert!(v["data"]["session"]["tabs_count"].is_number());
    // data.tabs array with tab details
    let tabs = v["data"]["tabs"].as_array().expect("tabs should be array");
    assert!(!tabs.is_empty(), "should have at least 1 tab");
    assert!(tabs[0]["tab_id"].is_string());
    assert!(tabs[0]["url"].is_string());
    assert!(tabs[0]["title"].is_string());
    // data.capabilities
    let caps = &v["data"]["capabilities"];
    assert!(caps["snapshot"].is_boolean());
    assert!(caps["pdf"].is_boolean());
    assert!(caps["upload"].is_boolean());
    // context
    assert_eq!(v["context"]["session_id"], "local-1");
    // meta
    assert!(v["meta"]["duration_ms"].is_number());

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

#[test]
fn lifecycle_status_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // §7.3 text: "[SID]\nstatus: running\nmode: local\ntabs: N"
    let out = headless(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status text");
    let text = stdout_str(&out);
    assert!(text.contains("[local-1]"), "should contain [local-1]");
    assert!(text.contains("status: running"));
    assert!(text.contains("mode: local"));
    assert!(text.contains("tabs:"), "should contain 'tabs:'");

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// 6. lifecycle_list_sessions — §7.2 JSON + Text
// ===========================================================================

#[test]
fn lifecycle_list_sessions_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.list-sessions");
    // Global: no context
    assert!(
        v["context"].is_null(),
        "context should be null for global command"
    );
    // data per §7.2
    assert!(v["data"]["total_sessions"].as_u64().unwrap_or(0) >= 1);
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let s = &sessions[0];
    assert!(s["session_id"].is_string());
    assert!(s["mode"].is_string());
    assert!(s["status"].is_string());
    assert!(s["headless"].is_boolean());
    assert!(s["tabs_count"].is_number());
    // meta per §2.4
    assert!(
        v["meta"]["duration_ms"].is_number(),
        "list-sessions: meta.duration_ms"
    );

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

#[test]
fn lifecycle_list_sessions_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // §7.2 text: "1 session\n[SID]\nstatus: running\ntabs: N"
    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions text");
    let text = stdout_str(&out);
    assert!(text.contains("session"), "should contain 'session'");
    assert!(text.contains("[local-1]"), "should contain [local-1]");
    assert!(text.contains("status: running"));
    assert!(
        text.contains("tabs:"),
        "list-sessions text: should contain 'tabs:' per §7.2"
    );

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// 7. lifecycle_restart — §7.5 JSON + Text
// ===========================================================================

#[test]
fn lifecycle_restart_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless_json(&["browser", "restart", "--session", "local-1"], 30);
    assert_success(&out, "restart");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.restart");
    // data per §7.5
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    assert_eq!(v["data"]["session"]["mode"], "local");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["headless"].is_boolean());
    assert!(v["data"]["session"]["tabs_count"].is_number());
    assert_eq!(v["data"]["reopened"], true);
    // context
    assert_eq!(v["context"]["session_id"], "local-1");
    // meta
    assert!(v["meta"]["duration_ms"].is_number());

    // status still works
    let out = headless_json(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status after restart");
    let v = parse_json(&out);
    assert_eq!(v["data"]["session"]["status"], "running");

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

#[test]
fn lifecycle_restart_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    // §7.5 text: "[SID t1]\nok browser.restart\nstatus: running"
    let out = headless(&["browser", "restart", "--session", "local-1"], 30);
    assert_success(&out, "restart text");
    let text = stdout_str(&out);
    // §7.5 note: restart text uses [SID t1] format (includes tab_id)
    assert!(
        text.contains("[local-1 t"),
        "restart text: header should include tab_id per §7.5"
    );
    assert!(
        text.contains("ok browser.restart"),
        "should contain 'ok browser.restart'"
    );
    assert!(text.contains("status: running"));

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// 8. lifecycle_close_after_operations — §7.4 JSON
// ===========================================================================

#[test]
fn lifecycle_close_after_operations() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start");
    let sv = parse_json(&out);
    let tab_id = sv["data"]["tab"]["tab_id"].as_str().unwrap();

    let out = headless(
        &[
            "browser",
            "goto",
            TEST_URL,
            "--session",
            "local-1",
            "--tab",
            tab_id,
        ],
        30,
    );
    assert_success(&out, "goto");

    let out = headless(
        &[
            "browser",
            "snapshot",
            "--session",
            "local-1",
            "--tab",
            tab_id,
        ],
        30,
    );
    assert_success(&out, "snapshot");

    let out = headless_json(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close after operations");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], "closed");
    assert!(v["data"]["closed_tabs"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "close: context per §4"
    );
}

// ===========================================================================
// 9. lifecycle_close_s1t2 — §7.4 JSON + Text
// ===========================================================================

#[test]
fn lifecycle_close_s1t2_closes_all_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start");

    let out = headless(
        &["browser", "new-tab", TEST_URL, "--session", "local-1"],
        30,
    );
    assert_success(&out, "new-tab");

    let out = headless_json(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close 2 tabs");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], "closed");
    assert_eq!(v["data"]["closed_tabs"], serde_json::json!(2));
    assert_eq!(
        v["context"]["session_id"], "local-1",
        "close: context per §4"
    );
}

#[test]
fn lifecycle_close_s1t2_closes_all_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "start");

    let out = headless(
        &["browser", "new-tab", TEST_URL, "--session", "local-1"],
        30,
    );
    assert_success(&out, "new-tab");

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close 2 tabs text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.close"));
    assert!(
        text.contains("closed_tabs: 2"),
        "should show closed_tabs: 2, got: {text}"
    );
}

// ===========================================================================
// 10. lifecycle_double_close — §3.1 error JSON + Text
// ===========================================================================

#[test]
fn lifecycle_double_close_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "first close");

    // §3.1 error JSON
    let out = headless_json(&["browser", "close", "--session", "local-1"], 30);
    assert_failure(&out, "second close should fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close");
    assert!(v["data"].is_null(), "data should be null on failure");
    assert_eq!(v["error"]["code"], "SESSION_NOT_FOUND");
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    // §3.1: details field always present (may be null or object)
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details should be object or null per §3.1"
    );
    assert!(v["meta"]["duration_ms"].is_number());
}

#[test]
fn lifecycle_double_close_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "first close");

    // §3.1 error text: "error CODE: message"
    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_failure(&out, "second close text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "should contain 'error SESSION_NOT_FOUND:', got: {text}"
    );
}

// ===========================================================================
// 10b. error path on status — §3 error format on another command
// ===========================================================================

#[test]
fn lifecycle_status_nonexistent_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "status", "--session", "nonexistent"], 10);
    assert_failure(&out, "status nonexistent");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.status");
    assert!(v["data"].is_null());
    assert_eq!(v["error"]["code"], "SESSION_NOT_FOUND");
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details per §3.1"
    );
}

#[test]
fn lifecycle_status_nonexistent_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "status", "--session", "nonexistent"], 10);
    assert_failure(&out, "status nonexistent text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "should contain 'error SESSION_NOT_FOUND:', got: {text}"
    );
}

// ===========================================================================
// 11. lifecycle_concurrent_two_sessions — JSON
// ===========================================================================

#[test]
fn lifecycle_concurrent_two_sessions() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            "work",
            "--set-session-id",
            "work-session",
        ],
        30,
    );
    assert_success(&out, "start work-session");

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            "personal",
            "--set-session-id",
            "personal-session",
        ],
        30,
    );
    assert_success(&out, "start personal-session");

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_sessions"], serde_json::json!(2));
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(ids.contains(&"work-session"));
    assert!(ids.contains(&"personal-session"));

    // status each
    for sid in &["work-session", "personal-session"] {
        let out = headless_json(&["browser", "status", "--session", sid], 10);
        assert_success(&out, &format!("status {sid}"));
        let v = parse_json(&out);
        assert_eq!(v["data"]["session"]["status"], "running");
    }

    let out = headless(&["browser", "close", "--session", "work-session"], 30);
    assert_success(&out, "close work");
    let out = headless(&["browser", "close", "--session", "personal-session"], 30);
    assert_success(&out, "close personal");
}

// ===========================================================================
// 12. lifecycle_concurrent_parallel_operations — JSON
// ===========================================================================

#[test]
fn lifecycle_concurrent_parallel_operations() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            "alpha",
            "--set-session-id",
            "alpha-session",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start alpha");
    let alpha_tab = parse_json(&out)["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            "beta",
            "--set-session-id",
            "beta-session",
            "--open-url",
            "https://example.org",
        ],
        30,
    );
    assert_success(&out, "start beta");
    let beta_tab = parse_json(&out)["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Ensure navigation completes before parallel eval
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "--session",
            "alpha-session",
            "--tab",
            &alpha_tab,
        ],
        30,
    );
    assert_success(&out, "goto alpha");
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "--session",
            "beta-session",
            "--tab",
            &beta_tab,
        ],
        30,
    );
    assert_success(&out, "goto beta");

    // Parallel eval on different sessions
    let at = alpha_tab.clone();
    let bt = beta_tab.clone();
    let t1 = std::thread::spawn(move || {
        headless_json(
            &[
                "browser",
                "eval",
                "window.location.href",
                "--session",
                "alpha-session",
                "--tab",
                &at,
            ],
            30,
        )
    });
    let t2 = std::thread::spawn(move || {
        headless_json(
            &[
                "browser",
                "eval",
                "window.location.href",
                "--session",
                "beta-session",
                "--tab",
                &bt,
            ],
            30,
        )
    });

    let out1 = t1.join().expect("thread 1");
    let out2 = t2.join().expect("thread 2");
    assert_success(&out1, "eval alpha");
    assert_success(&out2, "eval beta");

    let v1 = parse_json(&out1);
    let v2 = parse_json(&out2);
    assert!(
        v1["data"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("example.com")
    );
    assert!(
        v2["data"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("example.org")
    );

    let out = headless(&["browser", "close", "--session", "alpha-session"], 30);
    assert_success(&out, "close alpha");
    let out = headless(&["browser", "close", "--session", "beta-session"], 30);
    assert_success(&out, "close beta");
}

// ===========================================================================
// 13. lifecycle_start_reuse_existing — Local 1 profile = max 1 session
// ===========================================================================

#[test]
fn lifecycle_start_reuse_existing_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // First start
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "first start");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["reused"], false,
        "first start: reused should be false"
    );
    assert_eq!(v["data"]["session"]["session_id"], "local-1");

    // Second start with same profile — should reuse
    let out = headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "second start (reuse)");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["reused"], true,
        "second start: reused should be true"
    );
    assert_eq!(
        v["data"]["session"]["session_id"], "local-1",
        "second start: should return same session_id"
    );
    assert_eq!(v["data"]["session"]["status"], "running");

    // list-sessions: only 1 session, not 2
    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(
        v["data"]["total_sessions"],
        serde_json::json!(1),
        "should have exactly 1 session, not 2"
    );

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

#[test]
fn lifecycle_start_reuse_existing_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "first start");

    // Second start — text output should show the existing session
    let out = headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "second start (reuse) text");
    let text = stdout_str(&out);
    assert!(
        text.contains("[local-1"),
        "should contain existing session id"
    );
    assert!(text.contains("ok browser.start"));
    assert!(text.contains("status: running"));

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
}

// ===========================================================================
// 14. lifecycle_start_reuse_with_open_url — reuse navigates to URL
// ===========================================================================

#[test]
fn lifecycle_start_reuse_with_open_url_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // First start
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            TEST_URL,
        ],
        30,
    );
    assert_success(&out, "first start");
    let v = parse_json(&out);
    assert_eq!(v["data"]["reused"], false);
    let tab_id = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    // Second start with different URL — should reuse and navigate
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://arxiv.org",
        ],
        30,
    );
    assert_success(&out, "second start (reuse + navigate)");
    let v = parse_json(&out);
    assert_eq!(v["data"]["reused"], true, "should be reused");
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    // tab URL should be updated to the new URL
    assert!(
        v["data"]["tab"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("arxiv.org"),
        "tab url should contain arxiv.org after navigate, got: {}",
        v["data"]["tab"]["url"]
    );

    // Verify via eval
    let out = headless_json(
        &[
            "browser",
            "eval",
            "window.location.href",
            "--session",
            "local-1",
            "--tab",
            &tab_id,
        ],
        30,
    );
    assert_success(&out, "eval location");
    let v = parse_json(&out);
    assert!(
        v["data"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("arxiv.org"),
        "actual URL should be arxiv.org"
    );

    let out = headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close reuse");
}

#[test]
fn lifecycle_start_bootstraps_default_config() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let path = config_path();
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let out = headless_json(&["browser", "start", "--headless"], 30);
    assert_success(&out, "start should bootstrap config");
    let v = parse_json(&out);
    let session_id = v["data"]["session"]["session_id"]
        .as_str()
        .expect("session id");

    assert!(
        path.exists(),
        "config.toml should be created on first start"
    );
    let text = std::fs::read_to_string(&path).expect("read config");
    assert!(text.contains("[browser]"));
    assert!(text.contains("default_profile = \"actionbook\""));

    close_session(session_id);
}

#[test]
fn lifecycle_start_env_over_config_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    std::fs::write(
        config_path(),
        r#"[browser]
headless = false
"#,
    )
    .expect("write config");

    let out = headless_json_with_env(
        &["browser", "start"],
        &[("ACTIONBOOK_BROWSER_HEADLESS", "true")],
        30,
    );
    assert_success(&out, "start env over config");
    let v = parse_json(&out);
    let session_id = v["data"]["session"]["session_id"]
        .as_str()
        .expect("session id");

    assert_eq!(v["data"]["session"]["headless"], true);

    close_session(session_id);
}

#[test]
fn lifecycle_start_cli_over_env_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json_with_env(
        &["browser", "start", "--headless"],
        &[("ACTIONBOOK_BROWSER_HEADLESS", "false")],
        30,
    );
    assert_success(&out, "start cli over env");
    let v = parse_json(&out);
    let session_id = v["data"]["session"]["session_id"]
        .as_str()
        .expect("session id");

    assert_eq!(v["data"]["session"]["headless"], true);
    close_session(session_id);
}

#[test]
fn lifecycle_start_concurrent_same_profile_rejects_second_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "warm daemon");

    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();

    for _ in 0..2 {
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            headless_json(
                &[
                    "browser",
                    "start",
                    "--mode",
                    "local",
                    "--headless",
                    "--profile",
                    "testrace",
                ],
                30,
            )
        }));
    }

    barrier.wait();

    let outputs: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("join start thread"))
        .collect();

    let successes: Vec<_> = outputs
        .iter()
        .filter(|output| output.status.success())
        .collect();
    let failures: Vec<_> = outputs
        .iter()
        .filter(|output| !output.status.success())
        .collect();

    assert_eq!(
        successes.len(),
        1,
        "expected exactly one successful create\noutputs: {outputs:#?}"
    );
    assert_eq!(
        failures.len(),
        1,
        "expected exactly one rejected concurrent start\noutputs: {outputs:#?}"
    );

    let success = parse_json(successes[0]);
    assert_eq!(success["data"]["reused"], false);
    let session_id = success["data"]["session"]["session_id"]
        .as_str()
        .expect("successful session id")
        .to_string();

    let failure = parse_json(failures[0]);
    assert_eq!(failure["ok"], false);
    assert_eq!(failure["command"], "browser.start");
    assert_eq!(failure["error"]["code"], "SESSION_STARTING");
    assert_eq!(
        failure["error"]["hint"],
        "retry after a few seconds or use browser status to check"
    );

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions after concurrent start");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_sessions"], serde_json::json!(1));

    close_session(&session_id);
}
