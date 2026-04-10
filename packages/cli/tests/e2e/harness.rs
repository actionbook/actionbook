//! Shared E2E test harness.
//!
//! Provides environment isolation, CLI invocation helpers, local HTTP server,
//! per-test session isolation, and common assertions.

use assert_cmd::Command;
use std::env;
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::process::Output;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ── Isolated environment ────────────────────────────────────────────

/// Shared isolated environment — used by most tests via the global daemon.
pub struct IsolatedEnv {
    _tmp: tempfile::TempDir,
    pub actionbook_home: String,
}

static ENV: OnceLock<IsolatedEnv> = OnceLock::new();

pub fn shared_env() -> &'static IsolatedEnv {
    ENV.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let actionbook_home = tmp.path().join("actionbook-home");
        fs::create_dir_all(&actionbook_home).unwrap();

        IsolatedEnv {
            actionbook_home: actionbook_home.to_string_lossy().to_string(),
            _tmp: tmp,
        }
    })
}

// ── Per-test isolated environment (SoloEnv) ────────────────────────

/// Fully isolated environment for tests that need a clean slate (e.g., default
/// session ID tests, config bootstrap tests). Each SoloEnv gets its own
/// ACTIONBOOK_HOME and daemon instance, so it never interferes with parallel tests.
pub struct SoloEnv {
    _tmp: tempfile::TempDir,
    pub actionbook_home: String,
}

impl SoloEnv {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create solo temp dir");
        let home = tmp.path().join("actionbook-home");
        fs::create_dir_all(&home).unwrap();
        SoloEnv {
            actionbook_home: home.to_string_lossy().to_string(),
            _tmp: tmp,
        }
    }

    pub fn headless(&self, args: &[&str], timeout_secs: u64) -> Output {
        let mut command = Command::cargo_bin("actionbook").expect("binary exists");
        command
            .env("ACTIONBOOK_HOME", &self.actionbook_home)
            .args(args)
            .timeout(Duration::from_secs(timeout_secs));
        command.output().expect("failed to execute command")
    }

    pub fn headless_json(&self, args: &[&str], timeout_secs: u64) -> Output {
        let mut command = Command::cargo_bin("actionbook").expect("binary exists");
        command
            .env("ACTIONBOOK_HOME", &self.actionbook_home)
            .arg("--json")
            .args(args)
            .timeout(Duration::from_secs(timeout_secs));
        command.output().expect("failed to execute command")
    }

    pub fn headless_json_with_env(
        &self,
        args: &[&str],
        extra_env: &[(&str, &str)],
        timeout_secs: u64,
    ) -> Output {
        let mut command = Command::cargo_bin("actionbook").expect("binary exists");
        command
            .env("ACTIONBOOK_HOME", &self.actionbook_home)
            .arg("--json")
            .args(args)
            .timeout(Duration::from_secs(timeout_secs));
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command.output().expect("failed to execute command")
    }

    pub fn config_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.actionbook_home).join("config.toml")
    }
}

impl Drop for SoloEnv {
    fn drop(&mut self) {
        // Kill this env's daemon and Chrome processes, wait for exit,
        // then clean up socket/ready/pid files (SIGKILL prevents daemon's
        // own cleanup path from running).
        let dir = std::path::Path::new(&self.actionbook_home);
        let pid_path = dir.join("daemon.pid");
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path)
            && let Ok(pid) = pid_str.trim().parse::<u32>()
        {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
            // Wait for the process to actually exit before cleaning up files.
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                // kill -0 checks if process exists without sending a signal.
                let status = std::process::Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .output();
                if status.is_err() || !status.unwrap().status.success() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let profiles_dir = dir.join("profiles");
        if profiles_dir.exists() {
            let _ = std::process::Command::new("pkill")
                .args(["-f", &format!("--user-data-dir={}", profiles_dir.display())])
                .output();
        }
        // Clean up daemon files that SIGKILL leaves behind.
        let _ = std::fs::remove_file(dir.join("daemon.sock"));
        let _ = std::fs::remove_file(dir.join("daemon.ready"));
        let _ = std::fs::remove_file(dir.join("daemon.pid"));
    }
}

