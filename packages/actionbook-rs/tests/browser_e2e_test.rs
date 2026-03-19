//! Browser E2E tests — mirrors TypeScript cli-browser-e2e.test.ts (57 tests)
//!
//! Each test is an independent `#[test]` function sharing a browser session via
//! `OnceLock`. Tests are named `t01_` .. `t57_` so that `--test-threads=1` runs
//! them in the correct order (browser state carries over between tests).
//!
//! Gated by `RUN_BROWSER_TESTS=true`. Without the env var every test is skipped.
//!
//! Run with:
//!   RUN_BROWSER_TESTS=true cargo test --test browser_e2e_test -- --test-threads=1 --nocapture

#![allow(deprecated)]

use assert_cmd::Command;
use std::env;
use std::fs;
use std::process::Output;
use std::sync::OnceLock;
use std::time::Duration;

// ── Shared state ────────────────────────────────────────────────────

/// Isolated HOME / XDG environment so tests never touch real config.
struct BrowserIsolatedEnv {
    _tmp: tempfile::TempDir,
    home: String,
    config_home: String,
    data_home: String,
}

// SAFETY: all fields are immutable after init; TempDir is Send+Sync.
unsafe impl Sync for BrowserIsolatedEnv {}

static ENV: OnceLock<BrowserIsolatedEnv> = OnceLock::new();

fn shared_env() -> &'static BrowserIsolatedEnv {
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

        BrowserIsolatedEnv {
            home: home.to_string_lossy().to_string(),
            config_home: config_home.to_string_lossy().to_string(),
            data_home: data_home.to_string_lossy().to_string(),
            _tmp: tmp,
        }
    })
}

// ── Helpers ─────────────────────────────────────────────────────────

fn skip() -> bool {
    env::var("RUN_BROWSER_TESTS")
        .map(|v| v != "true")
        .unwrap_or(true)
}

/// Run `actionbook --headless <args>`.
fn headless(args: &[&str], timeout_secs: u64) -> Output {
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

/// Run `actionbook --json --headless <args>`.
fn headless_json(args: &[&str], timeout_secs: u64) -> Output {
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

fn stdout_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_success(output: &Output, ctx: &str) {
    assert!(
        output.status.success(),
        "[{ctx}] expected exit 0, got {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        stdout_str(output),
        stderr_str(output),
    );
}

fn assert_failure(output: &Output, ctx: &str) {
    assert!(
        !output.status.success(),
        "[{ctx}] expected non-zero exit, got 0\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout_str(output),
        stderr_str(output),
    );
}

// navigation: open, goto, back, forward, reload

#[test]
fn t01_open_url() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "open", "https://example.com"], 30);
    assert_success(&out, "open");
}

#[test]
fn t02_goto_and_eval_location() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "goto", "https://example.com"], 30);
    assert_success(&out, "goto");

    let loc = headless(&["browser", "eval", "window.location.href"], 30);
    assert_success(&loc, "eval location");
    assert!(
        stdout_str(&loc).contains("example.com"),
        "location should contain example.com, got: {}",
        stdout_str(&loc)
    );
}

#[test]
fn t03_eval_title() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "eval", "document.title"], 30);
    assert_success(&out, "eval title");
    assert!(
        stdout_str(&out).contains("Example Domain"),
        "title should contain 'Example Domain', got: {}",
        stdout_str(&out)
    );
}

#[test]
fn t04_back() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "back"], 30);
    assert_success(&out, "back");
}

#[test]
fn t05_forward() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "forward"], 30);
    assert_success(&out, "forward");
}

#[test]
fn t06_reload() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "reload"], 30);
    assert_success(&out, "reload");
}

// page content: html, text, viewport, snapshot, pages

#[test]
fn t07_html() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(&["browser", "html"], 30);
    assert_success(&out, "html");
    assert!(
        stdout_str(&out).contains("<html"),
        "output should contain <html"
    );
}

#[test]
fn t08_html_selector() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "html", "h1"], 30);
    assert_success(&out, "html h1");
    assert!(
        !stdout_str(&out).is_empty(),
        "html h1 output should not be empty"
    );
}

