//! Browser batch-snapshot E2E tests: `browser batch-snapshot`.

use std::collections::HashSet;

use serde_json::Value;

use crate::harness::{
    SessionGuard, assert_context_object, assert_meta, assert_success, headless_json, new_tab_json,
    parse_json, skip, start_session, url_a, url_b, url_c, url_cursor_fixture, wait_page_ready,
};

fn batch_results(v: &Value) -> &[Value] {
    v["data"]["results"]
        .as_array()
        .expect("data.results must be an array")
}

fn batch_snapshot_content(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("batch snapshot path should be readable: {path} ({e})");
    })
}

fn assert_yaml_snapshot_artifact(entry: &Value) {
    let path = entry["path"].as_str().expect("path must be a string");
    assert!(
        path.ends_with(".yaml"),
        "batch snapshot artifact must use .yaml extension: {path}"
    );

    let content = batch_snapshot_content(path);
    // Trim leading whitespace/BOM — Windows may emit a UTF-8 BOM or CRLF prefix.
    let trimmed = content.trim_start();
    assert!(
        trimmed.starts_with("- "),
        "snapshot content must use YAML DSL list entries: {content}"
    );
    assert!(
        !content.contains(" url="),
        "snapshot content must not use legacy inline `url=` formatting: {content}"
    );
}

#[test]
fn batch_snapshot_json_multiple_tabs_emit_yaml_artifacts() {
    if skip() {
        return;
    }

    let (sid, t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let t2 = new_tab_json(&sid, &url_b());
    wait_page_ready(&sid, &t2);
    let t3 = new_tab_json(&sid, &url_c());
    wait_page_ready(&sid, &t3);
    let t4 = new_tab_json(&sid, &url_cursor_fixture());
    wait_page_ready(&sid, &t4);

    let args = vec![
        "browser",
        "batch-snapshot",
        "--session",
        sid.as_str(),
        "--tabs",
        t1.as_str(),
        t2.as_str(),
        t3.as_str(),
        t4.as_str(),
    ];
    let out = headless_json(&args, 60);
    assert_success(&out, "batch-snapshot 4 tabs");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser batch-snapshot");
    assert!(v["error"].is_null(), "error must be null on success");
    assert_context_object(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "tab_id must be null for batch command"
    );
    assert_meta(&v);

    assert_eq!(v["data"]["format"], "batch-snapshot");
    let results = batch_results(&v);
    assert_eq!(results.len(), 4, "expected 4 tab results");
    assert!(
        results.iter().all(|entry| entry["status"] == "ok"),
        "all tabs should succeed: {results:?}"
    );

    let tab_ids: Vec<&str> = results
        .iter()
        .map(|entry| entry["tab_id"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        tab_ids,
        vec![t1.as_str(), t2.as_str(), t3.as_str(), t4.as_str()]
    );

    let mut paths = HashSet::new();
    for entry in results {
        assert_yaml_snapshot_artifact(entry);
        assert!(entry["nodes"].is_array(), "nodes must be an array");
        assert!(entry["stats"].is_object(), "stats must be an object");

        let node_count = entry["stats"]["node_count"].as_u64().unwrap_or(0);
        let interactive_count = entry["stats"]["interactive_count"].as_u64().unwrap_or(0);
        let nodes_len = entry["nodes"].as_array().unwrap().len() as u64;

        assert!(node_count > 0, "node_count must be > 0: {entry:?}");
        assert_eq!(
            nodes_len, node_count,
            "nodes length should match node_count"
        );
        assert!(
            interactive_count <= node_count,
            "interactive_count must not exceed node_count: {entry:?}"
        );

        let path = entry["path"].as_str().unwrap().to_string();
        assert!(
            paths.insert(path.clone()),
            "each tab should produce a distinct artifact path, saw duplicate {path}"
        );
    }
}

#[test]
fn batch_snapshot_partial_failure_keeps_ok_envelope() {
    if skip() {
        return;
    }

    let (sid, t1) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let args = vec![
        "browser",
        "batch-snapshot",
        "--session",
        sid.as_str(),
        "--tabs",
        t1.as_str(),
        "t999",
    ];
    let out = headless_json(&args, 30);
    assert_success(&out, "batch-snapshot partial failure");
    let v = parse_json(&out);

    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["format"], "batch-snapshot");

    let results = batch_results(&v);
    assert_eq!(results.len(), 2);

    assert_eq!(results[0]["tab_id"], t1);
    assert_eq!(results[0]["status"], "ok");
    assert_yaml_snapshot_artifact(&results[0]);

    assert_eq!(results[1]["tab_id"], "t999");
    assert_eq!(results[1]["status"], "error");
    assert_eq!(results[1]["code"], "TAB_NOT_FOUND");
    assert!(
        results[1]["message"]
            .as_str()
            .unwrap_or("")
            .contains("t999"),
        "error message should mention the missing tab: {:?}",
        results[1]
    );
}
