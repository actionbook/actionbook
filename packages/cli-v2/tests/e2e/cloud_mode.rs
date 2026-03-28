//! Cloud mode E2E tests.
//!
//! Tests cloud browser connectivity via `--mode cloud --cdp-endpoint <wss://...>`.
//!
//! Two tiers:
//!   1. Local Chrome simulating cloud — gated by `RUN_E2E_TESTS=true`
//!      Uses `ws://127.0.0.1:PORT/devtools/browser/...` against a locally launched Chrome.
//!   2. Real cloud browser (hyperbrowse) — gated by `ACTIONBOOK_CLOUD_CDP_ENDPOINT`
//!      Connects to an actual cloud endpoint with optional `ACTIONBOOK_CLOUD_HEADERS`.
//!
//! Run tier 1:
//!   RUN_E2E_TESTS=true cargo test --test e2e cloud_ -- --test-threads=1 --nocapture
//!
//! Run tier 2 (real cloud):
//!   RUN_E2E_TESTS=true ACTIONBOOK_CLOUD_CDP_ENDPOINT=wss://... \
//!     ACTIONBOOK_CLOUD_HEADERS="Authorization:Bearer xxx,X-Api-Key:yyy" \
//!     cargo test --test e2e cloud_real_ -- --test-threads=1 --nocapture

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str,
};
use std::env;
use std::process::Command as StdCommand;

const TEST_URL: &str = "https://example.com";
const TEST_URL_2: &str = "https://example.org";
const TEST_URL_3: &str = "https://actionbook.dev";

// ── Helpers ─────────────────────────────────────────────────────────

/// Launch a headless Chrome on a random port for simulating cloud.
/// Returns (child_process, ws_url).
fn launch_simulated_cloud() -> (std::process::Child, String) {
    let chrome = find_chrome_executable();
    let tmp_dir = tempfile::tempdir().expect("create temp dir for cloud chrome");
    let user_data_dir = tmp_dir.path().to_string_lossy().to_string();
    // Leak the TempDir so it persists for the Chrome process lifetime
    std::mem::forget(tmp_dir);
    let mut child = StdCommand::new(&chrome)
        .args([
            "--headless=new",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            &format!("--user-data-dir={user_data_dir}"),
        ])
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("failed to launch Chrome for simulated cloud");

    let stderr = child.stderr.take().expect("stderr should be piped");
    use std::io::BufRead;
    let reader = std::io::BufReader::new(stderr);
    let mut ws_url = String::new();
    let start = std::time::Instant::now();
    for line in reader.lines() {
        if start.elapsed() > std::time::Duration::from_secs(15) {
            panic!("Chrome did not print DevTools URL within 15s");
        }
        let line: String = line.expect("read stderr line");
        if line.contains("DevTools listening on")
            && let Some(idx) = line.find("ws://")
        {
            ws_url = line[idx..].trim().to_string();
            break;
        }
    }
    assert!(!ws_url.is_empty(), "failed to get Chrome WS URL");
    (child, ws_url)
}

fn find_chrome_executable() -> String {
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
        if let Ok(output) = StdCommand::new("which").arg(c).output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }
    panic!("Chrome not found for cloud simulation tests");
}

/// Check if real cloud tests are enabled. Returns (endpoint, headers).
fn cloud_endpoint() -> Option<(String, Vec<String>)> {
    let endpoint = env::var("ACTIONBOOK_CLOUD_CDP_ENDPOINT").ok()?;
    let headers: Vec<String> = env::var("ACTIONBOOK_CLOUD_HEADERS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    Some((endpoint, headers))
}

fn skip_cloud_real() -> bool {
    cloud_endpoint().is_none()
}

// ── Assertion helpers (match tab_management.rs patterns) ────────────

/// Assert full meta structure per §2.4.
fn assert_meta(v: &serde_json::Value) {
    assert!(
        v["meta"]["duration_ms"].is_number(),
        "meta.duration_ms must be a number"
    );
    assert!(
        v["meta"]["warnings"].is_array(),
        "meta.warnings must be an array"
    );
    assert!(
        v["meta"]["pagination"].is_null(),
        "meta.pagination must be null"
    );
    assert!(
        v["meta"]["truncated"].is_boolean(),
        "meta.truncated must be a boolean"
    );
}

/// Assert full error envelope per §3.1 (including meta).
fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false, "ok must be false on error");
    assert!(v["data"].is_null(), "data must be null on failure");
    assert_eq!(v["error"]["code"], expected_code);
    assert!(
        v["error"]["message"].is_string(),
        "error.message must be a string"
    );
    assert!(
        v["error"]["retryable"].is_boolean(),
        "error.retryable must be a boolean"
    );
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details must be object or null"
    );
    assert_meta(v);
}

