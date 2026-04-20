//! E2E tests for browser title / url / viewport commands.
//!
//! All three commands are Tab-level: require `--session <SID> --tab <TID>`.
//! Tests are strict per api-reference.md §10.4 / §10.5 / §10.6.
//!
//! All 21 tests must pass against the production implementation.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stderr_str, stdout_str, unique_session, wait_page_ready,
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

/// Start a headless session and navigate to url, return (session_id, tab_id).
///
/// Uses an explicit `browser goto` after `browser start` so the tab has a
/// confirmed real URL in CI environments where `--open-url` may resolve to
/// `chrome-error://chromewebdata/` on network hiccups.
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
    // --open-url starts loading but does not block until load completes.
    // Explicit goto ensures the page is fully loaded before tests query the URL.
    let goto_out = headless_json(
        &["browser", "goto", url, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&goto_out, "goto after start");
    wait_page_ready(&sid, &tid);
    (sid, tid)
}

/// Close a session.
fn close_session(session_id: &str) {
    let out = headless(&["browser", "close", "--session", session_id], 30);
    assert_success(&out, &format!("close {session_id}"));
}

// ===========================================================================
// Group 1: browser title (§10.4)
// ===========================================================================

#[test]
fn title_json_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "title", "--session", &sid, "--tab", &tid], 10);
    assert_success(&out, "title json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser title");
    assert!(v["error"].is_null());
    assert_meta(&v);

    // context — tab-level, including url per §2.5
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be a string"
    );
    assert!(
        v["context"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "context.url must reference actionbook.dev: got {:?}",
        v["context"]["url"]
    );

    // §10.4 data contract
    assert!(
        v["data"]["value"].is_string(),
        "data.value must be a string"
    );
    assert!(
        !v["data"]["value"].as_str().unwrap_or("").is_empty(),
        "data.value (title) must not be empty for actionbook.dev"
    );

    close_session(&sid);
}

#[test]
fn title_text_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Get expected title from JSON to pin text body precisely (B2)
    let json_out = headless_json(&["browser", "title", "--session", &sid, "--tab", &tid], 10);
    assert_success(&json_out, "title json (for text comparison)");
    let jv = parse_json(&json_out);
    let expected_title = jv["data"]["value"].as_str().unwrap_or("").to_string();
    assert!(
        !expected_title.is_empty(),
        "title must not be empty for actionbook.dev"
    );

    let out = headless(&["browser", "title", "--session", &sid, "--tab", &tid], 10);
    assert_success(&out, "title text");
    let text = stdout_str(&out);

    // §2.5: header is `[sid tid] <url>` — must include URL
    let header_line = text.lines().next().unwrap_or("");
    assert!(
        header_line.starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: got {header_line}"
    );
    assert!(
        header_line.contains("actionbook.dev"),
        "header must contain URL per §2.5: got {header_line}"
    );

    // §10.4: body is raw title value — no "title: " or "value: " prefix
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.len() >= 2,
        "text must have header + body lines: got {text:.200}"
    );
    assert_eq!(
        lines[1].trim(),
        expected_title,
        "text body must be exactly the title value (no 'title: ' prefix)"
    );

    close_session(&sid);
}

#[test]
fn title_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "title",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "title nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser title");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    // §3.1: context must be null when session not found
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn title_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "title",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "title nonexistent session text");
    let text = stderr_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain 'error SESSION_NOT_FOUND:' got {text}"
    );
}

#[test]
fn title_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "title",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "title nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser title");
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
fn title_missing_session_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "title", "--tab", "any-tab"], 10);
    assert_failure(&out, "title missing --session");
}

#[test]
fn title_missing_tab_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "title", "--session", "any-sid"], 10);
    assert_failure(&out, "title missing --tab");
}

// ===========================================================================
// Group 2: browser url (§10.5)
// ===========================================================================

#[test]
fn url_json_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(&["browser", "url", "--session", &sid, "--tab", &tid], 10);
    assert_success(&out, "url json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser url");
    assert!(v["error"].is_null());
    assert_meta(&v);

    // context — tab-level, including url per §2.5
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be a string"
    );
    assert!(
        v["context"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "context.url must reference actionbook.dev: got {:?}",
        v["context"]["url"]
    );

    // §10.5 data contract
    assert!(
        v["data"]["value"].is_string(),
        "data.value must be a string"
    );
    let url_val = v["data"]["value"].as_str().unwrap_or("");
    assert!(
        url_val.starts_with("http"),
        "data.value must be a URL starting with http: got {url_val}"
    );
    assert!(
        url_val.contains("actionbook.dev"),
        "data.value must contain actionbook.dev: got {url_val}"
    );

    close_session(&sid);
}

#[test]
fn url_text_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    // Get expected URL from JSON to pin text body precisely (B2)
    let json_out = headless_json(&["browser", "url", "--session", &sid, "--tab", &tid], 10);
    assert_success(&json_out, "url json (for text comparison)");
    let jv = parse_json(&json_out);
    let expected_url = jv["data"]["value"].as_str().unwrap_or("").to_string();
    assert!(
        expected_url.starts_with("http"),
        "data.value must be a URL: got {expected_url}"
    );

    let out = headless(&["browser", "url", "--session", &sid, "--tab", &tid], 10);
    assert_success(&out, "url text");
    let text = stdout_str(&out);

    // §2.5: header is `[sid tid] <url>` — must include URL
    let header_line = text.lines().next().unwrap_or("");
    assert!(
        header_line.starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: got {header_line}"
    );
    assert!(
        header_line.contains("actionbook.dev"),
        "header must contain URL per §2.5: got {header_line}"
    );

    // §10.5: body is raw URL value — no "url: " or "value: " prefix
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.len() >= 2,
        "text must have header + body lines: got {text:.200}"
    );
    assert_eq!(
        lines[1].trim(),
        expected_url,
        "text body must be exactly the URL value (no 'url: ' prefix)"
    );

    close_session(&sid);
}

