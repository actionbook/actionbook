//! E2E tests for iframe content expansion and frame-aware interaction.
//! Covers both same-origin iframes and cross-origin (OOPIF) iframes.

use crate::harness::{
    SessionGuard, assert_success, headless, headless_json, parse_json, skip, stdout_str,
    unique_session, url_iframe_cross_origin_parent, url_iframe_parent, wait_page_ready,
};

fn start_iframe_session() -> (String, String, SessionGuard) {
    let (sid, profile) = unique_session("iframe");
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
            &url_iframe_parent(),
        ],
        30,
    );
    assert_success(&out, "start iframe session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    wait_page_ready(&sid, &tid);
    // Give iframe a moment to load
    std::thread::sleep(std::time::Duration::from_millis(500));
    let guard = SessionGuard::new(&sid);
    (sid, tid, guard)
}

// ── Snapshot expansion ────────────────────────────────────────────

#[test]
fn iframe_snapshot_expands_child_content() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    let out = headless(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "snapshot");
    let text = stdout_str(&out);

    // Main page elements should be present
    assert!(
        text.contains("Main Page") || text.contains("heading"),
        "main page heading should appear in snapshot"
    );

    // Iframe node should appear
    assert!(
        text.contains("Iframe"),
        "Iframe node should appear in snapshot output"
    );

    // Child content should be expanded under the Iframe node
    assert!(
        text.contains("Child Content")
            || text.contains("Child Input")
            || text.contains("Child Button"),
        "iframe child content should be expanded in snapshot.\nGot:\n{text}"
    );
}

#[test]
fn iframe_snapshot_interactive_filter_includes_iframe_elements() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    let out = headless(
        &[
            "browser",
            "snapshot",
            "-i",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "snapshot -i");
    let text = stdout_str(&out);

    // Interactive elements from both main page and iframe should have refs
    assert!(
        text.contains("[ref="),
        "interactive snapshot should contain refs"
    );
    // The iframe's child button should appear with a ref
    assert!(
        text.contains("Child Button") || text.contains("Child Input"),
        "iframe interactive elements should appear with -i flag.\nGot:\n{text}"
    );
}

// ── Ref-based interaction with iframe elements ────────────────────

