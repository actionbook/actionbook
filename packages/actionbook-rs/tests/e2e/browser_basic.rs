//! Basic browser E2E tests: open → goto arxiv.org → snapshot → close.
//!
//! All steps run in a single test function to guarantee execution order,
//! since Rust does not guarantee test ordering even with `--test-threads=1`.

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str};

#[test]
fn browser_basic_open_goto_snapshot_close() {
    if skip() {
        return;
    }

    // Step 1: open browser with arxiv.org
    let out = headless(&["browser", "open", "https://arxiv.org"], 30);
    assert_success(&out, "open arxiv.org");

    // Step 2: goto arxiv.org and verify location
    let out = headless(&["browser", "goto", "https://arxiv.org"], 30);
    assert_success(&out, "goto arxiv.org");

    let loc = headless(&["browser", "eval", "window.location.href"], 30);
    assert_success(&loc, "eval location");
    assert!(
        stdout_str(&loc).contains("arxiv.org"),
        "location should contain arxiv.org, got: {}",
        stdout_str(&loc)
    );

    // Step 3: snapshot and verify arxiv content
    let out = headless_json(&["browser", "snapshot"], 30);
    assert_success(&out, "snapshot");

    let output = stdout_str(&out);
    assert!(
        output.contains("arxiv") || output.contains("arXiv"),
        "snapshot should contain arxiv content, got (first 500 chars): {}",
        &output[..output.len().min(500)]
    );

    // Step 4: close browser
    let out = headless(&["browser", "close"], 30);
    assert_success(&out, "close");
}
