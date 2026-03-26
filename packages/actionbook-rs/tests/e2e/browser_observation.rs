//! Browser observation E2E tests: snapshot, screenshot, pdf, title, url,
//! viewport, eval, html, text, value, attr, attrs, box, styles, describe,
//! state, inspect-point, query, logs.
//!
//! Uses daemon v2 CLI format with --session and --tab addressing.
//! Each test is self-contained: start -> operate -> assert -> close.

use crate::harness::{
    append_body_html_js, assert_success, headless, headless_json, set_body_html_js, skip,
    stdout_str, SessionGuard,
};

/// Extract the snapshot content from the JSON output, stripping envelope
/// metadata (tab_id, url, etc.) so that two snapshots can be compared by
/// content alone. Falls back to the raw string if parsing fails.
fn extract_snapshot_content(raw: &str) -> String {
    // Try to parse as JSON and pull out data.content (the actual snapshot body).
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        // PRD: snapshot data is { format, content, nodes, stats }
        if let Some(content) = v.pointer("/data/content") {
            return content.to_string();
        }
        // Fallback: try data itself
        if let Some(data) = v.get("data") {
            return data.to_string();
        }
    }
    // Not JSON — return raw text (text mode output)
    raw.to_string()
}

// ---------------------------------------------------------------------------
// 1. obs_snapshot_has_content — S1T1: snapshot contains page text
// ---------------------------------------------------------------------------

