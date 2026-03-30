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

/// Isolated XDG environment so tests never touch real config.
///
/// NOTE: We only override XDG_CONFIG_HOME and XDG_DATA_HOME, NOT HOME.
/// On macOS, Chrome derives its profile directory from HOME — overriding
/// HOME causes Chrome to fail to bind its CDP port.  Actionbook reads its
/// own config via XDG paths, so this is sufficient for isolation.
pub struct IsolatedEnv {
    _tmp: tempfile::TempDir,
    pub config_home: String,
    pub data_home: String,
}

static ENV: OnceLock<IsolatedEnv> = OnceLock::new();

/// Returns a shared isolated environment (created once per test binary run).
pub fn shared_env() -> &'static IsolatedEnv {
    ENV.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");

        fs::create_dir_all(&config_home).unwrap();
        fs::create_dir_all(&data_home).unwrap();

        IsolatedEnv {
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

/// Run `actionbook browser <args>` with the isolated environment.
///
/// Uses the daemon v2 CLI path. The daemon auto-starts on first command.
pub fn headless(args: &[&str], timeout_secs: u64) -> Output {
    let env = shared_env();
    Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("XDG_CONFIG_HOME", &env.config_home)
        .env("XDG_DATA_HOME", &env.data_home)
        .args(args)
        .timeout(Duration::from_secs(timeout_secs))
        .output()
        .expect("failed to execute command")
}

/// Run `actionbook --json browser <args>` with the isolated environment.
pub fn headless_json(args: &[&str], timeout_secs: u64) -> Output {
    let env = shared_env();
    Command::cargo_bin("actionbook")
        .expect("binary exists")
        .env("XDG_CONFIG_HOME", &env.config_home)
        .env("XDG_DATA_HOME", &env.data_home)
        .arg("--json")
        .args(args)
        .timeout(Duration::from_secs(timeout_secs))
        .output()
        .expect("failed to execute command")
}

// ── Cleanup helpers ─────────────────────────────────────────────────

/// RAII guard that ensures sessions are cleaned up even when a test panics.
///
/// Create at the start of each test with `let _guard = SessionGuard::new();`.
/// On creation it calls `ensure_no_sessions()` to start clean.  On drop
/// (including panic-triggered unwind) it calls `ensure_no_sessions()` again,
/// preventing one test's leftover session from cascading failures into all
/// subsequent tests.
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

/// Close all active sessions so the next test starts with a clean slate.
///
/// Strategy: attempt graceful close twice with a short delay.  If sessions
/// still persist (e.g. after a panic left the browser hung), stop the
/// daemon entirely — it will auto-restart on the next CLI command.
pub fn ensure_no_sessions() {
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(500));
        }
        let out = headless_json(&["browser", "list-sessions"], 10);
        if !out.status.success() {
            return;
        }
        let text = stdout_str(&out);
        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return,
        };
        let sessions = parsed
            .get("data")
            .and_then(|data| data.get("sessions"))
            .or_else(|| parsed.get("sessions"))
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        if sessions.is_empty() {
            return;
        }
        if attempt < 2 {
            // Graceful close
            for s in &sessions {
                if let Some(id) = s.get("session_id").and_then(|v| v.as_str()) {
                    let _ = headless(&["browser", "close", "-s", id], 10);
                }
            }
        } else {
            // Last resort: kill the daemon so all sessions are destroyed.
            // The daemon auto-restarts on the next CLI command.
            let _ = headless(&["daemon", "stop"], 10);
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

// ── Trusted HTML helpers ────────────────────────────────────────────

fn trusted_types_template_js(html: &str, clear_body: bool) -> String {
    let escaped = html
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    let clear = if clear_body {
        "document.body.textContent='';"
    } else {
        ""
    };
    format!(
        "(function(){{ var html='{}'; var t=document.createElement('template'); var content=html; if (window.trustedTypes && window.trustedTypes.createPolicy) {{ if (!window.__actionbookE2eTrustedTypesPolicy) {{ try {{ window.__actionbookE2eTrustedTypesPolicy = window.trustedTypes.createPolicy('actionbook-e2e', {{ createHTML: function(s) {{ return s; }} }}); }} catch (e) {{}} }} if (window.__actionbookE2eTrustedTypesPolicy) {{ content = window.__actionbookE2eTrustedTypesPolicy.createHTML(html); }} }} {} t.innerHTML = content; document.body.append(t.content); }})()",
        escaped, clear
    )
}

/// Generate JS that sets `document.body` content, compatible with
/// Chrome 146+ Trusted Types enforcement.
#[allow(dead_code)]
pub fn set_body_html_js(html: &str) -> String {
    trusted_types_template_js(html, true)
}

/// Generate JS that appends HTML to `document.body`, compatible
/// with Chrome 146+ Trusted Types enforcement.
#[allow(dead_code)]
pub fn append_body_html_js(html: &str) -> String {
    trusted_types_template_js(html, false)
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