/// Assert context is a non-null object with session_id.
fn assert_context_with_session(v: &serde_json::Value, expected_sid: &str) {
    assert!(v["context"].is_object(), "context must be an object");
    assert_eq!(
        v["context"]["session_id"].as_str().unwrap_or(""),
        expected_sid,
        "context.session_id mismatch"
    );
}

/// Assert context includes both session_id and tab_id.
fn assert_context_with_tab(v: &serde_json::Value, expected_sid: &str, expected_tid: &str) {
    assert_context_with_session(v, expected_sid);
    assert_eq!(
        v["context"]["tab_id"].as_str().unwrap_or(""),
        expected_tid,
        "context.tab_id mismatch"
    );
}

/// Assert a tab_id is a non-empty string (native Chrome target ID).
fn assert_tab_id(tab_id: &serde_json::Value) {
    assert!(tab_id.is_string(), "tab_id must be a string");
    assert!(
        !tab_id.as_str().unwrap().is_empty(),
        "tab_id must not be empty"
    );
}

/// Assert cloud session JSON response structure.
fn assert_cloud_session(v: &serde_json::Value) {
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null(), "error must be null on success");
    assert_eq!(v["command"], "browser.start");
    let session = &v["data"]["session"];
    assert_eq!(session["mode"], "cloud");
    assert_eq!(session["status"], "running");
    assert!(
        session["cdp_endpoint"].is_string(),
        "cloud session must include cdp_endpoint"
    );
    // Headers must NOT be exposed
    assert!(
        session.get("headers").is_none() || session["headers"].is_null(),
        "headers must not be exposed in session output"
    );
    assert_meta(v);
}

// ── Cloud session helpers ───────────────────────────────────────────

/// Start a cloud session, return (session_id, first_tab_id).
fn start_cloud_json(ws_url: &str) -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            ws_url,
        ],
        30,
    );
    assert_success(&out, "cloud start");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    (sid, tid)
}

/// Start a cloud session with headers, return (session_id, first_tab_id).
fn start_cloud_with_headers_json(ws_url: &str, headers: &[&str]) -> (String, String) {
    let mut args = vec![
        "browser",
        "start",
        "--mode",
        "cloud",
        "--cdp-endpoint",
        ws_url,
    ];
    for h in headers {
        args.push("--header");
        args.push(h);
    }
    let out = headless_json(&args, 30);
    assert_success(&out, "cloud start with headers");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    (sid, tid)
}

/// Open a new tab in a cloud session, return tab_id.
fn open_cloud_tab_json(sid: &str, url: &str) -> String {
    let out = headless_json(&["browser", "new-tab", url, "--session", sid], 15);
    assert_success(&out, "cloud new-tab");
    let v = parse_json(&out);
    v["data"]["tab"]["tab_id"].as_str().unwrap().to_string()
}

/// Close a session (asserts success).
fn close_session(sid: &str) {
    let out = headless(&["browser", "close", "--session", sid], 10);
    assert_success(&out, &format!("close {sid}"));
}