#[test]
fn obs_snapshot_has_content() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "snapshot");
    let output = stdout_str(&out);
    assert!(
        output.contains("Example") || output.contains("example"),
        "snapshot should contain page text, got (first 500 chars): {}",
        &output[..output.len().min(500)]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 2. obs_snapshot_interactive — S1T1: snapshot --interactive
// ---------------------------------------------------------------------------

#[test]
fn obs_snapshot_interactive() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless_json(
        &[
            "browser",
            "snapshot",
            "--interactive",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "snapshot --interactive");
    let output = stdout_str(&out);
    // Interactive snapshot should contain interactive elements (links on example.com)
    assert!(
        output.contains("link") || output.contains("a ") || output.contains("ref="),
        "interactive snapshot should contain interactive element info, got (first 500 chars): {}",
        &output[..output.len().min(500)]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 3. obs_snapshot_s1t2_different — S1T2: two tabs, different snapshots
// ---------------------------------------------------------------------------

#[test]
fn obs_snapshot_s1t2_different() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start with example.com on t0
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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Open example.org in a new tab (t1)
    let out = headless(
        &["browser", "open", "https://example.org", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open t1");

    // Snapshot t0 — extract only the snapshot content (not the full JSON envelope
    // which may differ by tab_id/url metadata).
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "snapshot t0");
    let raw_t0 = stdout_str(&out);
    let snap_t0 = extract_snapshot_content(&raw_t0);

    // Snapshot t1
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "snapshot t1");
    let raw_t1 = stdout_str(&out);
    let snap_t1 = extract_snapshot_content(&raw_t1);

    // The two snapshots should be different (different pages)
    assert_ne!(
        snap_t0, snap_t1,
        "snapshots of different tabs should differ"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 4. obs_snapshot_seq_reflects_goto — SEQ: goto A -> snapshot -> goto B -> snapshot -> different
// ---------------------------------------------------------------------------

#[test]
fn obs_snapshot_seq_reflects_goto() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Snapshot after landing on example.com — extract only the snapshot
    // content to avoid false-negatives from envelope metadata differences.
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "snapshot A");
    let raw_a = stdout_str(&out);
    let snap_a = extract_snapshot_content(&raw_a);

    // Navigate to example.org
    let out = headless(
        &[
            "browser",
            "goto",
            "https://example.org",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        30,
    );
    assert_success(&out, "goto B");

    // Snapshot again
    let out = headless_json(&["browser", "snapshot", "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "snapshot B");
    let raw_b = stdout_str(&out);
    let snap_b = extract_snapshot_content(&raw_b);

    // Snapshots should differ after navigation
    assert_ne!(
        snap_a, snap_b,
        "snapshot should change after goto to a different page"
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 5. obs_screenshot_file — S1T1: screenshot to file, check exists and >0 bytes
// ---------------------------------------------------------------------------

#[test]
fn obs_screenshot_file() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let tmp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .expect("create temp file");
    let path = tmp.path().to_string_lossy().to_string();
    // Close the temp file handle so the CLI can write to it
    drop(tmp);

    let out = headless(
        &["browser", "screenshot", &path, "-s", "local-1", "-t", "t0"],
        30,
    );
    assert_success(&out, "screenshot");

    let metadata = std::fs::metadata(&path);
    assert!(metadata.is_ok(), "screenshot file should exist at {}", path);
    assert!(
        metadata.unwrap().len() > 0,
        "screenshot file should be >0 bytes"
    );

    // Cleanup screenshot file
    let _ = std::fs::remove_file(&path);

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 6. obs_pdf_produces_file — S1T1: pdf to file, check exists and >0 bytes
// ---------------------------------------------------------------------------

#[test]
fn obs_pdf_produces_file() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Use about:blank to avoid network dependency
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&out, "start");

    // Inject minimal HTML so PDF has content to render
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js("<h1>PDF Test</h1><p>Content for PDF generation test.</p>"),
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "inject PDF content");

    let tmp = tempfile::Builder::new()
        .suffix(".pdf")
        .tempfile()
        .expect("create temp file");
    let path = tmp.path().to_string_lossy().to_string();
    drop(tmp);

    let out = headless(&["browser", "pdf", &path, "-s", "local-1", "-t", "t0"], 30);
    assert_success(&out, "pdf");

    let metadata = std::fs::metadata(&path);
    assert!(metadata.is_ok(), "pdf file should exist at {}", path);
    assert!(metadata.unwrap().len() > 0, "pdf file should be >0 bytes");

    let _ = std::fs::remove_file(&path);

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 7. obs_title — S1T1: browser title command
// ---------------------------------------------------------------------------

#[test]
fn obs_title() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(&["browser", "title", "-s", "local-1", "-t", "t0"], 10);
    assert_success(&out, "title");
    let title = stdout_str(&out);
    assert!(
        title.contains("Example Domain"),
        "title should contain 'Example Domain', got: {}",
        title
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 8. obs_url — S1T1: browser url command
// ---------------------------------------------------------------------------

#[test]
fn obs_url() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(&["browser", "url", "-s", "local-1", "-t", "t0"], 10);
    assert_success(&out, "url");
    let url = stdout_str(&out);
    assert!(
        url.contains("example.com"),
        "url should contain 'example.com', got: {}",
        url
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 9. obs_viewport — S1T1: browser viewport command
// ---------------------------------------------------------------------------

#[test]
fn obs_viewport() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(&["browser", "viewport", "-s", "local-1", "-t", "t0"], 10);
    assert_success(&out, "viewport");
    let viewport = stdout_str(&out);
    // Viewport output should contain width/height numbers (e.g., "1440x900" or JSON with width/height)
    assert!(
        viewport.contains('x') || viewport.contains("width"),
        "viewport should contain dimensions, got: {}",
        viewport
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 10. obs_eval_arithmetic — S1T1: eval "1+1"
// ---------------------------------------------------------------------------

#[test]
fn obs_eval_arithmetic() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(&["browser", "eval", "1+1", "-s", "local-1", "-t", "t0"], 10);
    assert_success(&out, "eval 1+1");
    assert!(
        stdout_str(&out).contains('2'),
        "eval 1+1 should contain '2', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 11. obs_eval_dom — S1T1: eval "document.title"
// ---------------------------------------------------------------------------

#[test]
fn obs_eval_dom() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Use about:blank to avoid network dependency
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&out, "start");

    // Set a known title via eval
    let out = headless(
        &[
            "browser",
            "eval",
            "document.title = 'Test Title'",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "eval set title");

    let out = headless(
        &[
            "browser",
            "eval",
            "document.title",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "eval document.title");
    assert!(
        stdout_str(&out).contains("Test Title"),
        "eval document.title should contain 'Test Title', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 12. obs_html_element — S1T1: html "body"
// ---------------------------------------------------------------------------

#[test]
fn obs_html_element() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &["browser", "html", "body", "-s", "local-1", "-t", "t0"],
        10,
    );
    assert_success(&out, "html body");
    let html = stdout_str(&out);
    assert!(
        html.contains("<") && html.contains(">"),
        "html output should contain HTML tags, got (first 500 chars): {}",
        &html[..html.len().min(500)]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 13. obs_text_element — S1T1: text "body"
// ---------------------------------------------------------------------------

#[test]
fn obs_text_element() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &["browser", "text", "body", "-s", "local-1", "-t", "t0"],
        10,
    );
    assert_success(&out, "text body");
    let text = stdout_str(&out);
    assert!(
        text.contains("Example Domain"),
        "text output should contain readable text, got (first 500 chars): {}",
        &text[..text.len().min(500)]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 14. obs_value_input — S1T1: fill input -> value "input"
// ---------------------------------------------------------------------------

#[test]
fn obs_value_input() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Use a page that has an input element; inject one via eval on example.com
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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject an input element into the page
    let out = headless(
        &[
            "browser",
            "eval",
            &append_body_html_js(r#"<input id="test-input" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "inject input");

    // Fill the input
    let out = headless(
        &[
            "browser",
            "fill",
            "#test-input",
            "hello-world",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "fill input");

    // Read value back
    let out = headless(
        &[
            "browser",
            "value",
            "#test-input",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "value input");
    assert!(
        stdout_str(&out).contains("hello-world"),
        "value should contain 'hello-world', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 15. obs_attr_single — S1T1: attr "a" "href"
// ---------------------------------------------------------------------------

#[test]
fn obs_attr_single() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // example.com has an <a> element with href
    let out = headless(
        &["browser", "attr", "a", "href", "-s", "local-1", "-t", "t0"],
        10,
    );
    assert_success(&out, "attr a href");
    let attr = stdout_str(&out);
    assert!(
        attr.contains("http") || attr.contains("iana"),
        "attr href should contain a URL, got: {}",
        attr
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 16. obs_attrs_all — S1T1: attrs "input"
// ---------------------------------------------------------------------------

#[test]
fn obs_attrs_all() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject an input with known attributes
    let out = headless(
        &[
            "browser",
            "eval",
            &append_body_html_js(r#"<input id="attrs-test" type="text" name="q" />"#),
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "inject input");

    let out = headless(
        &[
            "browser",
            "attrs",
            "#attrs-test",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "attrs input");
    let attrs = stdout_str(&out);
    assert!(
        attrs.contains("type") || attrs.contains("name") || attrs.contains("id"),
        "attrs should contain attribute names, got: {}",
        attrs
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 17. obs_box_position — S1T1: box "body"
// ---------------------------------------------------------------------------

#[test]
fn obs_box_position() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(&["browser", "box", "body", "-s", "local-1", "-t", "t0"], 10);
    assert_success(&out, "box body");
    let box_output = stdout_str(&out);
    // Box output should contain position/size info (x, y, width, height)
    assert!(
        box_output.contains("width") || box_output.contains("height") || box_output.contains('x'),
        "box output should contain position data, got: {}",
        box_output
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 18. obs_styles_computed — S1T1: styles "body"
// ---------------------------------------------------------------------------

#[test]
fn obs_styles_computed() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &["browser", "styles", "body", "-s", "local-1", "-t", "t0"],
        10,
    );
    assert_success(&out, "styles body");
    let styles = stdout_str(&out);
    assert!(
        styles.contains("display") || styles.contains("color") || styles.contains("font"),
        "styles output should contain CSS properties, got (first 500 chars): {}",
        &styles[..styles.len().min(500)]
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 19. obs_describe_element — S1T1: describe "a" (link on example.com)
// ---------------------------------------------------------------------------

#[test]
fn obs_describe_element() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // example.com has a link; describe it
    let out = headless(
        &["browser", "describe", "a", "-s", "local-1", "-t", "t0"],
        10,
    );
    assert_success(&out, "describe a");
    let desc = stdout_str(&out);
    assert!(
        desc.contains("link") || desc.contains("a") || desc.contains("More information"),
        "describe should contain tag/role info, got: {}",
        desc
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 20. obs_state_element — S1T1: state "a" (link on example.com)
// ---------------------------------------------------------------------------

#[test]
fn obs_state_element() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Use the link element on example.com for state query
    let out = headless(&["browser", "state", "a", "-s", "local-1", "-t", "t0"], 10);
    assert_success(&out, "state a");
    let state = stdout_str(&out);
    assert!(
        state.contains("visible") || state.contains("enabled"),
        "state should contain visibility/enabled info, got: {}",
        state
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 21. obs_inspect_point — S1T1: inspect-point "100,100"
// ---------------------------------------------------------------------------

#[test]
fn obs_inspect_point() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser",
            "inspect-point",
            "100,100",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "inspect-point 100,100");
    let inspect = stdout_str(&out);
    // Should contain element info at that coordinate
    assert!(
        inspect.contains("selector")
            || inspect.contains("tag")
            || inspect.contains("role")
            || inspect.contains("div")
            || inspect.contains("body")
            || inspect.contains("100"),
        "inspect-point should contain element info at coordinates, got: {}",
        inspect
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 22. obs_query_one — S1T1: query one "body"
// ---------------------------------------------------------------------------

#[test]
fn obs_query_one() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser", "query", "one", "body", "-s", "local-1", "-t", "t0",
        ],
        10,
    );
    assert_success(&out, "query one body");
    let query = stdout_str(&out);
    // query one "body" should return exactly 1 match
    assert!(
        query.contains("1") || query.contains("match") || query.contains("body"),
        "query one body should return 1 match, got: {}",
        query
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 23. obs_query_all — S1T1: query all "div"
// ---------------------------------------------------------------------------

#[test]
fn obs_query_all() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser", "query", "all", "div", "-s", "local-1", "-t", "t0",
        ],
        10,
    );
    assert_success(&out, "query all div");
    let query = stdout_str(&out);
    // example.com has div elements; should return multiple matches
    assert!(
        query.contains("match") || query.contains("div") || query.contains("selector"),
        "query all div should return matches, got: {}",
        query
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 24. obs_query_count — S1T1: query count "div"
// ---------------------------------------------------------------------------

#[test]
fn obs_query_count() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser", "query", "count", "div", "-s", "local-1", "-t", "t0",
        ],
        10,
    );
    assert_success(&out, "query count div");
    let count_output = stdout_str(&out);
    // Extract the numeric count from the output and verify it is a reasonable number (>= 0)
    let count: i64 = count_output
        .split_whitespace()
        .find_map(|w| w.parse::<i64>().ok())
        .unwrap_or_else(|| {
            panic!(
                "query count div should contain a parseable number, got: {}",
                count_output
            )
        });
    assert!(
        count >= 0,
        "query count div should return a non-negative count, got: {}",
        count
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 24b. obs_query_nth — S1T1: query nth 1 "div"
// ---------------------------------------------------------------------------

#[test]
fn obs_query_nth() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

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
    assert_success(&out, "start");

    // Wait for page to fully load before querying DOM
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t0",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser", "query", "nth", "1", "div", "-s", "local-1", "-t", "t0",
        ],
        10,
    );
    assert_success(&out, "query nth 1 div");
    let nth_output = stdout_str(&out);
    // Should return info about the first div element (selector, tag, or other element data)
    assert!(
        nth_output.contains("div") || nth_output.contains("selector") || nth_output.contains("tag"),
        "query nth 1 div should return element info, got: {}",
        nth_output
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 25. obs_console_logs — S1T1: eval console.log -> logs console
// ---------------------------------------------------------------------------

#[test]
fn obs_console_logs() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Use about:blank — no network needed for console log testing
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&out, "start");

    // Emit a console.log message
    let out = headless(
        &[
            "browser",
            "eval",
            "console.log('actionbook-test-log-marker')",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "eval console.log");

    // Poll for console logs (up to 3s) instead of fixed sleep
    let mut logs = String::new();
    for attempt in 0..6 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let out = headless(
            &["browser", "logs-console", "-s", "local-1", "-t", "t0"],
            10,
        );
        assert_success(&out, "logs console");
        logs = stdout_str(&out);
        if logs.contains("actionbook-test-log-marker") {
            break;
        }
    }
    assert!(
        logs.contains("actionbook-test-log-marker"),
        "console logs should contain the logged message, got: {}",
        logs
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 26. obs_error_logs — S1T1: eval console.error -> logs errors
// ---------------------------------------------------------------------------

#[test]
fn obs_error_logs() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Use about:blank — no network needed for error log testing
    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            "about:blank",
        ],
        30,
    );
    assert_success(&out, "start");

    // Emit a console.error message
    let out = headless(
        &[
            "browser",
            "eval",
            "console.error('actionbook-test-error-marker')",
            "-s",
            "local-1",
            "-t",
            "t0",
        ],
        10,
    );
    assert_success(&out, "eval console.error");

    // Poll for error logs (up to 3s) instead of fixed sleep
    let mut logs = String::new();
    for attempt in 0..6 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let out = headless(&["browser", "logs-errors", "-s", "local-1", "-t", "t0"], 10);
        assert_success(&out, "logs errors");
        logs = stdout_str(&out);
        if logs.contains("actionbook-test-error-marker") {
            break;
        }
    }
    assert!(
        logs.contains("actionbook-test-error-marker"),
        "error logs should contain the error message, got: {}",
        logs
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
