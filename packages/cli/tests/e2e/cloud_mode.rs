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
//! Uses shared assertion helpers from harness — no code duplication.

use crate::harness::{
    SessionGuard, SoloEnv, assert_context_with_session, assert_context_with_tab,
    assert_error_envelope, assert_failure, assert_meta, assert_native_tab_id, assert_success,
    assert_tab_id, headless, headless_json, parse_json, skip, stderr_str, stdout_str, url_a, url_b,
    url_c,
};
use std::env;
use std::process::Command as StdCommand;

// ── Helpers ─────────────────────────────────────────────────────────

/// RAII guard for external Chrome process + temp directory cleanup.
struct ChromeGuard {
    child: std::process::Child,
    _tmp: tempfile::TempDir,
}

impl Drop for ChromeGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // _tmp drops here, cleaning up the user-data-dir
    }
}

/// Launch a headless Chrome on a random port for simulating cloud.
/// Returns (guard, ws_url).
fn launch_simulated_cloud() -> (ChromeGuard, String) {
    let chrome = find_chrome_executable();
    let tmp_dir = tempfile::tempdir().expect("create temp dir for cloud chrome");
    let user_data_dir = tmp_dir.path().to_string_lossy().to_string();
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
    (
        ChromeGuard {
            child,
            _tmp: tmp_dir,
        },
        ws_url,
    )
}

fn find_chrome_executable() -> String {
    #[cfg(windows)]
    {
        let program_files =
            std::env::var("PROGRAMFILES").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let program_files_x86 = std::env::var("PROGRAMFILES(X86)")
            .unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_default();

        let candidates = [
            format!("{}\\Google\\Chrome\\Application\\chrome.exe", program_files),
            format!(
                "{}\\Google\\Chrome\\Application\\chrome.exe",
                program_files_x86
            ),
            format!("{}\\Google\\Chrome\\Application\\chrome.exe", local_appdata),
        ];
        for c in &candidates {
            if std::path::Path::new(c).exists() {
                return c.clone();
            }
        }
        if let Ok(output) = StdCommand::new("where").arg("chrome").output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return path;
            }
        }
        panic!("Chrome not found for cloud simulation tests");
    }

    #[cfg(not(windows))]
    {
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
}

/// Check if real cloud tests are enabled. Returns (endpoint, headers).
#[allow(dead_code)]
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

#[allow(dead_code)]
fn skip_cloud_real() -> bool {
    cloud_endpoint().is_none()
}

/// Assert cloud session JSON response structure.
fn assert_cloud_session(v: &serde_json::Value) {
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null(), "error must be null on success");
    assert_eq!(v["command"], "browser start");
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    let session = &v["data"]["session"];
    assert_eq!(session["mode"], "cloud");
    assert_eq!(session["status"], "running");
    assert!(
        session["cdp_endpoint"].is_string(),
        "cloud session must include cdp_endpoint"
    );
    assert!(
        session.get("headers").is_none() || session["headers"].is_null(),
        "headers must not be exposed in session output"
    );
    assert_meta(v);
}

// ── Cloud session helpers ───────────────────────────────────────────

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
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    (sid, tid)
}

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
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    (sid, tid)
}

fn open_cloud_tab_json(sid: &str, url: &str) -> String {
    let out = headless_json(&["browser", "new-tab", url, "--session", sid], 15);
    assert_success(&out, "cloud new-tab");
    let v = parse_json(&out);
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    v["data"]["tab"]["tab_id"].as_str().unwrap().to_string()
}

// ═══════════════════════════════════════════════════════════════════
// Group 1: Cloud session lifecycle
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_start_and_close_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();

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
    let _guard = SessionGuard::new(&sid);
    assert!(!sid.is_empty(), "session_id should not be empty");
    assert!(!tab_id.is_empty(), "should discover at least one tab");
    assert!(
        !v["data"]["reused"].as_bool().unwrap_or(true),
        "first start should not be reused"
    );

    assert_context_with_tab(&v, &sid, &tab_id);

    let out = headless_json(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "cloud close");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["data"]["status"], "closed");
    assert_context_with_session(&v, &sid);
    assert_meta(&v);
}

#[test]
fn cloud_start_and_close_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();

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
    assert!(text.contains("ok browser start"));
    assert!(text.contains("mode: cloud"));

    let first_line = text.lines().next().unwrap_or("");
    let sid = first_line
        .trim_start_matches('[')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let _guard = SessionGuard::new(&sid);
    assert!(!sid.is_empty(), "should extract session_id from text");

    let out = headless(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "cloud close text");
    let text = stdout_str(&out);
    assert!(text.contains("ok browser close"));
}

