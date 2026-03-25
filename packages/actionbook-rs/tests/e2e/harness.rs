//! Shared E2E test harness.
//!
//! Provides environment isolation, CLI invocation helpers, and common assertions.
//! All state is lazily initialized via `OnceLock` so a single browser session is
//! reused across the entire test binary.

use assert_cmd::Command;
use std::env;
use std::fs;
use std::process::Output;
use std::sync::OnceLock;
use std::time::Duration;

// ── Isolated environment ────────────────────────────────────────────

/// Isolated HOME / XDG environment so tests never touch real config.
pub struct IsolatedEnv {
    _tmp: tempfile::TempDir,
    pub home: String,
    pub config_home: String,
    pub data_home: String,
}

// SAFETY: all fields are immutable after init; TempDir is Send+Sync.
unsafe impl Sync for IsolatedEnv {}

static ENV: OnceLock<IsolatedEnv> = OnceLock::new();

/// Returns a shared isolated environment (created once per test binary run).
pub fn shared_env() -> &'static IsolatedEnv {
    ENV.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");

        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        fs::create_dir_all(&data_home).unwrap();

        // On macOS, suppress Keychain permission dialogs.
        if cfg!(target_os = "macos") {
            let config_dir = home.join("Library/Application Support/actionbook");
            fs::create_dir_all(&config_dir).unwrap();
            fs::write(
                config_dir.join("config.toml"),
                "[profiles.actionbook]\ncdp_port = 9222\nextra_args = [\"--use-mock-keychain\"]\n",
            )
            .unwrap();
        }

        IsolatedEnv {
            home: home.to_string_lossy().to_string(),
            config_home: config_home.to_string_lossy().to_string(),
            data_home: data_home.to_string_lossy().to_string(),
            _tmp: tmp,
        }
    })
}

// ── Gate ────────────────────────────────────────────────────────────

/// Returns `true` when the E2E gate env var is NOT set — callers should
/// `return` early to skip the test.
pub fn skip() -> bool {
    env::var("RUN_E2E_TESTS")
        .map(|v| v != "true")
        .unwrap_or(true)
}

// ── CLI runners ─────────────────────────────────────────────────────

/// Run `actionbook --headless <args>` with the isolated environment.
pub fn headless(args: &[&str], timeout_secs: u64) -> Output {
    let env = shared_env();
    Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("HOME", &env.home)
        .env("XDG_CONFIG_HOME", &env.config_home)
        .env("XDG_DATA_HOME", &env.data_home)
        .arg("--headless")
        .args(args)
        .timeout(Duration::from_secs(timeout_secs))
        .output()
        .expect("failed to execute command")
}

/// Run `actionbook --json --headless <args>` with the isolated environment.
pub fn headless_json(args: &[&str], timeout_secs: u64) -> Output {
    let env = shared_env();
    Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("HOME", &env.home)
        .env("XDG_CONFIG_HOME", &env.config_home)
        .env("XDG_DATA_HOME", &env.data_home)
        .arg("--json")
        .arg("--headless")
        .args(args)
        .timeout(Duration::from_secs(timeout_secs))
        .output()
        .expect("failed to execute command")
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

#[allow(dead_code)]
pub fn assert_failure(output: &Output, ctx: &str) {
    assert!(
        !output.status.success(),
        "[{ctx}] expected non-zero exit, got 0\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout_str(output),
        stderr_str(output),
    );
}
