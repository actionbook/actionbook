//! Browser tab management E2E tests: list-tabs, new-tab, close-tab.
//!
//! Each test is self-contained: start -> operate -> assert -> close.
//! Covers BOTH JSON and text output.
//!
//! Uses local HTTP server from harness — no external network dependency.

use crate::harness::{
    SessionGuard, assert_context_object, assert_error_envelope, assert_failure, assert_meta,
    assert_success, assert_tab_id, headless, headless_json, new_tab_json, parse_json, skip,
    start_named_session, start_session, stdout_str, unique_session, url_a, url_b, url_c,
};

// ===========================================================================
// Group 1: list-tabs — Basic
// ===========================================================================

#[test]
fn tab_list_tabs_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.list-tabs");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);

    assert!(v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 1);
    let tabs = v["data"]["tabs"].as_array().expect("tabs array");
    let tab = &tabs[0];
    assert_tab_id(&tab["tab_id"]);
    assert!(tab["url"].is_string());
    assert!(tab["title"].is_string());
    assert!(
        !tab.as_object().unwrap().contains_key("native_tab_id"),
        "native_tab_id should not be present"
    );
    assert_meta(&v);
}

#[test]
fn tab_list_tabs_text() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}]")));
    assert!(text.contains("tab"));
}

#[test]
fn tab_list_tabs_after_new_tab_json() {
    if skip() {
        return;
    }
    let (sid, t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let t2 = new_tab_json(&sid, &url_b());

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after new-tab");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(2));
    let tabs = v["data"]["tabs"].as_array().unwrap();
    let ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(ids.contains(&t1.as_str()), "should have t1");
    assert!(ids.contains(&t2.as_str()), "should have t2");

    for tab in tabs {
        assert_tab_id(&tab["tab_id"]);
        assert!(tab["url"].is_string());
        assert!(tab["title"].is_string());
    }
}

#[test]
fn tab_list_tabs_after_new_tab_text() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let _t2 = new_tab_json(&sid, &url_b());

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after new-tab text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}]")));
    assert!(text.contains("2"));
}

// ===========================================================================
// Group 2: new-tab — Basic
// ===========================================================================

#[test]
fn tab_new_tab_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(&["browser", "new-tab", &url_b, "--session", &sid], 30);
    assert_success(&out, "new-tab json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.new-tab");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_string(),
        "context.tab_id should be present"
    );

    let tab = &v["data"]["tab"];
    assert_tab_id(&tab["tab_id"]);
    assert!(tab["url"].is_string());
    assert!(tab["title"].is_string());
    assert!(!tab.as_object().unwrap().contains_key("native_tab_id"));
    assert_eq!(v["data"]["created"], true);
    assert_eq!(v["data"]["new_window"], false);
    assert_meta(&v);
}

#[test]
fn tab_new_tab_text() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless(&["browser", "new-tab", &url_b, "--session", &sid], 30);
    assert_success(&out, "new-tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains(&format!("[{sid}")),
        "header should contain session_id"
    );
    assert!(text.contains("ok browser.new-tab"));
    assert!(text.contains("title:"));
}

#[test]
fn tab_new_tab_sequential_ids_json() {
    if skip() {
        return;
    }
    let (sid, t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let t2 = new_tab_json(&sid, &url_b());
    let t3 = new_tab_json(&sid, &url_c());

    assert!(!t1.is_empty() && !t2.is_empty() && !t3.is_empty());
    assert!(
        t1 != t2 && t2 != t3 && t1 != t3,
        "all tab_ids must be unique"
    );

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs 3 tabs");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(3));
}

#[test]
fn tab_new_tab_alias_open_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let url_b = url_b();
    let out = headless_json(&["browser", "open", &url_b, "--session", &sid], 30);
    assert_success(&out, "browser open alias");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.new-tab");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);

    let tab = &v["data"]["tab"];
    assert_tab_id(&tab["tab_id"]);
    assert_eq!(v["data"]["created"], true);
    assert_eq!(v["data"]["new_window"], false);
    assert_meta(&v);
}

// ===========================================================================
// Group 3: close-tab — Basic
// ===========================================================================

