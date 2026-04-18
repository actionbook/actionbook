//! Shared E2E test harness.
//!
//! Provides environment isolation, CLI invocation helpers, local HTTP server,
//! per-test session isolation, and common assertions.

#[cfg(not(windows))]
use assert_cmd::Command;
use std::env;
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::process::Output;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// If `ACTIONBOOK_E2E_MODE` is set, rewrite every `browser start` in `args`
/// to target that mode instead of whatever the test hardcoded.
///
/// For `extension` target also:
///   - strip `--headless` / `--executable-path` / `--cdp-endpoint` (local-only
///     flags clap would reject on extension or that have no meaning)
///   - if neither `--open-url` nor `--tab-id` is present, inject
///     `--open-url http://127.0.0.1:<local_server_port>/` so the tests can
///     actually land on a page (protocol 0.3.0 requires one or the other)
///
/// Everything else (non-`browser start` commands, arg order, etc.) is left
/// untouched so tests see identical context before and after start.
fn effective_override_mode() -> Option<String> {
    env::var("ACTIONBOOK_E2E_MODE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn remove_flag_with_value(v: &mut Vec<String>, flag: &str) {
    let mut i = 0;
    while i < v.len() {
        if v[i] == flag {
            v.remove(i);
            if i < v.len() {
                v.remove(i);
            }
        } else {
            i += 1;
        }
    }
}

fn rewrite_args_for_mode(args: &[&str]) -> Vec<String> {
    let Some(target_mode) = effective_override_mode() else {
        return args.iter().map(|s| s.to_string()).collect();
    };

    let Some(start_idx) = args
        .windows(2)
        .position(|w| w[0] == "browser" && w[1] == "start")
    else {
        return args.iter().map(|s| s.to_string()).collect();
    };

    let mut out: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    let mut replaced = false;
    let mut i = start_idx + 2;
    while i + 1 < out.len() {
        if out[i] == "--mode" {
            out[i + 1] = target_mode.clone();
            replaced = true;
            break;
        }
        i += 1;
    }
    if !replaced {
        out.insert(start_idx + 2, "--mode".to_string());
        out.insert(start_idx + 3, target_mode.clone());
    }

    if target_mode == "extension" {
        out.retain(|s| s != "--headless");
        remove_flag_with_value(&mut out, "--executable-path");
        remove_flag_with_value(&mut out, "--cdp-endpoint");

        let has_open_url = out.iter().any(|s| s == "--open-url");
        let has_tab_id = out.iter().any(|s| s == "--tab-id");
        if !has_open_url && !has_tab_id {
            let default_url = format!("http://127.0.0.1:{}/", local_server().port);
            out.insert(start_idx + 2, "--open-url".to_string());
            out.insert(start_idx + 3, default_url);
        }
    }

    out
}

/// Run a CLI command with a real timeout, avoiding pipe-inheritance hangs on Windows.
///
/// On Windows, `assert_cmd::Command::output()` pipes stdout/stderr. When the CLI
/// spawns a detached daemon, the daemon inherits those pipe handles via
/// `CreateProcess(bInheritHandles=TRUE)`. Even after the CLI exits, the daemon
/// still holds write handles to the pipes, so the test's `read_to_end()` blocks
/// forever. We avoid this by redirecting to temp files instead of pipes.
fn run_cli_with_timeout(
    actionbook_home: &str,
    args: &[&str],
    extra_env: &[(&str, &str)],
    timeout_secs: u64,
) -> Output {
    let rewritten = rewrite_args_for_mode(args);
    let args: Vec<&str> = rewritten.iter().map(String::as_str).collect();
    let args = args.as_slice();
    #[cfg(windows)]
    {
        let stdout_file = tempfile::NamedTempFile::new().expect("create stdout temp file");
        let stderr_file = tempfile::NamedTempFile::new().expect("create stderr temp file");
        let stdout_path = stdout_file.path().to_path_buf();
        let stderr_path = stderr_file.path().to_path_buf();

        let bin = assert_cmd::cargo::cargo_bin("actionbook");
        let mut cmd = std::process::Command::new(&bin);
        cmd.env("ACTIONBOOK_HOME", actionbook_home)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(stdout_file.reopen().map(std::process::Stdio::from).unwrap())
            .stderr(stderr_file.reopen().map(std::process::Stdio::from).unwrap());
        for (key, value) in extra_env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().expect("failed to spawn command");
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        let status = loop {
            match child.try_wait().expect("try_wait failed") {
                Some(s) => break s,
                None => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        break child.wait().expect("wait after kill");
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };

        let stdout = std::fs::read(&stdout_path).unwrap_or_default();
        let stderr = std::fs::read(&stderr_path).unwrap_or_default();

        Output {
            status,
            stdout,
            stderr,
        }
    }

    #[cfg(not(windows))]
    {
        let mut command = Command::cargo_bin("actionbook").expect("binary exists");
        command
            .env("ACTIONBOOK_HOME", actionbook_home)
            .args(args)
            .timeout(Duration::from_secs(timeout_secs));
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command.output().expect("failed to execute command")
    }
}

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
        run_cli_with_timeout(&self.actionbook_home, args, &[], timeout_secs)
    }

    pub fn headless_json(&self, args: &[&str], timeout_secs: u64) -> Output {
        let mut full_args = vec!["--json"];
        full_args.extend_from_slice(args);
        run_cli_with_timeout(&self.actionbook_home, &full_args, &[], timeout_secs)
    }

    pub fn headless_json_with_env(
        &self,
        args: &[&str],
        extra_env: &[(&str, &str)],
        timeout_secs: u64,
    ) -> Output {
        let mut full_args = vec!["--json"];
        full_args.extend_from_slice(args);
        run_cli_with_timeout(&self.actionbook_home, &full_args, extra_env, timeout_secs)
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
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
                // Wait for the process to actually exit before cleaning up files.
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_secs(3) {
                    let status = std::process::Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .output();
                    if status.is_err() || !status.unwrap().status.success() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_secs(3) {
                    let status = std::process::Command::new("tasklist")
                        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                        .output();
                    match status {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            if !stdout.contains(&pid.to_string()) {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        let profiles_dir = dir.join("profiles");
        if profiles_dir.exists() {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("pkill")
                    .args(["-f", &format!("--user-data-dir={}", profiles_dir.display())])
                    .output();
            }
            #[cfg(windows)]
            {
                // On Windows, use wmic to find and kill Chrome processes
                // with matching user-data-dir argument.
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/IM", "chrome.exe"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
        // Clean up daemon files that SIGKILL leaves behind.
        let _ = std::fs::remove_file(dir.join("daemon.sock"));
        let _ = std::fs::remove_file(dir.join("daemon.ready"));
        let _ = std::fs::remove_file(dir.join("daemon.pid"));
        #[cfg(windows)]
        let _ = std::fs::remove_file(dir.join("daemon.port"));
    }
}

// ── Gate ────────────────────────────────────────────────────────────

/// Test-name substrings that are inherently incompatible with extension-mode
/// rewrite (`ACTIONBOOK_E2E_MODE=extension`):
///   - `cloud_mode::*` — talks to a mock cloud CDP endpoint, not the bridge
///   - `concurrent_two_sessions` / `cross_session` — extension bridge is
///     1 CDP client per daemon, multi-session in one daemon cannot race
///   - `headless` tests — extension uses the host Chrome window, not headless
///   - `windows_daemon::*` — Windows-specific daemon path, unrelated to
///     extension
const EXTENSION_INCOMPATIBLE_SUBSTRINGS: &[&str] = &[
    "cloud_mode::",
    "concurrent_two_sessions",
    "cross_session",
    "windows_daemon::",
    "_headless",
    "lifecycle_open_headless",
];

/// Returns `true` when E2E tests should be skipped.
///
/// Skip conditions:
///   1. `RUN_E2E_TESTS != "true"` — e2e suite gate
///   2. `ACTIONBOOK_E2E_MODE=extension` AND the current test's name matches
///      any substring in `EXTENSION_INCOMPATIBLE_SUBSTRINGS` — these tests
///      cannot pass under extension-bridge semantics and are skipped by
///      design when running the extension-mode regression pass
pub fn skip() -> bool {
    if env::var("RUN_E2E_TESTS")
        .map(|v| v != "true")
        .unwrap_or(true)
    {
        return true;
    }

    if let Ok(m) = env::var("ACTIONBOOK_E2E_MODE")
        && m == "extension"
    {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("");
        if EXTENSION_INCOMPATIBLE_SUBSTRINGS
            .iter()
            .any(|pat| name.contains(pat))
        {
            eprintln!(
                "(skipping {name}: incompatible with extension mode — matches '{}')",
                EXTENSION_INCOMPATIBLE_SUBSTRINGS
                    .iter()
                    .find(|p| name.contains(*p))
                    .copied()
                    .unwrap_or("?")
            );
            return true;
        }
    }

    false
}

// ── Local HTTP server ──────────────────────────────────────────────

/// A lightweight local HTTP server for deterministic navigation tests.
/// Eliminates external network dependency. Spawns a thread per connection
/// to avoid blocking under parallel test load.
pub struct LocalServer {
    pub port: u16,
    _handle: std::thread::JoinHandle<()>,
}

static SERVER: OnceLock<LocalServer> = OnceLock::new();

pub fn local_server() -> &'static LocalServer {
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

    // Echo endpoint: returns the request method, headers, and body as JSON.
    // Used by `browser send` e2e tests to verify that the CLI correctly
    // forwarded method, headers, and body through CDP fetch.
    if path == "/api/echo" {
        let request_str = request.to_string();
        let method = request_str
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().next())
            .unwrap_or("GET")
            .to_string();

        // Parse headers (skip request line, stop at empty line)
        let mut req_headers = std::collections::HashMap::<String, String>::new();
        let mut in_headers = false;
        let mut header_end_idx = request_str.len();
        for (i, line) in request_str.lines().enumerate() {
            if i == 0 {
                in_headers = true;
                continue;
            }
            if line.is_empty() {
                header_end_idx = request_str
                    .find("\r\n\r\n")
                    .map(|p| p + 4)
                    .or_else(|| request_str.find("\n\n").map(|p| p + 2))
                    .unwrap_or(request_str.len());
                break;
            }
            if in_headers && let Some(colon) = line.find(':') {
                let key = line[..colon].trim().to_lowercase();
                let value = line[colon + 1..].trim().to_string();
                req_headers.insert(key, value);
            }
        }

        // Extract body after headers
        let req_body = if header_end_idx < request_str.len() {
            &request_str[header_end_idx..]
        } else {
            ""
        };

        // Build JSON response echoing the request
        let headers_json: Vec<String> = req_headers
            .iter()
            .map(|(k, v)| {
                format!(
                    "\"{}\":\"{}\"",
                    k.replace('\\', "\\\\").replace('"', "\\\""),
                    v.replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .collect();
        let body_escaped = req_body
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        let echo_body = format!(
            r#"{{"method":"{}","headers":{{{}}},"body":"{}"}}"#,
            method,
            headers_json.join(","),
            body_escaped
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: *\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            echo_body.len(),
            echo_body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

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

    if path == "/redirect-delayed-long" {
        let body = format!(
            r#"<!DOCTYPE html><html><head><title>Redirect Delayed Long</title></head>
<body>
<h1>Redirect Delayed Long</h1>
<script>
setTimeout(() => {{
  window.location.href = "http://127.0.0.1:{}/page-b";
}}, 600);
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

    // ── Mock API for search/manual e2e tests ──────────────────────────

    if path.starts_with("/api/search") {
        let query = path
            .split("q=")
            .nth(1)
            .and_then(|v| v.split('&').next())
            .unwrap_or("");

        let body = if query == "notfound" {
            r#"{"success":true,"data":[]}"#.to_string()
        } else {
            r#"{"success":true,"data":[{"name":"example.com","description":"Example site","groups":[{"name":"users","actions":[{"name":"list_users","method":"GET","path":"/api/users","summary":"List all users"},{"name":"create_user","method":"POST","path":"/api/users","summary":"Create a new user"}]}]}]}"#.to_string()
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    if path.starts_with("/api/manual") {
        let get_param = |name: &str| -> Option<String> {
            path.split('?').nth(1).and_then(|qs| {
                qs.split('&')
                    .find(|p| p.starts_with(&format!("{name}=")))
                    .map(|p| p.split('=').nth(1).unwrap_or("").to_string())
            })
        };
        let site = get_param("site");
        let group = get_param("group");
        let action = get_param("action");

        let body = match (site.as_deref(), group.as_deref(), action.as_deref()) {
            (Some("notfound"), _, _) => {
                let err = r#"{"success":false,"error":{"code":"NOT_FOUND","message":"Site \"notfound\" not found.","available":["example.com","github.com"]}}"#;
                let response = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    err.len(),
                    err
                );
                let _ = stream.write_all(response.as_bytes());
                return;
            }
            // L1: site overview
            (Some(_site), None, None) => {
                r#"{"success":true,"data":{"name":"example.com","description":"Example API with base URL `https://api.example.com/v1`","authentication":{"type":"apiKey","name":"Authorization","in":"header"},"groups":[{"name":"users","base_url":null,"actions":[{"name":"list_users","summary":"List all users"},{"name":"create_user","summary":"Create a user"}]},{"name":"posts","base_url":null,"actions":["list_posts","create_post","delete_post"]}]}}"#.to_string()
            }
            // L2: group overview — echo the requested group name
            (Some(_site), Some(group), None) => {
                format!(
                    r#"{{"success":true,"data":{{"group":"{group}","base_url":"https://api.example.com/v1","actions":[{{"name":"list_users","method":"GET","path":"/users","base_url":null,"summary":"List all users"}},{{"name":"create_user","method":"POST","path":"/users","base_url":null,"summary":"Create a new user"}}]}}}}"#
                )
            }
            // L3: action detail — echo the requested group and action names
            (Some(_site), Some(group), Some(action)) => {
                format!(
                    r#"{{"success":true,"data":{{"site":"example.com","group":"{group}","action":"{action}","method":"GET","path":"/users","base_url":"https://api.example.com/v1","description":"List all users with optional filtering","parameters":[{{"name":"page","in":"query","type":"integer","required":false,"description":"Page number"}},{{"name":"limit","in":"query","type":"integer","required":false,"description":"Items per page"}}],"requestBody":null,"responses":[{{"status":"200","description":"Successful response","schema":{{"type":"array","items":{{"type":"object","properties":{{"id":{{"type":"integer"}},"name":{{"type":"string"}}}}}}}}}}],"authentication":{{"type":"apiKey","name":"Authorization","in":"header"}}}}}}"#
                )
            }
            _ => {
                let err = r#"{"success":false,"error":{"code":"BAD_REQUEST","message":"Missing required parameter: site"}}"#;
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    err.len(),
                    err
                );
                let _ = stream.write_all(response.as_bytes());
                return;
            }
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
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

/// Base URL for the mock API server (for search/manual e2e tests).
pub fn api_base_url() -> String {
    format!("http://127.0.0.1:{}", local_server().port)
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

/// URL that redirects to page B after a longer client-side delay.
pub fn url_delayed_redirect_long() -> String {
    format!(
        "http://127.0.0.1:{}/redirect-delayed-long",
        local_server().port
    )
}

/// Root URL without a trailing slash. Browsers normalize this to `/`.
pub fn url_home_no_trailing_slash() -> String {
    format!("http://127.0.0.1:{}", local_server().port)
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
    run_cli_with_timeout(&env.actionbook_home, args, extra_env, timeout_secs)
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
    let mut full_args = vec!["--json"];
    full_args.extend_from_slice(args);
    run_cli_with_timeout(&env.actionbook_home, &full_args, extra_env, timeout_secs)
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
