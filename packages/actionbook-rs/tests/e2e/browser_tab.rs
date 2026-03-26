//! Browser tab management E2E tests.
//!
//! Covers opening, switching, closing, and listing tabs across sessions.
//! Uses daemon v2 CLI format with --session and --tab addressing.

use crate::harness::{assert_success, ensure_no_sessions, headless, skip, stdout_str};

/// S1T2: start → open second URL → list-tabs → shows 2 tabs → close
#[test]
fn tab_open_creates_new_tab() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session with first tab
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

    // Open a second tab
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open second tab");

    // List tabs — should show 2
    let out = headless(&["browser", "list-tabs", "-s", "s0"], 10);
    assert_success(&out, "list-tabs");
    let tabs_output = stdout_str(&out);
    assert!(
        tabs_output.contains("t0") && tabs_output.contains("t1"),
        "list-tabs should show t0 and t1, got: {}",
        tabs_output
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

/// S1T1: start with --open-url → eval location.href on t0 → correct URL → close
#[test]
fn tab_open_navigates_to_url() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session opening example.com
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

    // Eval location on t0
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
    assert_success(&out, "eval location");
    let location = stdout_str(&out);
    assert!(
        location.contains("example.com"),
        "t0 should be on example.com, got: {}",
        location
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second URL → eval on t0 → eval on t1 → different URLs → close
#[test]
fn tab_switch_changes_active() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session with example.com
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

    // Open second tab with example.org
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open second tab");

    // Eval on t0 — should be example.com
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
    assert_success(&out, "eval t0");
    let loc_t0 = stdout_str(&out);
    assert!(
        loc_t0.contains("example.com"),
        "t0 should be on example.com, got: {}",
        loc_t0
    );

    // Eval on t1 — should be example.org
    let out = headless(
        &[
            "browser",
            "eval",
            "window.location.href",
            "-s",
            "s0",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "eval t1");
    let loc_t1 = stdout_str(&out);
    assert!(
        loc_t1.contains("example.org"),
        "t1 should be on example.org, got: {}",
        loc_t1
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second tab → close-tab t1 → list-tabs → only 1 tab → close
#[test]
fn tab_close_removes_tab() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session
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

    // Open second tab
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open second tab");

    // Close tab t1
    let out = headless(&["browser", "close-tab", "-s", "s0", "-t", "t1"], 30);
    assert_success(&out, "close-tab t1");

    // List tabs — should only show t0
    let out = headless(&["browser", "list-tabs", "-s", "s0"], 10);
    assert_success(&out, "list-tabs after close");
    let tabs_output = stdout_str(&out);
    assert!(
        tabs_output.contains("t0"),
        "list-tabs should still show t0, got: {}",
        tabs_output
    );
    assert!(
        !tabs_output.contains("t1"),
        "list-tabs should NOT show t1 after close, got: {}",
        tabs_output
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second tab → close-tab t1 → eval on t0 → still works → close
#[test]
fn tab_close_preserves_other() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session
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

    // Open second tab
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open second tab");

    // Close tab t1
    let out = headless(&["browser", "close-tab", "-s", "s0", "-t", "t1"], 30);
    assert_success(&out, "close-tab t1");

    // Eval on t0 should still work
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
    assert_success(&out, "eval t0 after closing t1");
    let location = stdout_str(&out);
    assert!(
        location.contains("example.com"),
        "t0 should still be on example.com, got: {}",
        location
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open second tab → list-tabs → output contains both URLs → close
#[test]
fn tab_pages_lists_all() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session with example.com
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

    // Open second tab with example.org
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open second tab");

    // List tabs — should contain both URLs
    let out = headless(&["browser", "list-tabs", "-s", "s0"], 10);
    assert_success(&out, "list-tabs");
    let tabs_output = stdout_str(&out);
    assert!(
        tabs_output.contains("example.com"),
        "list-tabs should contain example.com, got: {}",
        tabs_output
    );
    assert!(
        tabs_output.contains("example.org"),
        "list-tabs should contain example.org, got: {}",
        tabs_output
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}

/// S1T2: start → open tab2 → open tab3 → list-tabs → shows t0, t1, t2 → close
#[test]
fn tab_open_sequential_ids() {
    if skip() {
        return;
    }
    ensure_no_sessions();

    // Start session with first tab (t0)
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

    // Open second tab (t1)
    let out = headless(&["browser", "open", "https://example.org", "-s", "s0"], 30);
    assert_success(&out, "open tab t1");

    // Open third tab (t2)
    let out = headless(
        &["browser", "open", "https://example.com/page2", "-s", "s0"],
        30,
    );
    assert_success(&out, "open tab t2");

    // List tabs — should show t0, t1, t2
    let out = headless(&["browser", "list-tabs", "-s", "s0"], 10);
    assert_success(&out, "list-tabs");
    let tabs_output = stdout_str(&out);
    assert!(
        tabs_output.contains("t0"),
        "list-tabs should show t0, got: {}",
        tabs_output
    );
    assert!(
        tabs_output.contains("t1"),
        "list-tabs should show t1, got: {}",
        tabs_output
    );
    assert!(
        tabs_output.contains("t2"),
        "list-tabs should show t2, got: {}",
        tabs_output
    );

    // Close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}