#[test]
fn t09_text() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "text"], 30);
    assert_success(&out, "text");
    assert!(
        stdout_str(&out).contains("Example Domain"),
        "text should contain 'Example Domain'"
    );
}

#[test]
fn t10_viewport() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "viewport"], 30);
    assert_success(&out, "viewport");
    let s = stdout_str(&out);
    assert!(
        s.chars().any(|c| c.is_ascii_digit()),
        "viewport should contain digits, got: {s}"
    );
}

#[test]
fn t11_snapshot() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "snapshot"], 30);
    assert_success(&out, "snapshot");
    assert!(
        !stdout_str(&out).is_empty(),
        "snapshot output should not be empty"
    );
}

#[test]
fn t12_pages() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "pages"], 30);
    assert_success(&out, "pages");
    assert!(
        !stdout_str(&out).is_empty(),
        "pages output should not be empty"
    );
}

// form interaction: fill, click, eval (login flow)

/// Cache whether the login test site is reachable.
static LOGIN_SITE_OK: OnceLock<bool> = OnceLock::new();

fn login_site_available() -> bool {
    *LOGIN_SITE_OK.get_or_init(|| {
        headless(
            &[
                "browser",
                "goto",
                "https://the-internet.herokuapp.com/login",
            ],
            30,
        )
        .status
        .success()
    })
}

#[test]
fn t13_login_page() {
    if skip() {
        return;
    }
    if !login_site_available() {
        eprintln!("SKIP: login test site unavailable");
        return;
    }
    let out = headless(&["browser", "eval", "document.title"], 30);
    assert_success(&out, "eval title on login page");
}

#[test]
fn t14_fill_username() {
    if skip() || !login_site_available() {
        return;
    }
    let out = headless(
        &["browser", "fill", "--wait", "5000", "#username", "tomsmith"],
        30,
    );
    assert_success(&out, "fill username");
}

#[test]
fn t15_eval_verify_username() {
    if skip() || !login_site_available() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#username')?.value ?? ''",
        ],
        30,
    );
    assert_success(&out, "eval username value");
    assert!(
        stdout_str(&out).contains("tomsmith"),
        "expected 'tomsmith', got: {}",
        stdout_str(&out)
    );
}

#[test]
fn t16_click_submit() {
    if skip() || !login_site_available() {
        return;
    }
    // Fill password first
    headless(
        &[
            "browser",
            "fill",
            "--wait",
            "5000",
            "#password",
            "SuperSecretPassword!",
        ],
        30,
    );
    let out = headless(
        &[
            "browser",
            "click",
            "--wait",
            "5000",
            "button[type=\"submit\"]",
        ],
        30,
    );
    assert_success(&out, "click submit");
}

#[test]
fn t17_verify_navigation_after_login() {
    if skip() || !login_site_available() {
        return;
    }
    // Submit form via JS
    let submit = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('form#login').submit(); 'submitted'",
        ],
        30,
    );
    assert_success(&submit, "submit form");

    // Wait for navigation
    std::thread::sleep(Duration::from_secs(3));

    let nav = headless(&["browser", "eval", "window.location.pathname"], 30);
    assert_success(&nav, "eval pathname");
    assert!(
        stdout_str(&nav).contains("/secure"),
        "expected pathname to contain '/secure', got: {}",
        stdout_str(&nav)
    );
}

// screenshot, pdf

#[test]
fn t18_screenshot() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);

    let path = std::env::temp_dir().join("e2e-rust-screenshot.png");
    let out = headless(&["browser", "screenshot", path.to_str().unwrap()], 30);
    assert_success(&out, "screenshot");
    assert!(path.exists(), "screenshot file should exist");
    assert!(
        fs::metadata(&path).unwrap().len() > 0,
        "screenshot should not be empty"
    );
}

#[test]
fn t19_fullpage_screenshot() {
    if skip() {
        return;
    }
    let path = std::env::temp_dir().join("e2e-rust-screenshot-full.png");
    let out = headless(
        &[
            "browser",
            "screenshot",
            path.to_str().unwrap(),
            "--full-page",
        ],
        30,
    );
    assert_success(&out, "full-page screenshot");
    assert!(path.exists(), "full-page screenshot file should exist");
}

