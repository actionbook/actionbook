//! Browser lifecycle E2E tests: start, list-sessions, status, close, restart.
//!
//! Each test is self-contained: start -> operate -> assert -> close.
//! Covers BOTH JSON and text output.
//! All assertions strictly follow api-reference.md section 7.
//!
//! Tests that verify default session IDs or config bootstrap use `SoloEnv`
//! (per-test isolated ACTIONBOOK_HOME) to avoid racing with parallel tests.
//! Other tests use per-test session isolation via the shared daemon.

use assert_cmd::Command;
use std::process::Output;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use crate::harness::{
    SessionGuard, SoloEnv, assert_failure, assert_success, headless, headless_json, new_tab_json,
    parse_json, skip, start_session, stdout_str, unique_session, url_a, url_b,
};

// ===========================================================================
// 1. lifecycle_open_and_close — JSON (needs default session ID → SoloEnv)
// ===========================================================================

#[test]
fn lifecycle_open_and_close_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    let out = env.headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.start");
    assert!(v["error"].is_null(), "start: error should be null");
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    assert_eq!(v["data"]["session"]["mode"], "local");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["headless"].is_boolean());
    assert!(v["data"]["session"]["cdp_endpoint"].is_string());
    assert!(v["data"]["tab"]["tab_id"].is_string());
    assert!(!v["data"]["tab"]["tab_id"].as_str().unwrap().is_empty());
    assert!(v["data"]["tab"]["url"].is_string());
    assert!(v["data"]["tab"]["title"].is_string());
    assert_eq!(v["data"]["reused"], false);
    assert_eq!(v["context"]["session_id"], "local-1");
    assert!(v["context"]["tab_id"].is_string());
    assert!(v["meta"]["duration_ms"].is_number());

    // status
    let out = env.headless_json(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.status");
    assert_eq!(v["context"]["session_id"], "local-1");

    // close
    let out = env.headless_json(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.close");
    assert_eq!(v["data"]["session_id"], "local-1");
    assert_eq!(v["data"]["status"], "closed");
    assert!(v["data"]["closed_tabs"].is_number());
    assert_eq!(v["context"]["session_id"], "local-1");
    assert!(v["meta"]["duration_ms"].is_number());
}

// ===========================================================================
// 2. lifecycle_open_and_close — Text (SoloEnv)
// ===========================================================================

#[test]
fn lifecycle_open_and_close_text() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    let out = env.headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start text");
    let text = stdout_str(&out);
    assert!(
        text.contains("[local-1"),
        "start text: should contain [local-1"
    );
    assert!(text.contains("ok browser.start"));
    assert!(text.contains("mode: local"));
    assert!(text.contains("status: running"));
    assert!(text.contains("title:"));

    let out = env.headless(&["browser", "status", "--session", "local-1"], 10);
    assert_success(&out, "status text");
    let text = stdout_str(&out);
    assert!(text.contains("[local-1]"));
    assert!(text.contains("status: running"));
    assert!(text.contains("mode: local"));

    let out = env.headless(&["browser", "close", "--session", "local-1"], 30);
    assert_success(&out, "close text");
    let text = stdout_str(&out);
    assert!(text.contains("[local-1]"));
    assert!(text.contains("ok browser.close"));
    assert!(text.contains("closed_tabs:"));
}

// ===========================================================================
// 3. lifecycle_open_headless — JSON (SoloEnv)
// ===========================================================================

#[test]
fn lifecycle_open_headless_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    let out = env.headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "start headless");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["session"]["headless"], true);

    env.headless(&["browser", "close", "--session", "local-1"], 30);
}

// ===========================================================================
// 4. lifecycle_open_with_url — isolated
// ===========================================================================

#[test]
fn lifecycle_open_with_url_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let v = parse_json(&headless_json(
        &["browser", "status", "--session", &sid],
        10,
    ));
    assert_eq!(v["ok"], true);
    assert!(
        v["data"]["tabs"].as_array().unwrap()[0]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-a"),
        "tab.url should contain page-a"
    );
}