#[test]
fn url_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "url",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "url nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser url");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn url_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "url",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "url nonexistent session text");
    let text = stderr_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain 'error SESSION_NOT_FOUND:' got {text}"
    );
}

#[test]
fn url_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "url",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "url nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser url");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "context.tab_id must be null when tab not found"
    );

    close_session(&sid);
}

#[test]
fn url_missing_session_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "url", "--tab", "any-tab"], 10);
    assert_failure(&out, "url missing --session");
}

#[test]
fn url_missing_tab_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "url", "--session", "any-sid"], 10);
    assert_failure(&out, "url missing --tab");
}

// ===========================================================================
// Group 3: browser viewport (§10.6)
// ===========================================================================

#[test]
fn viewport_json_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &["browser", "viewport", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "viewport json");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "browser viewport");
    assert!(v["error"].is_null());
    assert_meta(&v);

    // context — tab-level, including url per §2.5
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(
        v["context"]["url"].is_string(),
        "context.url must be a string"
    );
    assert!(
        v["context"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("actionbook.dev"),
        "context.url must reference actionbook.dev: got {:?}",
        v["context"]["url"]
    );

    // §10.6 data contract
    assert!(
        v["data"]["width"].is_number(),
        "data.width must be a number"
    );
    assert!(
        v["data"]["height"].is_number(),
        "data.height must be a number"
    );
    let width = v["data"]["width"].as_u64().unwrap_or(0);
    let height = v["data"]["height"].as_u64().unwrap_or(0);
    assert!(width > 0, "data.width must be > 0, got {width}");
    assert!(height > 0, "data.height must be > 0, got {height}");

    close_session(&sid);
}

#[test]
fn viewport_text_happy_path() {
    if skip() {
        return;
    }
    let (sid, tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless(
        &["browser", "viewport", "--session", &sid, "--tab", &tid],
        10,
    );
    assert_success(&out, "viewport text");
    let text = stdout_str(&out);

    // §2.5: header is `[sid tid] <url>` — must include URL
    let header_line = text.lines().next().unwrap_or("");
    assert!(
        header_line.starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session_id tab_id]: got {header_line}"
    );
    assert!(
        header_line.contains("actionbook.dev"),
        "header must contain URL per §2.5: got {header_line}"
    );

    // Body: `{width}x{height}` format
    assert!(
        text.contains('x'),
        "text body must contain 'x' separator (e.g. 1280x800): got {text:.200}"
    );
    // Must match WxH pattern — at least one digit, 'x', at least one digit
    let body_line = text
        .lines()
        .find(|l| l.contains('x') && !l.contains('['))
        .unwrap_or("");
    let parts: Vec<&str> = body_line.trim().splitn(2, 'x').collect();
    assert_eq!(
        parts.len(),
        2,
        "viewport text body must be WxH format: got {body_line}"
    );
    assert!(
        parts[0].trim().parse::<u64>().is_ok(),
        "width part must be numeric: got '{}'",
        parts[0]
    );
    assert!(
        parts[1].trim().parse::<u64>().is_ok(),
        "height part must be numeric: got '{}'",
        parts[1]
    );

    assert!(
        !text.contains("ok browser viewport"),
        "text must not contain 'ok browser viewport'"
    );

    close_session(&sid);
}

#[test]
fn viewport_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "viewport",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "viewport nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser viewport");
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
    assert!(
        v["context"].is_null(),
        "context must be null when session not found"
    );
}

#[test]
fn viewport_session_not_found_text() {
    if skip() {
        return;
    }

    let out = headless(
        &[
            "browser",
            "viewport",
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "viewport nonexistent session text");
    let text = stderr_str(&out);
    assert!(
        text.contains("error SESSION_NOT_FOUND:"),
        "text must contain 'error SESSION_NOT_FOUND:' got {text}"
    );
}

#[test]
fn viewport_tab_not_found_json() {
    if skip() {
        return;
    }
    let (sid, _tid) = start_session(URL_A);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "viewport",
            "--session",
            &sid,
            "--tab",
            "nonexistent-tab-id",
        ],
        10,
    );
    assert_failure(&out, "viewport nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser viewport");
    assert_error_envelope(&v, "TAB_NOT_FOUND");
    assert!(v["context"].is_object(), "context must be present");
    assert_eq!(v["context"]["session_id"], sid);
    assert!(
        v["context"]["tab_id"].is_null(),
        "context.tab_id must be null when tab not found"
    );

    close_session(&sid);
}

#[test]
fn viewport_missing_session_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "viewport", "--tab", "any-tab"], 10);
    assert_failure(&out, "viewport missing --session");
}

#[test]
fn viewport_missing_tab_arg() {
    if skip() {
        return;
    }

    let out = headless_json(&["browser", "viewport", "--session", "any-sid"], 10);
    assert_failure(&out, "viewport missing --tab");
}