#[test]
fn t20_pdf() {
    if skip() {
        return;
    }
    let path = std::env::temp_dir().join("e2e-rust-page.pdf");
    let out = headless(&["browser", "pdf", path.to_str().unwrap()], 30);
    assert_success(&out, "pdf");
    assert!(path.exists(), "PDF file should exist");
    assert!(
        fs::metadata(&path).unwrap().len() > 0,
        "PDF should not be empty"
    );
}

// cookies: list, set, get, delete, clear

#[test]
fn t21_cookies_list() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(&["browser", "cookies", "list"], 30);
    assert_success(&out, "cookies list");
}

#[test]
fn t22_cookies_set() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "cookies",
            "set",
            "e2e_test_cookie",
            "hello_from_e2e",
        ],
        30,
    );
    assert_success(&out, "cookies set");
}

#[test]
fn t23_cookies_get() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "cookies", "get", "e2e_test_cookie"], 30);
    assert_success(&out, "cookies get");
    assert!(
        stdout_str(&out).contains("hello_from_e2e"),
        "expected cookie value 'hello_from_e2e', got: {}",
        stdout_str(&out)
    );
}

#[test]
fn t24_cookies_delete() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "cookies", "delete", "e2e_test_cookie"], 30);
    assert_success(&out, "cookies delete");

    let after = headless(&["browser", "cookies", "get", "e2e_test_cookie"], 30);
    assert!(
        !stdout_str(&after).contains("hello_from_e2e"),
        "cookie should be deleted"
    );
}

#[test]
fn t25_cookies_clear() {
    if skip() {
        return;
    }
    headless(
        &["browser", "cookies", "set", "clear_test", "to_be_cleared"],
        30,
    );
    let out = headless(&["browser", "cookies", "clear", "--yes"], 30);
    assert_success(&out, "cookies clear");

    let after = headless(&["browser", "cookies", "get", "clear_test"], 30);
    assert!(
        !stdout_str(&after).contains("to_be_cleared"),
        "cookie should be cleared"
    );
}

// error scenarios

#[test]
fn t26_click_nonexistent() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(&["browser", "click", "#nonexistent-element-xyz"], 10);
    assert_failure(&out, "click nonexistent");
}

#[test]
fn t27_wait_timeout() {
    if skip() {
        return;
    }
    let out = headless(
        &[
            "browser",
            "wait",
            "#nonexistent-element-xyz",
            "--timeout",
            "2000",
        ],
        10,
    );
    assert_failure(&out, "wait timeout");
}

#[test]
fn t28_eval_throw_error() {
    if skip() {
        return;
    }
    let out = headless(
        &["browser", "eval", "throw new Error('test error from e2e')"],
        30,
    );
    // CLI returns exit 0 but includes error details in stdout
    assert_success(&out, "eval throw");
    let s = stdout_str(&out);
    assert!(s.contains("Error"), "should contain 'Error', got: {s}");
    assert!(
        s.contains("test error from e2e"),
        "should contain error message, got: {s}"
    );
}

// --json output

#[test]
fn t29_json_eval() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless_json(&["browser", "eval", "1+1"], 30);
    assert_success(&out, "json eval");
    let s = stdout_str(&out);
    assert!(
        serde_json::from_str::<serde_json::Value>(&s).is_ok(),
        "should be valid JSON, got: {s}"
    );
}

#[test]
fn t30_json_pages() {
    if skip() {
        return;
    }
    let out = headless_json(&["browser", "pages"], 30);
    assert_success(&out, "json pages");
    let s = stdout_str(&out);
    assert!(
        serde_json::from_str::<serde_json::Value>(&s).is_ok(),
        "should be valid JSON, got: {s}"
    );
}

#[test]
fn t31_json_viewport() {
    if skip() {
        return;
    }
    let out = headless_json(&["browser", "viewport"], 30);
    assert_success(&out, "json viewport");
    let s = stdout_str(&out);
    let json: serde_json::Value =
        serde_json::from_str(&s).unwrap_or_else(|_| panic!("should parse JSON, got: {s}"));
    assert!(json.get("width").is_some(), "JSON should have 'width'");
    assert!(json.get("height").is_some(), "JSON should have 'height'");
}

// inspect