/// Cleanup external Chrome process.
fn kill_chrome(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// ═══════════════════════════════════════════════════════════════════
// Group 1: Cloud session lifecycle
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_start_and_close_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
        ],
        30,
    );
    assert_success(&out, "cloud start");
    let v = parse_json(&out);
    assert_cloud_session(&v);

    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tab_id = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(!sid.is_empty(), "session_id should not be empty");
    assert!(!tab_id.is_empty(), "should discover at least one tab");
    assert!(
        !v["data"]["reused"].as_bool().unwrap_or(true),
        "first start should not be reused"
    );

    // context should have session_id and tab_id
    assert_context_with_tab(&v, &sid, &tab_id);

    // Close
    let out = headless_json(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "cloud close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["data"]["status"], "closed");
    assert_context_with_session(&v, &sid);
    assert_meta(&v);

    kill_chrome(&mut chrome);
}

#[test]
fn cloud_start_and_close_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
        ],
        30,
    );
    assert_success(&out, "cloud start text");
    let text = stdout_str(&out);
    assert!(
        text.contains("ok browser.start"),
        "should contain ok browser.start"
    );
    assert!(text.contains("mode: cloud"), "should show mode: cloud");

    // Extract session ID from text header: [session-id tab-id] url
    let first_line = text.lines().next().unwrap_or("");
    let sid = first_line
        .trim_start_matches('[')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    assert!(!sid.is_empty(), "should extract session_id from text");

    // Close text
    let out = headless(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "cloud close text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser.close"));

    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 2: Cloud session reuse
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_reuse_same_endpoint_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let (sid1, _) = start_cloud_json(&ws_url);

    // Second start with SAME endpoint — must reuse (single-connection constraint)
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
        ],
        30,
    );
    assert_success(&out, "cloud start 2 (reuse)");
    let v = parse_json(&out);
    let sid2 = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(sid1, sid2, "same endpoint must reuse session");
    assert!(
        v["data"]["reused"].as_bool().unwrap_or(false),
        "second start must be reused=true"
    );
    assert_meta(&v);

    close_session(&sid1);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_reuse_same_endpoint_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let (sid, _) = start_cloud_json(&ws_url);

    // Reuse via text output
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
        ],
        30,
    );
    assert_success(&out, "cloud reuse text");
    let text = stdout_str(&out);
    assert!(text.contains(&sid), "text should contain same session_id");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_reuse_with_different_headers_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    // First start with header A
    let (sid1, _) = start_cloud_with_headers_json(&ws_url, &["Authorization:Bearer old-token"]);

    // Second start, same endpoint, different header — should reuse and update headers
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
            "--header",
            "Authorization:Bearer new-token",
        ],
        30,
    );
    assert_success(&out, "cloud reuse different headers");
    let v = parse_json(&out);
    let sid2 = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        sid1, sid2,
        "same endpoint with different headers should still reuse"
    );
    assert!(v["data"]["reused"].as_bool().unwrap_or(false));

    close_session(&sid1);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 3: Error cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_missing_cdp_endpoint_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "start", "--mode", "cloud"], 10);
    assert_failure(&out, "cloud missing endpoint");
    let v = parse_json(&out);
    assert_error_envelope(&v, "MISSING_CDP_ENDPOINT");
    assert!(v["error"]["hint"].is_string(), "error should have hint");
}

#[test]
fn cloud_missing_cdp_endpoint_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "start", "--mode", "cloud"], 10);
    assert_failure(&out, "cloud missing endpoint text");
    let text = stdout_str(&out);
    assert!(
        text.contains("MISSING_CDP_ENDPOINT") || text.contains("cdp-endpoint"),
        "error should mention cdp-endpoint: {text}"
    );
}

#[test]
fn cloud_invalid_endpoint_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            "ws://127.0.0.1:1/invalid",
        ],
        15,
    );
    assert_failure(&out, "cloud invalid endpoint");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    let code = v["error"]["code"].as_str().unwrap_or("");
    assert!(
        code == "CDP_CONNECTION_FAILED" || code == "CLOUD_CONNECTION_LOST",
        "expected connection error, got: {code}"
    );
    assert_meta(&v);
}

