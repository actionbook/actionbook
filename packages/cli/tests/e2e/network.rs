//! Browser network observation E2E tests.
//!
//! Covers the planned `browser network requests` and
//! `browser network request <id>` commands for ACT-882.

use crate::harness::{
    SessionGuard, assert_meta, assert_success, headless_json, parse_json, skip, start_session,
    url_network_load, url_network_xhr,
};

fn wait_requests_done(session_id: &str, tab_id: &str) {
    let out = headless_json(
        &[
            "browser",
            "wait",
            "condition",
            "window.__ab_requests_done === true",
            "--session",
            session_id,
            "--tab",
            tab_id,
            "--timeout",
            "5000",
        ],
        10,
    );
    assert_success(&out, "wait requests done");
}

fn request_items(value: &serde_json::Value) -> &[serde_json::Value] {
    value["data"]["requests"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

#[test]
fn network_requests_lists_page_load_requests() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_load());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "network",
            "requests",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "network requests page load");
    let v = parse_json(&out);
    let requests = request_items(&v);

    assert_eq!(v["command"], "browser network requests");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert!(requests.len() >= 3, "expected document + css + js requests");
    assert!(
        requests
            .iter()
            .any(|req| req["resource_type"] == "Document"),
        "document request should be present"
    );
    assert!(
        requests
            .iter()
            .any(|req| req["resource_type"] == "Stylesheet"),
        "stylesheet request should be present"
    );
    assert!(
        requests.iter().any(|req| req["resource_type"] == "Script"),
        "script request should be present"
    );
    assert_eq!(
        v["data"]["total"].as_u64().unwrap_or(0),
        requests.len() as u64
    );
    assert_meta(&v);
}

#[test]
fn network_requests_filter_by_type() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_xhr());
    let _guard = SessionGuard::new(&sid);
    wait_requests_done(&sid, &tid);

    let out = headless_json(
        &[
            "browser",
            "network",
            "requests",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--type",
            "xhr,fetch",
        ],
        15,
    );
    assert_success(&out, "network requests filter type");
    let v = parse_json(&out);
    let requests = request_items(&v);

    assert_eq!(v["command"], "browser network requests");
    assert!(!requests.is_empty(), "xhr/fetch requests should be present");
    assert!(
        requests
            .iter()
            .all(|req| { matches!(req["resource_type"].as_str(), Some("XHR") | Some("Fetch")) })
    );
    assert_eq!(
        v["data"]["filtered"].as_u64().unwrap_or(0),
        requests.len() as u64
    );
    assert_meta(&v);
}

#[test]
fn network_requests_clear() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_xhr());
    let _guard = SessionGuard::new(&sid);
    wait_requests_done(&sid, &tid);

    let clear_out = headless_json(
        &[
            "browser",
            "network",
            "requests",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--clear",
        ],
        15,
    );
    assert_success(&clear_out, "network requests clear");
    let clear_v = parse_json(&clear_out);
    assert_eq!(clear_v["command"], "browser network requests");
    assert_eq!(clear_v["data"]["cleared"], true);

    let list_out = headless_json(
        &[
            "browser",
            "network",
            "requests",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&list_out, "network requests list after clear");
    let list_v = parse_json(&list_out);
    let requests = request_items(&list_v);

    assert_eq!(requests.len(), 0);
    assert_eq!(list_v["data"]["total"], 0);
    assert_eq!(list_v["data"]["filtered"], 0);
}

#[test]
fn network_request_detail() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session(&url_network_xhr());
    let _guard = SessionGuard::new(&sid);
    wait_requests_done(&sid, &tid);

    let list_out = headless_json(
        &[
            "browser",
            "network",
            "requests",
            "--session",
            &sid,
            "--tab",
            &tid,
            "--filter",
            "source=fetch",
        ],
        15,
    );
    assert_success(&list_out, "network requests list for detail");
    let list_v = parse_json(&list_out);
    let request_id = list_v["data"]["requests"][0]["request_id"]
        .as_str()
        .expect("request id")
        .to_string();

    let detail_out = headless_json(
        &[
            "browser",
            "network",
            "request",
            &request_id,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&detail_out, "network request detail");
    let v = parse_json(&detail_out);

    assert_eq!(v["command"], "browser network request");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
    assert_eq!(v["data"]["request"]["request_id"], request_id);
    assert!(
        v["data"]["request"]["url"]
            .as_str()
            .is_some_and(|url| url.contains("/api/data?source=fetch"))
    );
    assert_eq!(v["data"]["request"]["status"], 200);
    assert_eq!(
        v["data"]["request"]["response_headers"]["x-ab-fixture"],
        "api-data"
    );
    assert!(
        v["data"]["request"]["response_body"]
            .as_str()
            .is_some_and(|body| body.contains("\"source\":\"fetch\""))
    );
    assert_meta(&v);
}

#[test]
fn network_requests_empty_on_fresh_tab() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "network",
            "requests",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        15,
    );
    assert_success(&out, "network requests fresh tab");
    let v = parse_json(&out);
    let requests = request_items(&v);

    assert_eq!(v["command"], "browser network requests");
    assert!(
        requests.len() <= 1,
        "fresh about:blank tab should be empty or minimal"
    );
    assert!(v["data"]["total"].as_u64().unwrap_or(0) <= 1);
    assert_meta(&v);
}
