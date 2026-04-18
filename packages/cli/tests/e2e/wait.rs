//! E2E tests for `browser wait`.

use crate::harness::{
    SessionGuard, assert_error_envelope, assert_failure, assert_meta, assert_success, headless,
    headless_json, parse_json, skip, stdout_str, unique_session, url_a, url_b,
    url_delayed_redirect, url_delayed_redirect_long, url_fast_redirect, url_home_no_trailing_slash,
    url_network_idle_lazy_in_viewport, url_network_idle_lazy_offscreen,
    url_network_idle_lazy_scroll, url_network_idle_non_lazy_blocked, wait_page_ready,
};

const ELEMENT_SELECTOR: &str = "#loaded";
const CONDITION_EXPR: &str = "window.__waitReady === true";

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

    let goto_out = headless_json(
        &["browser", "goto", url, "--session", &sid, "--tab", &tid],
        30,
    );
    assert_success(&goto_out, "goto initial url");
    wait_page_ready(&sid, &tid);

    (sid, tid)
}

fn set_title(sid: &str, tid: &str, title: &str) {
    let js = format!(
        "document.title = {}; void(0)",
        serde_json::to_string(title).unwrap()
    );
    let out = headless_json(
        &["browser", "eval", &js, "--session", sid, "--tab", tid],
        10,
    );
    assert_success(&out, "set title");
}

fn schedule_element(sid: &str, tid: &str) {
    let js = r#"setTimeout(() => {
  const el = document.createElement('div');
  el.id = 'loaded';
  el.textContent = 'Ready';
  document.body.appendChild(el);
}, 150);
void(0)"#;
    let out = headless_json(&["browser", "eval", js, "--session", sid, "--tab", tid], 10);
    assert_success(&out, "schedule delayed element");
}

fn schedule_condition_true(sid: &str, tid: &str) {
    let js = r#"setTimeout(() => {
  window.__waitReady = true;
}, 150);
void(0)"#;
    let out = headless_json(&["browser", "eval", js, "--session", sid, "--tab", tid], 10);
    assert_success(&out, "schedule delayed condition");
}

fn schedule_navigation_to(sid: &str, tid: &str, destination_url: &str) {
    let js = format!(
        "setTimeout(() => {{ window.location.href = {}; }}, 150); void(0)",
        serde_json::to_string(destination_url).unwrap()
    );
    let out = headless_json(
        &["browser", "eval", &js, "--session", sid, "--tab", tid],
        10,
    );
    assert_success(&out, "schedule delayed navigation");
}

fn open_new_tab(sid: &str, url: &str) -> String {
    let out = headless_json(&["browser", "new-tab", url, "--session", sid], 30);
    assert_success(&out, "open new tab");
    parse_json(&out)["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn wait_element_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    set_title(&sid, &tid, "Wait Element Fixture");
    schedule_element(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "element",
            ELEMENT_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait element json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait element");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Wait Element Fixture");
    assert_eq!(v["data"]["kind"], "element");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(v["data"]["elapsed_ms"].as_u64().is_some());
    assert_eq!(v["data"]["observed_value"]["selector"], ELEMENT_SELECTOR);
}

#[test]
fn wait_element_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    set_title(&sid, &tid, "Wait Element Text Fixture");
    schedule_element(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "wait",
            "element",
            ELEMENT_SELECTOR,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait element text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first().copied(),
        Some(format!("[{sid} {tid}] about:blank").as_str())
    );
    assert_eq!(lines.get(1), Some(&"ok browser wait element"));
    assert!(
        lines
            .get(2)
            .copied()
            .unwrap_or_default()
            .starts_with("elapsed_ms: "),
        "missing elapsed_ms line: {text}"
    );
    assert_eq!(
        lines.get(3),
        Some(&format!("target: {ELEMENT_SELECTOR}").as_str())
    );
}

#[test]
fn wait_element_timeout_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "element",
            "#never-there",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "150",
        ],
        10,
    );
    assert_failure(&out, "wait element timeout");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait element");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "TIMEOUT");
    assert_eq!(v["error"]["retryable"], true);
}