#[test]
fn tab_close_tab_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let t2 = new_tab_json(&sid, &url_b());

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_success(&out, "close-tab json");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser.close-tab");
    assert!(v["error"].is_null());
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], t2);
    assert_eq!(v["data"]["closed_tab_id"], t2);
    assert_meta(&v);
}

#[test]
fn tab_close_tab_text() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let t2 = new_tab_json(&sid, &url_b());

    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_success(&out, "close-tab text");
    let text = stdout_str(&out);
    assert!(text.contains(&format!("[{sid}")));
    assert!(text.contains("ok browser.close-tab"));
}

#[test]
fn tab_close_tab_then_list_json() {
    if skip() {
        return;
    }
    let (sid, t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let t2 = new_tab_json(&sid, &url_b());
    let t3 = new_tab_json(&sid, &url_c());

    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_success(&out, "close t2");

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs after close");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(2));
    let tabs = v["data"]["tabs"].as_array().unwrap();
    let ids: Vec<&str> = tabs.iter().filter_map(|t| t["tab_id"].as_str()).collect();
    assert!(ids.contains(&t1.as_str()), "t1 should remain");
    assert!(ids.contains(&t3.as_str()), "t3 should remain");
    assert!(!ids.contains(&t2.as_str()), "t2 should be closed");
}

// ===========================================================================
// Group 4: Error Cases
// ===========================================================================

#[test]
fn tab_list_tabs_nonexistent_session_json() {
    if skip() {
        return;
    }
    let out = headless_json(&["browser", "list-tabs", "--session", "nonexistent"], 10);
    assert_failure(&out, "list-tabs nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.list-tabs");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn tab_list_tabs_nonexistent_session_text() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "list-tabs", "--session", "nonexistent"], 10);
    assert_failure(&out, "list-tabs nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

#[test]
fn tab_new_tab_nonexistent_session_json() {
    if skip() {
        return;
    }
    let url_a = url_a();
    let out = headless_json(
        &["browser", "new-tab", &url_a, "--session", "nonexistent"],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.new-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn tab_new_tab_nonexistent_session_text() {
    if skip() {
        return;
    }
    let url_a = url_a();
    let out = headless(
        &["browser", "new-tab", &url_a, "--session", "nonexistent"],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

#[test]
fn tab_close_tab_nonexistent_session_json() {
    if skip() {
        return;
    }
    let out = headless_json(
        &[
            "browser",
            "close-tab",
            "--session",
            "nonexistent",
            "--tab",
            "fake",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(v["context"].is_null());
}

#[test]
fn tab_close_tab_nonexistent_session_text() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "close-tab",
            "--session",
            "nonexistent",
            "--tab",
            "fake",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("error SESSION_NOT_FOUND:"));
}

#[test]
fn tab_close_tab_nonexistent_tab_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let out = headless_json(
        &[
            "browser",
            "close-tab",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(
        v["context"].is_object(),
        "context should be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);
}

#[test]
fn tab_close_tab_nonexistent_tab_text() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let out = headless(
        &[
            "browser",
            "close-tab",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab text");
    let text = stdout_str(&out);
    assert!(text.contains("error TAB_NOT_FOUND:"));
}

#[test]
fn tab_close_tab_double_close_json() {
    if skip() {
        return;
    }
    let (sid, _t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let t2 = new_tab_json(&sid, &url_b());

    // First close: success
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_success(&out, "first close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["closed_tab_id"], t2);

    // Second close: TAB_NOT_FOUND
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        30,
    );
    assert_failure(&out, "second close should fail");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}

// ===========================================================================
// Group 5: Concurrent — Same Session
// ===========================================================================

#[test]
fn tab_concurrent_multi_tab_same_session() {
    if skip() {
        return;
    }
    let (sid, t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);
    let t2 = new_tab_json(&sid, &url_b());
    let t3 = new_tab_json(&sid, &url_c());

    let sid1 = sid.clone();
    let t1c = t1.clone();
    let sid2 = sid.clone();
    let t2c = t2.clone();
    let sid3 = sid.clone();
    let t3c = t3.clone();

    let h1 = std::thread::spawn(move || {
        headless_json(
            &["browser", "eval", "1+1", "--session", &sid1, "--tab", &t1c],
            30,
        )
    });
    let h2 = std::thread::spawn(move || {
        headless_json(
            &["browser", "eval", "1+1", "--session", &sid2, "--tab", &t2c],
            30,
        )
    });
    let h3 = std::thread::spawn(move || {
        headless_json(
            &["browser", "eval", "1+1", "--session", &sid3, "--tab", &t3c],
            30,
        )
    });

    let out1 = h1.join().expect("thread t1");
    let out2 = h2.join().expect("thread t2");
    let out3 = h3.join().expect("thread t3");
    assert_success(&out1, "eval t1");
    assert_success(&out2, "eval t2");
    assert_success(&out3, "eval t3");

    for out in [&out1, &out2, &out3] {
        let v = parse_json(out);
        assert_eq!(v["context"]["session_id"], sid);
    }

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "list-tabs 3 tabs");
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(3));
}

// ===========================================================================
// Group 6: Concurrent — Cross-Session
// ===========================================================================

#[test]
fn tab_concurrent_multi_tab_cross_session() {
    if skip() {
        return;
    }
    let (sid_a, prof_a) = unique_session("cross-a");
    let (sid_b, prof_b) = unique_session("cross-b");

    let _ta = start_named_session(&sid_a, &prof_a, &url_a());
    let _guard_a = SessionGuard::new(&sid_a);
    let _tb = start_named_session(&sid_b, &prof_b, &url_c());
    let _guard_b = SessionGuard::new(&sid_b);
    let _t2a = new_tab_json(&sid_a, &url_b());
    let _t2b = new_tab_json(&sid_b, &url_a());

    let sa = sid_a.clone();
    let sb = sid_b.clone();
    let ha =
        std::thread::spawn(move || headless_json(&["browser", "list-tabs", "--session", &sa], 10));
    let hb =
        std::thread::spawn(move || headless_json(&["browser", "list-tabs", "--session", &sb], 10));

    let out_a = ha.join().unwrap();
    let out_b = hb.join().unwrap();
    assert_success(&out_a, "list-tabs session-a");
    assert_success(&out_b, "list-tabs session-b");

    let va = parse_json(&out_a);
    let vb = parse_json(&out_b);
    assert_eq!(va["data"]["total_tabs"], serde_json::json!(2));
    assert_eq!(vb["data"]["total_tabs"], serde_json::json!(2));
}

#[test]
fn tab_concurrent_close_tabs_cross_session() {
    if skip() {
        return;
    }
    let (sid_x, prof_x) = unique_session("close-x");
    let (sid_y, prof_y) = unique_session("close-y");

    let _tx = start_named_session(&sid_x, &prof_x, &url_a());
    let _guard_x = SessionGuard::new(&sid_x);
    let _ty = start_named_session(&sid_y, &prof_y, &url_c());
    let _guard_y = SessionGuard::new(&sid_y);
    let t2x = new_tab_json(&sid_x, &url_b());
    let t2y = new_tab_json(&sid_y, &url_a());

    let sx = sid_x.clone();
    let sy = sid_y.clone();
    let tx_clone = t2x.clone();
    let ty_clone = t2y.clone();
    let hx = std::thread::spawn(move || {
        headless_json(
            &["browser", "close-tab", "--session", &sx, "--tab", &tx_clone],
            30,
        )
    });
    let hy = std::thread::spawn(move || {
        headless_json(
            &["browser", "close-tab", "--session", &sy, "--tab", &ty_clone],
            30,
        )
    });

    let out_x = hx.join().unwrap();
    let out_y = hy.join().unwrap();
    assert_success(&out_x, "close-tab session-x");
    assert_success(&out_y, "close-tab session-y");

    let vx = parse_json(&out_x);
    let vy = parse_json(&out_y);
    assert_eq!(vx["data"]["closed_tab_id"], t2x);
    assert_eq!(vy["data"]["closed_tab_id"], t2y);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid_x], 10);
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(1));

    let out = headless_json(&["browser", "list-tabs", "--session", &sid_y], 10);
    let v = parse_json(&out);
    assert_eq!(v["data"]["total_tabs"], serde_json::json!(1));
}