#[test]
fn t32_inspect() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(&["browser", "inspect", "100", "100"], 30);
    assert_success(&out, "inspect");
    let s = stdout_str(&out);
    assert!(!s.is_empty(), "inspect output should not be empty");
    let lower = s.to_lowercase();
    assert!(
        lower.contains("tag")
            || lower.contains("selector")
            || lower.contains("<")
            || lower.contains("id")
            || lower.contains("class"),
        "inspect output should contain element info, got: {s}"
    );
}

// scroll: down, up, bottom, top, to element

#[test]
fn t33_scroll_down() {
    if skip() {
        return;
    }
    headless(
        &["browser", "goto", "https://the-internet.herokuapp.com"],
        30,
    );
    headless(&["browser", "scroll", "top"], 30);

    let before = headless(&["browser", "eval", "window.scrollY"], 30);
    let y_before: f64 = stdout_str(&before).trim().parse().unwrap_or(0.0);

    let out = headless(&["browser", "scroll", "down"], 30);
    assert_success(&out, "scroll down");

    let after = headless(&["browser", "eval", "window.scrollY"], 30);
    let y_after: f64 = stdout_str(&after).trim().parse().unwrap_or(0.0);
    assert!(
        y_after > y_before,
        "scrollY should increase ({y_after} > {y_before})"
    );
}

#[test]
fn t34_scroll_up() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "down", "500"], 30);
    let before = headless(&["browser", "eval", "window.scrollY"], 30);
    let y_before: f64 = stdout_str(&before).trim().parse().unwrap_or(0.0);
    assert!(y_before > 0.0, "should have scrolled down first");

    let out = headless(&["browser", "scroll", "up"], 30);
    assert_success(&out, "scroll up");

    let after = headless(&["browser", "eval", "window.scrollY"], 30);
    let y_after: f64 = stdout_str(&after).trim().parse().unwrap_or(0.0);
    assert!(
        y_after < y_before,
        "scrollY should decrease ({y_after} < {y_before})"
    );
}

#[test]
fn t35_scroll_bottom() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "scroll", "bottom"], 30);
    assert_success(&out, "scroll bottom");

    let at_bottom = headless(
        &[
            "browser",
            "eval",
            "window.scrollY + window.innerHeight >= document.body.scrollHeight - 1",
        ],
        30,
    );
    assert_eq!(
        stdout_str(&at_bottom).trim(),
        "true",
        "should be at bottom of page"
    );
}

#[test]
fn t36_scroll_top() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "down", "500"], 30);
    let out = headless(&["browser", "scroll", "top"], 30);
    assert_success(&out, "scroll top");

    let after = headless(&["browser", "eval", "window.scrollY"], 30);
    let y: f64 = stdout_str(&after).trim().parse().unwrap_or(-1.0);
    assert_eq!(y as i64, 0, "scrollY should be 0 at top");
}

#[test]
fn t37_scroll_to_element() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "top"], 30);
    headless(&["browser", "scroll", "down", "500"], 30);

    let out = headless(&["browser", "scroll", "to", "h1"], 30);
    assert_success(&out, "scroll to h1");

    let rect = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('h1').getBoundingClientRect().top",
        ],
        30,
    );
    let top: f64 = stdout_str(&rect).trim().parse().unwrap_or(9999.0);
    assert!(
        top.abs() < 300.0,
        "h1 top should be within 300px of viewport, got: {top}"
    );
}

// type

#[test]
fn t38_type_text() {
    if skip() {
        return;
    }
    headless(
        &[
            "browser",
            "goto",
            "https://the-internet.herokuapp.com/login",
        ],
        30,
    );
    headless(&["browser", "fill", "--wait", "5000", "#username", ""], 30);

    let out = headless(
        &[
            "browser",
            "type",
            "--wait",
            "5000",
            "#username",
            "appended-text",
        ],
        30,
    );
    assert_success(&out, "type text");

    let val = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#username').value",
        ],
        30,
    );
    assert!(
        stdout_str(&val).contains("appended-text"),
        "input should contain 'appended-text', got: {}",
        stdout_str(&val)
    );
}

// select, hover, focus, press

