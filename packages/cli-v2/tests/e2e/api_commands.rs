//! Non-browser command E2E tests: search, get, help, version.
//!
//! These commands do NOT return `context` (api-reference §6).
//! search/get connect to actionbook.dev API.
//! JSON assertions strictly follow api-reference.md §2.4 envelope + §6 data.
//! Text assertions follow §2.5 protocol + §6 text format.

use crate::harness::{assert_success, headless, headless_json, parse_json, skip, stdout_str};

// ---------------------------------------------------------------------------
// search — §6.1 JSON
// ---------------------------------------------------------------------------

#[test]
fn api_search_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["search", "google login"], 30);
    assert_success(&out, "search");
    let v = parse_json(&out);

    // §2.4 envelope: all 5 top-level fields
    assert_eq!(v["ok"], true, "search: ok");
    assert_eq!(v["command"], "search", "search: command");
    assert!(v["context"].is_null(), "search: non-browser must omit context");
    assert!(v["error"].is_null(), "search: error null on success");
    assert!(v["meta"].is_object(), "search: meta required");
    assert!(v["meta"]["duration_ms"].is_number(), "search: meta.duration_ms");
    assert!(v["meta"]["warnings"].is_array(), "search: meta.warnings");
    assert!(v["meta"]["truncated"].is_boolean(), "search: meta.truncated");

    // data per §6.1
    assert!(v["data"]["query"].is_string(), "data.query");
    assert!(v["data"]["items"].is_array(), "data.items");

    let items = v["data"]["items"].as_array().unwrap();
    if !items.is_empty() {
        let item = &items[0];
        assert!(item["area_id"].is_string(), "item.area_id");
        assert!(item["title"].is_string(), "item.title");
        assert!(item["summary"].is_string(), "item.summary");
        assert!(item["score"].is_number(), "item.score");
        assert!(item["url"].is_string(), "item.url");
    }
}

// ---------------------------------------------------------------------------
// search — §6.1 Text
// ---------------------------------------------------------------------------

#[test]
fn api_search_text() {
    if skip() {
        return;
    }

    // §6.1 text: "N result(s)\n1. area_id\n   title\n   score: 0.98\n   url"
    let out = headless(&["search", "google login"], 30);
    assert_success(&out, "search text");
    let text = stdout_str(&out);
    assert!(text.contains("result"), "should contain 'result'");
    // verify numbered list format and score when results exist
    if text.contains("1.") {
        assert!(text.contains("score:"), "search text: should contain 'score:'");
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
    assert!(v["context"].is_null());
    assert!(v["error"].is_null());
    assert!(v["data"]["items"].is_array());
}

// ---------------------------------------------------------------------------
// search — §6.1 pagination + §2.4 meta.pagination
// ---------------------------------------------------------------------------

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

    let items = v["data"]["items"].as_array().unwrap();
    assert!(items.len() <= 2, "page-size 2: got {} items", items.len());

    // §2.4 meta.pagination when paginated
    let pag = &v["meta"]["pagination"];
    assert!(pag.is_object(), "meta.pagination should be present for paginated results");
    assert!(pag["page"].is_number(), "pagination.page");
    assert!(pag["page_size"].is_number(), "pagination.page_size");
    assert!(pag["has_more"].is_boolean(), "pagination.has_more");
}

// ---------------------------------------------------------------------------
// get — §6.2 JSON
// ---------------------------------------------------------------------------

#[test]
fn api_get_json() {
    if skip() {
        return;
    }

    // search first for a valid area_id
    let out = headless_json(&["search", "google login"], 30);
    assert_success(&out, "search for get");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();
    if items.is_empty() {
        return;
    }
    let area_id = items[0]["area_id"].as_str().unwrap();

    let out = headless_json(&["get", area_id], 30);
    assert_success(&out, "get action");
    let v = parse_json(&out);

    // §2.4 envelope
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "get");
    assert!(v["context"].is_null(), "non-browser: no context");
    assert!(v["error"].is_null(), "error null on success");
    assert!(v["meta"].is_object());
    assert!(v["meta"]["duration_ms"].is_number());

    // data per §6.2
    assert!(v["data"]["area_id"].is_string());
    assert!(v["data"]["url"].is_string());
    assert!(v["data"]["description"].is_string());
    assert!(v["data"]["elements"].is_array());

    let elements = v["data"]["elements"].as_array().unwrap();
    if !elements.is_empty() {
        let el = &elements[0];
        assert!(el["element_id"].is_string(), "element.element_id");
        assert!(el["type"].is_string(), "element.type");
        assert!(el["description"].is_string(), "element.description");
        assert!(
            el["css"].is_string() || el["css"].is_null(),
            "element.css: string or null"
        );
        // §6.2: xpath field always present (string or null)
        assert!(
            el["xpath"].is_string() || el["xpath"].is_null(),
            "element.xpath: string or null per §6.2"
        );
        assert!(el["allow_methods"].is_array(), "element.allow_methods");
    }
}

