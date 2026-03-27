//! Contract E2E tests for snapshot data shape, context url/title, and storage command naming.
//!
//! Covers fixes from PR #307:
//! 1. Snapshot data shape: `{format, content, nodes, stats}` (not `{format, tree}`)
//! 2. Snapshot `context.url` and `context.title` are populated (not null)
//! 3. Storage commands use `browser.local-storage.*` / `browser.session-storage.*` naming
//! 4. Storage JSON data uses PRD 14.1-14.5 shapes

use crate::harness::{assert_success, headless, headless_json, skip, stdout_str, SessionGuard};
use std::sync::atomic::{AtomicUsize, Ordering};

const STORAGE_TEST_URL: &str = "https://actionbook.dev/";
static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn parse_envelope(out: &std::process::Output) -> serde_json::Value {
    let text = stdout_str(out);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON envelope: {e}\nraw: {text}");
    })
}

/// Start a headless local session navigated to `url`, return session_id and tab_id.
fn start_session_at(url: &str) -> (String, String) {
    let pid = std::process::id();
    let mut last_out = None;
    for attempt in 0..3 {
        let suffix = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let session_id = format!("t72-storage-{pid}-{suffix}");
        let profile = format!("t72-storage-profile-{pid}-{suffix}");
        let out = headless_json(
            &[
                "browser",
                "start",
                "--mode",
                "local",
                "--headless",
                "--profile",
                &profile,
                "--set-session-id",
                &session_id,
                "--open-url",
                url,
            ],
            30,
        );
        if out.status.success() {
            let json = parse_envelope(&out);
            if json["context"]["session_id"].as_str() == Some(session_id.as_str()) {
                let goto_out =
                    headless(&["browser", "goto", url, "-s", &session_id, "-t", "t0"], 30);
                assert_success(&goto_out, "goto url");
                return (session_id, "t0".to_string());
            }
        }

        last_out = Some(out);
        if attempt < 2 {
            let _ = headless(&["daemon", "stop"], 10);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    let out = last_out.expect("at least one start attempt");
    assert_success(&out, "start session");
    let json = parse_envelope(&out);
    let session_id = json["context"]["session_id"]
        .as_str()
        .expect("session_id in start context")
        .to_string();

    let goto_out = headless(&["browser", "goto", url, "-s", &session_id, "-t", "t0"], 30);
    assert_success(&goto_out, "goto url");
    (session_id, "t0".to_string())
}

fn close_session(session_id: &str) {
    let _ = headless(&["browser", "close", "-s", session_id], 15);
}

fn set_storage(kind: &str, session_id: &str, tab_id: &str, key: &str, value: &str) {
    let out = headless_json(
        &[
            "browser", kind, "set", key, value, "-s", session_id, "-t", tab_id,
        ],
        20,
    );
    assert_success(&out, "storage set");
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

/// Verify that local-storage list/get return PRD 14.1 / 14.2 data shapes:
/// `{storage, items}` and `{storage, item}`.
#[test]
fn contract_local_storage_data_shapes() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session_at(STORAGE_TEST_URL);
    let key = "t72-local-key";
    let value = "t72-local-value";
    set_storage("local-storage", &sid, &tid, key, value);

    let list_out = headless_json(
        &["browser", "local-storage", "list", "-s", &sid, "-t", &tid],
        20,
    );
    assert_success(&list_out, "local-storage list --json");
    let list_json = parse_envelope(&list_out);
    assert_eq!(list_json["command"], "browser.local-storage.list");
    assert_eq!(list_json["data"]["storage"], "local");
    let items = list_json["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("data.items must be an array, got: {}", list_json["data"]));
    assert!(
        items
            .iter()
            .any(|item| item["key"] == key && item["value"] == value),
        "items must include the inserted key/value, got: {}",
        list_json["data"]
    );
    assert!(
        list_json["data"].get("keys").is_none(),
        "legacy keys field must be absent, got: {}",
        list_json["data"]
    );

    let get_out = headless_json(
        &[
            "browser",
            "local-storage",
            "get",
            key,
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        20,
    );
    assert_success(&get_out, "local-storage get --json");
    let get_json = parse_envelope(&get_out);
    assert_eq!(get_json["command"], "browser.local-storage.get");
    assert_eq!(
        get_json["data"],
        serde_json::json!({
            "storage": "local",
            "item": {
                "key": key,
                "value": value,
            }
        })
    );

    close_session(&sid);
}

/// Verify that session-storage set/delete/clear return PRD 14.3-14.5 data shapes:
/// `{storage, action, affected}`.
#[test]
fn contract_session_storage_action_shapes() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();
    let (sid, tid) = start_session_at(STORAGE_TEST_URL);
    let key = "t72-session-key";
    let value = "t72-session-value";

    let set_out = headless_json(
        &[
            "browser",
            "session-storage",
            "set",
            key,
            value,
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        20,
    );
    assert_success(&set_out, "session-storage set --json");
    let set_json = parse_envelope(&set_out);
    assert_eq!(
        set_json["data"],
        serde_json::json!({
            "storage": "session",
            "action": "set",
            "affected": 1,
        })
    );
    assert!(
        set_json["data"].get("set").is_none(),
        "legacy set field must be absent, got: {}",
        set_json["data"]
    );

    let delete_out = headless_json(
        &[
            "browser",
            "session-storage",
            "delete",
            key,
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        20,
    );
    assert_success(&delete_out, "session-storage delete --json");
    let delete_json = parse_envelope(&delete_out);
    assert_eq!(
        delete_json["data"],
        serde_json::json!({
            "storage": "session",
            "action": "delete",
            "affected": 1,
        })
    );
    assert!(
        delete_json["data"].get("deleted").is_none(),
        "legacy deleted field must be absent, got: {}",
        delete_json["data"]
    );

    set_storage("session-storage", &sid, &tid, key, value);
    let clear_out = headless_json(
        &[
            "browser",
            "session-storage",
            "clear",
            "-s",
            &sid,
            "-t",
            &tid,
        ],
        20,
    );
    assert_success(&clear_out, "session-storage clear --json");
    let clear_json = parse_envelope(&clear_out);
    assert_eq!(clear_json["data"]["storage"], "session");
    assert_eq!(clear_json["data"]["action"], "clear");
    assert!(
        clear_json["data"]["affected"].as_u64().unwrap_or(0) >= 1,
        "clear should report at least one affected key, got: {}",
        clear_json["data"]
    );
    assert!(
        clear_json["data"].get("cleared").is_none(),
        "legacy cleared field must be absent, got: {}",
        clear_json["data"]
    );

    close_session(&sid);
}