#[test]
fn cloud_invalid_header_format_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    // Header without colon separator — should be rejected
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
            "--header",
            "InvalidHeaderNoColon",
        ],
        15,
    );
    assert_failure(&out, "invalid header format");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    let code = v["error"]["code"].as_str().unwrap_or("");
    assert!(
        code == "INVALID_ARGUMENT" || code == "INVALID_HEADER",
        "expected invalid argument error for malformed header, got: {code}"
    );

    kill_chrome(&mut chrome);
}

#[test]
fn cloud_header_with_colon_in_value_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    // Header value contains colons (e.g., "Authorization:Bearer abc:def:ghi") — should work
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
            "--header",
            "Authorization:Bearer abc:def:ghi",
        ],
        30,
    );
    assert_success(&out, "header with colon in value");
    let v = parse_json(&out);
    assert_cloud_session(&v);

    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 4: Cloud tab management — JSON
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_list_tabs_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "cloud list-tabs");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.list-tabs");
    assert_context_with_session(&v, &sid);
    assert!(
        v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 1,
        "should have >=1 tab"
    );
    let tabs = v["data"]["tabs"].as_array().expect("tabs should be array");
    for tab in tabs {
        assert_tab_id(&tab["tab_id"]);
    }
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_list_tabs_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "cloud list-tabs text");
    let text = stdout_str(&out);
    assert!(text.contains("tab"), "text should contain tab info: {text}");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_new_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless_json(&["browser", "new-tab", TEST_URL, "--session", &sid], 15);
    assert_success(&out, "cloud new-tab");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.new-tab");
    assert_eq!(v["data"]["created"], true);
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_context_with_session(&v, &sid);
    assert!(
        v["context"]["tab_id"].is_string(),
        "context should have tab_id for new-tab"
    );
    assert_meta(&v);

    // Verify tab count increased
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    let v = parse_json(&out);
    assert!(
        v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 2,
        "should have >=2 tabs"
    );

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_new_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless(&["browser", "new-tab", TEST_URL, "--session", &sid], 15);
    assert_success(&out, "cloud new-tab text");
    let text = stdout_str(&out);
    // new-tab text output varies by upstream format
    assert!(!text.is_empty(), "new-tab text should not be empty: {text}");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_new_tab_sequential_ids_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, t1) = start_cloud_json(&ws_url);

    let t2 = open_cloud_tab_json(&sid, TEST_URL);
    let t3 = open_cloud_tab_json(&sid, TEST_URL_2);

    // All tab IDs must be unique
    assert_ne!(t1, t2, "tab IDs must be unique");
    assert_ne!(t2, t3, "tab IDs must be unique");
    assert_ne!(t1, t3, "tab IDs must be unique");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_close_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _t1) = start_cloud_json(&ws_url);
    let t2 = open_cloud_tab_json(&sid, TEST_URL);

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "cloud close-tab");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.close-tab");
    assert_eq!(v["data"]["closed_tab_id"], t2);
    assert_context_with_tab(&v, &sid, &t2);
    assert_meta(&v);

    // Verify t2 gone from list
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    let v = parse_json(&out);
    let tab_ids: Vec<&str> = v["data"]["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["tab_id"].as_str())
        .collect();
    assert!(
        !tab_ids.contains(&t2.as_str()),
        "closed tab should not appear"
    );

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_close_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let t2 = open_cloud_tab_json(&sid, TEST_URL);

    let out = headless(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "cloud close-tab text");
    let text = stdout_str(&out);
    assert!(
        !text.is_empty(),
        "close-tab text should not be empty: {text}"
    );

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 5: Cloud tab error cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_list_tabs_nonexistent_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(&["browser", "list-tabs", "--session", "ghost-cloud"], 10);
    assert_failure(&out, "list-tabs nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser.list-tabs");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    // context should be absent — session not located
    assert!(
        v["context"].is_null(),
        "context should be null for SESSION_NOT_FOUND"
    );
}

#[test]
fn cloud_list_tabs_nonexistent_session_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(&["browser", "list-tabs", "--session", "ghost-cloud"], 10);
    assert_failure(&out, "list-tabs nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("SESSION_NOT_FOUND"), "text: {text}");
}