// ═══════════════════════════════════════════════════════════════════
// Group 2: Cloud session reuse
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_reuse_same_endpoint_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();

    let (sid1, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid1);

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
}

#[test]
fn cloud_reuse_same_endpoint_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();

    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

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
}

#[test]
fn cloud_reuse_with_different_headers_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();

    let (sid1, _) = start_cloud_with_headers_json(&ws_url, &["Authorization:Bearer old-token"]);
    let _guard = SessionGuard::new(&sid1);

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
}

// ═══════════════════════════════════════════════════════════════════
// Group 3: Error cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_missing_cdp_endpoint_json() {
    if skip() {
        return;
    }

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

    let out = headless(&["browser", "start", "--mode", "cloud"], 10);
    assert_failure(&out, "cloud missing endpoint text");
    let text = stderr_str(&out);
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
    let (_chrome, ws_url) = launch_simulated_cloud();

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
}

#[test]
fn cloud_header_with_colon_in_value_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();

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
    let _guard = SessionGuard::new(&sid);
}

// ═══════════════════════════════════════════════════════════════════
// Group 4: Cloud tab management — JSON
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_list_tabs_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "cloud list-tabs");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser list-tabs");
    assert_context_with_session(&v, &sid);
    assert!(
        v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 1,
        "should have >=1 tab"
    );
    let tabs = v["data"]["tabs"].as_array().expect("tabs should be array");
    for tab in tabs {
        assert_tab_id(&tab["tab_id"]);
        assert_native_tab_id(&tab["native_tab_id"]);
        assert_ne!(tab["tab_id"], tab["native_tab_id"]);
    }
    assert_meta(&v);
}

#[test]
fn cloud_list_tabs_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "cloud list-tabs text");
    let text = stdout_str(&out);
    assert!(text.contains("tab"), "text should contain tab info: {text}");
}

#[test]
fn cloud_new_tab_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let out = headless_json(&["browser", "new-tab", &url, "--session", &sid], 15);
    assert_success(&out, "cloud new-tab");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser new-tab");
    assert_eq!(v["data"]["created"], true);
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    assert_ne!(
        v["data"]["tab"]["tab_id"],
        v["data"]["tab"]["native_tab_id"]
    );
    assert_context_with_session(&v, &sid);
    assert_tab_id(&v["context"]["tab_id"]);
    assert_meta(&v);

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    let v = parse_json(&out);
    assert!(
        v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 2,
        "should have >=2 tabs"
    );
}

#[test]
fn cloud_new_tab_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let out = headless(&["browser", "new-tab", &url, "--session", &sid], 15);
    assert_success(&out, "cloud new-tab text");
    let text = stdout_str(&out);
    assert!(!text.is_empty(), "new-tab text should not be empty: {text}");
}

#[test]
fn cloud_new_tab_sequential_ids_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, t1) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let t2 = open_cloud_tab_json(&sid, &url_a());
    let t3 = open_cloud_tab_json(&sid, &url_b());

    assert_eq!(t1, "t1");
    assert_eq!(t2, "t2");
    assert_eq!(t3, "t3");
}

#[test]
fn cloud_close_tab_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _t1) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);
    let url = url_a();
    let t2 = open_cloud_tab_json(&sid, &url);

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "cloud close-tab");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser close-tab");
    assert_eq!(v["data"]["closed_tab_id"], t2);
    assert_context_with_tab(&v, &sid, &t2);
    assert_meta(&v);

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
}

#[test]
fn cloud_close_tab_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);
    let url = url_a();
    let t2 = open_cloud_tab_json(&sid, &url);

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
}

// ═══════════════════════════════════════════════════════════════════
// Group 5: Cloud tab error cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_list_tabs_nonexistent_session_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "list-tabs", "--session", "ghost-cloud"], 10);
    assert_failure(&out, "list-tabs nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser list-tabs");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
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

    let out = headless(&["browser", "list-tabs", "--session", "ghost-cloud"], 10);
    assert_failure(&out, "list-tabs nonexistent session text");
    let text = stderr_str(&out);
    assert!(text.contains("SESSION_NOT_FOUND"), "text: {text}");
}

