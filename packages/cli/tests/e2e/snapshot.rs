//! Browser snapshot E2E tests: `browser.snapshot`.
//!
//! All tests are Tab-level: require `--session <SID> --tab <TID>`.
//! Tests are strict per api-reference.md §10.1.
//!
//! ## TDD status (current impl = raw CDP dump stub)
//!
//! **Expected to FAIL until implementation lands:**
//! - `snap_json_envelope` (context.url/title missing from stub)
//! - `snap_json_data_fields` (format/content/nodes/stats not in stub output)
//! - `snap_json_meta_truncated_false` (meta.truncated not emitted by stub)
//! - `snap_text_output` (content not formatted; [ref=eN] labels absent)
//! - `snap_text_no_extra_prefix` (stub may emit raw JSON, not text content)
//! - `snap_interactive_flag_reduces_nodes` (--interactive flag not wired)
//! - `snap_compact_flag_reduces_nodes` (--compact flag not wired)
//! - `snap_depth_flag_limits_nodes` (--depth flag not wired)
//! - `snap_selector_flag_limits_subtree` (--selector flag not wired)
//!
//! **Expected to PASS against stub (error paths handled before snapshot logic):**
//! - `snap_session_not_found_json` / `snap_session_not_found_text`
//! - `snap_tab_not_found_json` / `snap_tab_not_found_text`
//! - `snap_missing_session_arg` / `snap_missing_tab_arg`

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, url_a, url_cursor_fixture, wait_page_ready,
};

const URL_A: &str = "https://actionbook.dev";

// ── Helpers ───────────────────────────────────────────────────────────

/// Assert full §2.4 meta structure.
fn assert_meta(v: &serde_json::Value) {
    assert!(
        v["meta"]["duration_ms"].is_number(),
        "meta.duration_ms must be a number"
    );
    assert!(
        v["meta"]["warnings"].is_array(),
        "meta.warnings must be an array"
    );
    assert!(
        v["meta"]["pagination"].is_null(),
        "meta.pagination must be null"
    );
    assert!(
        v["meta"]["truncated"].is_boolean(),
        "meta.truncated must be a boolean"
    );
}

/// Assert full §3.1 error envelope.
fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false, "ok must be false on error");
    assert!(v["data"].is_null(), "data must be null on failure");
    assert_eq!(v["error"]["code"], expected_code);
    assert!(
        v["error"]["message"].is_string(),
        "error.message must be a string"
    );
    assert!(
        v["error"]["retryable"].is_boolean(),
        "error.retryable must be a boolean"
    );
    assert!(
        v["error"]["details"].is_object() || v["error"]["details"].is_null(),
        "error.details must be object or null"
    );
    assert_meta(v);
}

/// Assert §10.1 snapshot data fields.
fn assert_snapshot_data(v: &serde_json::Value) {
    let data = &v["data"];
    assert_eq!(data["format"], "snapshot", "data.format must be 'snapshot'");
    assert!(data["path"].is_string(), "data.path must be a string");
    assert!(
        !data["path"].as_str().unwrap_or("").is_empty(),
        "data.path must not be empty"
    );
    assert!(
        data["path"].as_str().unwrap_or("").ends_with(".yaml"),
        "data.path must point to a .yaml snapshot artifact"
    );
    assert!(data["nodes"].is_array(), "data.nodes must be an array");
    assert!(
        !data["nodes"].as_array().unwrap().is_empty(),
        "data.nodes must not be empty"
    );
    // Each node must have ref, role, name
    for node in data["nodes"].as_array().unwrap() {
        assert!(
            node["ref"].is_string(),
            "node.ref must be a string, got: {node:?}"
        );
        assert!(
            node["role"].is_string(),
            "node.role must be a string, got: {node:?}"
        );
        assert!(
            node["name"].is_string(),
            "node.name must be a string (may be empty), got: {node:?}"
        );
    }
    // stats
    assert!(
        data["stats"]["node_count"].is_number(),
        "data.stats.node_count must be a number"
    );
    assert!(
        data["stats"]["interactive_count"].is_number(),
        "data.stats.interactive_count must be a number"
    );
    let node_count = data["stats"]["node_count"].as_u64().unwrap_or(0);
    assert!(
        node_count > 0,
        "data.stats.node_count must be > 0 for a loaded page"
    );
}

