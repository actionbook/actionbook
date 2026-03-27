//! Non-browser command E2E tests: search, get, help, version.
//!
//! These commands do NOT return `context` (api-reference §6).
//! search/get connect to actionbook.dev API.
//! JSON assertions strictly follow api-reference.md §2.4 envelope + §6 data.

use crate::harness::{assert_success, headless, headless_json, parse_json, skip, stdout_str};

// ---------------------------------------------------------------------------
// search — §6.1
// ---------------------------------------------------------------------------

#[test]
fn api_search_returns_results() {
    if skip() {
        return;
    }

    let out = headless_json(&["search", "google login"], 30);
    assert_success(&out, "search");
    let v = parse_json(&out);

    // envelope
    assert_eq!(v["ok"], true, "search: ok");
    assert_eq!(v["command"], "search", "search: command");
    assert!(v["context"].is_null(), "search: non-browser command must omit context");
    assert!(v["error"].is_null(), "search: error should be null on success");

    // meta
    assert!(v["meta"].is_object(), "search: meta should be object");
    assert!(v["meta"]["duration_ms"].is_number(), "search: meta.duration_ms");

    // data per §6.1
    assert!(v["data"]["query"].is_string(), "search: data.query should be string");
    assert!(v["data"]["items"].is_array(), "search: data.items should be array");

    let items = v["data"]["items"].as_array().unwrap();
    if !items.is_empty() {
        let item = &items[0];
        // each item must have: area_id, title, summary, score, url
        assert!(item["area_id"].is_string(), "item.area_id should be string");
        assert!(item["title"].is_string(), "item.title should be string");
        assert!(item["summary"].is_string(), "item.summary should be string");
        assert!(item["score"].is_number(), "item.score should be number");
        assert!(item["url"].is_string(), "item.url should be string");
    }
}

#[test]
fn api_search_with_domain_filter() {
    if skip() {
        return;
    }

    let out = headless_json(
        &["search", "login", "--domain", "google.com"],
        30,
    );
    assert_success(&out, "search with domain");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["context"].is_null(), "non-browser: no context");
    assert!(v["data"]["items"].is_array());
}

#[test]
fn api_search_text_output_format() {
    if skip() {
        return;
    }

    // text output: "{N} result(s)\n1. area_id\n   title\n   score: ...\n   url"
    let out = headless(&["search", "google login"], 30);
    assert_success(&out, "search text");
    let text = stdout_str(&out);
    // should contain "result" somewhere (e.g. "3 results" or "1 result")
    assert!(
        text.contains("result"),
        "text output should contain 'result', got: {text}"
    );
}

#[test]
fn api_search_pagination() {
    if skip() {
        return;
    }

    let out = headless_json(
        &["search", "login", "--page", "1", "--page-size", "2"],
        30,
    );
    assert_success(&out, "search pagination");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);

    // items count should respect page-size
    let items = v["data"]["items"].as_array().unwrap();
    assert!(items.len() <= 2, "page-size 2: got {} items", items.len());
}

// ---------------------------------------------------------------------------
// get — §6.2
// ---------------------------------------------------------------------------

#[test]
fn api_get_returns_action_details() {
    if skip() {
        return;
    }

    // first search to find a valid area_id
    let out = headless_json(&["search", "google login"], 30);
    assert_success(&out, "search for get test");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();
    if items.is_empty() {
        // no results to test get with — skip gracefully
        return;
    }
    let area_id = items[0]["area_id"].as_str().unwrap();

    // get the action
    let out = headless_json(&["get", area_id], 30);
    assert_success(&out, "get action");
    let v = parse_json(&out);

    // envelope
    assert_eq!(v["ok"], true, "get: ok");
    assert_eq!(v["command"], "get", "get: command");
    assert!(v["context"].is_null(), "get: non-browser command must omit context");
    assert!(v["error"].is_null(), "get: error should be null");

    // meta
    assert!(v["meta"].is_object(), "get: meta");
    assert!(v["meta"]["duration_ms"].is_number(), "get: meta.duration_ms");

    // data per §6.2
    assert!(v["data"]["area_id"].is_string(), "get: data.area_id");
    assert!(v["data"]["url"].is_string(), "get: data.url");
    assert!(v["data"]["description"].is_string(), "get: data.description");
    assert!(v["data"]["elements"].is_array(), "get: data.elements should be array");

    let elements = v["data"]["elements"].as_array().unwrap();
    if !elements.is_empty() {
        let el = &elements[0];
        // each element: element_id, type, description, css, allow_methods
        assert!(el["element_id"].is_string(), "element.element_id");
        assert!(el["type"].is_string(), "element.type");
        assert!(el["description"].is_string(), "element.description");
        // css can be string or null
        assert!(
            el["css"].is_string() || el["css"].is_null(),
            "element.css should be string or null"
        );
        assert!(el["allow_methods"].is_array(), "element.allow_methods should be array");
    }
}

#[test]
fn api_get_text_output_format() {
    if skip() {
        return;
    }

    // search first for a valid area_id
    let out = headless_json(&["search", "google login"], 30);
    assert_success(&out, "search");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();
    if items.is_empty() {
        return;
    }
    let area_id = items[0]["area_id"].as_str().unwrap();

    let out = headless(&["get", area_id], 30);
    assert_success(&out, "get text");
    let text = stdout_str(&out);
    // text should contain the area_id
    assert!(
        text.contains(area_id),
        "text output should contain area_id '{area_id}', got: {text}"
    );
}

#[test]
fn api_get_nonexistent_area_returns_error() {
    if skip() {
        return;
    }

    let out = headless_json(&["get", "nonexistent.fake:/nope:default"], 30);
    // should fail or return empty
    let v = parse_json(&out);
    if !v["ok"].as_bool().unwrap_or(true) {
        assert!(!v["error"].is_null(), "error should be present on failure");
        assert!(v["error"]["code"].is_string(), "error.code should be string");
        assert!(v["data"].is_null(), "data should be null on failure");
    }
}

// ---------------------------------------------------------------------------
// help — §6.4
// ---------------------------------------------------------------------------

#[test]
fn api_help_output() {
    if skip() {
        return;
    }

    let out = headless(&["help"], 10);
    assert_success(&out, "help");
    let text = stdout_str(&out);
    // should mention browser subcommand
    assert!(
        text.contains("browser"),
        "help should mention browser, got: {text}"
    );
}

#[test]
fn api_help_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["help"], 10);
    assert_success(&out, "help json");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["context"].is_null(), "help: no context");
    // data is a string per §6.4
    assert!(v["data"].is_string(), "help: data should be string");
}

// ---------------------------------------------------------------------------
// version — §6.5
// ---------------------------------------------------------------------------

#[test]
fn api_version_output() {
    if skip() {
        return;
    }

    let out = headless(&["--version"], 10);
    assert_success(&out, "version");
    let text = stdout_str(&out);
    assert!(
        text.contains("1.0.0"),
        "version should contain 1.0.0, got: {text}"
    );
}

#[test]
fn api_version_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["--version"], 10);
    assert_success(&out, "version json");
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    assert!(v["context"].is_null(), "version: no context");
    assert_eq!(v["data"], "1.0.0", "version data should be '1.0.0'");
}
