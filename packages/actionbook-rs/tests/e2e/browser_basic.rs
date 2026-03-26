//! Basic browser E2E tests: start → goto arxiv.org → snapshot → close.
//!
//! Uses daemon v2 CLI format with --session and --tab addressing.
//! All steps run in a single test function to guarantee execution order.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str};

#[test]
fn browser_basic_open_goto_snapshot_close() {
    if skip() {
        return;
    }

    // Step 1: start a headless browser session with arxiv.org
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "https://arxiv.org",
        ],
        30,
    );
    assert_success(&out, "start session");

    // Step 2: list sessions to get session ID
    let out = headless(&["browser", "list-sessions"], 10);
    assert_success(&out, "list-sessions");
    let sessions_output = stdout_str(&out);
    // Session output should contain s0
    assert!(
        sessions_output.contains("s0"),
        "list-sessions should show s0, got: {}",
        sessions_output
    );

    // Step 3: goto arxiv.org (session s0, tab t0)
    let out = headless(
        &[
            "browser",
            "goto",
            "https://arxiv.org",
            "-s",
            "s0",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto arxiv.org");

    // Step 4: eval to verify location
    let loc = headless(
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
    assert_success(&loc, "eval location");
    assert!(
        stdout_str(&loc).contains("arxiv.org"),
        "location should contain arxiv.org, got: {}",
        stdout_str(&loc)
    );

    // Step 5: snapshot and verify arxiv content
    let out = headless_json(&["browser", "snapshot", "-s", "s0", "-t", "t0"], 30);
    assert_success(&out, "snapshot");

    let output = stdout_str(&out);
    assert!(
        output.contains("arxiv") || output.contains("arXiv"),
        "snapshot should contain arxiv content, got (first 500 chars): {}",
        &output[..output.len().min(500)]
    );

    // Step 6: close session
    let out = headless(&["browser", "close", "-s", "s0"], 30);
    assert_success(&out, "close");
}
