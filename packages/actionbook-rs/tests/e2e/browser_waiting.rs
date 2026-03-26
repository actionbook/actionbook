//! Browser waiting E2E tests (api-reference §12).
//!
//! Covers: `wait element`, `wait navigation`, `wait network-idle`, `wait condition`.
//! All tests use S1T1 (single session, single tab).

use crate::harness::{
    assert_failure, assert_success, ensure_no_sessions, headless, skip, stdout_str,
};

#[test]
fn wait_element_exists() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start headless browser session with example.com
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Navigate to ensure page is loaded
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    // Wait for "body" element — should succeed immediately
    let out = headless(
        &[
            "browser",
            "wait",
            "element",
            "body",
            "-s",
            "s0",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait element body");

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn wait_element_timeout() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start headless browser session
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Navigate to page
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    // Wait for nonexistent element — should fail with timeout
    let out = headless(
        &[
            "browser",
            "wait",
            "element",
            "#nonexistent",
            "-s",
            "s0",
            "-t",
            "t0",
            "--timeout",
            "2000",
        ],
        30,
    );
    assert_failure(&out, "wait element #nonexistent should timeout");

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn wait_nav_after_click() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start headless browser session
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Navigate to example.com
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    // Record starting URL before click
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval location before nav");
    let start_url = stdout_str(&out);

    // Inject a link that navigates to a different page
    let out = headless(
        &[
            "browser",
            "eval",
            r#"(() => { const a = document.createElement('a'); a.href = 'https://example.com/nav-test'; a.id = 'nav-link'; a.textContent = 'Navigate'; document.body.appendChild(a); return 'injected'; })()"#,
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "inject nav link");

    // Click the link to trigger navigation
    let out = headless(
        &["browser", "click", "#nav-link", "-s", "s0", "-t", "t0"],
        30,
    );
    assert_success(&out, "click nav link");

    // Wait for navigation to complete
    let out = headless(
        &[
            "browser",
            "wait",
            "navigation",
            "-s",
            "s0",
            "-t",
            "t0",
            "--timeout",
            "10000",
        ],
        30,
    );
    assert_success(&out, "wait navigation");

    // Verify URL changed from the starting URL
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "eval location after nav");
    let url = stdout_str(&out);
    assert!(
        url != start_url,
        "URL should have changed after navigation, but still: {}",
        url
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn wait_network_idle() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start headless browser session
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Navigate to example.com
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    // Wait for network idle — example.com is simple, should idle quickly
    let out = headless(
        &[
            "browser",
            "wait",
            "network-idle",
            "-s",
            "s0",
            "-t",
            "t0",
            "--timeout",
            "10000",
        ],
        30,
    );
    assert_success(&out, "wait network-idle");

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

#[test]
fn wait_condition_true() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start headless browser session
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://example.com",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Navigate to example.com
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.com",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto example.com");

    // Wait for condition: document.readyState === 'complete'
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "s0",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait condition readyState complete");

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}
