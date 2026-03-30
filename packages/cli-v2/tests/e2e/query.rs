//! E2E tests for `browser query`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, wait_page_ready,
};

const SINGLE_QUERY: &str = ".single";
const ITEMS_QUERY: &str = ".item";
const MISSING_QUERY: &str = ".missing";
const DISABLED_QUERY: &str = ".disabled-item";

fn start_session() -> (String, String) {
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
            "about:blank",
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
        &[
            "browser",
            "goto",
            "about:blank",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&goto_out, "goto about:blank");
    wait_page_ready(&sid, &tid);

    (sid, tid)
}

fn inject_fixture(sid: &str, tid: &str) {
    let js = r#"document.body.style.margin = '0';
document.body.innerHTML = `
  <main id="query-root">
    <button class="single" id="single-target">Unique CTA</button>
    <div class="item">Item A</div>
    <div class="item">Item B</div>
    <div class="item" style="display:none">Item Hidden</div>
    <button class="disabled-item" disabled>Disabled CTA</button>
  </main>
`;
document.title = 'Query Contract Fixture';
void(0)"#;
    let out = headless_json(&["browser", "eval", js, "--session", sid, "--tab", tid], 10);
    assert_success(&out, "inject fixture");
}

fn assert_meta(v: &serde_json::Value) {
    assert!(v["meta"]["duration_ms"].is_number());
    assert!(v["meta"]["warnings"].is_array());
    assert!(v["meta"]["pagination"].is_null());
    assert!(v["meta"]["truncated"].is_boolean());
}

fn assert_error_envelope(v: &serde_json::Value, expected_code: &str) {
    assert_eq!(v["ok"], false);
    assert!(v["data"].is_null());
    assert_eq!(v["error"]["code"], expected_code);
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert!(v["error"]["details"].is_object() || v["error"]["details"].is_null());
    assert_meta(v);
}

fn assert_query_item(
    item: &serde_json::Value,
    selector: &str,
    tag: &str,
    text: &str,
    visible: bool,
    enabled: bool,
) {
    assert_eq!(item["selector"], selector);
    assert_eq!(item["tag"], tag);
    assert_eq!(item["text"], text);
    assert_eq!(item["visible"], visible);
    assert_eq!(item["enabled"], enabled);
}

#[test]
fn query_one_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "one",
            SINGLE_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query one json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["context"]["title"], "Query Contract Fixture");
    assert_eq!(v["data"]["mode"], "one");
    assert_eq!(v["data"]["query"], SINGLE_QUERY);
    assert_eq!(v["data"]["count"], 1);
    assert_query_item(
        &v["data"]["item"],
        ".single:nth-of-type(1)",
        "button",
        "Unique CTA",
        true,
        true,
    );
}

#[test]
fn query_one_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "query",
            "one",
            SINGLE_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query one text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first().copied(),
        Some(format!("[{sid} {tid}] about:blank").as_str())
    );
    assert_eq!(lines.get(1), Some(&"1 match"));
    assert_eq!(lines.get(2), Some(&"selector: .single:nth-of-type(1)"));
    assert_eq!(lines.get(3), Some(&"text: Unique CTA"));
}

#[test]
fn query_one_element_not_found_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "one",
            MISSING_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "query one missing");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "ELEMENT_NOT_FOUND");
    assert_eq!(v["error"]["details"]["query"], MISSING_QUERY);
    assert_eq!(v["error"]["details"]["count"], 0);
}

#[test]
fn query_one_multiple_matches_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "one",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "query one multiple");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "MULTIPLE_MATCHES");
    assert_eq!(v["error"]["details"]["query"], ITEMS_QUERY);
    assert_eq!(v["error"]["details"]["count"], 3);
    assert_eq!(
        v["error"]["details"]["sample_selectors"],
        serde_json::json!([
            ".item:nth-of-type(1)",
            ".item:nth-of-type(2)",
            ".item:nth-of-type(3)"
        ])
    );
}

#[test]
fn query_all_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "all",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query all json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_eq!(v["context"]["title"], "Query Contract Fixture");
    assert_eq!(v["data"]["mode"], "all");
    assert_eq!(v["data"]["query"], ITEMS_QUERY);
    assert_eq!(v["data"]["count"], 3);
    let items = v["data"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 3);
    assert_query_item(
        &items[0],
        ".item:nth-of-type(1)",
        "div",
        "Item A",
        true,
        true,
    );
    assert_query_item(
        &items[1],
        ".item:nth-of-type(2)",
        "div",
        "Item B",
        true,
        true,
    );
    assert_query_item(
        &items[2],
        ".item:nth-of-type(3)",
        "div",
        "Item Hidden",
        false,
        true,
    );
}

