//! Contract E2E tests for snapshot data shape, context url/title, and storage command naming.
//!
//! Covers fixes from PR #307:
//! 1. Snapshot data shape: `{format, content, nodes, stats}` (not `{format, tree}`)
//! 2. Snapshot `context.url` and `context.title` are populated (not null)
//! 3. Storage commands use `browser.local-storage.*` / `browser.session-storage.*` naming

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};

fn parse_envelope(out: &std::process::Output) -> serde_json::Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

/// Start a headless local session navigated to `url`, return session_id and tab_id.
fn start_session_at(url: &str) -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            url,
        ],
        30,
    );
    assert_success(&out, "start session");
    let json = parse_envelope(&out);
    let session_id = json["context"]["session_id"]
        .as_str()
        .expect("session_id in start context")
        .to_string();

    // Navigate to ensure the page is fully loaded
    let goto_out = headless(&["browser", "goto", url, "-s", &session_id, "-t", "t0"], 30);
    assert_success(&goto_out, "goto url");

    (session_id, "t0".to_string())
}

fn close_session(session_id: &str) {
    let _ = headless(&["browser", "close", "-s", session_id], 15);
}

/// Verify that `browser snapshot --json` returns the new data shape:
/// `{format, content, nodes, stats}` — and does NOT contain the old `tree` field.
#[test]
fn contract_snapshot_json_data_shape() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session_at("https://example.com");

    let out = headless_json(&["browser", "snapshot", "-s", &sid, "-t", "t0"], 30);
    assert_success(&out, "snapshot --json");
    let json = parse_envelope(&out);

    assert_eq!(
        json["ok"], true,
        "ok must be true on success, got: {}",
        json
    );

    let data = &json["data"];

    // format field must equal "snapshot"
    assert_eq!(
        data["format"], "snapshot",
        "data.format must be 'snapshot', got: {}",
        data["format"]
    );

    // content field must be a string
    assert!(
        data.get("content").and_then(|v| v.as_str()).is_some(),
        "data.content must exist and be a string, got data: {}",
        data
    );

    // nodes field must be an array
    assert!(
        data.get("nodes").and_then(|v| v.as_array()).is_some(),
        "data.nodes must exist and be an array, got data: {}",
        data
    );

    // stats field must be an object
    assert!(
        data.get("stats").and_then(|v| v.as_object()).is_some(),
        "data.stats must exist and be an object, got data: {}",
        data
    );

    // stats.node_count must be a non-negative integer
    assert!(
        data["stats"]
            .get("node_count")
            .and_then(|v| v.as_u64())
            .is_some(),
        "data.stats.node_count must be present and a non-negative integer, got stats: {}",
        data["stats"]
    );

    // old "tree" field must NOT be present
    assert!(
        data.get("tree").is_none(),
        "data.tree must NOT be present (old field), got data: {}",
        data
    );

    close_session(&sid);
}

/// Verify that `browser snapshot --json` populates `context.url` and `context.title`
/// (both must be non-null, non-empty strings).
#[test]
fn contract_snapshot_context_url_title() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session_at("https://example.com");

    let out = headless_json(&["browser", "snapshot", "-s", &sid, "-t", "t0"], 30);
    assert_success(&out, "snapshot --json for context url/title");
    let json = parse_envelope(&out);

    assert_eq!(
        json["ok"], true,
        "ok must be true on success, got: {}",
        json
    );

    let context = &json["context"];

    // context.url must be a non-null, non-empty string
    let url = context
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "context.url must be a non-null string, got context: {}",
                context
            )
        });
    assert!(
        !url.is_empty(),
        "context.url must not be empty, got context: {}",
        context
    );

    // context.title must be a non-null string (may be empty for some pages, but must not be null)
    assert!(
        context.get("title").map(|v| !v.is_null()).unwrap_or(false),
        "context.title must be a non-null string, got context: {}",
        context
    );
    assert!(
        context.get("title").and_then(|v| v.as_str()).is_some(),
        "context.title must be a string (not null), got context: {}",
        context
    );

    close_session(&sid);
}

/// Verify that `browser local-storage list --json` returns
/// `command == "browser.local-storage.list"` (not the old "browser.storage.list").
#[test]
fn contract_local_storage_command_name() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session_at("https://example.com");

    let out = headless_json(
        &["browser", "local-storage", "list", "-s", &sid, "-t", "t0"],
        20,
    );
    assert_success(&out, "local-storage list --json");
    let json = parse_envelope(&out);

    assert_eq!(
        json["ok"], true,
        "ok must be true on success, got: {}",
        json
    );
    assert_eq!(
        json["command"], "browser.local-storage.list",
        "command must be 'browser.local-storage.list' (not 'browser.storage.list'), got: {}",
        json["command"]
    );

    close_session(&sid);
}

/// Verify that `browser session-storage list --json` returns
/// `command == "browser.session-storage.list"` (not the old "browser.storage.list").
#[test]
fn contract_session_storage_command_name() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, _tid) = start_session_at("https://example.com");

    let out = headless_json(
        &["browser", "session-storage", "list", "-s", &sid, "-t", "t0"],
        20,
    );
    assert_success(&out, "session-storage list --json");
    let json = parse_envelope(&out);

    assert_eq!(
        json["ok"], true,
        "ok must be true on success, got: {}",
        json
    );
    assert_eq!(
        json["command"], "browser.session-storage.list",
        "command must be 'browser.session-storage.list' (not 'browser.storage.list'), got: {}",
        json["command"]
    );

    close_session(&sid);
}
