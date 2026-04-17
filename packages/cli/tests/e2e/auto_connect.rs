//! E2E tests for `browser start --auto-connect`.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

use crate::harness::{
    SoloEnv, assert_error_envelope, assert_failure, assert_success, parse_json, skip, url_a,
};

struct ExternalChromeGuard {
    child: std::process::Child,
    home_root: tempfile::TempDir,
}

impl ExternalChromeGuard {
    fn home_path(&self) -> &Path {
        self.home_root.path()
    }

    fn is_alive(&mut self) -> bool {
        self.child
            .try_wait()
            .expect("try_wait on external chrome")
            .is_none()
    }
}

impl Drop for ExternalChromeGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct SoloSessionGuard<'a> {
    env: &'a SoloEnv,
    session_id: Option<String>,
}

impl<'a> SoloSessionGuard<'a> {
    fn new(env: &'a SoloEnv, session_id: String) -> Self {
        Self {
            env,
            session_id: Some(session_id),
        }
    }
}

impl Drop for SoloSessionGuard<'_> {
    fn drop(&mut self) {
        if let Some(session_id) = self.session_id.take() {
            let _ = self
                .env
                .headless(&["browser", "close", "--session", &session_id], 30);
        }
    }
}

fn default_chrome_user_data_dir(home_root: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_root.join("Library/Application Support/Google/Chrome")
    }

    #[cfg(target_os = "linux")]
    {
        home_root.join(".config/google-chrome")
    }

    #[cfg(windows)]
    {
        home_root.join("Google/Chrome/User Data")
    }
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
            if Path::new(c).exists() {
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
        panic!("Chrome not found for auto-connect tests");
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
            if Path::new(c).exists() {
                return c.to_string();
            }
            if let Ok(output) = StdCommand::new("which").arg(c).output()
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
        }
        panic!("Chrome not found for auto-connect tests");
    }
}

fn launch_external_chrome(port: u16) -> ExternalChromeGuard {
    let chrome = find_chrome_executable();
    let home_root = tempfile::tempdir().expect("create temp home for external chrome");
    let user_data_dir = default_chrome_user_data_dir(home_root.path());
    fs::create_dir_all(&user_data_dir).expect("create default Chrome user-data-dir");

    let mut child = StdCommand::new(&chrome)
        .args([
            "--headless=new",
            &format!("--remote-debugging-port={port}"),
            "--no-first-run",
            "--no-default-browser-check",
            &format!("--user-data-dir={}", user_data_dir.to_string_lossy()),
        ])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("launch Chrome for auto-connect tests");

    wait_for_json_version(port);

    if let Some(status) = child.try_wait().expect("try_wait after launch") {
        panic!("Chrome exited early while starting auto-connect test: {status}");
    }

    ExternalChromeGuard { child, home_root }
}

fn json_version_response(port: u16) -> Option<String> {
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("socket addr");
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(200)).ok()?;
    stream
        .write_all(b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut body = String::new();
    stream.read_to_string(&mut body).ok()?;
    Some(body)
}

fn json_version_is_reachable(port: u16) -> bool {
    json_version_response(port)
        .map(|response| response.contains("200 OK"))
        .unwrap_or(false)
}

fn wait_for_json_version(port: u16) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        if json_version_is_reachable(port) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("Chrome did not expose /json/version on port {port} within 15s");
}

fn auto_connect_env_vars(home_root: &Path) -> [(String, String); 2] {
    let home = home_root.to_string_lossy().to_string();
    let local_appdata = home_root.to_string_lossy().to_string();
    [
        ("HOME".to_string(), home),
        ("LOCALAPPDATA".to_string(), local_appdata),
    ]
}

