//! E2E tests for `actionbook search` command.
//!
//! Uses a mock API server (local HTTP fixture) to validate the full CLI
//! invocation → HTTP request → format output pipeline.

use crate::harness::{
    api_base_url, assert_failure, assert_success, headless_json_with_env, headless_with_env, skip,
    stdout_str,
};

fn api_env() -> Vec<(&'static str, String)> {
    vec![("ACTIONBOOK_API_URL", api_base_url())]
}

fn headless_search(args: &[&str], timeout_secs: u64) -> std::process::Output {
    let env = api_env();
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    headless_with_env(args, &env_refs, timeout_secs)
}

fn headless_search_json(args: &[&str], timeout_secs: u64) -> std::process::Output {
    let env = api_env();
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    headless_json_with_env(args, &env_refs, timeout_secs)
}

#[test]
fn search_returns_results_text_mode() {
    if skip() {
        return;
    }

    let out = headless_search(&["search", "users"], 15);
    assert_success(&out, "search text mode");
    let text = stdout_str(&out);

    // Should contain site name and action names from mock data
    assert!(
        text.contains("example.com"),
        "output should contain site name: {text}"
    );
    assert!(
        text.contains("list_users") || text.contains("List Users"),
        "output should contain action names: {text}"
    );
}

#[test]
fn search_returns_results_json_mode() {
    if skip() {
        return;
    }

    let out = headless_search_json(&["search", "users"], 15);
    assert_success(&out, "search json mode");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");

    // JSON mode outputs the unwrapped data directly (array of sites)
    assert!(data.is_array(), "JSON output should be an array of sites");
    let sites = data.as_array().unwrap();
    assert!(!sites.is_empty(), "should have at least one site");
    assert_eq!(sites[0]["name"], "example.com");
}

#[test]
fn search_empty_results() {
    if skip() {
        return;
    }

    // "notfound" query returns empty results from mock
    let out = headless_search(&["search", "notfound"], 15);
    assert_success(&out, "search empty");
    let text = stdout_str(&out);

    assert!(
        text.contains("No results found")
            || text.contains("No actions found")
            || text.trim().is_empty(),
        "empty search should show no-results message or be empty: {text}"
    );
}

#[test]
fn search_empty_results_json_mode() {
    if skip() {
        return;
    }

    let out = headless_search_json(&["search", "notfound"], 15);
    assert_success(&out, "search empty json");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");
    assert!(data.is_array(), "JSON output should be an array");
    assert!(
        data.as_array().unwrap().is_empty(),
        "empty search should return empty array"
    );
}

#[test]
fn search_missing_query_fails() {
    if skip() {
        return;
    }

    // `actionbook search` without a query argument should fail
    let out = headless_search(&["search"], 15);
    assert_failure(&out, "search missing query");
}

#[test]
fn search_connection_failure_exits_nonzero() {
    if skip() {
        return;
    }

    // Point to an unreachable API to simulate connection failure
    let env: Vec<(&str, &str)> = vec![("ACTIONBOOK_API_URL", "http://127.0.0.1:1")];
    let out = headless_with_env(&["search", "test"], &env, 15);
    assert_failure(&out, "search connection failure");
}
