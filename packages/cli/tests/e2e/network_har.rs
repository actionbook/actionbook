//! Browser network HAR recording E2E tests.
//!
//! Covers the planned `browser network har start/stop` commands.

use std::fs;
use std::path::{Path, PathBuf};

use crate::harness::{
    SessionGuard, assert_error_envelope, assert_failure, assert_meta, assert_success,
    headless_json, new_tab_json, parse_json, skip, start_session, url_fast_redirect,
    url_network_load, url_network_xhr,
};

fn har_start(session_id: &str, tab_id: &str) -> std::process::Output {
    headless_json(
        &[
            "browser",
            "network",
            "har",
            "start",
            "--session",
            session_id,
            "--tab",
            tab_id,
        ],
        15,
    )
}

fn har_stop(session_id: &str, tab_id: &str) -> std::process::Output {
    headless_json(
        &[
            "browser",
            "network",
            "har",
            "stop",
            "--session",
            session_id,
            "--tab",
            tab_id,
        ],
        15,
    )
}

fn har_stop_with_out(session_id: &str, tab_id: &str, out_path: &Path) -> std::process::Output {
    let out = out_path.to_string_lossy().to_string();
    headless_json(
        &[
            "browser",
            "network",
            "har",
            "stop",
            "--session",
            session_id,
            "--tab",
            tab_id,
            "--out",
            &out,
        ],
        15,
    )
}

fn wait_requests_done(session_id: &str, tab_id: &str) {
    let out = headless_json(
        &[
            "browser",
            "wait",
            "condition",
            "window.__ab_requests_done === true",
            "--session",
            session_id,
            "--tab",
            tab_id,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait requests done");
}

fn har_json_from_file(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read har file {}: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse har file {} as json: {e}", path.display()))
}

fn har_entries(v: &serde_json::Value) -> &[serde_json::Value] {
    v["log"]["entries"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn assert_har_entry_shape(entry: &serde_json::Value) {
    assert!(entry["startedDateTime"].is_string());
    assert!(entry["time"].is_number());
    assert!(entry["request"].is_object());
    assert!(entry["response"].is_object());
    assert!(entry["timings"].is_object());
}

fn assert_timing_number(value: &serde_json::Value, key: &str) {
    assert!(
        value[key].is_number(),
        "timings.{key} should be numeric, got {}",
        value[key]
    );
}

#[test]
fn har_start_stop_creates_file() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let start_out = har_start(&sid, &tid);
    assert_success(&start_out, "har start");

    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            &url_network_load(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        20,
    );
    assert_success(&goto_out, "goto network load");

    let stop_out = har_stop(&sid, &tid);
    assert_success(&stop_out, "har stop");
    let stop_v = parse_json(&stop_out);
    let path = PathBuf::from(
        stop_v["data"]["path"]
            .as_str()
            .expect("har stop should return path"),
    );
    assert!(path.exists(), "HAR file should exist at {}", path.display());

    let har = har_json_from_file(&path);
    assert!(!har_entries(&har).is_empty(), "HAR should contain entries");
}

#[test]
fn har_stop_returns_path_and_count() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_load());
    let _guard = SessionGuard::new(&sid);

    let start_out = har_start(&sid, &tid);
    assert_success(&start_out, "har start");

    let reload_out = headless_json(
        &[
            "browser",
            "goto",
            &url_network_load(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        20,
    );
    assert_success(&reload_out, "goto network load");

    let stop_out = har_stop(&sid, &tid);
    assert_success(&stop_out, "har stop");
    let v = parse_json(&stop_out);

    assert_eq!(v["command"], "browser network har stop");
    assert!(v["data"]["path"].is_string());
    assert!(v["data"]["count"].is_number());
    assert_meta(&v);
}

#[test]
fn har_custom_output_path() {
    if skip() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let custom_path = temp.path().join("custom.har");

    let (sid, tid) = start_session(&url_network_load());
    let _guard = SessionGuard::new(&sid);

    let start_out = har_start(&sid, &tid);
    assert_success(&start_out, "har start");

    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            &url_network_load(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        20,
    );
    assert_success(&goto_out, "goto network load");

    let stop_out = har_stop_with_out(&sid, &tid, &custom_path);
    assert_success(&stop_out, "har stop custom out");
    let v = parse_json(&stop_out);

    assert_eq!(v["data"]["path"], custom_path.to_string_lossy().to_string());
    assert!(custom_path.exists(), "custom HAR path should be created");
}

#[test]
fn har_valid_json_structure() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_load());
    let _guard = SessionGuard::new(&sid);

    assert_success(&har_start(&sid, &tid), "har start");
    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            &url_network_load(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        20,
    );
    assert_success(&goto_out, "goto network load");

    let stop_v = parse_json(&har_stop(&sid, &tid));
    let path = PathBuf::from(stop_v["data"]["path"].as_str().expect("har path"));
    let har = har_json_from_file(&path);

    assert_eq!(har["log"]["version"], "1.2");
    assert!(har["log"]["creator"].is_object());
    assert!(har["log"]["creator"]["name"].is_string());
    assert!(har["log"]["creator"]["version"].is_string());
    assert!(har["log"]["entries"].is_array());
}

#[test]
fn har_entry_has_request_response() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    assert_success(&har_start(&sid, &tid), "har start");
    assert_success(
        &headless_json(
            &[
                "browser",
                "goto",
                &url_network_xhr(),
                "--session",
                &sid,
                "--tab",
                &tid,
            ],
            20,
        ),
        "goto network xhr",
    );
    wait_requests_done(&sid, &tid);

    let stop_v = parse_json(&har_stop(&sid, &tid));
    let path = PathBuf::from(stop_v["data"]["path"].as_str().expect("har path"));
    let har = har_json_from_file(&path);
    let entry = har_entries(&har)
        .iter()
        .find(|entry| {
            entry["request"]["url"]
                .as_str()
                .is_some_and(|url| url.contains("/api/data?source=fetch"))
        })
        .expect("fetch api entry");

    assert_har_entry_shape(entry);
    assert!(entry["request"]["method"].is_string());
    assert!(entry["request"]["url"].is_string());
    assert!(entry["request"]["headers"].is_array());
    assert!(entry["response"]["status"].is_number());
    assert!(entry["response"]["headers"].is_array());
    assert!(entry["response"]["content"]["mimeType"].is_string());
}