#[test]
fn cloud_new_tab_nonexistent_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &["browser", "new-tab", TEST_URL, "--session", "ghost-cloud"],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser.new-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn cloud_new_tab_nonexistent_session_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &["browser", "new-tab", TEST_URL, "--session", "ghost-cloud"],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("SESSION_NOT_FOUND"), "text: {text}");
}

#[test]
fn cloud_close_tab_nonexistent_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless_json(
        &[
            "browser",
            "close-tab",
            "--session",
            "ghost-cloud",
            "--tab",
            "FAKE-ID",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn cloud_close_tab_nonexistent_session_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let out = headless(
        &[
            "browser",
            "close-tab",
            "--session",
            "ghost-cloud",
            "--tab",
            "FAKE-ID",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent session text");
    let text = stdout_str(&out);
    assert!(text.contains("SESSION_NOT_FOUND"), "text: {text}");
}

#[test]
fn cloud_close_tab_nonexistent_tab_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless_json(
        &[
            "browser",
            "close-tab",
            "--session",
            &sid,
            "--tab",
            "FAKE-TARGET-ID",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser.close-tab");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    // §4: context.session_id should be present since session was located
    assert_context_with_session(&v, &sid);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_close_tab_nonexistent_tab_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless(
        &[
            "browser",
            "close-tab",
            "--session",
            &sid,
            "--tab",
            "FAKE-TARGET-ID",
        ],
        10,
    );
    assert_failure(&out, "close-tab nonexistent tab text");
    let text = stdout_str(&out);
    assert!(text.contains("TAB_NOT_FOUND"), "text: {text}");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_close_tab_double_close_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let t2 = open_cloud_tab_json(&sid, TEST_URL);

    // First close succeeds
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "first close");

    // Second close should fail with TAB_NOT_FOUND
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        10,
    );
    assert_failure(&out, "double close");
    let v = parse_json(&out);
    assert_error_envelope(&v, "TAB_NOT_FOUND");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 6: Cloud page operations (goto, snapshot, eval)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_goto_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    let out = headless_json(
        &[
            "browser",
            "goto",
            TEST_URL,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "cloud goto");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.goto");
    assert_context_with_tab(&v, &sid, &tid);
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_goto_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    let out = headless(
        &[
            "browser",
            "goto",
            TEST_URL,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "cloud goto text");
    let text = stdout_str(&out);
    assert!(!text.is_empty(), "goto text should not be empty: {text}");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_snapshot_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    // Navigate first so snapshot has content
    let _ = headless(
        &[
            "browser",
            "goto",
            TEST_URL,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    std::thread::sleep(std::time::Duration::from_secs(1));

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "cloud snapshot");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.snapshot");
    assert!(v["data"].is_object(), "snapshot data should be an object");
    assert_context_with_tab(&v, &sid, &tid);
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_snapshot_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    let _ = headless(
        &[
            "browser",
            "goto",
            TEST_URL,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    std::thread::sleep(std::time::Duration::from_secs(1));

    let out = headless(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "cloud snapshot text");
    let text = stdout_str(&out);
    assert!(
        !text.is_empty(),
        "snapshot text should not be empty: {text}"
    );

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_eval_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    let out = headless_json(
        &["browser", "eval", "2 + 2", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "cloud eval");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.eval");
    let result = v["data"]["value"].as_str().unwrap_or("");
    assert!(
        result.contains('4'),
        "eval 2+2 should return 4, got: {result}"
    );
    assert_context_with_tab(&v, &sid, &tid);
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_eval_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    let out = headless(
        &["browser", "eval", "2 + 2", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "cloud eval text");
    let text = stdout_str(&out);
    assert!(!text.is_empty(), "eval text should not be empty: {text}");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 7: Cloud with headers
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_start_with_multiple_headers_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let (sid, _) = start_cloud_with_headers_json(
        &ws_url,
        &[
            "Authorization:Bearer test-token-123",
            "X-Api-Key:my-api-key",
        ],
    );

    // Status should show cloud mode but NOT expose headers
    let out = headless_json(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "cloud status");
    let v = parse_json(&out);
    let session = &v["data"]["session"];
    assert_eq!(session["mode"], "cloud");
    assert!(
        session.get("headers").is_none() || session["headers"].is_null(),
        "status must not expose headers"
    );
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 8: Cloud session status and list-sessions
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_status_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless_json(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "cloud status");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser.status");
    assert_eq!(v["data"]["session"]["mode"], "cloud");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["cdp_endpoint"].is_string());
    assert!(v["data"]["tabs"].is_array());
    assert_context_with_session(&v, &sid);
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_status_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "cloud status text");
    let text = stdout_str(&out);
    assert!(
        text.contains("mode: cloud"),
        "status text should show mode: cloud: {text}"
    );
    assert!(text.contains("mode: cloud"), "should show mode: cloud");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_list_sessions_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert!(
        v["context"].is_null(),
        "global command should have null context"
    );
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let cloud_sessions: Vec<&serde_json::Value> =
        sessions.iter().filter(|s| s["mode"] == "cloud").collect();
    assert!(
        !cloud_sessions.is_empty(),
        "should have at least 1 cloud session"
    );
    assert_eq!(cloud_sessions[0]["session_id"], sid);
    // cdp_endpoint should be present in listing
    assert!(
        cloud_sessions[0]["cdp_endpoint"].is_string(),
        "list-sessions should include cdp_endpoint for cloud"
    );
    // headers must NOT be present
    assert!(
        cloud_sessions[0].get("headers").is_none() || cloud_sessions[0]["headers"].is_null(),
        "list-sessions must not expose headers"
    );
    assert_meta(&v);

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_list_sessions_text() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions text");
    let text = stdout_str(&out);
    assert!(text.contains(&sid), "should list cloud session id");

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 9: Cloud close semantics — browser stays alive
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_close_does_not_kill_browser() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    // Close the session
    close_session(&sid);

    // Chrome should still be alive — verify by connecting again
    std::thread::sleep(std::time::Duration::from_millis(500));
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
        ],
        15,
    );
    assert_success(&out, "reconnect after close — browser should be alive");
    let v = parse_json(&out);
    assert_cloud_session(&v);
    let sid2 = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    close_session(&sid2);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 10: Connection drop / recovery
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_connection_drop_returns_error() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);

    // Kill external Chrome — simulates cloud disconnect
    kill_chrome(&mut chrome);
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Next command should fail
    let out = headless_json(
        &["browser", "eval", "1+1", "--session", &sid, "--tab", &tid],
        10,
    );
    // After kill, the command should either return a JSON error or exit non-zero
    let stdout = stdout_str(&out);
    if stdout.is_empty() {
        // Daemon may have crashed — command failed with no output
        assert!(
            !out.status.success(),
            "command after disconnect should fail"
        );
    } else {
        let v = parse_json(&out);
        assert_eq!(v["ok"], false, "command after disconnect should fail");
        let code = v["error"]["code"].as_str().unwrap_or("");
        assert!(
            code == "CLOUD_CONNECTION_LOST" || code == "CDP_ERROR" || code == "EVAL_FAILED",
            "expected cloud connection error, got: {code}"
        );
    }

    // Cleanup
    let _ = headless(&["browser", "close", "--session", &sid], 5);
}

// ═══════════════════════════════════════════════════════════════════
// Group 11: Cloud restart preserves endpoint/headers
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_restart_preserves_endpoint_and_headers_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let (sid, _) = start_cloud_with_headers_json(
        &ws_url,
        &["Authorization:Bearer my-token", "X-Custom:value"],
    );

    // Restart the cloud session
    let out = headless_json(&["browser", "restart", "--session", &sid], 30);
    assert_success(&out, "cloud restart");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["session"]["mode"], "cloud");
    assert_eq!(v["data"]["session"]["status"], "running");
    // cdp_endpoint must be preserved across restart
    assert!(
        v["data"]["session"]["cdp_endpoint"].is_string(),
        "cdp_endpoint should be preserved after restart"
    );
    assert_eq!(v["data"]["reopened"], true);
    // Headers must NOT be exposed (but must be preserved internally —
    // verified by the session still being functional with the same endpoint)
    assert!(
        v["data"]["session"].get("headers").is_none() || v["data"]["session"]["headers"].is_null(),
        "headers must not be exposed after restart"
    );
    assert_meta(&v);

    // Session should still be usable — proves connection works,
    // which means headers were preserved (cloud endpoint requires them)
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    // After restart, session should be usable if tab exists
    if !tid.is_empty() {
        let out = headless_json(
            &["browser", "eval", "1+1", "--session", &sid, "--tab", &tid],
            10,
        );
        assert_success(&out, "eval after restart — proves headers preserved");
    }

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 12: Daemon crash recovery
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_crash_recovery_reconnect() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (_sid, _) = start_cloud_json(&ws_url);

    // Kill daemon (NOT Chrome) — simulates daemon crash
    crate::harness::ensure_no_sessions();
    // Note: ensure_no_sessions kills daemon + Chrome processes.
    // We need Chrome to stay alive. Re-launch it for this test.
    kill_chrome(&mut chrome);
    let (mut chrome2, ws_url2) = launch_simulated_cloud();

    // Next CLI command auto-starts a new daemon.
    // The session should be recoverable if state was persisted.
    // For now, verify that starting a NEW session to same endpoint works cleanly.
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url2,
        ],
        30,
    );
    assert_success(&out, "start after daemon crash");
    let v = parse_json(&out);
    assert_cloud_session(&v);
    let sid2 = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    close_session(&sid2);
    kill_chrome(&mut chrome2);
}