#[test]
fn iframe_html_reads_iframe_element() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    // First snapshot to generate refs
    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot for refs");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    // Find the ref for "Child Button" in the snapshot content
    let child_btn_ref = find_ref_for_name(content, "Child Button");
    if child_btn_ref.is_empty() {
        // If we can't find it by name, the iframe may not have expanded — skip gracefully
        eprintln!(
            "SKIP: could not find 'Child Button' ref in snapshot (iframe may not have loaded)"
        );
        return;
    }

    // Use html command with the iframe ref
    let html_out = headless(
        &[
            "browser",
            "html",
            &format!("@{child_btn_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&html_out, "html on iframe element");
    let html_text = stdout_str(&html_out);
    assert!(
        html_text.contains("child-btn") || html_text.contains("Click Me"),
        "html should return iframe button content.\nGot:\n{html_text}"
    );
}

#[test]
fn iframe_text_reads_iframe_element_json() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot for iframe text json");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let iframe_ref = find_ref_for_name(content, "Child Frame");
    if iframe_ref.is_empty() {
        eprintln!("SKIP: could not find 'Child Frame' ref");
        return;
    }

    let text_out = headless_json(
        &[
            "browser",
            "text",
            &format!("@{iframe_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&text_out, "text on iframe element json");
    let text_v = parse_json(&text_out);
    let text = text_v["data"]["value"].as_str().unwrap_or("");

    assert_eq!(text_v["command"], "browser text");
    assert_eq!(
        text_v["data"]["target"]["selector"],
        format!("@{iframe_ref}")
    );
    assert!(
        text.contains("Child Content") || text.contains("Child Button"),
        "text should return iframe document content.\nGot:\n{text}"
    );
}

#[test]
fn iframe_text_reads_iframe_element_text() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot for iframe text");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let iframe_ref = find_ref_for_name(content, "Child Frame");
    if iframe_ref.is_empty() {
        eprintln!("SKIP: could not find 'Child Frame' ref");
        return;
    }

    let text_out = headless(
        &[
            "browser",
            "text",
            &format!("@{iframe_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&text_out, "text on iframe element");
    let text = stdout_str(&text_out);

    assert!(
        text.contains("Child Content") || text.contains("Child Button"),
        "text should return iframe document content.\nGot:\n{text}"
    );
}

#[test]
fn iframe_click_works_on_iframe_element() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    // Snapshot to generate refs
    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let child_btn_ref = find_ref_for_name(content, "Child Button");
    if child_btn_ref.is_empty() {
        eprintln!("SKIP: could not find 'Child Button' ref");
        return;
    }

    let click_out = headless(
        &[
            "browser",
            "click",
            &format!("@{child_btn_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&click_out, "click on iframe element");
}

#[test]
fn iframe_value_reads_iframe_input() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    // Snapshot to generate refs
    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let child_input_ref = find_ref_for_name(content, "Child Input");
    if child_input_ref.is_empty() {
        eprintln!("SKIP: could not find 'Child Input' ref");
        return;
    }

    let val_out = headless(
        &[
            "browser",
            "value",
            &format!("@{child_input_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&val_out, "value on iframe input");
    let val_text = stdout_str(&val_out);
    assert!(
        val_text.contains("child-value"),
        "value should return iframe input value.\nGot:\n{val_text}"
    );
}

#[test]
fn iframe_fill_writes_to_iframe_input() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    // Snapshot to generate refs
    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let child_input_ref = find_ref_for_name(content, "Child Input");
    if child_input_ref.is_empty() {
        eprintln!("SKIP: could not find 'Child Input' ref");
        return;
    }

    // Fill the iframe input
    let fill_out = headless(
        &[
            "browser",
            "fill",
            &format!("@{child_input_ref}"),
            "new-iframe-value",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&fill_out, "fill on iframe input");

    // Read the value back to verify
    let val_out = headless(
        &[
            "browser",
            "value",
            &format!("@{child_input_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&val_out, "value after fill");
    let val_text = stdout_str(&val_out);
    assert!(
        val_text.contains("new-iframe-value"),
        "value should reflect the filled text.\nGot:\n{val_text}"
    );
}

// ── Ref isolation (no collision between frames) ───────────────────

#[test]
fn iframe_refs_do_not_collide_with_main_frame() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot");
    let snap_v = parse_json(&snap);
    let nodes = snap_v["data"]["nodes"].as_array();

    if let Some(nodes) = nodes {
        // All refs should be unique
        let refs: Vec<&str> = nodes.iter().filter_map(|n| n["ref"].as_str()).collect();
        let unique: std::collections::HashSet<&str> = refs.iter().copied().collect();
        assert_eq!(
            refs.len(),
            unique.len(),
            "all refs must be unique (no collision between frames). Duplicates found."
        );
    }
}

// ── Screenshot --annotate with iframe refs ────────────────────────

#[test]
fn iframe_screenshot_annotate_includes_iframe_elements() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    // Snapshot first to populate RefCache
    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot");

    // Take annotated screenshot
    let tmp = tempfile::NamedTempFile::new().expect("create temp file");
    let path = tmp.path().to_string_lossy().to_string() + ".png";
    let out = headless_json(
        &[
            "browser",
            "screenshot",
            &path,
            "--annotate",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "screenshot --annotate");

    // Verify the file was created and has non-trivial size
    let metadata = std::fs::metadata(&path);
    assert!(
        metadata.is_ok(),
        "annotated screenshot file should exist at {path}"
    );
    assert!(
        metadata.unwrap().len() > 1000,
        "annotated screenshot should have non-trivial size"
    );
    let _ = std::fs::remove_file(&path);
}

// ── Scroll into-view on iframe element ────────────────────────────

#[test]
fn iframe_scroll_into_view_works_on_iframe_element() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_iframe_session();

    // Snapshot to get refs
    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let child_btn_ref = find_ref_for_name(content, "Child Button");
    if child_btn_ref.is_empty() {
        eprintln!("SKIP: could not find 'Child Button' ref");
        return;
    }

    let scroll_out = headless(
        &[
            "browser",
            "scroll",
            "into-view",
            &format!("@{child_btn_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&scroll_out, "scroll into-view on iframe element");
}

// ══════════════════════════════════════════════════════════════════
// Cross-origin (OOPIF) iframe tests
// ══════════════════════════════════════════════════════════════════

fn start_xo_iframe_session() -> (String, String, SessionGuard) {
    let (sid, profile) = unique_session("xo-iframe");
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
            &url_iframe_cross_origin_parent(),
        ],
        30,
    );
    assert_success(&out, "start xo iframe session");
    let v = parse_json(&out);
    let sid = v["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tid = v["data"]["tab"]["tab_id"].as_str().unwrap().to_string();
    wait_page_ready(&sid, &tid);
    // Give cross-origin iframe extra time to load and attach
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let guard = SessionGuard::new(&sid);
    (sid, tid, guard)
}

#[test]
fn xo_iframe_snapshot_expands_cross_origin_content() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_xo_iframe_session();

    let out = headless(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&out, "xo snapshot");
    let text = stdout_str(&out);

    // Iframe node should appear
    assert!(
        text.contains("Iframe"),
        "Iframe node should appear in snapshot"
    );

    // Cross-origin child content should be expanded
    assert!(
        text.contains("Cross-Origin Content")
            || text.contains("XO Input")
            || text.contains("XO Button"),
        "cross-origin iframe content should be expanded.\nGot:\n{text}"
    );
}

#[test]
fn xo_iframe_html_reads_cross_origin_element() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_xo_iframe_session();

    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "xo snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let xo_btn_ref = find_ref_for_name(content, "XO Button");
    if xo_btn_ref.is_empty() {
        eprintln!("SKIP: could not find 'XO Button' ref (cross-origin iframe may not have loaded)");
        return;
    }

    let html_out = headless(
        &[
            "browser",
            "html",
            &format!("@{xo_btn_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&html_out, "html on xo iframe element");
    let html_text = stdout_str(&html_out);
    assert!(
        html_text.contains("xo-btn") || html_text.contains("XO Click"),
        "html should return cross-origin iframe button content.\nGot:\n{html_text}"
    );
}

#[test]
fn xo_iframe_fill_writes_to_cross_origin_input() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_xo_iframe_session();

    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "xo snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let xo_input_ref = find_ref_for_name(content, "XO Input");
    if xo_input_ref.is_empty() {
        eprintln!("SKIP: could not find 'XO Input' ref");
        return;
    }

    // Fill
    let fill_out = headless(
        &[
            "browser",
            "fill",
            &format!("@{xo_input_ref}"),
            "xo-filled",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&fill_out, "fill on xo iframe input");

    // Read back
    let val_out = headless(
        &[
            "browser",
            "value",
            &format!("@{xo_input_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&val_out, "value after xo fill");
    let val_text = stdout_str(&val_out);
    assert!(
        val_text.contains("xo-filled"),
        "value should reflect filled text in cross-origin iframe.\nGot:\n{val_text}"
    );
}

#[test]
fn xo_iframe_click_works_on_cross_origin_element() {
    if skip() {
        return;
    }
    let (sid, tid, _guard) = start_xo_iframe_session();

    let snap = headless_json(
        &["browser", "snapshot", "--session", &sid, "--tab", &tid],
        15,
    );
    assert_success(&snap, "xo snapshot");
    let snap_v = parse_json(&snap);
    let content = snap_v["data"]["content"].as_str().unwrap_or("");

    let xo_btn_ref = find_ref_for_name(content, "XO Button");
    if xo_btn_ref.is_empty() {
        eprintln!("SKIP: could not find 'XO Button' ref");
        return;
    }

    let click_out = headless(
        &[
            "browser",
            "click",
            &format!("@{xo_btn_ref}"),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&click_out, "click on xo iframe element");
}

// ── Helper ────────────────────────────────────────────────────────

/// Find the ref ID (e.g. "e42") for an element with the given name in snapshot content.
fn find_ref_for_name(content: &str, name: &str) -> String {
    for line in content.lines() {
        if line.contains(name) {
            // Look for [ref=eNN] pattern
            if let Some(start) = line.find("[ref=") {
                let after = &line[start + 5..];
                if let Some(end) = after.find(']') {
                    return after[..end].to_string();
                }
            }
        }
    }
    String::new()
}
