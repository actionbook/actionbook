//! Shared E2E test harness.
//!
//! Provides environment isolation, CLI invocation helpers, and common assertions.

use assert_cmd::Command;
use std::env;
use std::fs;
use std::process::Output;
use std::sync::OnceLock;
use std::time::Duration;

// ── Isolated environment ────────────────────────────────────────────

/// Isolated XDG environment so tests never touch real config.
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

// ── Gate ────────────────────────────────────────────────────────────

/// Returns `true` when E2E tests should be skipped.
pub fn skip() -> bool {
    env::var("RUN_E2E_TESTS")
        .map(|v| v != "true")
        .unwrap_or(true)
}

// ── CLI runners ─────────────────────────────────────────────────────

/// Run `actionbook <args>` with the isolated environment.
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

/// Run `actionbook --json <args>` with the isolated environment.
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

/// RAII guard that ensures sessions are cleaned up even when a test panics.
#[allow(dead_code)]
pub struct SessionGuard;

#[allow(dead_code)]
impl SessionGuard {
    pub fn new() -> Self {
        ensure_no_sessions();
        Self
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        ensure_no_sessions();
    }
}

/// Close all active sessions so the next test starts clean.
pub fn ensure_no_sessions() {
    // Kill the daemon entirely between tests for clean state.
    // The daemon auto-starts on next CLI command.
    kill_daemon();
    reset_home();
    std::thread::sleep(Duration::from_millis(500));
}

/// Kill the daemon process and clean up socket/PID files.
fn kill_daemon() {
    let env = shared_env();
    let dir = std::path::Path::new(&env.actionbook_home);

    // Read PID and kill
    let pid_path = dir.join("daemon.pid");
    if let Ok(pid_str) = std::fs::read_to_string(&pid_path)
        && let Ok(pid) = pid_str.trim().parse::<u32>()
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }

    // Clean up files
    let _ = std::fs::remove_file(dir.join("daemon.sock"));
    let _ = std::fs::remove_file(dir.join("daemon.ready"));
    let _ = std::fs::remove_file(dir.join("daemon.pid"));

    // Also kill any Chrome processes spawned in this data dir
    let profiles_dir = dir.join("profiles");
    if profiles_dir.exists() {
        // Kill chrome processes using this user-data-dir
        let _ = std::process::Command::new("pkill")
            .args(["-f", &format!("--user-data-dir={}", profiles_dir.display())])
            .output();
    }
}

fn reset_home() {
    let env = shared_env();
    let dir = std::path::Path::new(&env.actionbook_home);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    let _ = std::fs::create_dir_all(dir);
}

// ── Assertions ──────────────────────────────────────────────────────

pub fn stdout_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn assert_failure(output: &Output, ctx: &str) {
    assert!(
        !output.status.success(),
        "[{ctx}] expected non-zero exit, got 0\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout_str(output),
        stderr_str(output),
    );
}

/// Parse JSON envelope from command stdout.
#[allow(dead_code)]
pub fn parse_json(out: &Output) -> serde_json::Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nraw stdout: {text}");
    })
}

pub fn config_path() -> std::path::PathBuf {
    std::path::Path::new(&shared_env().actionbook_home).join("config.toml")
}
