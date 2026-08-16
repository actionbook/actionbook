//! Browser network HAR recording E2E tests.
//!
//! Covers the planned `browser network har start/stop` commands.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::harness::{
    SessionGuard, SoloEnv, assert_error_envelope, assert_failure, assert_meta, assert_success,
    headless_json, new_tab_json, parse_json, skip, start_session, unique_session,
    url_fast_redirect, url_network_load, url_network_xhr,
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

fn har_start_with_max_entries(
    session_id: &str,
    tab_id: &str,
    max_entries: usize,
) -> std::process::Output {
    let max_entries = max_entries.to_string();
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
            "--max-entries",
            &max_entries,
        ],
        15,
    )
}

/// Start HAR recording capturing **all** resource types, not just the default
/// xhr,fetch. Used by tests that navigate to static fixtures (network-load,
/// redirect) where Document/Script/Stylesheet entries are the whole point.
fn har_start_all(session_id: &str, tab_id: &str) -> std::process::Output {
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
            "--resource-types",
            "all",
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

fn issue_bulk_requests(session_id: &str, tab_id: &str, count: usize, prefix: &str) {
    let api_prefix = url_network_xhr().replace("/network-xhr", "/api/data?source=");
    let expression = format!(
        "await Promise.all(Array.from({{ length: {count} }}, (_, i) => fetch(`{api_prefix}{prefix}-${{i}}`).then(r => r.text())))"
    );
    let argv = [
        "browser".to_string(),
        "eval".to_string(),
        expression,
        "--session".to_string(),
        session_id.to_string(),
        "--tab".to_string(),
        tab_id.to_string(),
    ];
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let out = headless_json(&args, 30);
    assert_success(&out, "issue bulk requests");
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

fn har_start_in_env(env: &SoloEnv, session_id: &str, tab_id: &str) -> std::process::Output {
    env.headless_json(
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

fn har_stop_in_env(env: &SoloEnv, session_id: &str, tab_id: &str) -> std::process::Output {
    env.headless_json(
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

fn wait_requests_done_in_env(env: &SoloEnv, session_id: &str, tab_id: &str) {
    let out = env.headless_json(
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

fn issue_bulk_requests_in_env(env: &SoloEnv, session_id: &str, tab_id: &str, count: usize) {
    let api_prefix = url_network_xhr().replace("/network-xhr", "/api/data?source=");
    let expression = format!(
        "await Promise.all(Array.from({{ length: {count} }}, (_, i) => fetch(`{api_prefix}act971-${{i}}`).then(r => r.text())))"
    );
    let argv = [
        "browser".to_string(),
        "eval".to_string(),
        expression,
        "--session".to_string(),
        session_id.to_string(),
        "--tab".to_string(),
        tab_id.to_string(),
    ];
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let out = env.headless_json(&args, 30);
    assert_success(&out, "issue bulk requests");
}

fn wait_page_ready_in_env(env: &SoloEnv, session_id: &str, tab_id: &str) {
    for _ in 0..10 {
        let out = env.headless_json(
            &[
                "browser",
                "eval",
                "document.readyState",
                "--session",
                session_id,
                "--tab",
                tab_id,
            ],
            5,
        );
        if out.status.success() {
            let v = parse_json(&out);
            if v["data"]["value"].as_str() == Some("complete") {
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn start_named_session_in_env(
    env: &SoloEnv,
    session_id: &str,
    profile: &str,
    url: &str,
    har_out: Option<&Path>,
) -> std::process::Output {
    let mut argv = vec![
        "browser".to_string(),
        "start".to_string(),
        "--mode".to_string(),
        "local".to_string(),
        "--headless".to_string(),
        "--profile".to_string(),
        profile.to_string(),
        "--set-session-id".to_string(),
        session_id.to_string(),
    ];
    if let Some(path) = har_out {
        argv.push("--har-out".to_string());
        argv.push(path.to_string_lossy().to_string());
    }
    argv.push("--open-url".to_string());
    argv.push(url.to_string());
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    env.headless_json(&args, 30)
}

fn start_named_session_tab_in_env(
    env: &SoloEnv,
    session_id: &str,
    profile: &str,
    url: &str,
    har_out: Option<&Path>,
) -> String {
    let out = start_named_session_in_env(env, session_id, profile, url, har_out);
    assert_success(&out, &format!("start {session_id}"));
    let v = parse_json(&out);
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .expect("tab id")
        .to_string();
    wait_page_ready_in_env(env, session_id, &tid);
    tid
}

fn new_tab_json_in_env(env: &SoloEnv, session_id: &str, url: &str) -> String {
    let out = env.headless_json(&["browser", "new-tab", url, "--session", session_id], 30);
    assert_success(&out, "new-tab");
    let v = parse_json(&out);
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .expect("new-tab tab id")
        .to_string();
    wait_page_ready_in_env(env, session_id, &tid);
    tid
}

fn daemon_pid(env: &SoloEnv) -> u32 {
    let pid_path = Path::new(&env.actionbook_home).join("daemon.pid");
    fs::read_to_string(&pid_path)
        .unwrap_or_else(|e| panic!("read daemon pid {}: {e}", pid_path.display()))
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse daemon pid {}: {e}", pid_path.display()))
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .is_ok_and(|out| out.status.success())
}

#[cfg(unix)]
fn send_sigterm_and_wait(env: &SoloEnv) {
    let pid = daemon_pid(env);
    let out = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .expect("spawn kill -TERM");
    assert!(
        out.status.success(),
        "kill -TERM {pid} failed: status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let pid_file_exists = Path::new(&env.actionbook_home).join("daemon.pid").exists();
        if !pid_alive(pid) && !pid_file_exists {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    panic!("daemon pid {pid} did not exit after SIGTERM");
}

fn daemon_log_path(env: &SoloEnv) -> PathBuf {
    Path::new(&env.actionbook_home).join("daemon.log")
}

fn har_dir_listing(env: &SoloEnv) -> BTreeSet<String> {
    let dir = Path::new(&env.actionbook_home).join("har");
    match fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => BTreeSet::new(),
    }
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

    // network-load is a static page (no XHR), so we need `--resource-types all`
    // to end up with non-empty entries.
    let start_out = har_start_all(&sid, &tid);
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
fn har_entry_captures_response_body() {
    // Core new behavior: response.content.text is populated for XHR/fetch.
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

    // At least one XHR/fetch entry should have response.content.text set to a
    // non-empty string. If this fails, the body-capture spawn is broken.
    let populated = har_entries(&har)
        .iter()
        .filter(|e| {
            e["response"]["content"]["text"]
                .as_str()
                .is_some_and(|t| !t.is_empty())
        })
        .count();
    assert!(
        populated > 0,
        "expected at least one XHR/fetch entry with response.content.text populated, got 0"
    );
}

#[test]
fn har_no_bodies_flag_skips_body_capture() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let start_out = headless_json(
        &[
            "browser",
            "network",
            "har",
            "start",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--no-bodies",
        ],
        15,
    );
    assert_success(&start_out, "har start --no-bodies");

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

    // With --no-bodies, no entry should have response.content.text.
    let populated = har_entries(&har)
        .iter()
        .filter(|e| e["response"]["content"].get("text").is_some())
        .count();
    assert_eq!(
        populated, 0,
        "--no-bodies should prevent any text field from being written"
    );
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

    // Document / static assets aren't in the default xhr,fetch filter.
    assert_success(&har_start_all(&sid, &tid), "har start");
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

    // Top-level redirects are Document-typed, not XHR/Fetch.
    assert_success(&har_start_all(&sid, &tid), "har start");
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

    // Session 1 loads a static page (Document/CSS/JS), so needs all types.
    // Session 2 issues XHR/fetch, which is the default.
    assert_success(&har_start_all(&sid1, &tid1), "har start session 1");
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

mod truncation {
    use super::*;

    #[test]
    fn har_truncation_surfaces_in_envelope() {
        if skip() {
            return;
        }

        let (sid, tid) = start_session(&url_network_xhr());
        let _guard = SessionGuard::new(&sid);
        wait_requests_done(&sid, &tid);

        let start_out = har_start_with_max_entries(&sid, &tid, 5);
        assert_success(&start_out, "har start --max-entries 5");

        issue_bulk_requests(&sid, &tid, 12, "truncation");

        let stop_out = har_stop(&sid, &tid);
        assert_success(&stop_out, "har stop after truncation");
        let v = parse_json(&stop_out);

        assert_eq!(v["meta"]["truncated"], true);
        let warnings = v["meta"]["warnings"]
            .as_array()
            .expect("meta warnings array");
        let warning = warnings
            .first()
            .and_then(|value| value.as_str())
            .expect("first warning string");
        assert!(
            warning.starts_with("HAR_TRUNCATED:"),
            "expected HAR_TRUNCATED warning, got {warning:?}"
        );
        assert!(
            warning.contains("max_entries=5"),
            "expected warning to mention cap, got {warning:?}"
        );
        assert!(
            v["data"]["dropped"]
                .as_u64()
                .is_some_and(|dropped| dropped >= 5)
        );
        assert_eq!(v["data"]["max_entries"], 5);
    }

    #[test]
    fn har_clean_stop_has_no_truncation_marker() {
        if skip() {
            return;
        }

        let (sid, tid) = start_session(&url_network_xhr());
        let _guard = SessionGuard::new(&sid);
        wait_requests_done(&sid, &tid);

        let start_out = har_start_with_max_entries(&sid, &tid, 100);
        assert_success(&start_out, "har start --max-entries 100");

        issue_bulk_requests(&sid, &tid, 3, "clean-stop");

        let stop_out = har_stop(&sid, &tid);
        assert_success(&stop_out, "har stop clean");
        let v = parse_json(&stop_out);

        assert_ne!(v["meta"]["truncated"], true);
        let warnings = v["meta"]["warnings"]
            .as_array()
            .expect("meta warnings array should exist");
        assert!(
            warnings.is_empty(),
            "clean stop should not emit warnings, got {warnings:?}"
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "SIGTERM-based graceful shutdown is Unix-only")]
    fn sigterm_flushes_har_when_har_out_set_and_single_recorder() {
        if skip() {
            return;
        }

        let env = SoloEnv::new();
        let (sid, profile) = unique_session("act971-positive");
        let har_path = Path::new(&env.actionbook_home).join("act971-positive.har");
        let tid = start_named_session_tab_in_env(
            &env,
            &sid,
            &profile,
            &url_network_xhr(),
            Some(&har_path),
        );
        wait_requests_done_in_env(&env, &sid, &tid);

        assert_success(&har_start_in_env(&env, &sid, &tid), "har start");
        issue_bulk_requests_in_env(&env, &sid, &tid, 4);

        send_sigterm_and_wait(&env);

        assert!(
            har_path.exists(),
            "SIGTERM with --har-out and a single recorder should flush HAR to {}",
            har_path.display()
        );
        let har = har_json_from_file(&har_path);
        assert!(
            har["log"]["entries"].is_array(),
            "flushed HAR should contain log.entries array: {har:?}"
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "SIGTERM-based graceful shutdown is Unix-only")]
    fn sigterm_without_har_out_does_not_implicit_write() {
        if skip() {
            return;
        }

        let env = SoloEnv::new();
        let before = har_dir_listing(&env);
        let (sid, profile) = unique_session("act971-noharout");
        let tid = start_named_session_tab_in_env(&env, &sid, &profile, &url_network_xhr(), None);
        wait_requests_done_in_env(&env, &sid, &tid);

        assert_success(&har_start_in_env(&env, &sid, &tid), "har start");
        issue_bulk_requests_in_env(&env, &sid, &tid, 3);

        send_sigterm_and_wait(&env);

        let after = har_dir_listing(&env);
        assert_eq!(
            after, before,
            "SIGTERM without --har-out must not implicitly write under ACTIONBOOK_HOME/har"
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "SIGTERM-based graceful shutdown is Unix-only")]
    fn sigterm_with_multi_recorder_skips_flush_and_warns() {
        if skip() {
            return;
        }

        let env = SoloEnv::new();
        let (sid, profile) = unique_session("act971-multi");
        let har_path = Path::new(&env.actionbook_home).join("act971-multi.har");
        let log_path = daemon_log_path(&env);
        let tid_a = start_named_session_tab_in_env(
            &env,
            &sid,
            &profile,
            &url_network_xhr(),
            Some(&har_path),
        );
        wait_requests_done_in_env(&env, &sid, &tid_a);

        let tid_b = new_tab_json_in_env(&env, &sid, &url_network_xhr());
        wait_requests_done_in_env(&env, &sid, &tid_b);

        assert_success(&har_start_in_env(&env, &sid, &tid_a), "har start tab a");
        assert_success(&har_start_in_env(&env, &sid, &tid_b), "har start tab b");
        issue_bulk_requests_in_env(&env, &sid, &tid_a, 2);
        issue_bulk_requests_in_env(&env, &sid, &tid_b, 2);

        send_sigterm_and_wait(&env);

        assert!(
            !har_path.exists(),
            "multi-recorder SIGTERM flush must skip writing {}",
            har_path.display()
        );
        let daemon_log = fs::read_to_string(&log_path).unwrap_or_default();
        assert!(
            daemon_log.contains("skipping HAR flush"),
            "expected daemon log {} to mention skip, got:\n{}",
            log_path.display(),
            daemon_log
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "SIGTERM-based graceful shutdown is Unix-only")]
    fn har_stop_after_sigterm_flush_stays_parseable() {
        if skip() {
            return;
        }

        let env = SoloEnv::new();
        let (sid, profile) = unique_session("act971-stop-after");
        let har_path = Path::new(&env.actionbook_home).join("act971-stop-after.har");
        let tid = start_named_session_tab_in_env(
            &env,
            &sid,
            &profile,
            &url_network_xhr(),
            Some(&har_path),
        );
        wait_requests_done_in_env(&env, &sid, &tid);

        assert_success(&har_start_in_env(&env, &sid, &tid), "har start");
        issue_bulk_requests_in_env(&env, &sid, &tid, 4);

        send_sigterm_and_wait(&env);

        let before = fs::read(&har_path)
            .unwrap_or_else(|e| panic!("read flushed har {}: {e}", har_path.display()));
        let _: serde_json::Value = serde_json::from_slice(&before)
            .unwrap_or_else(|e| panic!("parse flushed har {}: {e}", har_path.display()));

        let stop_out = har_stop_in_env(&env, &sid, &tid);
        assert_failure(&stop_out, "har stop after sigterm flush");

        let after = fs::read(&har_path)
            .unwrap_or_else(|e| panic!("re-read flushed har {}: {e}", har_path.display()));
        let _: serde_json::Value = serde_json::from_slice(&after)
            .unwrap_or_else(|e| panic!("parse har after failed stop {}: {e}", har_path.display()));
        assert_eq!(
            after, before,
            "failed har stop after SIGTERM flush must not mutate existing flushed HAR"
        );
    }
}
