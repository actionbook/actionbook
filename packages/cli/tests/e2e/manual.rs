//! E2E tests for `actionbook manual` command.
//!
//! Uses a mock API server (local HTTP fixture) to validate the full CLI
//! invocation for all three levels of progressive discovery.

use crate::harness::{
    api_base_url, assert_failure, assert_success, headless_json_with_env, headless_with_env, skip,
    stderr_str, stdout_str,
};

fn api_env() -> Vec<(&'static str, String)> {
    vec![("ACTIONBOOK_API_URL", api_base_url())]
}

fn headless_manual(args: &[&str], timeout_secs: u64) -> std::process::Output {
    let env = api_env();
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    headless_with_env(args, &env_refs, timeout_secs)
}

fn headless_manual_json(args: &[&str], timeout_secs: u64) -> std::process::Output {
    let env = api_env();
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    headless_json_with_env(args, &env_refs, timeout_secs)
}

// ── L1: Site Overview ────────────────────────────────────────────────

#[test]
fn manual_site_overview_text() {
    if skip() {
        return;
    }

    let out = headless_manual(&["manual", "example.com"], 15);
    assert_success(&out, "manual L1 text");
    let text = stdout_str(&out);

    // L1 overview should show site name, groups, and action counts
    assert!(
        text.contains("example.com"),
        "should contain site name: {text}"
    );
    assert!(
        text.contains("users"),
        "should contain group name 'users': {text}"
    );
    assert!(
        text.contains("posts"),
        "should contain group name 'posts': {text}"
    );
}

#[test]
fn manual_site_overview_json() {
    if skip() {
        return;
    }

    let out = headless_manual_json(&["manual", "example.com"], 15);
    assert_success(&out, "manual L1 json");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");

    assert_eq!(data["name"], "example.com");
    assert!(data["groups"].is_array(), "groups should be an array");
    let groups = data["groups"].as_array().unwrap();
    assert!(groups.len() >= 2, "should have at least 2 groups");
}

// ── L2: Group Overview ───────────────────────────────────────────────

#[test]
fn manual_group_overview_text() {
    if skip() {
        return;
    }

    let out = headless_manual(&["manual", "example.com", "users"], 15);
    assert_success(&out, "manual L2 text");
    let text = stdout_str(&out);

    // L2 shows group name and action table with method/path
    assert!(text.contains("users"), "should contain group name: {text}");
    assert!(
        text.contains("list_users") || text.contains("List Users"),
        "should contain action name: {text}"
    );
    assert!(text.contains("GET"), "should contain HTTP method: {text}");
}

#[test]
fn manual_group_overview_json() {
    if skip() {
        return;
    }

    let out = headless_manual_json(&["manual", "example.com", "users"], 15);
    assert_success(&out, "manual L2 json");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");

    assert_eq!(data["group"], "users");
    assert!(data["actions"].is_array(), "actions should be an array");
    let actions = data["actions"].as_array().unwrap();
    assert!(!actions.is_empty(), "should have actions");
    assert_eq!(actions[0]["name"], "list_users");
    assert_eq!(actions[0]["method"], "GET");
}

// ── L3: Action Detail ────────────────────────────────────────────────

#[test]
fn manual_action_detail_text() {
    if skip() {
        return;
    }

    let out = headless_manual(&["manual", "example.com", "users", "list_users"], 15);
    assert_success(&out, "manual L3 text");
    let text = stdout_str(&out);

    // L3 shows full action details including method, path, parameters
    assert!(
        text.contains("list_users") || text.contains("List Users"),
        "should contain action name: {text}"
    );
    assert!(text.contains("GET"), "should contain HTTP method: {text}");
    assert!(text.contains("/users"), "should contain path: {text}");
    // Parameters should be shown
    assert!(
        text.contains("page") || text.contains("limit"),
        "should contain parameter names: {text}"
    );
}

#[test]
fn manual_action_detail_json() {
    if skip() {
        return;
    }

    let out = headless_manual_json(&["manual", "example.com", "users", "list_users"], 15);
    assert_success(&out, "manual L3 json");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");

    assert_eq!(data["site"], "example.com");
    assert_eq!(data["action"], "list_users");
    assert_eq!(data["method"], "GET");
    assert_eq!(data["path"], "/users");
    assert!(
        data["parameters"].is_array(),
        "parameters should be an array"
    );
}

// ── Parameter forwarding ─────────────────────────────────────────────

#[test]
fn manual_group_param_forwarded_correctly() {
    if skip() {
        return;
    }

    // Use "posts" group — mock echoes the actual group param, so if the CLI
    // sends the wrong value the assertion will catch it.
    let out = headless_manual_json(&["manual", "example.com", "posts"], 15);
    assert_success(&out, "manual L2 posts");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");

    assert_eq!(
        data["group"], "posts",
        "mock should echo the requested group param"
    );
}

#[test]
fn manual_action_param_forwarded_correctly() {
    if skip() {
        return;
    }

    // Use "posts"/"create_post" — mock echoes actual group+action params
    let out = headless_manual_json(&["manual", "example.com", "posts", "create_post"], 15);
    assert_success(&out, "manual L3 create_post");

    let text = stdout_str(&out);
    let data: serde_json::Value = serde_json::from_str(&text).expect("output should be valid JSON");

    assert_eq!(
        data["group"], "posts",
        "mock should echo the requested group param"
    );
    assert_eq!(
        data["action"], "create_post",
        "mock should echo the requested action param"
    );
}

// ── Edge cases ───────────────────────────────────────────────────────

#[test]
fn manual_no_site_exits_with_error() {
    if skip() {
        return;
    }

    // `actionbook manual` without site should exit non-zero
    let out = headless_manual(&["manual"], 15);
    assert_failure(&out, "manual no site");

    let err = stderr_str(&out);
    assert!(
        err.contains("show_help") || err.contains("INTERNAL_ERROR"),
        "should show error: {err}"
    );
}

#[test]
fn manual_not_found_site_shows_error() {
    if skip() {
        return;
    }

    let out = headless_manual(&["manual", "notfound"], 15);
    assert_failure(&out, "manual not found");

    let err = stderr_str(&out);
    assert!(
        err.contains("not found") || err.contains("NOT_FOUND"),
        "should show not-found error: {err}"
    );
}

#[test]
fn manual_alias_man_works() {
    if skip() {
        return;
    }

    // `man` should be an alias for `manual`
    let out = headless_manual(&["man", "example.com"], 15);
    assert_success(&out, "manual alias man");
    let text = stdout_str(&out);

    assert!(
        text.contains("example.com"),
        "man alias should work like manual: {text}"
    );
}

#[test]
fn manual_connection_failure_exits_nonzero() {
    if skip() {
        return;
    }

    let env: Vec<(&str, &str)> = vec![("ACTIONBOOK_API_URL", "http://127.0.0.1:1")];
    let out = headless_with_env(&["manual", "example.com"], &env, 15);
    assert_failure(&out, "manual connection failure");
}