#[test]
fn lifecycle_open_with_url_text() {
    if skip() {
        return;
    }
    let (sid, profile) = unique_session("url-text");
    let url = url_a();
    let out = headless(
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
            &url,
        ],
        30,
    );
    assert_success(&out, "start with url text");
    let _guard = SessionGuard::new(&sid);
    let text = stdout_str(&out);
    assert!(
        text.contains("page-a"),
        "start text should contain page-a URL, got: {text}"
    );
}

// ===========================================================================
// 5. lifecycle_status — isolated
// ===========================================================================

#[test]
fn lifecycle_status_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.status");
    assert_eq!(v["data"]["session"]["session_id"], sid);
    assert_eq!(v["data"]["session"]["status"], "running");
    assert_eq!(v["data"]["session"]["mode"], "local");
    assert!(v["data"]["session"]["headless"].is_boolean());
    assert!(v["data"]["session"]["tabs_count"].is_number());
    let tabs = v["data"]["tabs"].as_array().expect("tabs should be array");
    assert!(!tabs.is_empty());
    assert!(tabs[0]["tab_id"].is_string());
    assert!(tabs[0]["url"].is_string());
    assert!(tabs[0]["title"].is_string());
    let caps = &v["data"]["capabilities"];
    assert!(caps["snapshot"].is_boolean());
    assert!(caps["pdf"].is_boolean());
    assert!(caps["upload"].is_boolean());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["meta"]["duration_ms"].is_number());
}

#[test]
fn lifecycle_status_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "status text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}]")));
    assert!(text.contains("status: running"));
    assert!(text.contains("mode: local"));
    assert!(text.contains("tabs:"));
}

// ===========================================================================
// 6. lifecycle_list_sessions — isolated
// ===========================================================================

#[test]
fn lifecycle_list_sessions_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.list-sessions");
    assert!(v["context"].is_null());
    assert!(v["data"]["total_sessions"].as_u64().unwrap_or(0) >= 1);
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let our = sessions
        .iter()
        .find(|s| s["session_id"].as_str() == Some(sid.as_str()))
        .expect("our session should appear in list");
    assert!(our["mode"].is_string());
    assert!(our["status"].is_string());
    assert!(our["headless"].is_boolean());
    assert!(our["tabs_count"].is_number());
    assert!(v["meta"]["duration_ms"].is_number());
}

#[test]
fn lifecycle_list_sessions_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions text");
    let text = stdout_str(&out);
    assert!(text.contains("session"));
    assert!(text.contains(&format!("[{sid}]")));
    assert!(text.contains("status: running"));
    assert!(text.contains("tabs:"));
}

// ===========================================================================
// 7. lifecycle_restart — isolated
// ===========================================================================

#[test]
fn lifecycle_restart_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "restart", "--session", &sid], 30);
    assert_success(&out, "restart");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.restart");
    assert_eq!(v["data"]["session"]["session_id"], sid);
    assert_eq!(v["data"]["session"]["mode"], "local");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["headless"].is_boolean());
    assert!(v["data"]["session"]["tabs_count"].is_number());
    assert_eq!(v["data"]["reopened"], true);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["meta"]["duration_ms"].is_number());

    let out = headless_json(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "status after restart");
    let v = parse_json(&out);
    assert_eq!(v["data"]["session"]["status"], "running");
}

#[test]
fn lifecycle_restart_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "restart", "--session", &sid], 30);
    assert_success(&out, "restart text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}")));
    assert!(text.contains("ok browser.restart"));
    assert!(text.contains("status: running"));
}

// ===========================================================================
// 8. lifecycle_close_after_operations — isolated
// ===========================================================================

#[test]
fn lifecycle_close_after_operations() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let out = headless(
        &["browser", "goto", &url, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "goto");

    let out = headless(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot");

    let out = headless_json(&["browser", "close", "--session", &sid], 30);
    assert_success(&out, "close after operations");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], "closed");
    assert!(v["data"]["closed_tabs"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(v["context"]["session_id"], sid);
}

// ===========================================================================
// 9. lifecycle_close_s1t2 — isolated
// ===========================================================================

#[test]
fn lifecycle_close_s1t2_closes_all_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid); // safety net on panic
    let url = url_a();
    let _t2 = new_tab_json(&sid, &url);

    let out = headless_json(&["browser", "close", "--session", &sid], 30);
    assert_success(&out, "close 2 tabs");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], "closed");
    assert_eq!(v["data"]["closed_tabs"], serde_json::json!(2));
    assert_eq!(v["context"]["session_id"], sid);
}