#[test]
fn har_entry_has_timings() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_load());
    let _guard = SessionGuard::new(&sid);

    assert_success(&har_start(&sid, &tid), "har start");
    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            &url_network_load(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        20,
    );
    assert_success(&goto_out, "goto network load");

    let stop_v = parse_json(&har_stop(&sid, &tid));
    let path = PathBuf::from(stop_v["data"]["path"].as_str().expect("har path"));
    let har = har_json_from_file(&path);
    let entry = har_entries(&har).first().expect("first har entry");
    let timings = &entry["timings"];

    assert_timing_number(timings, "blocked");
    assert_timing_number(timings, "dns");
    assert_timing_number(timings, "connect");
    assert_timing_number(timings, "ssl");
    assert_timing_number(timings, "send");
    assert_timing_number(timings, "wait");
    assert_timing_number(timings, "receive");
}

#[test]
fn har_redirect_chain_multiple_entries() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    assert_success(&har_start(&sid, &tid), "har start");
    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            &url_fast_redirect(),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        20,
    );
    assert_success(&goto_out, "goto redirect");

    let stop_v = parse_json(&har_stop(&sid, &tid));
    let path = PathBuf::from(stop_v["data"]["path"].as_str().expect("har path"));
    let har = har_json_from_file(&path);
    let entries = har_entries(&har);

    assert!(
        entries.len() >= 2,
        "redirect HAR should contain at least two entries"
    );
    assert!(
        entries.iter().any(|e| e["response"]["status"] == 302),
        "redirect HAR should include 302 hop"
    );
    assert!(
        entries.iter().any(|e| {
            e["request"]["url"]
                .as_str()
                .is_some_and(|url| url.contains("/page-b"))
        }),
        "redirect HAR should include final destination entry"
    );
}

#[test]
fn har_stop_without_start_errors() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let out = har_stop(&sid, &tid);
    assert_failure(&out, "har stop without start");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser network har stop");
    assert_error_envelope(&v, "HAR_NOT_RECORDING");
}

#[test]
fn har_start_while_recording_errors() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    assert_success(&har_start(&sid, &tid), "first har start");
    let out = har_start(&sid, &tid);
    assert_failure(&out, "second har start");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser network har start");
    assert_error_envelope(&v, "HAR_ALREADY_RECORDING");
}

#[test]
fn har_tab_close_cleans_recorder() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    assert_success(&har_start(&sid, &tid), "har start");

    let close_out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &tid],
        20,
    );
    assert_success(&close_out, "close tab while recording");

    let stop_out = har_stop(&sid, &tid);
    assert_failure(&stop_out, "har stop after tab close");
    let v = parse_json(&stop_out);
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}