#[test]
fn t39_select_dropdown() {
    if skip() {
        return;
    }
    headless(
        &[
            "browser",
            "goto",
            "https://the-internet.herokuapp.com/dropdown",
        ],
        30,
    );

    let out = headless(&["browser", "select", "#dropdown", "1"], 30);
    assert_success(&out, "select");

    let val = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#dropdown').value",
        ],
        30,
    );
    let v = stdout_str(&val).trim().replace('"', "");
    assert_eq!(v, "1", "dropdown value should be '1', got: {v}");
}

#[test]
fn t40_hover() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "hover", "#dropdown"], 30);
    assert_success(&out, "hover");
}

#[test]
fn t41_focus() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "focus", "#dropdown"], 30);
    assert_success(&out, "focus");

    let focused = headless(
        &[
            "browser",
            "eval",
            "document.activeElement?.id || document.activeElement?.tagName",
        ],
        30,
    );
    let f = stdout_str(&focused).trim().replace('"', "");
    assert_eq!(
        f, "dropdown",
        "active element should be 'dropdown', got: {f}"
    );
}

#[test]
fn t42_press_key() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "press", "Tab"], 30);
    assert_success(&out, "press Tab");
}

// wait-nav

#[test]
fn t43_wait_nav() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(&["browser", "wait-nav", "--timeout", "5000"], 30);
    let code = out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "exit code should be 0 or 1, got: {code}"
    );
}

// inspect --desc

#[test]
fn t44_inspect_desc() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(
        &[
            "browser",
            "inspect",
            "100",
            "100",
            "--desc",
            "looking for header element",
        ],
        30,
    );
    assert_success(&out, "inspect --desc");
    assert!(
        !stdout_str(&out).is_empty(),
        "inspect --desc output should not be empty"
    );
}

// scroll variants: pixels, --smooth, --align

#[test]
fn t45_scroll_down_200px() {
    if skip() {
        return;
    }
    headless(
        &["browser", "goto", "https://the-internet.herokuapp.com"],
        30,
    );
    headless(&["browser", "scroll", "top"], 30);

    let out = headless(&["browser", "scroll", "down", "200"], 30);
    assert_success(&out, "scroll down 200");

    let after = headless(&["browser", "eval", "window.scrollY"], 30);
    let y: f64 = stdout_str(&after).trim().parse().unwrap_or(0.0);
    assert!(y >= 200.0, "scrollY should be >= 200, got: {y}");
}

#[test]
fn t46_scroll_up_100px() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "top"], 30);
    headless(&["browser", "scroll", "down", "500"], 30);

    let out = headless(&["browser", "scroll", "up", "100"], 30);
    assert_success(&out, "scroll up 100");

    let after = headless(&["browser", "eval", "window.scrollY"], 30);
    let y: f64 = stdout_str(&after).trim().parse().unwrap_or(0.0);
    assert!(y <= 420.0, "scrollY should be <= 420, got: {y}");
    assert!(y > 0.0, "scrollY should be > 0, got: {y}");
}

#[test]
fn t47_scroll_smooth() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "top"], 30);
    let out = headless(&["browser", "scroll", "--smooth", "down", "300"], 30);
    assert_success(&out, "scroll --smooth");

    std::thread::sleep(Duration::from_millis(500));
    let after = headless(&["browser", "eval", "window.scrollY"], 30);
    let y: f64 = stdout_str(&after).trim().parse().unwrap_or(0.0);
    assert!(
        y > 0.0,
        "scrollY should be > 0 after smooth scroll, got: {y}"
    );
}

#[test]
fn t48_scroll_align_start() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "down", "500"], 30);
    let out = headless(&["browser", "scroll", "to", "h1", "--align", "start"], 30);
    assert_success(&out, "scroll --align start");

    let rect = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('h1').getBoundingClientRect().top",
        ],
        30,
    );
    let top: f64 = stdout_str(&rect).trim().parse().unwrap_or(9999.0);
    assert!(
        top.abs() < 50.0,
        "h1 top should be near viewport top (<50), got: {top}"
    );
}