#[test]
fn lifecycle_close_s1t2_closes_all_text() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let url = url_a();
    let _t2 = new_tab_json(&sid, &url);

    let out = headless(&["browser", "close", "--session", &sid], 30);
    assert_success(&out, "close 2 tabs text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.close"));
    assert!(
        text.contains("closed_tabs: 2"),
        "should show closed_tabs: 2, got: {text}"
    );
}

// ===========================================================================
// 10. lifecycle_double_close — isolated
// ===========================================================================

#[test]
fn lifecycle_double_close_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid); // safety net if first close panics

    let out = headless(&["browser", "close", "--session", &sid], 30);
    assert_success(&out, "first close");

    let out = headless_json(&["browser", "close", "--session", &sid], 30);
    assert_failure(&out, "second close should fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close");
    assert!(v["data"].is_null());
    assert_eq!(v["error"]["code"], "SESSION_NOT_FOUND");
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert!(v["error"]["details"].is_object() || v["error"]["details"].is_null());
    assert!(v["meta"]["duration_ms"].is_number());
}

#[test]
fn lifecycle_double_close_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "close", "--session", &sid], 30);
    assert_success(&out, "first close");

    let out = headless(&["browser", "close", "--session", &sid], 30);
    assert_failure(&out, "second close text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

// ===========================================================================
// 10b. error path on status
// ===========================================================================

#[test]
fn lifecycle_status_nonexistent_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "status", "--session", "nonexistent"], 10);
    assert_failure(&out, "status nonexistent");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.status");
    assert!(v["data"].is_null());
    assert_eq!(v["error"]["code"], "SESSION_NOT_FOUND");
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert!(v["error"]["details"].is_object() || v["error"]["details"].is_null());
}

#[test]
fn lifecycle_status_nonexistent_text() {
    if skip() {
        return;
    }

    let out = headless(&["browser", "status", "--session", "nonexistent"], 10);
    assert_failure(&out, "status nonexistent text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

// ===========================================================================
// 11. lifecycle_concurrent_two_sessions — isolated
// ===========================================================================

#[test]
fn lifecycle_concurrent_two_sessions() {
    if skip() {
        return;
    }
    let (sid_w, prof_w) = unique_session("work");
    let (sid_p, prof_p) = unique_session("personal");

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof_w,
            "--set-session-id",
            &sid_w,
        ],
        30,
    );
    assert_success(&out, "start work");
    let _guard_w = SessionGuard::new(&sid_w);

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof_p,
            "--set-session-id",
            &sid_p,
        ],
        30,
    );
    assert_success(&out, "start personal");
    let _guard_p = SessionGuard::new(&sid_p);

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert!(v["data"]["total_sessions"].as_u64().unwrap_or(0) >= 2);
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(ids.contains(&sid_w.as_str()));
    assert!(ids.contains(&sid_p.as_str()));

    for sid in [&sid_w, &sid_p] {
        let out = headless_json(&["browser", "status", "--session", sid], 10);
        assert_success(&out, &format!("status {sid}"));
        let v = parse_json(&out);
        assert_eq!(v["data"]["session"]["status"], "running");
    }
}

// ===========================================================================
// 12. lifecycle_concurrent_parallel_operations — isolated
// ===========================================================================