#[test]
fn cloud_new_tab_nonexistent_session_json() {
    if skip() {
        return;
    }

    let url = url_a();
    let out = headless_json(
        &["browser", "new-tab", &url, "--session", "ghost-cloud"],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser new-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn cloud_new_tab_nonexistent_session_text() {
    if skip() {
        return;
    }

    let url = url_a();
    let out = headless(
        &["browser", "new-tab", &url, "--session", "ghost-cloud"],
        10,
    );
    assert_failure(&out, "new-tab nonexistent session text");
    let text = stderr_str(&out);
    assert!(text.contains("SESSION_NOT_FOUND"), "text: {text}");
}

// ═══════════════════════════════════════════════════════════════════
// Group 5b: Cloud close-tab error cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_close_tab_nonexistent_session_json() {
    if skip() {
        return;
    }

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
    assert_eq!(v["command"], "browser close-tab");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn cloud_close_tab_nonexistent_session_text() {
    if skip() {
        return;
    }

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
    let text = stderr_str(&out);
    assert!(text.contains("SESSION_NOT_FOUND"));
}

#[test]
fn cloud_close_tab_nonexistent_tab_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

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
    assert_eq!(v["command"], "browser close-tab");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert_context_with_session(&v, &sid);
}

#[test]
fn cloud_close_tab_nonexistent_tab_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

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
    let text = stderr_str(&out);
    assert!(text.contains("TAB_NOT_FOUND"));
}

#[test]
fn cloud_close_tab_double_close_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);
    let url = url_a();
    let t2 = open_cloud_tab_json(&sid, &url);

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "first close");

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        10,
    );
    assert_failure(&out, "double close");
    let v = parse_json(&out);
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}

// ═══════════════════════════════════════════════════════════════════
// Group 6: Cloud page operations (goto, snapshot, eval)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_goto_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let out = headless_json(
        &["browser", "goto", &url, "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "cloud goto");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser goto");
    assert_context_with_tab(&v, &sid, &tid);
    assert_meta(&v);
}

#[test]
fn cloud_goto_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let out = headless(
        &["browser", "goto", &url, "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "cloud goto text");
    let text = stdout_str(&out);
    assert!(!text.is_empty(), "goto text should not be empty: {text}");
}

#[test]
fn cloud_snapshot_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let _ = headless(
        &["browser", "goto", &url, "--session", &sid, "--tab", &tid],
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
    assert_eq!(v["command"], "browser snapshot");
    assert!(v["data"].is_object(), "snapshot data should be an object");
    assert_context_with_tab(&v, &sid, &tid);
    assert_meta(&v);
}

#[test]
fn cloud_snapshot_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let _ = headless(
        &["browser", "goto", &url, "--session", &sid, "--tab", &tid],
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
}

#[test]
fn cloud_eval_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "eval", "2 + 2", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "cloud eval");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser eval");
    assert_eq!(
        v["data"]["value"],
        serde_json::json!(4),
        "eval 2+2 should return typed 4"
    );
    assert_context_with_tab(&v, &sid, &tid);
    assert_meta(&v);
}

#[test]
fn cloud_eval_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, tid) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &["browser", "eval", "2 + 2", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "cloud eval text");
    let text = stdout_str(&out);
    assert!(!text.is_empty(), "eval text should not be empty: {text}");
}

// ═══════════════════════════════════════════════════════════════════
// Group 7: Cloud with multiple headers
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_start_with_multiple_headers_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_with_headers_json(
        &ws_url,
        &[
            "Authorization:Bearer test-token-123",
            "X-Api-Key:my-api-key",
        ],
    );
    let _guard = SessionGuard::new(&sid);

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
}

// ═══════════════════════════════════════════════════════════════════
// Group 8: Cloud session status and list-sessions
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_status_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "cloud status");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["command"], "browser status");
    assert_eq!(v["data"]["session"]["mode"], "cloud");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["cdp_endpoint"].is_string());
    assert!(v["data"]["tabs"].is_array());
    assert_context_with_session(&v, &sid);
    assert_meta(&v);
}

#[test]
fn cloud_status_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "status", "--session", &sid], 10);
    assert_success(&out, "cloud status text");
    let text = stdout_str(&out);
    assert!(text.contains("mode: cloud"), "should show mode: cloud");
}

#[test]
fn cloud_list_sessions_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert!(v["context"].is_null());
    let sessions = v["data"]["sessions"].as_array().expect("sessions array");
    let our_session = sessions
        .iter()
        .find(|s| s["session_id"].as_str() == Some(sid.as_str()))
        .expect("our cloud session should appear in list");
    assert_eq!(our_session["mode"], "cloud");
    assert!(our_session["cdp_endpoint"].is_string());
    assert!(
        our_session.get("headers").is_none() || our_session["headers"].is_null(),
        "list-sessions must not expose headers"
    );
    assert_meta(&v);
}