fn snapshot_path(v: &serde_json::Value) -> &str {
    v["data"]["path"].as_str().unwrap_or("")
}

fn snapshot_content(v: &serde_json::Value) -> String {
    let path = snapshot_path(v);
    assert!(
        path.ends_with(".yaml"),
        "snapshot path must use .yaml extension: {path}"
    );
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("snapshot path should be readable: {path} ({e})");
    })
}

/// Assert saved snapshot file uses the YAML DSL shape and contains refs.
fn assert_content_is_yaml_snapshot(v: &serde_json::Value) {
    let content = snapshot_content(v);
    assert!(
        content.contains("[ref="),
        "snapshot file must contain [ref=eN] labels, got: {content:.100}"
    );
    assert!(
        !content.contains(" url="),
        "snapshot file must not use the legacy inline `url=` text format: {content:.200}"
    );
    assert!(
        content.lines().any(|line| line.ends_with(':')),
        "snapshot file must contain at least one YAML-style container line ending with ':': {content:.200}"
    );
}

/// Start a headless session, return (session_id, tab_id).
fn start_session(url: &str) -> (String, String) {
    let (sid, profile) = unique_session("s");
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--set-session-id",
            &sid,
            "--profile",
            &profile,
            "--open-url",
            url,
        ],
        30,
    );
    assert_success(&out, "start session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    wait_page_ready(&sid, &tid);
    (sid, tid)
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

// ===========================================================================
// Group 1: snapshot — Happy Path JSON (§10.1)
// ===========================================================================

#[test]
fn snap_json_envelope() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser snapshot");
    assert!(v["error"].is_null());

    // context — tab-level
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be present"
    );
    // URL must match the actual page, not chrome://newtab/ or stale values
    let ctx_url = v["context"]["url"].as_str().unwrap_or("");
    assert!(
        ctx_url.contains("actionbook.dev"),
        "context.url must reflect actual page URL, got: {ctx_url}"
    );
    assert!(
        v["context"]["title"].is_string(),
        "context.title must be present"
    );
    let ctx_title = v["context"]["title"].as_str().unwrap_or("");
    assert!(
        !ctx_title.is_empty() && ctx_title != "New Tab",
        "context.title must reflect actual page title, got: {ctx_title}"
    );

    assert_meta(&v);

    close_session(&sid);
}

#[test]
fn snap_json_data_fields() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot data fields");
    let v = parse_json(&out);

    // §10.1 data contract
    assert_snapshot_data(&v);
    assert_content_is_yaml_snapshot(&v);

    // §10.1 strict: data must only contain {format, path, nodes, stats}
    // No internal fields (__ctx_*, snapshot, etc.) should leak into public data
    let data_keys: Vec<&str> = v["data"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    let allowed = ["format", "path", "nodes", "stats"];
    for key in &data_keys {
        assert!(
            allowed.contains(key),
            "data must only contain §10.1 fields, found unexpected key: '{key}'"
        );
    }
    for key in &allowed {
        assert!(
            data_keys.contains(key),
            "data must contain §10.1 field: '{key}'"
        );
    }

    close_session(&sid);
}

#[test]
fn snap_json_meta_truncated_false() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot meta.truncated");
    let v = parse_json(&out);

    // For a normal page, truncated should be false
    assert_eq!(
        v["meta"]["truncated"], false,
        "meta.truncated must be false for a non-truncated snapshot"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 2: snapshot — Text Output (§2.5)
// ===========================================================================

#[test]
fn snap_text_output() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot text");
    let text = stdout_str(&out);

    // §2.5: prefix is `[sid tid] url`
    assert!(
        text.contains(&format!("[{sid} {tid}]")),
        "text must contain [session_id tab_id]: got {text:.200}"
    );
    // URL must appear on the first line prefix
    assert!(
        text.contains("actionbook.dev") || text.contains("https://"),
        "text prefix must include the tab URL: got {text:.200}"
    );

    // Body: snapshot content directly — must contain [ref=eN] labels
    assert!(
        text.contains("[ref="),
        "text output must contain [ref=eN] labels: got {text:.200}"
    );

    // Must NOT contain "ok browser snapshot" (observation commands don't use action format)
    assert!(
        !text.contains("ok browser snapshot"),
        "text output must not contain 'ok browser snapshot' — content is output directly"
    );

    close_session(&sid);
}