#[test]
fn har_session_close_cleans_recorder() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");

    assert_success(&har_start(&sid, &tid), "har start");

    let close_out = headless_json(&["browser", "close", "--session", &sid], 30);
    assert_success(&close_out, "close session while recording");

    let stop_out = har_stop(&sid, &tid);
    assert_failure(&stop_out, "har stop after session close");
    let v = parse_json(&stop_out);
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn har_per_tab_independent_recording() {
    if skip() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let har_a = temp.path().join("tab-a.har");
    let har_b = temp.path().join("tab-b.har");

    let (sid, tid_a) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid_b = new_tab_json(&sid, "about:blank");

    assert_success(&har_start(&sid, &tid_a), "har start tab a");
    let goto_a = headless_json(
        &[
            "browser",
            "goto",
            &url_network_load(),
            "--session",
            &sid,
            "--tab",
            &tid_a,
        ],
        20,
    );
    assert_success(&goto_a, "goto tab a");

    assert_success(&har_start(&sid, &tid_b), "har start tab b");
    let goto_b = headless_json(
        &[
            "browser",
            "goto",
            &url_network_xhr(),
            "--session",
            &sid,
            "--tab",
            &tid_b,
        ],
        20,
    );
    assert_success(&goto_b, "goto tab b");
    wait_requests_done(&sid, &tid_b);

    assert_success(
        &har_stop_with_out(&sid, &tid_a, &har_a),
        "har stop tab a custom out",
    );
    assert_success(
        &har_stop_with_out(&sid, &tid_b, &har_b),
        "har stop tab b custom out",
    );

    let har_json_a = har_json_from_file(&har_a);
    let har_json_b = har_json_from_file(&har_b);
    let urls_a: Vec<&str> = har_entries(&har_json_a)
        .iter()
        .filter_map(|entry| entry["request"]["url"].as_str())
        .collect();
    let urls_b: Vec<&str> = har_entries(&har_json_b)
        .iter()
        .filter_map(|entry| entry["request"]["url"].as_str())
        .collect();

    assert!(
        urls_a.iter().all(|url| !url.contains("/api/data?source=")),
        "tab A HAR should not include XHR fixture requests from tab B"
    );
    assert!(
        urls_b
            .iter()
            .any(|url| url.contains("/api/data?source=fetch")),
        "tab B HAR should include its own XHR/fetch requests"
    );
}

#[test]
fn har_cross_session_independent_recording() {
    if skip() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let har_s1 = temp.path().join("session1.har");
    let har_s2 = temp.path().join("session2.har");

    // Two independent sessions, each with their own tab and HAR recorder.
    let (sid1, tid1) = start_session("about:blank");
    let _guard1 = SessionGuard::new(&sid1);
    let (sid2, tid2) = start_session("about:blank");
    let _guard2 = SessionGuard::new(&sid2);

    assert_success(&har_start(&sid1, &tid1), "har start session 1");
    assert_success(&har_start(&sid2, &tid2), "har start session 2");

    // Session 1: navigate to the network-load fixture (static assets, no XHR).
    assert_success(
        &headless_json(
            &[
                "browser",
                "goto",
                &url_network_load(),
                "--session",
                &sid1,
                "--tab",
                &tid1,
            ],
            20,
        ),
        "goto session 1",
    );

    // Session 2: navigate to the XHR fixture (issues fetch/XHR calls).
    assert_success(
        &headless_json(
            &[
                "browser",
                "goto",
                &url_network_xhr(),
                "--session",
                &sid2,
                "--tab",
                &tid2,
            ],
            20,
        ),
        "goto session 2",
    );
    wait_requests_done(&sid2, &tid2);

    assert_success(
        &har_stop_with_out(&sid1, &tid1, &har_s1),
        "har stop session 1",
    );
    assert_success(
        &har_stop_with_out(&sid2, &tid2, &har_s2),
        "har stop session 2",
    );

    let har_json_s1 = har_json_from_file(&har_s1);
    let har_json_s2 = har_json_from_file(&har_s2);

    let urls_s1: Vec<&str> = har_entries(&har_json_s1)
        .iter()
        .filter_map(|e| e["request"]["url"].as_str())
        .collect();
    let urls_s2: Vec<&str> = har_entries(&har_json_s2)
        .iter()
        .filter_map(|e| e["request"]["url"].as_str())
        .collect();

    // Session 1 should not contain XHR fixture requests from session 2.
    assert!(
        urls_s1.iter().all(|url| !url.contains("/api/data?source=")),
        "session 1 HAR should not include XHR requests from session 2"
    );
    // Session 2 should contain its own XHR/fetch requests.
    assert!(
        urls_s2
            .iter()
            .any(|url| url.contains("/api/data?source=fetch")),
        "session 2 HAR should include its own fetch requests"
    );
    // Session 1 should have entries (the static page load).
    assert!(!urls_s1.is_empty(), "session 1 HAR should not be empty");
}