#[test]
fn cloud_list_sessions_text() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions text");
    let text = stdout_str(&out);
    assert!(text.contains(&sid), "should list cloud session id");
}

// ═══════════════════════════════════════════════════════════════════
// Group 9: Cloud close semantics — browser stays alive
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_close_does_not_kill_browser() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, &format!("close {sid}"));

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
    assert_success(&out, "reconnect after close");
    let v = parse_json(&out);
    assert_cloud_session(&v);
    let sid2 = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let _guard = SessionGuard::new(&sid2);
}

// ═══════════════════════════════════════════════════════════════════
// Group 10: Connection drop (SoloEnv — kills external Chrome)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_connection_drop_returns_error() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let out = env.headless_json(
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
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let _ = chrome.child.kill();
    let _ = chrome.child.wait();
    std::thread::sleep(std::time::Duration::from_secs(1));

    let out = env.headless_json(
        &["browser", "eval", "1+1", "--session", &sid, "--tab", &tid],
        10,
    );
    let stdout = stdout_str(&out);
    if stdout.is_empty() {
        assert!(
            !out.status.success(),
            "command after disconnect should fail"
        );
    } else {
        let v = parse_json(&out);
        assert_eq!(v["ok"], false);
        let code = v["error"]["code"].as_str().unwrap_or("");
        assert!(
            code == "CLOUD_CONNECTION_LOST" || code == "CDP_ERROR" || code == "EVAL_FAILED",
            "expected cloud connection error, got: {code}"
        );
    }

    let _ = env.headless(&["browser", "close", "--session", &sid], 5);
}