#[test]
fn snap_text_no_extra_prefix() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot text no prefix");
    let text = stdout_str(&out);

    // Must NOT have descriptive prefix like "Snapshot:", "snapshot:", etc.
    assert!(
        !text.contains("Snapshot:"),
        "must not contain 'Snapshot:' prefix"
    );
    assert!(
        !text.contains("snapshot:"),
        "must not contain 'snapshot:' prefix"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 3: snapshot — Optional Flags (§10.1)
// ===========================================================================

#[test]
fn snap_interactive_flag_reduces_nodes() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Full snapshot
    let out_full = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let v_full = parse_json(&out_full);
    let full_count = v_full["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // Interactive-only snapshot
    let out_interactive = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--interactive",
        ],
        30,
    );
    assert_success(&out_interactive, "snapshot interactive");
    let v_interactive = parse_json(&out_interactive);
    assert_snapshot_data(&v_interactive);

    let interactive_count = v_interactive["data"]["stats"]["node_count"]
        .as_u64()
        .unwrap_or(0);

    // Interactive mode must return fewer or equal nodes than full snapshot
    assert!(
        interactive_count <= full_count,
        "interactive snapshot must have <= nodes than full snapshot: {interactive_count} > {full_count}"
    );

    // All nodes in interactive snapshot must be interactive
    // (node_count should equal interactive_count)
    let interactive_interactive_count = v_interactive["data"]["stats"]["interactive_count"]
        .as_u64()
        .unwrap_or(0);
    assert_eq!(
        interactive_count, interactive_interactive_count,
        "in --interactive mode, node_count must equal interactive_count"
    );

    close_session(&sid);
}

#[test]
fn snap_compact_flag_reduces_nodes() {
    if skip() {
        return;
    }
    // Use a local static page so both snapshot calls see identical content —
    // `actionbook.dev` loads dynamic content between calls, causing the
    // compact snapshot to occasionally have MORE nodes than the full one.
    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    // Full snapshot
    let out_full = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let v_full = parse_json(&out_full);
    let full_count = v_full["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // Compact snapshot
    let out_compact = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--compact",
        ],
        30,
    );
    assert_success(&out_compact, "snapshot compact");
    let v_compact = parse_json(&out_compact);
    assert_snapshot_data(&v_compact);

    let compact_count = v_compact["data"]["stats"]["node_count"]
        .as_u64()
        .unwrap_or(0);

    // Compact must remove empty structural nodes — fewer or equal nodes
    assert!(
        compact_count <= full_count,
        "--compact snapshot must have <= nodes than full: {compact_count} > {full_count}"
    );

    close_session(&sid);
}

#[test]
fn snap_depth_flag_limits_nodes() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Full snapshot
    let out_full = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let v_full = parse_json(&out_full);
    let full_count = v_full["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // Depth-limited snapshot (depth=2 — enough to include some nodes but cut deep ones)
    let out_depth = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--depth",
            "2",
        ],
        30,
    );
    assert_success(&out_depth, "snapshot depth 2");
    let v_depth = parse_json(&out_depth);

    let depth_count = v_depth["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // depth=2 must return fewer or equal nodes than full tree
    assert!(
        depth_count <= full_count,
        "--depth 2 must return <= nodes than full snapshot: {depth_count} > {full_count}"
    );

    close_session(&sid);
}

