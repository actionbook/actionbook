//! Browser interaction E2E tests: click, fill, type, select, hover, focus,
//! press, drag, upload, mouse-move, cursor-position, scroll.
//!
//! Uses daemon v2 CLI format with --session and --tab addressing.
//! Each test is self-contained: start → operate → assert → close.

use crate::harness::{assert_success, headless, set_body_html_js, skip, stdout_str, SessionGuard};

// ---------------------------------------------------------------------------
// 1. int_click_element — S1T1: inject button via eval → click → verify
// ---------------------------------------------------------------------------

#[test]
fn int_click_element() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject a button that sets a flag when clicked
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(
                r#"<button id="btn" onclick="window.__clicked=true">Click me</button>"#,
            ),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject button");

    // Click the button
    let out = headless(
        &["browser", "click", "#btn", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "click button");

    // Verify click happened
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__clicked === true",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval verify click");
    assert!(
        stdout_str(&out).contains("true"),
        "click should have set __clicked to true, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 2. int_click_s1t2_isolation — S1T2: click on tab_a → tab_b unaffected
// ---------------------------------------------------------------------------

#[test]
fn int_click_s1t2_isolation() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Start session with tab t0
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Open second tab (t1)
    let out = headless(
        &["browser", "open", "https://example.com", "-s", "local-1"],
        30,
    );
    assert_success(&out, "open t1");

    // Inject button on t0
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<button id="btn" onclick="window.__clicked=true">Click</button>"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject button on t0");

    // Inject marker on t1
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__clicked = false",
            "-s",
            "local-1",
            "-t",
            "t2",
        ],
        10,
    );
    assert_success(&out, "set marker on t1");

    // Click on t0
    let out = headless(
        &["browser", "click", "#btn", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "click on t0");

    // Verify t1 is unaffected
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__clicked",
            "-s",
            "local-1",
            "-t",
            "t2",
        ],
        10,
    );
    assert_success(&out, "eval t1 marker");
    assert!(
        stdout_str(&out).contains("false"),
        "t1 should be unaffected by click on t0, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 3. int_click_seq — SEQ: click btn_a → eval → click btn_b → eval
// ---------------------------------------------------------------------------

#[test]
fn int_click_seq() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject two buttons
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(
                r#"<button id="a" onclick="window.__a=true">A</button><button id="b" onclick="window.__b=true">B</button>"#,
            ),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject buttons");

    // Click button A
    let out = headless(&["browser", "click", "#a", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "click #a");

    // Verify A clicked
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__a === true",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval __a");
    assert!(
        stdout_str(&out).contains("true"),
        "__a should be true, got: {}",
        stdout_str(&out)
    );

    // Click button B
    let out = headless(&["browser", "click", "#b", "-s", "local-1", "-t", "t1"], 30);
    assert_success(&out, "click #b");

    // Verify B clicked
    let out = headless(
        &[
            "browser",
            "eval",
            "window.__b === true",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval __b");
    assert!(
        stdout_str(&out).contains("true"),
        "__b should be true, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 4. int_fill_input — S1T1: inject input → fill "hello" → eval .value
// ---------------------------------------------------------------------------

#[test]
fn int_fill_input() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject an input element
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="test" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject input");

    // Fill the input
    let out = headless(
        &[
            "browser", "fill", "#test", "hello", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "fill input");

    // Verify value
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#test').value",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval value");
    assert!(
        stdout_str(&out).contains("hello"),
        "input value should be 'hello', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 5. int_fill_s1t2_isolation — S1T2: fill on tab_a → tab_b input unchanged
// ---------------------------------------------------------------------------

#[test]
fn int_fill_s1t2_isolation() {
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

    // Open second tab (t1)
    let out = headless(&["browser", "open", "about:blank", "-s", "local-1"], 30);
    assert_success(&out, "open t1");

    // Inject input on t0
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="test" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject input on t0");

    // Inject input on t1
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="test" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t2",
        ],
        30,
    );
    assert_success(&out, "inject input on t1");

    // Fill on t0
    let out = headless(
        &[
            "browser",
            "fill",
            "#test",
            "filled-on-t0",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "fill on t0");

    // Verify t1 input is still empty
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#test').value",
            "-s",
            "local-1",
            "-t",
            "t2",
        ],
        10,
    );
    assert_success(&out, "eval t1 value");
    let val = stdout_str(&out);
    assert!(
        !val.contains("filled-on-t0"),
        "t1 input should not contain t0's fill value, got: {}",
        val
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

#[test]
fn int_fill_helper_stays_trusted_types_safe() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    let trusted_types_url = "data:text/html,%3Cmeta%20http-equiv%3D%22Content-Security-Policy%22%20content%3D%22require-trusted-types-for%20%27script%27%3B%20trusted-types%20actionbook-e2e%22%3Ett";

    let out = headless(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
            "--open-url",
            trusted_types_url,
        ],
        30,
    );
    assert_success(&out, "start trusted types page");

    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="tt-fill" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject input on trusted types page");

    let out = headless(
        &[
            "browser", "fill", "#tt-fill", "trusted", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "fill trusted types input");

    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#tt-fill').value",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        15,
    );
    assert_success(&out, "verify trusted types input value");
    assert!(
        stdout_str(&out).contains("trusted"),
        "trusted types fill should persist value, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 6. int_fill_seq — SEQ: fill input_a "aaa" → fill input_b "bbb" → both correct
// ---------------------------------------------------------------------------

#[test]
fn int_fill_seq() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject two inputs
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="a" type="text" /><input id="b" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject inputs");

    // Fill input A
    let out = headless(
        &["browser", "fill", "#a", "aaa", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "fill #a");

    // Fill input B
    let out = headless(
        &["browser", "fill", "#b", "bbb", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "fill #b");

    // Verify A
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#a').value",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval #a value");
    assert!(
        stdout_str(&out).contains("aaa"),
        "#a value should be 'aaa', got: {}",
        stdout_str(&out)
    );

    // Verify B
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#b').value",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval #b value");
    assert!(
        stdout_str(&out).contains("bbb"),
        "#b value should be 'bbb', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 7. int_type_text — S1T1: click input → type "world" → eval .value
// ---------------------------------------------------------------------------

#[test]
fn int_type_text() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject an input
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="test" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject input");

    // Click input to focus it
    let out = headless(
        &["browser", "click", "#test", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "click input");

    // Type text into the input
    let out = headless(
        &[
            "browser", "type", "#test", "world", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "type world");

    // Verify value contains "world"
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#test').value",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval value");
    assert!(
        stdout_str(&out).contains("world"),
        "input value should contain 'world', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 8. int_select_dropdown — S1T1: inject select → select "opt2" → verify
// ---------------------------------------------------------------------------

#[test]
fn int_select_dropdown() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject a select element with options
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(
                r#"<select id="sel"><option value="opt1">One</option><option value="opt2">Two</option><option value="opt3">Three</option></select>"#,
            ),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject select");

    // Select opt2
    let out = headless(
        &[
            "browser", "select", "#sel", "opt2", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "select opt2");

    // Verify selected value
    let out = headless(
        &[
            "browser",
            "eval",
            "document.querySelector('#sel').value",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval selected value");
    assert!(
        stdout_str(&out).contains("opt2"),
        "selected value should be 'opt2', got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 9. int_hover — S1T1: hover "body" → exit 0
// ---------------------------------------------------------------------------

#[test]
fn int_hover() {
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

    // Wait for page load so the DOM is accessible
    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            "document.readyState === 'complete'",
            "-s",
            "local-1",
            "-t",
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &["browser", "hover", "body", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "hover body");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 10. int_focus — S1T1: inject input → focus → eval activeElement.tagName
// ---------------------------------------------------------------------------

#[test]
fn int_focus() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject an input
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="focus-test" type="text" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject input");

    // Focus the input
    let out = headless(
        &[
            "browser",
            "focus",
            "#focus-test",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "focus input");

    // Verify activeElement is INPUT
    let out = headless(
        &[
            "browser",
            "eval",
            "document.activeElement.tagName",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval activeElement");
    assert!(
        stdout_str(&out).contains("INPUT"),
        "activeElement should be INPUT, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 11. int_press_key — S1T1: press "Enter" → exit 0
// ---------------------------------------------------------------------------

#[test]
fn int_press_key() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &["browser", "press", "Enter", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "press Enter");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 12. int_drag — S1T1: drag from "body" to "body" → exit 0
// ---------------------------------------------------------------------------

#[test]
fn int_drag() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser", "drag", "body", "body", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "drag body to body");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 13. int_upload — S1T1: create temp file → inject file input → upload → exit 0
// ---------------------------------------------------------------------------

#[test]
fn int_upload() {
    if skip() {
        return;
    }
    let _guard = SessionGuard::new();

    // Create a temporary file to upload
    let tmp = tempfile::Builder::new()
        .prefix("actionbook-upload-test-")
        .suffix(".txt")
        .tempfile()
        .expect("create temp file for upload");
    let file_path = tmp.path().to_string_lossy().to_string();

    // Write some content to the temp file
    std::io::Write::write_all(&mut tmp.as_file(), b"test upload content").expect("write temp file");

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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject a file input element
    let out = headless(
        &[
            "browser",
            "eval",
            &set_body_html_js(r#"<input id="file-upload" type="file" />"#),
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "inject file input");

    // Upload file (selector is first positional, then file paths)
    let out = headless(
        &[
            "browser",
            "upload",
            "#file-upload",
            &file_path,
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "upload file");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 14. int_mouse_move — S1T1: mouse-move 200 300 → exit 0
// ---------------------------------------------------------------------------

#[test]
fn int_mouse_move() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser",
            "mouse-move",
            "200,300",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "mouse-move 200,300");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 15. int_cursor_position — S1T1: cursor-position → stdout contains coords
// ---------------------------------------------------------------------------

#[test]
fn int_cursor_position() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &["browser", "cursor-position", "-s", "local-1", "-t", "t1"],
        10,
    );
    assert_success(&out, "cursor-position");
    let pos = stdout_str(&out);
    // Output should contain coordinate values (numbers)
    assert!(
        pos.chars().any(|c| c.is_ascii_digit()),
        "cursor-position should contain coordinate numbers, got: {}",
        pos
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 16. int_scroll_down — S1T1: scroll down --amount 500 → eval scrollY > 0
// ---------------------------------------------------------------------------

#[test]
fn int_scroll_down() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject tall content to make the page scrollable
    let out = headless(
        &[
            "browser",
            "eval",
            "document.body.style.height = '5000px'",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "make page scrollable");

    // Scroll down
    let out = headless(
        &[
            "browser", "scroll", "down", "500", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "scroll down");

    // Verify scrollY > 0
    let out = headless(
        &[
            "browser",
            "eval",
            "window.scrollY > 0",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval scrollY");
    assert!(
        stdout_str(&out).contains("true"),
        "scrollY should be > 0 after scrolling down, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 17. int_scroll_top — S1T1: scroll down → scroll top → eval scrollY == 0
// ---------------------------------------------------------------------------

#[test]
fn int_scroll_top() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    // Inject tall content to make the page scrollable
    let out = headless(
        &[
            "browser",
            "eval",
            "document.body.style.height = '5000px'",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "make page scrollable");

    // Scroll down first
    let out = headless(
        &[
            "browser", "scroll", "down", "500", "-s", "local-1", "-t", "t1",
        ],
        30,
    );
    assert_success(&out, "scroll down");

    // Verify we scrolled down
    let out = headless(
        &[
            "browser",
            "eval",
            "window.scrollY > 0",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval scrollY after down");
    assert!(
        stdout_str(&out).contains("true"),
        "scrollY should be > 0 after scrolling down, got: {}",
        stdout_str(&out)
    );

    // Scroll to top
    let out = headless(
        &["browser", "scroll", "top", "-s", "local-1", "-t", "t1"],
        30,
    );
    assert_success(&out, "scroll top");

    // Verify scrollY == 0
    let out = headless(
        &[
            "browser",
            "eval",
            "window.scrollY === 0",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        10,
    );
    assert_success(&out, "eval scrollY after top");
    assert!(
        stdout_str(&out).contains("true"),
        "scrollY should be 0 after scroll top, got: {}",
        stdout_str(&out)
    );

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}

// ---------------------------------------------------------------------------
// 18. int_scroll_into_view — S1T1: scroll into-view "body" → exit 0
// ---------------------------------------------------------------------------

#[test]
fn int_scroll_into_view() {
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
            "t1",
            "--timeout",
            "5000",
        ],
        30,
    );
    assert_success(&out, "wait for page load");

    let out = headless(
        &[
            "browser",
            "scroll",
            "into-view",
            "body",
            "-s",
            "local-1",
            "-t",
            "t1",
        ],
        30,
    );
    assert_success(&out, "scroll into-view body");

    let out = headless(&["browser", "close", "-s", "local-1"], 30);
    assert_success(&out, "close");
}