#[test]
fn query_all_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "query",
            "all",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query all text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first().copied(),
        Some(format!("[{sid} {tid}] about:blank").as_str())
    );
    assert_eq!(lines.get(1), Some(&"3 matches"));
    assert_eq!(lines.get(2), Some(&"1. .item:nth-of-type(1)"));
    assert_eq!(lines.get(3), Some(&"   Item A"));
}

#[test]
fn query_all_empty_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "all",
            MISSING_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query all empty json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["mode"], "all");
    assert_eq!(v["data"]["query"], MISSING_QUERY);
    assert_eq!(v["data"]["count"], 0);
    assert_eq!(v["data"]["items"], serde_json::json!([]));
}

#[test]
fn query_one_disabled_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "one",
            DISABLED_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query disabled json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["mode"], "one");
    assert_eq!(v["data"]["query"], DISABLED_QUERY);
    assert_eq!(v["data"]["count"], 1);
    assert_query_item(
        &v["data"]["item"],
        ".disabled-item:nth-of-type(2)",
        "button",
        "Disabled CTA",
        true,
        false,
    );
}

#[test]
fn query_nth_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "nth",
            "2",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query nth json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["mode"], "nth");
    assert_eq!(v["data"]["query"], ITEMS_QUERY);
    assert_eq!(v["data"]["index"], 2);
    assert_eq!(v["data"]["count"], 3);
    assert_query_item(
        &v["data"]["item"],
        ".item:nth-of-type(2)",
        "div",
        "Item B",
        true,
        true,
    );
}

#[test]
fn query_nth_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "query",
            "nth",
            "2",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query nth text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first().copied(),
        Some(format!("[{sid} {tid}] about:blank").as_str())
    );
    assert_eq!(lines.get(1), Some(&"match 2/3"));
    assert_eq!(lines.get(2), Some(&"selector: .item:nth-of-type(2)"));
    assert_eq!(lines.get(3), Some(&"text: Item B"));
}

#[test]
fn query_nth_index_out_of_range_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "nth",
            "4",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "query nth out of range");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_error_envelope(&v, "INDEX_OUT_OF_RANGE");
    assert_eq!(v["error"]["details"]["query"], ITEMS_QUERY);
    assert_eq!(v["error"]["details"]["count"], 3);
    assert_eq!(v["error"]["details"]["index"], 4);
}

#[test]
fn query_count_json_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "count",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query count json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["mode"], "count");
    assert_eq!(v["data"]["query"], ITEMS_QUERY);
    assert_eq!(v["data"]["count"], 3);
    assert!(v["data"].get("item").is_none());
    assert!(v["data"].get("items").is_none());
}

#[test]
fn query_count_text_happy_path() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless(
        &[
            "browser",
            "query",
            "count",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query count text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(
        lines.first().copied(),
        Some(format!("[{sid} {tid}] about:blank").as_str())
    );
    assert_eq!(lines.get(1), Some(&"3"));
}

#[test]
fn query_count_zero_json() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "count",
            MISSING_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query count zero json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["mode"], "count");
    assert_eq!(v["data"]["query"], MISSING_QUERY);
    assert_eq!(v["data"]["count"], 0);
}

#[test]
fn query_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "query",
            "count",
            ITEMS_QUERY,
            "--session",
            "nonexistent-sid",
            "--tab",
            "any-tab",
        ],
        10,
    );
    assert_failure(&out, "query nonexistent session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn query_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session();
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "one",
            SINGLE_QUERY,
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "query nonexistent tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}

#[test]
fn query_js_exception_returns_error() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let patch_out = headless_json(
        &[
            "browser",
            "eval",
            "document.querySelectorAll = function() { throw new Error('query boom'); }; void(0)",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        5,
    );
    assert_success(&patch_out, "patch querySelectorAll");

    let out = headless_json(
        &[
            "browser",
            "query",
            "count",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_failure(&out, "query js exception");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.query");
    assert_error_envelope(&v, "JS_EXCEPTION");
}

#[test]
fn query_all_does_not_mutate_dom() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session();
    let _guard = SessionGuard::new(&sid);
    inject_fixture(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "query",
            "all",
            ITEMS_QUERY,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, "query all stateless");

    let verify = headless_json(
        &[
            "browser",
            "eval",
            r#"JSON.stringify({
  itemCount: document.querySelectorAll('.item').length,
  singleText: document.querySelector('#single-target').textContent,
  hiddenDisplay: document.querySelectorAll('.item')[2].style.display
})"#,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&verify, "verify dom unchanged");
    let v = parse_json(&verify);

    let state = serde_json::from_str::<serde_json::Value>(v["data"]["value"].as_str().unwrap())
        .expect("eval stringified state");
    assert_eq!(state["itemCount"], 3);
    assert_eq!(state["singleText"], "Unique CTA");
    assert_eq!(state["hiddenDisplay"], "none");
}