#[test]
fn snap_selector_flag_limits_subtree() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Full snapshot for baseline
    let out_full = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let v_full = parse_json(&out_full);
    let full_count = v_full["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // Selector-limited snapshot (body = top-level container)
    let out_sel = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--selector",
            "body",
        ],
        30,
    );
    assert_success(&out_sel, "snapshot selector body");
    let v_sel = parse_json(&out_sel);

    let sel_count = v_sel["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // Selector snapshot should return roughly the same or fewer nodes.
    // Allow a small tolerance (±2) because the AX tree transform pipeline
    // may include/exclude boundary nodes differently for scoped vs full snapshots.
    assert!(
        sel_count <= full_count + 2,
        "--selector body must return approximately <= nodes than full snapshot: {sel_count} vs {full_count}"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 3b: snapshot — Content Format Validation (§10.1)
// ===========================================================================

#[test]
fn snap_json_ref_starts_from_e1() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot ref e1");
    let v = parse_json(&out);

    // §10.1: refs start from e1 (1-based), never e0
    let content = snapshot_content(&v);
    assert!(
        content.contains("[ref=e1]"),
        "content must contain [ref=e1] (1-based ref): got {content:.200}"
    );
    assert!(
        !content.contains("[ref=e0]"),
        "content must NOT contain [ref=e0] (0-based): got {content:.200}"
    );

    // Verify nodes array also uses e1+
    let empty = vec![];
    let nodes = v["data"]["nodes"].as_array().unwrap_or(&empty);
    if let Some(first) = nodes.first() {
        let first_ref = first["ref"].as_str().unwrap_or("");
        assert!(
            first_ref.starts_with("e") && first_ref != "e0",
            "first node ref must be e1+, got: {first_ref}"
        );
    }

    close_session(&sid);
}

#[test]
fn snap_json_content_no_noise_roles() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot no noise");
    let v = parse_json(&out);

    let content = snapshot_content(&v);

    // Noise roles must be filtered out of content per snapshot algorithm.
    // Note: StaticText is NOT noise — it carries visible text content.
    let noise_roles = ["RootWebArea", "InlineTextBox", "LineBreak", "ListMarker"];
    for role in &noise_roles {
        // Check that these roles don't appear as `- RoleName` in the content lines
        let pattern = format!("- {role} ");
        assert!(
            !content.contains(&pattern),
            "content must not contain noise role '{role}': got {content:.300}"
        );
    }

    close_session(&sid);
}

#[test]
fn snap_json_nodes_have_required_fields() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "snapshot node fields");
    let v = parse_json(&out);

    // §10.1: each node in data.nodes must have ref, role, name, value
    let nodes = v["data"]["nodes"].as_array().expect("nodes must be array");
    assert!(!nodes.is_empty(), "nodes must not be empty");
    for (i, node) in nodes.iter().enumerate() {
        assert!(
            node["ref"].is_string(),
            "nodes[{i}].ref must be string: {node:?}"
        );
        assert!(
            node["role"].is_string(),
            "nodes[{i}].role must be string: {node:?}"
        );
        assert!(
            node["name"].is_string(),
            "nodes[{i}].name must be string: {node:?}"
        );
        assert!(
            node["value"].is_string(),
            "nodes[{i}].value must be string (may be empty): {node:?}"
        );

        // ref format: eN where N >= 1
        let ref_val = node["ref"].as_str().unwrap();
        assert!(
            ref_val.starts_with('e'),
            "nodes[{i}].ref must start with 'e': {ref_val}"
        );
        let num: Result<u32, _> = ref_val[1..].parse();
        assert!(
            num.is_ok() && num.unwrap() >= 1,
            "nodes[{i}].ref must be e1+: {ref_val}"
        );
    }

    close_session(&sid);
}

#[test]
fn snap_interactive_compact_combined() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Full snapshot for baseline
    let out_full = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out_full, "snapshot full");
    let v_full = parse_json(&out_full);
    let full_count = v_full["data"]["stats"]["node_count"].as_u64().unwrap_or(0);

    // Combined --interactive --compact
    let out_combined = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--interactive",
            "--compact",
        ],
        30,
    );
    assert_success(&out_combined, "snapshot interactive+compact");
    let v_combined = parse_json(&out_combined);
    assert_snapshot_data(&v_combined);

    let combined_count = v_combined["data"]["stats"]["node_count"]
        .as_u64()
        .unwrap_or(0);

    // Combined must be <= either filter alone, and <= full
    assert!(
        combined_count <= full_count,
        "--interactive --compact must return <= full: {combined_count} > {full_count}"
    );

    // All nodes must be interactive
    let interactive_count = v_combined["data"]["stats"]["interactive_count"]
        .as_u64()
        .unwrap_or(0);
    assert_eq!(
        combined_count, interactive_count,
        "combined mode: node_count must equal interactive_count"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 3c: snapshot — --cursor flag (cursor-interactive detection)
// ===========================================================================

#[test]
fn snap_cursor_flag_increases_refs() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Default snapshot
    let out_default = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out_default, "snapshot default");
    let v_default = parse_json(&out_default);
    let default_count = v_default["data"]["stats"]["node_count"]
        .as_u64()
        .unwrap_or(0);

    // With --cursor: should detect additional cursor-interactive elements
    let out_cursor = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--cursor",
        ],
        30,
    );
    assert_success(&out_cursor, "snapshot cursor");
    let v_cursor = parse_json(&out_cursor);
    let cursor_count = v_cursor["data"]["stats"]["node_count"]
        .as_u64()
        .unwrap_or(0);

    // --cursor must return >= refs than default (adds cursor-interactive on top)
    assert!(
        cursor_count >= default_count,
        "--cursor must return >= refs than default: {cursor_count} < {default_count}"
    );

    close_session(&sid);
}