#[test]
#[cfg_attr(windows, ignore)]
fn auto_connect_attaches_to_running_chrome() {
    if skip() {
        return;
    }
    if json_version_is_reachable(9222) {
        eprintln!("skipping auto-connect attach test because port 9222 is already in use");
        return;
    }

    let mut chrome = launch_external_chrome(9222);
    assert!(chrome.is_alive(), "spawned Chrome should be alive");

    let env = SoloEnv::new();
    let extra_env = auto_connect_env_vars(chrome.home_path());
    let env_refs = [
        ("HOME", extra_env[0].1.as_str()),
        ("LOCALAPPDATA", extra_env[1].1.as_str()),
    ];

    let start = env.headless_json_with_env(&["browser", "start", "--auto-connect"], &env_refs, 30);
    assert_success(&start, "browser start --auto-connect");
    let start_v = parse_json(&start);
    let session_id = start_v["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    let tab_id = start_v["data"]["tab"]["tab_id"]
        .as_str()
        .expect("tab id")
        .to_string();
    let _guard = SoloSessionGuard::new(&env, session_id.clone());

    let goto = env.headless_json_with_env(
        &[
            "browser",
            "goto",
            &url_a(),
            "--session",
            &session_id,
            "--tab",
            &tab_id,
        ],
        &env_refs,
        20,
    );
    assert_success(&goto, "goto after auto-connect");

    let title = env.headless_json_with_env(
        &[
            "browser",
            "title",
            "--session",
            &session_id,
            "--tab",
            &tab_id,
        ],
        &env_refs,
        10,
    );
    assert_success(&title, "title after auto-connect");
    let title_v = parse_json(&title);
    assert_eq!(title_v["data"]["title"], "Page A");
}

#[test]
#[cfg_attr(windows, ignore)]
fn auto_connect_without_running_chrome_returns_not_found() {
    if skip() {
        return;
    }
    if json_version_is_reachable(9222) || json_version_is_reachable(9229) {
        eprintln!("skipping auto-connect negative test because probe ports are already in use");
        return;
    }

    let env = SoloEnv::new();
    let fake_home = tempfile::tempdir().expect("temp home for negative auto-connect test");
    let extra_env = auto_connect_env_vars(fake_home.path());
    let env_refs = [
        ("HOME", extra_env[0].1.as_str()),
        ("LOCALAPPDATA", extra_env[1].1.as_str()),
    ];

    let out = env.headless_json_with_env(&["browser", "start", "--auto-connect"], &env_refs, 10);
    assert_failure(&out, "auto-connect without running Chrome");
    let v = parse_json(&out);
    assert_error_envelope(&v, "CHROME_AUTO_CONNECT_NOT_FOUND");
    assert!(
        v["error"]["hint"]
            .as_str()
            .unwrap_or("")
            .contains("remote debugging"),
        "not-found hint should mention how to start Chrome with remote debugging"
    );
}

#[test]
#[cfg_attr(windows, ignore)]
fn close_attached_auto_connect_session_does_not_kill_chrome() {
    if skip() {
        return;
    }
    if json_version_is_reachable(9222) {
        eprintln!("skipping auto-connect close semantic test because port 9222 is already in use");
        return;
    }

    let mut chrome = launch_external_chrome(9222);
    assert!(
        chrome.is_alive(),
        "spawned Chrome should be alive before attach"
    );

    let env = SoloEnv::new();
    let extra_env = auto_connect_env_vars(chrome.home_path());
    let env_refs = [
        ("HOME", extra_env[0].1.as_str()),
        ("LOCALAPPDATA", extra_env[1].1.as_str()),
    ];

    let start = env.headless_json_with_env(&["browser", "start", "--auto-connect"], &env_refs, 30);
    assert_success(&start, "browser start --auto-connect");
    let start_v = parse_json(&start);
    let session_id = start_v["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();

    let close = env.headless_json_with_env(
        &["browser", "close", "--session", &session_id],
        &env_refs,
        30,
    );
    assert_success(&close, "close attached auto-connect session");

    assert!(
        chrome.is_alive(),
        "closing an attached auto-connect session must not kill external Chrome"
    );
    assert!(
        json_version_is_reachable(9222),
        "external Chrome should still expose /json/version after session close"
    );
}