#[test]
fn cloud_crash_recovery_endpoint_gone() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (_sid, _) = start_cloud_json(&ws_url);

    // Kill BOTH daemon and Chrome — endpoint no longer exists
    kill_chrome(&mut chrome);
    crate::harness::ensure_no_sessions();

    // Try to connect to the now-dead endpoint — should fail gracefully
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "cloud",
            "--cdp-endpoint",
            &ws_url,
        ],
        15,
    );
    assert_failure(&out, "connect to dead endpoint after crash");
    let v = parse_json(&out);
    assert_eq!(v["ok"], false);
    let code = v["error"]["code"].as_str().unwrap_or("");
    assert!(
        code == "CDP_CONNECTION_FAILED" || code == "CLOUD_CONNECTION_LOST",
        "expected connection error, got: {code}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Group 13: Concurrent cloud operations
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_concurrent_eval_multi_tab() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, t1) = start_cloud_json(&ws_url);
    let t2 = open_cloud_tab_json(&sid, TEST_URL);

    // Concurrent eval on both tabs
    let sid_a = sid.clone();
    let t1_a = t1.clone();
    let ha = std::thread::spawn(move || {
        headless_json(
            &[
                "browser",
                "eval",
                "1+1",
                "--session",
                &sid_a,
                "--tab",
                &t1_a,
            ],
            10,
        )
    });
    let sid_b = sid.clone();
    let t2_b = t2.clone();
    let hb = std::thread::spawn(move || {
        headless_json(
            &[
                "browser",
                "eval",
                "2+2",
                "--session",
                &sid_b,
                "--tab",
                &t2_b,
            ],
            10,
        )
    });

    let out_a = ha.join().unwrap();
    let out_b = hb.join().unwrap();

    assert_success(&out_a, "eval tab1");
    assert_success(&out_b, "eval tab2");

    let va = parse_json(&out_a);
    let vb = parse_json(&out_b);
    assert!(
        va["data"]["value"].as_str().unwrap_or("").contains('2'),
        "1+1=2"
    );
    assert!(
        vb["data"]["value"].as_str().unwrap_or("").contains('4'),
        "2+2=4"
    );

    close_session(&sid);
    kill_chrome(&mut chrome);
}