#[test]
fn lifecycle_concurrent_parallel_operations() {
    if skip() {
        return;
    }
    let (sid_a, prof_a) = unique_session("alpha");
    let (sid_b, prof_b) = unique_session("beta");
    let url_a = url_a();
    let url_b = url_b();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof_a,
            "--set-session-id",
            &sid_a,
            "--open-url",
            &url_a,
        ],
        30,
    );
    assert_success(&out, "start alpha");
    let _guard_a = SessionGuard::new(&sid_a);
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
            &prof_b,
            "--set-session-id",
            &sid_b,
            "--open-url",
            &url_b,
        ],
        30,
    );
    assert_success(&out, "start beta");
    let _guard_b = SessionGuard::new(&sid_b);
    let beta_tab = parse_json(&out)["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = headless(
        &[
            "browser",
            "goto",
            &url_a,
            "--session",
            &sid_a,
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
            &url_b,
            "--session",
            &sid_b,
            "--tab",
            &beta_tab,
        ],
        30,
    );
    assert_success(&out, "goto beta");

    let sa = sid_a.clone();
    let sb = sid_b.clone();
    let at = alpha_tab.clone();
    let bt = beta_tab.clone();
    let t1 = std::thread::spawn(move || {
        headless_json(
            &[
                "browser",
                "eval",
                "window.location.href",
                "--session",
                &sa,
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
                &sb,
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
            .contains("page-a")
    );
    assert!(
        v2["data"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("page-b")
    );
}

// ===========================================================================
// 13. lifecycle_start_reuse_existing — SoloEnv
// ===========================================================================

#[test]
fn lifecycle_start_reuse_existing_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    let out = env.headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "first start");
    let v = parse_json(&out);
    assert_eq!(v["data"]["reused"], false);
    assert_eq!(v["data"]["session"]["session_id"], "local-1");

    let out = env.headless_json(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "second start (reuse)");
    let v = parse_json(&out);
    assert_eq!(v["data"]["reused"], true);
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    assert_eq!(v["data"]["session"]["status"], "running");

    let out = env.headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_sessions"], serde_json::json!(1));

    env.headless(&["browser", "close", "--session", "local-1"], 30);
}

#[test]
fn lifecycle_start_reuse_existing_text() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    env.headless(&["browser", "start", "--mode", "local", "--headless"], 30);

    let out = env.headless(&["browser", "start", "--mode", "local", "--headless"], 30);
    assert_success(&out, "second start (reuse) text");
    let text = stdout_str(&out);
    assert!(text.contains("[local-1"));
    assert!(text.contains("ok browser.start"));
    assert!(text.contains("status: running"));

    env.headless(&["browser", "close", "--session", "local-1"], 30);
}

// ===========================================================================
// 14. lifecycle_start_reuse_with_open_url — SoloEnv
// ===========================================================================

#[test]
fn lifecycle_start_reuse_with_open_url_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();
    let url_a = url_a();
    let url_b = url_b();

    let out = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            &url_a,
        ],
        30,
    );
    assert_success(&out, "first start");
    let v = parse_json(&out);
    assert_eq!(v["data"]["reused"], false);
    let tab_id = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    let out = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            &url_b,
        ],
        30,
    );
    assert_success(&out, "second start (reuse + navigate)");
    let v = parse_json(&out);
    assert_eq!(v["data"]["reused"], true);
    assert_eq!(v["data"]["session"]["session_id"], "local-1");
    assert!(
        v["data"]["tab"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "tab url should contain page-b after navigate, got: {}",
        v["data"]["tab"]["url"]
    );

    let out = env.headless_json(
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
    assert!(v["data"]["value"].as_str().unwrap_or("").contains("page-b"));

    env.headless(&["browser", "close", "--session", "local-1"], 30);
}

// ===========================================================================
// 15. lifecycle_start_bootstraps_default_config — SoloEnv
// ===========================================================================

#[test]
fn lifecycle_start_bootstraps_default_config() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();
    let path = env.config_path();

    let out = env.headless_json(&["browser", "start", "--headless"], 30);
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
    assert!(text.contains("profile_name = \"actionbook\""));

    env.headless(&["browser", "close", "--session", session_id], 30);
}

// ===========================================================================
// 16. Config precedence — SoloEnv (writes config file)
// ===========================================================================

#[test]
fn lifecycle_start_env_over_config_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    std::fs::write(env.config_path(), "[browser]\nheadless = false\n").expect("write config");

    let out = env.headless_json_with_env(
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

    env.headless(&["browser", "close", "--session", session_id], 30);
}

#[test]
fn lifecycle_start_cli_over_env_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    let out = env.headless_json_with_env(
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

    env.headless(&["browser", "close", "--session", session_id], 30);
}