// ── Gate ────────────────────────────────────────────────────────────

/// Returns `true` when E2E tests should be skipped.
pub fn skip() -> bool {
    env::var("RUN_E2E_TESTS")
        .map(|v| v != "true")
        .unwrap_or(true)
}

// ── Local HTTP server ──────────────────────────────────────────────

/// A lightweight local HTTP server for deterministic navigation tests.
/// Eliminates external network dependency. Spawns a thread per connection
/// to avoid blocking under parallel test load.
struct LocalServer {
    port: u16,
    _handle: std::thread::JoinHandle<()>,
}

static SERVER: OnceLock<LocalServer> = OnceLock::new();

fn local_server() -> &'static LocalServer {
    SERVER.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local server");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // Spawn a thread per connection to avoid blocking under parallel load.
                std::thread::spawn(move || handle_http(stream));
            }
        });
        LocalServer {
            port,
            _handle: handle,
        }
    })
}

fn handle_http(mut stream: std::net::TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = [0u8; 2048];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buf[..n]);

    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let title = match path {
        "/" => "Home",
        "/page-a" => "Page A",
        "/page-b" => "Page B",
        "/page-c" => "Page C",
        "/slow" => {
            std::thread::sleep(Duration::from_millis(250));
            "Slow Page"
        }
        other => other.trim_start_matches('/'),
    };

    if path == "/redirect-fast" {
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/page-b\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
            local_server().port
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path == "/redirect-delayed" {
        let body = format!(
            r#"<!DOCTYPE html><html><head><title>Redirect Delayed</title></head>
<body>
<h1>Redirect Delayed</h1>
<script>
setTimeout(() => {{
  window.location.href = "http://127.0.0.1:{}/page-b";
}}, 150);
</script>
</body></html>"#,
            local_server().port
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path.starts_with("/api/data") {
        let source = path
            .split("source=")
            .nth(1)
            .and_then(|value| value.split('&').next())
            .unwrap_or("default");
        let body = format!(r#"{{"ok":true,"source":"{source}"}}"#);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Ab-Fixture: api-data\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path == "/network-fixture.css" {
        let body = "body { background: rgb(245, 248, 255); }";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path == "/network-fixture.js" {
        let body = "window.__ab_network_script_loaded = true;";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path == "/network-load" {
        let port = local_server().port;
        let body = format!(
            r#"<!DOCTYPE html><html><head><title>Network Load Fixture</title>
<link rel="stylesheet" href="http://127.0.0.1:{port}/network-fixture.css">
<script src="http://127.0.0.1:{port}/network-fixture.js" defer></script>
</head>
<body>
<h1>Network Load Fixture</h1>
<p id="network-load-status">ready</p>
</body></html>"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path == "/network-xhr" {
        let port = local_server().port;
        let body = format!(
            r#"<!DOCTYPE html><html><head><title>Network XHR Fixture</title></head>
<body>
<h1>Network XHR Fixture</h1>
<script>
window.__ab_requests_done = false;
window.__ab_requests_error = null;
Promise.all([
  fetch("http://127.0.0.1:{port}/api/data?source=fetch").then(r => r.json()),
  new Promise((resolve, reject) => {{
    const xhr = new XMLHttpRequest();
    xhr.open("GET", "http://127.0.0.1:{port}/api/data?source=xhr");
    xhr.onload = () => resolve(xhr.responseText);
    xhr.onerror = () => reject(new Error("xhr failed"));
    xhr.send();
  }})
]).then(() => {{
  window.__ab_requests_done = true;
}}).catch(err => {{
  window.__ab_requests_error = String(err);
  window.__ab_requests_done = true;
}});
</script>
</body></html>"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    // Cross-origin iframe parent: embeds child from a different port
    if path.starts_with("/iframe-xo-parent") {
        let xo_port = path
            .split("xo_port=")
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        let body = format!(
            r#"<!DOCTYPE html><html><head><title>XO Iframe Parent</title></head>
<body>
<h1>Main XO Page</h1>
<input id="main-xo-input" value="main-xo-value" aria-label="Main XO Input">
<iframe src="http://127.0.0.1:{xo_port}/iframe-child" title="XO Child Frame" width="400" height="300"></iframe>
<p>XO Footer</p>
</body></html>"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    // iframe test pages: parent embeds child via same-origin iframe
    if path == "/iframe-parent" {
        let port = local_server().port;
        let body = format!(
            r#"<!DOCTYPE html><html><head><title>Iframe Parent</title></head>
<body>
<h1>Main Page</h1>
<input id="main-input" value="main-value" aria-label="Main Input">
<iframe src="http://127.0.0.1:{port}/iframe-child" title="Child Frame" width="400" height="300"></iframe>
<p>Footer</p>
</body></html>"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }
    if path == "/iframe-child" {
        let body = r#"<!DOCTYPE html><html><head><title>Iframe Child</title></head>
<body>
<h2>Child Content</h2>
<input id="child-input" value="child-value" aria-label="Child Input">
<button id="child-btn" aria-label="Child Button">Click Me</button>
</body></html>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    // Deterministic fixture for cursor-interactive detection tests.
    // Contains a cursor:pointer div, an onclick div, and a tabindex div —
    // all of which should be captured only when cursor detection is active.
    if path == "/cursor-fixture" {
        let body = r#"<!DOCTYPE html><html><head><title>Cursor Fixture</title></head>
<body>
<h1>Cursor Fixture Page</h1>
<div id="cursor-div" style="cursor:pointer">cursor-pointer-item</div>
<div id="onclick-div" onclick="void(0)">onclick-item</div>
<div id="tabindex-div" tabindex="0">tabindex-item</div>
<p>plain-text-paragraph</p>
</body></html>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    let body = format!(
        "<!DOCTYPE html><html><head><title>{title}</title></head>\
         <body><h1>{title}</h1></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

/// URL for page A (primary test page).
pub fn url_a() -> String {
    format!("http://127.0.0.1:{}/page-a", local_server().port)
}

/// URL for page B (secondary test page).
pub fn url_b() -> String {
    format!("http://127.0.0.1:{}/page-b", local_server().port)
}

/// URL for page C (tertiary test page).
pub fn url_c() -> String {
    format!("http://127.0.0.1:{}/page-c", local_server().port)
}

/// URL that immediately redirects to page B via HTTP 302.
pub fn url_fast_redirect() -> String {
    format!("http://127.0.0.1:{}/redirect-fast", local_server().port)
}

/// URL that redirects to page B after a short client-side delay.
pub fn url_delayed_redirect() -> String {
    format!("http://127.0.0.1:{}/redirect-delayed", local_server().port)
}

/// URL for a slow page used to verify CLI-level timeouts.
pub fn url_slow() -> String {
    format!("http://127.0.0.1:{}/slow", local_server().port)
}

/// URL for an iframe test page (parent with embedded same-origin child iframe).
pub fn url_iframe_parent() -> String {
    format!("http://127.0.0.1:{}/iframe-parent", local_server().port)
}

/// URL for cursor-interactive fixture (cursor:pointer, onclick, tabindex elements).
pub fn url_cursor_fixture() -> String {
    format!("http://127.0.0.1:{}/cursor-fixture", local_server().port)
}

/// URL for a page that loads a document, stylesheet, and script.
pub fn url_network_load() -> String {
    format!("http://127.0.0.1:{}/network-load", local_server().port)
}

/// URL for a page that performs fetch + XHR requests and marks completion.
pub fn url_network_xhr() -> String {
    format!("http://127.0.0.1:{}/network-xhr", local_server().port)
}

// ── Cross-origin server (second port for OOPIF tests) ─────────────

static CROSS_ORIGIN_SERVER: OnceLock<LocalServer> = OnceLock::new();

fn cross_origin_server() -> &'static LocalServer {
    CROSS_ORIGIN_SERVER.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind cross-origin server");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || handle_http_cross_origin(stream));
            }
        });
        LocalServer {
            port,
            _handle: handle,
        }
    })
}

fn handle_http_cross_origin(mut stream: std::net::TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = [0u8; 2048];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let body = match path {
        "/iframe-child" => r#"<!DOCTYPE html><html><head><title>Cross-Origin Child</title></head>
<body>
<h2>Cross-Origin Content</h2>
<input id="xo-input" value="xo-value" aria-label="XO Input">
<button id="xo-btn" aria-label="XO Button">XO Click</button>
</body></html>"#
            .to_string(),
        _ => "<!DOCTYPE html><html><head><title>XO</title></head><body><h1>XO</h1></body></html>"
            .to_string(),
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

/// URL for an iframe test page with cross-origin child iframe.
/// Parent is on local_server port, child iframe is on cross_origin_server port.
pub fn url_iframe_cross_origin_parent() -> String {
    // Ensure cross-origin server is initialized
    let xo_port = cross_origin_server().port;
    format!(
        "http://127.0.0.1:{}/iframe-xo-parent?xo_port={}",
        local_server().port,
        xo_port
    )
}

// ── Per-test session isolation ─────────────────────────────────────

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Generate a unique (session_id, profile) pair for a test.
/// Includes PID to prevent collisions with leftover daemons from prior runs.
pub fn unique_session(prefix: &str) -> (String, String) {
    let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    (
        format!("{prefix}-{pid}-{id}"),
        format!("profile-{prefix}-{pid}-{id}"),
    )
}

// ── CLI runners ─────────────────────────────────────────────────────

/// Run `actionbook <args>` with the shared isolated environment.
pub fn headless(args: &[&str], timeout_secs: u64) -> Output {
    headless_with_env(args, &[], timeout_secs)
}

pub fn headless_with_env(args: &[&str], extra_env: &[(&str, &str)], timeout_secs: u64) -> Output {
    let env = shared_env();
    let mut command = Command::cargo_bin("actionbook").expect("binary exists");
    command
        .env("ACTIONBOOK_HOME", &env.actionbook_home)
        .args(args)
        .timeout(Duration::from_secs(timeout_secs));
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("failed to execute command")
}

/// Run `actionbook --json <args>` with the shared isolated environment.
pub fn headless_json(args: &[&str], timeout_secs: u64) -> Output {
    headless_json_with_env(args, &[], timeout_secs)
}

pub fn headless_json_with_env(
    args: &[&str],
    extra_env: &[(&str, &str)],
    timeout_secs: u64,
) -> Output {
    let env = shared_env();
    let mut command = Command::cargo_bin("actionbook").expect("binary exists");
    command
        .env("ACTIONBOOK_HOME", &env.actionbook_home)
        .arg("--json")
        .args(args)
        .timeout(Duration::from_secs(timeout_secs));
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("failed to execute command")
}

// ── Cleanup helpers ─────────────────────────────────────────────────

/// RAII guard that ensures a single session is cleaned up even when a test panics.
/// Calls `browser close` which kills Chrome and removes the profile directory.
pub struct SessionGuard {
    session_id: Option<String>,
}

impl SessionGuard {
    /// Create a guard that will close the given session on drop.
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: Some(session_id.to_string()),
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(ref sid) = self.session_id {
            let _ = headless(&["browser", "close", "--session", sid], 10);
        }
    }
}

// ── Assertions ──────────────────────────────────────────────────────

pub fn stdout_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn stderr_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

pub fn assert_success(output: &Output, ctx: &str) {
    assert!(
        output.status.success(),
        "[{ctx}] expected exit 0, got {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        stdout_str(output),
        stderr_str(output),
    );
}

pub fn assert_failure(output: &Output, ctx: &str) {
    assert!(
        !output.status.success(),
        "[{ctx}] expected non-zero exit, got 0\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout_str(output),
        stderr_str(output),
    );
}

/// Parse JSON envelope from command stdout.
pub fn parse_json(out: &Output) -> serde_json::Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nraw stdout: {text}");
    })
}

// ── Common JSON structure assertions ───────────────────────────────

/// Assert full meta structure per api-reference.md section 2.4.
pub fn assert_meta(v: &serde_json::Value) {
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

/// Assert full error envelope per api-reference.md section 3.1.
pub fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
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

/// Assert a tab_id uses the short `tN` format.
pub fn assert_tab_id(tab_id: &serde_json::Value) {
    let tab_id = tab_id.as_str().expect("tab_id must be a string");
    let suffix = tab_id
        .strip_prefix('t')
        .expect("tab_id must start with 't'");
    assert!(
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()),
        "tab_id must match short format tN, got {tab_id}"
    );
}

/// Assert a native_tab_id is exposed as a non-empty string.
pub fn assert_native_tab_id(native_tab_id: &serde_json::Value) {
    let native_tab_id = native_tab_id
        .as_str()
        .expect("native_tab_id must be a string");
    assert!(!native_tab_id.is_empty(), "native_tab_id must not be empty");
}

/// Assert context is a non-null object.
pub fn assert_context_object(v: &serde_json::Value) {
    assert!(v["context"].is_object(), "context must be an object");
}

/// Assert context includes session_id.
pub fn assert_context_with_session(v: &serde_json::Value, expected_sid: &str) {
    assert_context_object(v);
    assert_eq!(
        v["context"]["session_id"].as_str().unwrap_or(""),
        expected_sid,
        "context.session_id mismatch"
    );
}

/// Assert context includes both session_id and tab_id.
pub fn assert_context_with_tab(v: &serde_json::Value, expected_sid: &str, expected_tid: &str) {
    assert_context_with_session(v, expected_sid);
    assert_tab_id(&v["context"]["tab_id"]);
    assert_eq!(
        v["context"]["tab_id"].as_str().unwrap_or(""),
        expected_tid,
        "context.tab_id mismatch"
    );
}

// ── Common session helpers ──────────────────────────────────────────

/// Start a headless session with a unique session ID and profile, return (session_id, tab_id).
pub fn start_session(url: &str) -> (String, String) {
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
            url,
        ],
        30,
    );
    assert_success(&out, &format!("start session {sid}"));
    let v = parse_json(&out);
    let actual_sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    wait_page_ready(&actual_sid, &tid);
    (actual_sid, tid)
}

/// Start a headless session with explicit session ID and profile, return tab_id.
pub fn start_named_session(session_id: &str, profile: &str, url: &str) -> String {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--profile",
            profile,
            "--set-session-id",
            session_id,
            "--open-url",
            url,
        ],
        30,
    );
    assert_success(&out, &format!("start {session_id}"));
    let v = parse_json(&out);
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    wait_page_ready(session_id, &tid);
    tid
}

/// Poll `document.readyState === 'complete'` every 200ms, up to 2s.
/// Prevents flaky failures under parallel load where Chrome hasn't finished
/// rendering when the test starts interacting with the page.
pub fn wait_page_ready(session_id: &str, tab_id: &str) {
    for _ in 0..10 {
        let out = headless_json(
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
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Poll `browser url` until it contains the expected substring, up to 5s.
/// Prevents flaky failures when `--open-url` navigation hasn't reflected yet.
#[allow(dead_code)]
pub fn wait_url_contains(session_id: &str, tab_id: &str, expected: &str) {
    for _ in 0..25 {
        let out = headless_json(
            &["browser", "url", "--session", session_id, "--tab", tab_id],
            5,
        );
        if out.status.success() {
            let v = parse_json(&out);
            if let Some(url) = v["data"]["url"].as_str()
                && url.contains(expected)
            {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Close a session (asserts success).
#[allow(dead_code)]
pub fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

/// Open a new tab via JSON, return tab_id.
pub fn new_tab_json(session_id: &str, url: &str) -> String {
    let out = headless_json(&["browser", "new-tab", url, "--session", session_id], 30);
    assert_success(&out, "new-tab");
    let v = parse_json(&out);
    assert_tab_id(&v["data"]["tab"]["tab_id"]);
    assert_native_tab_id(&v["data"]["tab"]["native_tab_id"]);
    v["data"]["tab"]["tab_id"].as_str().unwrap().to_string()
}