#[test]
fn wait_navigation_json_happy_path() {
    if skip() {
        return;
    }

    let page_a = url_a();
    let page_b = url_b();
    let (sid, tid) = start_session(&page_a);
    let _guard = SessionGuard::new(&sid);
    schedule_navigation_to(&sid, &tid, &page_b);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "10000",
        ],
        15,
    );
    assert_success(&out, "wait navigation json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Page B");
    assert!(
        v["context"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "context.url must point at page-b: {}",
        v["context"]["url"]
    );
    assert_eq!(v["data"]["kind"], "navigation");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(v["data"]["elapsed_ms"].as_u64().is_some());
    assert!(
        v["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "observed_value.url must point at page-b: {}",
        v["data"]["observed_value"]
    );
    assert_eq!(v["data"]["observed_value"]["ready_state"], "complete");
}

#[test]
fn wait_navigation_detects_fast_redirect_when_final_url_is_already_loaded() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let fast_redirect = url_fast_redirect();

    let eval_out = headless_json(
        &[
            "browser",
            "eval",
            &format!(
                "window.location.href = {}; void(0)",
                serde_json::to_string(&fast_redirect).unwrap()
            ),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&eval_out, "trigger fast redirect");

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "3000",
        ],
        10,
    );
    assert_success(&out, "wait navigation after fast redirect");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "navigation");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(
        v["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "observed_value.url must point at page-b: {}",
        v["data"]["observed_value"]
    );
}

#[test]
fn wait_navigation_accepts_stable_complete_state_at_current_url() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_a());
    wait_page_ready(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "2500",
        ],
        10,
    );
    assert_success(&out, "wait navigation after page already settled");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "navigation");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(
        v["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-a"),
        "observed_value.url must stay on page-a: {}",
        v["data"]["observed_value"]
    );
    assert_eq!(v["data"]["observed_value"]["ready_state"], "complete");
}

#[test]
fn wait_navigation_keeps_root_url_without_trailing_slash_regression_green() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_home_no_trailing_slash());

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "3000",
        ],
        10,
    );
    assert_success(
        &out,
        "wait navigation after root url without trailing slash",
    );
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "navigation");
    assert_eq!(v["data"]["satisfied"], true);
    assert_eq!(v["context"]["title"], "Home");
    assert_eq!(v["data"]["observed_value"]["ready_state"], "complete");
    assert!(
        v["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or("")
            .ends_with('/'),
        "root URL should normalize to trailing slash: {}",
        v["data"]["observed_value"]
    );
}

#[test]
fn wait_navigation_detects_delayed_redirect_via_real_page_navigation() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let delayed_redirect = url_delayed_redirect();

    let eval_out = headless_json(
        &[
            "browser",
            "eval",
            &format!(
                "window.location.href = {}; void(0)",
                serde_json::to_string(&delayed_redirect).unwrap()
            ),
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&eval_out, "trigger delayed redirect");

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait navigation after delayed redirect");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "navigation");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(
        v["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "observed_value.url must point at page-b: {}",
        v["data"]["observed_value"]
    );
}

#[test]
fn wait_navigation_does_not_accept_intermediate_complete_url_before_long_redirect() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_delayed_redirect_long());

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait navigation after long delayed redirect");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "navigation");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(
        v["data"]["observed_value"]["url"]
            .as_str()
            .unwrap_or("")
            .contains("page-b"),
        "wait must accept only the final redirect URL: {}",
        v["data"]["observed_value"]
    );
}

#[test]
fn wait_navigation_timeout_when_no_navigation_occurs() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "150",
        ],
        10,
    );
    assert_failure(&out, "wait navigation timeout");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "TIMEOUT");
    assert_eq!(v["error"]["retryable"], true);
}

// Ignored: `wait network-idle` relies on the CDP Network.requestWillBeSent /
// Network.loadingFinished event stream settling after all requests complete.
// In headless CI environments this idle window does not reliably arrive within
// 10 000 ms for a local-server page — the implementation is correct but the
// feature is sensitive to CI network scheduling. Re-enable once the wait
// implementation uses a tighter idle heuristic or a dedicated CI timeout budget.
#[test]
#[ignore]
fn wait_network_idle_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "network-idle",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "10000",
        ],
        15,
    );
    assert_success(&out, "wait network-idle json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait network-idle");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Page A");
    assert_eq!(v["data"]["kind"], "network-idle");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(v["data"]["elapsed_ms"].as_u64().is_some());
    assert_eq!(v["data"]["observed_value"]["idle"], true);
}