// ═══════════════════════════════════════════════════════════════════
// Group 11: Cloud restart preserves endpoint/headers
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_restart_preserves_endpoint_and_headers_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_with_headers_json(
        &ws_url,
        &["Authorization:Bearer my-token", "X-Custom:value"],
    );
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "restart", "--session", &sid], 30);
    assert_success(&out, "cloud restart");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["session"]["mode"], "cloud");
    assert_eq!(v["data"]["session"]["status"], "running");
    assert!(v["data"]["session"]["cdp_endpoint"].is_string());
    assert_eq!(v["data"]["reopened"], true);
    assert!(
        v["data"]["session"].get("headers").is_none() || v["data"]["session"]["headers"].is_null(),
    );
    assert_meta(&v);

    let tid = v["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if !tid.is_empty() {
        let out = headless_json(
            &["browser", "eval", "1+1", "--session", &sid, "--tab", &tid],
            10,
        );
        assert_success(&out, "eval after restart");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Group 12a: Daemon crash recovery — reconnect (SoloEnv)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_crash_recovery_reconnect() {
    if skip() {
        return;
    }
    // SoloEnv because we kill the daemon to simulate crash.
    let env = SoloEnv::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let out = env.headless_json(
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

    // Kill Chrome, then launch a fresh one on a new port.
    let _ = chrome.child.kill();
    let _ = chrome.child.wait();
    let (_chrome2, ws_url2) = launch_simulated_cloud();

    // Start a new session to the fresh endpoint — should work cleanly
    // after the daemon auto-restarts.
    let out = env.headless_json(
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
    let _ = env.headless(&["browser", "close", "--session", &sid2], 10);
}

// ═══════════════════════════════════════════════════════════════════
// Group 12b: Daemon crash recovery — endpoint gone (SoloEnv)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_crash_recovery_endpoint_gone() {
    if skip() {
        return;
    }
    let env = SoloEnv::new();
    let (mut chrome, ws_url) = launch_simulated_cloud();

    let out = env.headless_json(
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

    let _ = chrome.child.kill();
    let _ = chrome.child.wait();
    std::thread::sleep(std::time::Duration::from_secs(1));

    let out = env.headless_json(
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
    // After killing Chrome, the CLI must either:
    // (a) Return a connection error (CDP_CONNECTION_FAILED / CLOUD_CONNECTION_LOST), or
    // (b) Exit non-zero (daemon itself may crash with no JSON output).
    // It must NOT succeed with ok=true and a new functional session.
    let stdout = stdout_str(&out);
    if stdout.is_empty() {
        // Daemon crashed — non-zero exit with no output is acceptable.
        assert!(
            !out.status.success(),
            "empty stdout must come with non-zero exit"
        );
    } else {
        let v = parse_json(&out);
        if v["ok"].as_bool() == Some(true) {
            // Reused existing session — verify it's actually dead by trying eval.
            let sid = v["data"]["session"]["session_id"].as_str().unwrap_or("");
            let tid = v["data"]["tab"]["tab_id"].as_str().unwrap_or("");
            if !sid.is_empty() && !tid.is_empty() {
                let eval_out = env.headless_json(
                    &["browser", "eval", "1+1", "--session", sid, "--tab", tid],
                    10,
                );
                // The eval should fail since Chrome is dead.
                assert!(
                    !eval_out.status.success(),
                    "eval on dead cloud session should fail"
                );
            }
        } else {
            // JSON error — verify it's a connection error.
            let code = v["error"]["code"].as_str().unwrap_or("");
            assert!(
                code == "CDP_CONNECTION_FAILED" || code == "CLOUD_CONNECTION_LOST",
                "expected connection error, got: {code}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Group 13: Concurrent cloud operations
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_concurrent_eval_multi_tab() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, t1) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);
    let url = url_a();
    let t2 = open_cloud_tab_json(&sid, &url);

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
    // 1+1=2 and 2+2=4 return as numbers, not strings
    assert_eq!(va["data"]["value"], 2, "tab1 eval 1+1");
    assert_eq!(vb["data"]["value"], 4, "tab2 eval 2+2");
}

#[test]
fn cloud_concurrent_tab_create() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);
    let _guard = SessionGuard::new(&sid);

    let urls = [url_a(), url_b(), url_c()];
    let handles: Vec<_> = urls
        .iter()
        .map(|url| {
            let sid = sid.clone();
            let url = url.clone();
            std::thread::spawn(move || {
                headless_json(&["browser", "new-tab", &url, "--session", &sid], 15)
            })
        })
        .collect();

    for h in handles {
        let out = h.join().unwrap();
        assert_success(&out, "concurrent new-tab");
    }

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    let v = parse_json(&out);
    assert!(v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 4);
}

// ═══════════════════════════════════════════════════════════════════
// Group 14: Cloud double-close session
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cloud_double_close_session_json() {
    if skip() {
        return;
    }
    let (_chrome, ws_url) = launch_simulated_cloud();
    let (sid, _) = start_cloud_json(&ws_url);

    let out = headless_json(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "first close");

    let out = headless_json(&["browser", "close", "--session", &sid], 10);
    assert_success(&out, "double close idempotent");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], "closed");
    assert_eq!(v["data"]["closed_tabs"], 0);
    let warnings = v["meta"]["warnings"]
        .as_array()
        .expect("meta warnings array should exist");
    assert!(
        warnings.iter().any(|w| {
            w.as_str()
                .map(|s| s.contains("not found") || s.contains("already closed"))
                .unwrap_or(false)
        }),
        "expected idempotent close warning, got {warnings:?}"
    );
    assert!(v["error"].is_null());
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
    let _guard = SessionGuard::new(&sid);
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

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
}

#[test]
fn cloud_real_goto_and_snapshot() {
    if skip() || skip_cloud_real() {
        return;
    }

    let url = url_a();
    let (endpoint, headers) = cloud_endpoint().unwrap();
    let mut args = vec![
        "browser".to_string(),
        "start".to_string(),
        "--mode".to_string(),
        "cloud".to_string(),
        "--cdp-endpoint".to_string(),
        endpoint.clone(),
        "--open-url".to_string(),
        url,
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
    let _guard = SessionGuard::new(&sid);
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    std::thread::sleep(std::time::Duration::from_secs(2));

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "real cloud snapshot");
}

#[test]
fn cloud_real_tab_management() {
    if skip() || skip_cloud_real() {
        return;
    }

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
    let _guard = SessionGuard::new(&sid);

    let url = url_a();
    let out = headless_json(&["browser", "new-tab", &url, "--session", &sid], 15);
    assert_success(&out, "real cloud new-tab");
    let v = parse_json(&out);
    let t2 = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();

    let out = headless_json(&["browser", "list-tabs", "--session", &sid], 10);
    assert_success(&out, "real cloud list-tabs");
    let v = parse_json(&out);
    assert!(v["data"]["total_tabs"].as_u64().unwrap_or(0) >= 2);

    let out = headless_json(
        &["browser", "close-tab", "--session", &sid, "--tab", &t2],
        15,
    );
    assert_success(&out, "real cloud close-tab");
}