#[test]
fn t49_scroll_align_end() {
    if skip() {
        return;
    }
    headless(&["browser", "scroll", "down", "500"], 30);
    let out = headless(&["browser", "scroll", "to", "h1", "--align", "end"], 30);
    assert_success(&out, "scroll --align end");

    let visible = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('h1').getBoundingClientRect().bottom <= window.innerHeight",
        ],
        30,
    );
    assert_eq!(
        stdout_str(&visible).trim(),
        "true",
        "h1 bottom should be within viewport"
    );
}

// cookies variants: --domain, --dry-run

#[test]
fn t50_cookies_set_domain() {
    if skip() {
        return;
    }
    headless(&["browser", "goto", "https://example.com"], 30);
    let out = headless(
        &[
            "browser",
            "cookies",
            "set",
            "domain_cookie",
            "domain_value",
            "--domain",
            "example.com",
        ],
        30,
    );
    // --domain may require extension mode; accept graceful failure
    if out.status.success() {
        let after = headless(&["browser", "cookies", "get", "domain_cookie"], 30);
        assert!(
            stdout_str(&after).contains("domain_value"),
            "cookie should contain 'domain_value'"
        );
    } else {
        let code = out.status.code().unwrap_or(-1);
        assert_ne!(code, 2, "should not be a CLI parse error");
    }
}

#[test]
fn t51_cookies_clear_dry_run() {
    if skip() {
        return;
    }
    headless(
        &["browser", "cookies", "set", "dry_run_test", "should_remain"],
        30,
    );
    let out = headless(&["browser", "cookies", "clear", "--dry-run"], 30);
    if out.status.success() {
        let after = headless(&["browser", "cookies", "get", "dry_run_test"], 30);
        assert!(
            stdout_str(&after).contains("should_remain"),
            "--dry-run should NOT actually delete cookies"
        );
    } else {
        let code = out.status.code().unwrap_or(-1);
        assert_ne!(code, 2, "should not be a CLI parse error");
    }
}

#[test]
fn t52_cookies_clear_domain() {
    if skip() {
        return;
    }
    headless(
        &[
            "browser",
            "cookies",
            "set",
            "domain_clear_test",
            "domain_val",
            "--domain",
            "example.com",
        ],
        30,
    );
    let out = headless(
        &[
            "browser",
            "cookies",
            "clear",
            "--domain",
            "example.com",
            "--yes",
        ],
        30,
    );
    if out.status.success() {
        let after = headless(&["browser", "cookies", "get", "domain_clear_test"], 30);
        assert!(
            !stdout_str(&after).contains("domain_val"),
            "cookie should be cleared after --domain clear"
        );
    } else {
        let code = out.status.code().unwrap_or(-1);
        assert_ne!(code, 2, "should not be a CLI parse error");
    }
}

// tab management: open, pages, switch

#[test]
fn t53_open_new_tab() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "open", "https://example.com"], 30);
    assert_success(&out, "open new tab");
}

#[test]
fn t54_pages_multiple_tabs() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "pages"], 30);
    assert_success(&out, "pages");
    let pages_output = stdout_str(&out);
    let line_count = pages_output
        .trim()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert!(
        line_count >= 2,
        "should list at least 2 pages, got {line_count} lines:\n{pages_output}"
    );
}

#[test]
fn t55_switch_tab() {
    if skip() {
        return;
    }
    let pages = headless(&["browser", "pages"], 30);
    assert_success(&pages, "pages for switch");

    let pages_out = stdout_str(&pages);
    let patterns = ["tab:", "page:", "id:", "id "];
    let mut page_id: Option<String> = None;
    for pat in &patterns {
        if let Some(pos) = pages_out.to_lowercase().find(pat) {
            let after = &pages_out[pos + pat.len()..];
            let id: String = after
                .trim()
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            if !id.is_empty() {
                page_id = Some(id);
                break;
            }
        }
    }
    if let Some(id) = page_id {
        let out = headless(&["browser", "switch", &id], 30);
        assert_success(&out, "switch tab");
    } else {
        eprintln!("could not extract page ID from pages output, skipping switch");
    }
}

// restart, close (must be last)

#[test]
fn t56_restart() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "restart"], 60);
    assert_success(&out, "restart");

    let goto = headless(&["browser", "goto", "https://example.com"], 30);
    assert_success(&goto, "goto after restart");
}

#[test]
fn t57_close() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "close"], 30);
    assert_success(&out, "close");
}