// ===========================================================================
// 17. Concurrent same-profile race — SoloEnv
// ===========================================================================

#[test]
fn lifecycle_start_concurrent_same_profile_rejects_second_json() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();

    let out = env.headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "warm daemon");

    let barrier = Arc::new(Barrier::new(3));
    let home = env.actionbook_home.clone();
    let mut handles = Vec::new();

    for _ in 0..2 {
        let barrier = Arc::clone(&barrier);
        let home = home.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut cmd = Command::cargo_bin("actionbook").expect("binary exists");
            cmd.env("ACTIONBOOK_HOME", &home)
                .arg("--json")
                .args([
                    "browser",
                    "start",
                    "--mode",
                    "local",
                    "--headless",
                    "--profile",
                    "testrace",
                ])
                .timeout(Duration::from_secs(30));
            cmd.output().expect("execute command")
        }));
    }

    barrier.wait();

    let outputs: Vec<_> = handles
        .into_iter()
        .map(|handle: std::thread::JoinHandle<Output>| handle.join().expect("join"))
        .collect();

    let successes: Vec<_> = outputs.iter().filter(|o| o.status.success()).collect();
    let failures: Vec<_> = outputs.iter().filter(|o| !o.status.success()).collect();

    assert_eq!(
        successes.len(),
        1,
        "expected exactly one success\noutputs: {outputs:#?}"
    );
    assert_eq!(
        failures.len(),
        1,
        "expected exactly one rejection\noutputs: {outputs:#?}"
    );

    let success = parse_json(successes[0]);
    assert_eq!(success["data"]["reused"], false);
    let session_id = success["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();

    let failure = parse_json(failures[0]);
    assert_eq!(failure["ok"], false);
    assert_eq!(failure["command"], "browser.start");
    assert_eq!(failure["error"]["code"], "SESSION_STARTING");
    assert_eq!(
        failure["error"]["hint"],
        "retry after a few seconds or use browser status to check"
    );

    env.headless(&["browser", "close", "--session", &session_id], 30);
}

// ===========================================================================
// set-session-id must not silently reuse an existing session
// ===========================================================================

/// When a session with profile P already exists and the user requests
/// `--set-session-id NEW_ID --profile P`, the command must NOT silently
/// reuse the existing session. It should fail with SESSION_ALREADY_EXISTS
/// because profile P is already occupied.
#[test]
fn lifecycle_set_session_id_rejects_reuse_of_occupied_profile() {
    if skip() {
        return;
    }
    let (sid1, prof1) = unique_session("reuse");
    let (sid2, _) = unique_session("reuse2");

    // Start first session with profile
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof1,
            "--set-session-id",
            &sid1,
        ],
        30,
    );
    assert_success(&out, "start first session");
    let _guard = SessionGuard::new(&sid1);

    // Try to start second session with SAME profile but DIFFERENT session ID.
    // Must NOT silently return the first session.
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof1,
            "--set-session-id",
            &sid2,
        ],
        30,
    );
    assert_failure(&out, "second start with set-session-id must fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["code"], "SESSION_ALREADY_EXISTS",
        "must return SESSION_ALREADY_EXISTS, not silently reuse"
    );
}

/// When --set-session-id matches an already-running session's ID,
/// the command must fail with SESSION_ALREADY_EXISTS, not silently reuse.
#[test]
fn lifecycle_set_session_id_rejects_duplicate_id() {
    if skip() {
        return;
    }
    let (sid, prof) = unique_session("dupid");

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof,
            "--set-session-id",
            &sid,
        ],
        30,
    );
    assert_success(&out, "start session");
    let _guard = SessionGuard::new(&sid);

    // Try to start again with the SAME session ID (different profile).
    let (_, prof2) = unique_session("dupid2");
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            &prof2,
            "--set-session-id",
            &sid,
        ],
        30,
    );
    assert_failure(&out, "duplicate set-session-id must fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["code"], "SESSION_ALREADY_EXISTS",
        "must return SESSION_ALREADY_EXISTS for duplicate ID"
    );
}