// ---------------------------------------------------------------------------
// get — §6.2 Text
// ---------------------------------------------------------------------------

#[test]
fn api_get_text() {
    if skip() {
        return;
    }

    let out = headless_json(&["search", "google login"], 30);
    assert_success(&out, "search");
    let v = parse_json(&out);
    let items = v["data"]["items"].as_array().unwrap();
    if items.is_empty() {
        return;
    }
    let area_id = items[0]["area_id"].as_str().unwrap();

    // §6.2 text: "area_id\nurl\n\ndescription\n\n[element_id] type\ncss: ...\nmethods: ..."
    let out = headless(&["get", area_id], 30);
    assert_success(&out, "get text");
    let text = stdout_str(&out);
    assert!(text.contains(area_id), "text should contain area_id");
    assert!(text.contains("https://") || text.contains("http://"), "text should contain URL");
    // element format when elements exist
    if text.contains("css:") || text.contains("methods:") {
        assert!(text.contains("methods:"), "text should contain 'methods:' per §6.2");
    }
}

#[test]
fn api_get_nonexistent_area() {
    if skip() {
        return;
    }

    let out = headless_json(&["get", "nonexistent.fake:/nope:default"], 30);
    let v = parse_json(&out);
    if !v["ok"].as_bool().unwrap_or(true) {
        // §3.1 error structure
        assert!(v["data"].is_null(), "data null on failure");
        assert!(v["error"]["code"].is_string(), "error.code");
        assert!(v["error"]["message"].is_string(), "error.message");
        assert!(v["error"]["retryable"].is_boolean(), "error.retryable");
    }
}

// ---------------------------------------------------------------------------
// help — §6.4 JSON + Text
// ---------------------------------------------------------------------------

#[test]
fn api_help_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["help"], 10);
    assert_success(&out, "help json");
    let v = parse_json(&out);
    // §2.4 full envelope
    assert_eq!(v["ok"], true);
    assert!(v["command"].is_string(), "help: command field required");
    assert!(v["context"].is_null(), "help: no context");
    assert!(v["error"].is_null(), "help: error null on success");
    assert!(v["meta"].is_object(), "help: meta required");
    // §6.4: data is a string
    assert!(v["data"].is_string(), "help: data should be string");
}

#[test]
fn api_help_text() {
    if skip() {
        return;
    }

    let out = headless(&["help"], 10);
    assert_success(&out, "help text");
    let text = stdout_str(&out);
    assert!(text.contains("browser"), "help text should mention browser");
}

// ---------------------------------------------------------------------------
// version — §6.5 JSON + Text
// ---------------------------------------------------------------------------

#[test]
fn api_version_json() {
    if skip() {
        return;
    }

    let out = headless_json(&["--version"], 10);
    assert_success(&out, "version json");
    let v = parse_json(&out);
    // §2.4 full envelope
    assert_eq!(v["ok"], true);
    assert!(v["command"].is_string(), "version: command field required");
    assert!(v["context"].is_null(), "version: no context");
    assert!(v["error"].is_null(), "version: error null on success");
    assert!(v["meta"].is_object(), "version: meta required");
    // §6.5: data is "1.0.0"
    assert_eq!(v["data"], "1.0.0");
}

#[test]
fn api_version_text() {
    if skip() {
        return;
    }

    let out = headless(&["--version"], 10);
    assert_success(&out, "version text");
    let text = stdout_str(&out);
    assert!(text.contains("1.0.0"), "version should contain 1.0.0");
}