#[test]
fn snap_cursor_content_has_clickable() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_cursor_fixture());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--cursor",
        ],
        30,
    );
    assert_success(&out, "snapshot cursor content");
    let v = parse_json(&out);

    let generic_count = v["data"]["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter(|node| node["role"].as_str() == Some("generic"))
                .count()
        })
        .unwrap_or(0);
    assert!(
        v["data"]["stats"]["interactive_count"]
            .as_u64()
            .unwrap_or(0)
            >= 3,
        "cursor fixture should surface 3 interactive cursor nodes in stats"
    );
    assert!(
        generic_count >= 3,
        "cursor fixture should surface the clickable divs as generic nodes"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 4: snapshot — Error Paths (§3.1)
// ===========================================================================

#[test]
fn snap_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "snapshot nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser snapshot");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    // §3.1: context must be null when session not found
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn snap_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "snapshot",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "snapshot nonexistent session text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain error SESSION_NOT_FOUND: got {text}"
    );
}

#[test]
fn snap_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "snapshot nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser snapshot");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    // §3.1: TAB_NOT_FOUND — context has session_id but tab_id must be absent/null
    assert!(
        v["context"].is_object(),
        "context must be present when session found"
    );
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "context.tab_id must be null when tab not found"
    );

    close_session(&sid);
}

#[test]
fn snap_tab_not_found_text() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &[
            "browser",
            "snapshot",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "snapshot nonexistent tab text");
    let text = stdout_str(&out);
    assert!(
        text.contains("error TAB_NOT_FOUND:"),
        "text must contain error TAB_NOT_FOUND: got {text}"
    );

    close_session(&sid);
}

// ===========================================================================
// Group 5: Missing Args (§10.1 — --session and --tab required)
// ===========================================================================

#[test]
fn snap_missing_session_arg() {
    if skip() {
        return;
    }

    // Missing --session
    let out = headless_json(&["browser", "snapshot", "--tab", "some-tab"], 10);
    assert_failure(&out, "snapshot missing --session");
}

#[test]
fn snap_missing_tab_arg() {
    if skip() {
        return;
    }

    // Missing --tab
    let out = headless_json(&["browser", "snapshot", "--session", "some-session"], 10);
    assert_failure(&out, "snapshot missing --tab");
}

// ===========================================================================
// Group 6: snapshot — --cursor default on (§10.1)
// ===========================================================================

/// Verify that the default `browser snapshot` (no flags) includes cursor-interactive
/// elements. Uses a deterministic local fixture with a known `cursor:pointer` div,
/// an `onclick` div, and a `tabindex` div — elements that only appear when cursor
/// detection is active.
///
/// **Expected to FAIL until #212 implementation lands** (cursor default = true).
#[test]
fn snap_cursor_on_by_default() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(&url_cursor_fixture());
    let _guard = SessionGuard::new(&sid);

    // Default snapshot — no flags. After #212, cursor is on by default.
    let out = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&out, "cursor on by default");
    let v = parse_json(&out);
    assert_snapshot_data(&v);

    let generic_count = v["data"]["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter(|node| node["role"].as_str() == Some("generic"))
                .count()
        })
        .unwrap_or(0);
    assert!(
        v["data"]["stats"]["interactive_count"]
            .as_u64()
            .unwrap_or(0)
            >= 3,
        "default snapshot should include cursor-interactive nodes in stats"
    );
    assert!(
        generic_count >= 3,
        "default snapshot should include the cursor fixture divs in data.nodes"
    );

    close_session(&sid);
}