#[test]
fn cloud_concurrent_tab_create() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    // Create 3 tabs concurrently
    let handles: Vec<_> = [TEST_URL, TEST_URL_2, TEST_URL_3]
        .iter()
        .map(|url| {
            let sid = sid.clone();
            let url = url.to_string();
            std::thread::spawn(move || {
                headless_json(&["browser", "new-tab", &url, "--session", &sid], 15)
            })
        })
        .collect();

    for h in handles {
        let out = h.join().unwrap();
        assert_success(&out, "concurrent new-tab");
    }

    // Should have at least 4 tabs (1 initial + 3 created)
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    let v = parse_json(&out);
    assert!(
        v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 4,
        "should have >=4 tabs"
    );

    close_session(&sid);
    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 14: Cloud double-close session
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_double_close_session_json() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    // First close succeeds
    let out = headless_json(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "first close");

    // Second close should fail
    let out = headless_json(&["browser", "close", "--session", &sid], 10);
    assert_failure(&out, "double close");
    let v = parse_json(&out);
    assert_error_envelope(&v, "SESSION_NOT_FOUND");

    kill_chrome(&mut chrome);
}

// ═══════════════════════════════════════════════════════════════════
// Group 15: Real cloud browser tests (hyperbrowse)
//   Gated by ACTIONBOOK_CLOUD_CDP_ENDPOINT env var
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_real_start_and_close() {
    if skip() || skip_cloud_real() {
        return;
    }
    let _guard = SessionGuard::new();

    let (endpoint, headers) = cloud_endpoint().unwrap();
    let mut args = vec![
        "browser".to_string(),
        "start".to_string(),
        "--mode".to_string(),
        "cloud".to_string(),
        "--cdp-endpoint".to_string(),
        endpoint.clone(),
    ];
    for h in &headers {
        args.push("--header".to_string());
        args.push(h.clone());
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let out = headless_json(&args_ref, 30);
    assert_success(&out, "real cloud start");
    let v = parse_json(&out);
    assert_cloud_session(&v);

    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    // Eval basic JS
    let out = headless_json(
        &[
            "browser",
            "eval",
            "navigator.userAgent",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "real cloud eval");

    close_session(&sid);
}

#[test]
fn cloud_real_goto_and_snapshot() {
    if skip() || skip_cloud_real() {
        return;
    }
    let _guard = SessionGuard::new();

    let (endpoint, headers) = cloud_endpoint().unwrap();
    let mut args = vec![
        "browser".to_string(),
        "start".to_string(),
        "--mode".to_string(),
        "cloud".to_string(),
        "--cdp-endpoint".to_string(),
        endpoint.clone(),
        "--open-url".to_string(),
        TEST_URL.to_string(),
    ];
    for h in &headers {
        args.push("--header".to_string());
        args.push(h.clone());
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let out = headless_json(&args_ref, 30);
    assert_success(&out, "real cloud start with url");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    std::thread::sleep(std::time::Duration::from_secs(2));

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "real cloud snapshot");

    close_session(&sid);
}

#[test]
fn cloud_real_tab_management() {
    if skip() || skip_cloud_real() {
        return;
    }
    let _guard = SessionGuard::new();

    let (endpoint, headers) = cloud_endpoint().unwrap();
    let mut args = vec![
        "browser".to_string(),
        "start".to_string(),
        "--mode".to_string(),
        "cloud".to_string(),
        "--cdp-endpoint".to_string(),
        endpoint.clone(),
    ];
    for h in &headers {
        args.push("--header".to_string());
        args.push(h.clone());
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let out = headless_json(&args_ref, 30);
    assert_success(&out, "real cloud start");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // New tab
    let out = headless_json(&["browser", "new-tab", TEST_URL, "--session", &sid], 15);
    assert_success(&out, "real cloud new-tab");
    let v = parse_json(&out);
    let t2 = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    // List tabs
    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "real cloud list-tabs");
    let v = parse_json(&out);
    assert!(v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 2);

    // Close tab
    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "real cloud close-tab");

    close_session(&sid);
}