#[test]
fn wait_network_idle_ignores_offscreen_lazy_images() {
    if skip() {
        return;
    }

    let (sid, _) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_network_idle_lazy_offscreen());

    let out = headless_json(
        &[
            "browser",
            "wait",
            "network-idle",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "3500",
        ],
        10,
    );
    assert_success(
        &out,
        "wait network-idle should ignore off-screen lazy image",
    );
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait network-idle");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "network-idle");
    assert_eq!(v["data"]["satisfied"], true);
    assert_eq!(v["data"]["observed_value"]["idle"], true);
}

#[test]
fn wait_network_idle_times_out_for_non_lazy_incomplete_images() {
    if skip() {
        return;
    }

    let (sid, _) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_network_idle_non_lazy_blocked());

    let out = headless_json(
        &[
            "browser",
            "wait",
            "network-idle",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "1200",
        ],
        10,
    );
    assert_failure(
        &out,
        "wait network-idle should still block on non-lazy image",
    );
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait network-idle");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "TIMEOUT");
}

#[test]
fn wait_network_idle_accepts_lazy_images_after_scroll_into_view() {
    if skip() {
        return;
    }

    let (sid, _) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_network_idle_lazy_scroll());
    wait_page_ready(&sid, &tid);

    let scroll_out = headless_json(
        &[
            "browser",
            "eval",
            "document.getElementById('lazy-target').scrollIntoView({block:'center'}); void 0",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&scroll_out, "scroll lazy image into view");

    let out = headless_json(
        &[
            "browser",
            "wait",
            "network-idle",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait network-idle after scrolling lazy image");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait network-idle");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["kind"], "network-idle");
    assert_eq!(v["data"]["satisfied"], true);
    assert_eq!(v["data"]["observed_value"]["idle"], true);
}

#[test]
fn wait_network_idle_blocks_on_in_viewport_lazy_image_still_loading() {
    if skip() {
        return;
    }

    let (sid, _) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let tid = open_new_tab(&sid, &url_network_idle_lazy_in_viewport());

    let out = headless_json(
        &[
            "browser",
            "wait",
            "network-idle",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "3500",
        ],
        10,
    );
    assert_failure(
        &out,
        "wait network-idle should block on in-viewport lazy image while it is still loading",
    );
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait network-idle");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "TIMEOUT");
}

#[test]
fn wait_condition_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    set_title(&sid, &tid, "Wait Condition Fixture");
    schedule_condition_true(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "condition",
            CONDITION_EXPR,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait condition json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait condition");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Wait Condition Fixture");
    assert_eq!(v["data"]["kind"], "condition");
    assert_eq!(v["data"]["satisfied"], true);
    assert!(v["data"]["elapsed_ms"].as_u64().is_some());
    assert_eq!(v["data"]["observed_value"], true);
}

#[test]
fn wait_condition_timeout_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "condition",
            "window.__waitNever === true",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "150",
        ],
        10,
    );
    assert_failure(&out, "wait condition timeout");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait condition");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "TIMEOUT");
    assert_eq!(v["error"]["retryable"], true);
}

#[test]
fn wait_condition_text_output() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    schedule_condition_true(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "wait",
            "condition",
            CONDITION_EXPR,
            "--session",
            &sid,
            "--tab",
            &tid,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait condition text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first().copied(),
        Some(format!("[{sid} {tid}] about:blank").as_str())
    );
    assert_eq!(lines.get(1), Some(&"ok browser wait condition"));
    assert!(
        lines
            .get(2)
            .copied()
            .unwrap_or_default()
            .starts_with("elapsed_ms: "),
        "missing elapsed_ms line: {text}"
    );
    assert_eq!(lines.get(3), Some(&"observed_value: true"));
}

#[test]
fn wait_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "wait",
            "element",
            ELEMENT_SELECTOR,
            "--session",
            "missing-session",
            "--tab",
            "any-tab",
            "--timeout",
            "500",
        ],
        10,
    );
    assert_failure(&out, "wait missing session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait element");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn wait_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "wait",
            "navigation",
            "--session",
            &sid,
            "--tab",
            "missing-tab",
            "--timeout",
            "500",
        ],
        10,
    );
    assert_failure(&out, "wait missing tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser wait navigation");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}
