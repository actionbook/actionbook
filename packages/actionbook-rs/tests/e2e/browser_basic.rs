//! Basic browser E2E tests: open → goto arxiv.org → snapshot → close.
//!
//! Tests are prefixed `t01_` .. `t04_` for deterministic ordering when run with
//! `--test-threads=1`. Each test depends on the browser state left by the
//! previous one (shared session via `OnceLock` in the harness).

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str};

// ── t01: open ───────────────────────────────────────────────────────

#[test]
fn t01_open_browser() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "open", "https://arxiv.org"], 30);
    assert_success(&out, "open arxiv.org");
}

// ── t02: goto ───────────────────────────────────────────────────────

#[test]
fn t02_goto_arxiv() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "goto", "https://arxiv.org"], 30);
    assert_success(&out, "goto arxiv.org");

    // Verify we're on arxiv.org by checking location.
    let loc = headless(&["browser", "eval", "window.location.href"], 30);
    assert_success(&loc, "eval location");
    assert!(
        stdout_str(&loc).contains("arxiv.org"),
        "location should contain arxiv.org, got: {}",
        stdout_str(&loc)
    );
}

// ── t03: snapshot ───────────────────────────────────────────────────

#[test]
fn t03_snapshot_arxiv() {
    if skip() {
        return;
    }
    let out = headless_json(&["browser", "snapshot"], 30);
    assert_success(&out, "snapshot");

    let output = stdout_str(&out);
    // Snapshot should contain arxiv-related content (e.g. page title or
    // common text present on the arxiv homepage).
    assert!(
        output.contains("arxiv") || output.contains("arXiv"),
        "snapshot should contain arxiv content, got (first 500 chars): {}",
        &output[..output.len().min(500)]
    );
}

// ── t04: close ──────────────────────────────────────────────────────

#[test]
fn t04_close_browser() {
    if skip() {
        return;
    }
    let out = headless(&["browser", "close"], 30);
    assert_success(&out, "close");
}
