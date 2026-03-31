//! E2E tests for `browser session-storage` and `browser local-storage`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, unique_session, url_a, wait_page_ready,
};

#[derive(Clone, Copy)]
struct StorageKind {
    cli_name: &'static str,
    data_name: &'static str,
}

const LOCAL: StorageKind = StorageKind {
    cli_name: "local-storage",
    data_name: "local",
};

const SESSION: StorageKind = StorageKind {
    cli_name: "session-storage",
    data_name: "session",
};

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

fn assert_tab_context(v: &serde_json::Value, expected_sid: &str, expected_tid: &str, url: &str) {
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], expected_sid);
    assert_eq!(v["context"]["tab_id"], expected_tid);
    assert!(
        v["context"]["url"].as_str().unwrap_or("").contains(url),
        "context.url must contain {url}: got {}",
        v["context"]["url"]
    );
}

fn assert_storage_item(item: &serde_json::Value, expected_key: &str, expected_value: &str) {
    assert_eq!(item["key"], expected_key);
    assert_eq!(item["value"], expected_value);
}

fn command_name(kind: StorageKind, op: &str) -> String {
    format!("browser {} {}", kind.cli_name, op)
}

fn set_storage(
    kind: StorageKind,
    sid: &str,
    tid: &str,
    key: &str,
    value: &str,
) -> serde_json::Value {
    let out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "set",
            key,
            value,
            "--session",
            sid,
            "--tab",
            tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} set {key}", kind.cli_name));
    parse_json(&out)
}

#[test]
fn local_storage_list_json_happy_path() {
    storage_list_json_happy_path(LOCAL, "theme", "dark");
}

#[test]
fn session_storage_list_json_happy_path() {
    storage_list_json_happy_path(SESSION, "token", "abc123");
}

fn storage_list_json_happy_path(kind: StorageKind, key: &str, value: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_storage(kind, &sid, &tid, key, value);

    let out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "list",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} list json", kind.cli_name));
    let v = parse_json(&out);

    assert_eq!(v["command"], command_name(kind, "list"));
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_tab_context(&v, &sid, &tid, &base_url);
    assert_eq!(v["data"]["storage"], kind.data_name);
    let items = v["data"]["items"]
        .as_array()
        .expect("data.items must be an array");
    let item = items
        .iter()
        .find(|item| item["key"] == key)
        .expect("storage item must be listed");
    assert_storage_item(item, key, value);
}

#[test]
fn local_storage_list_text_output() {
    storage_list_text_output(LOCAL, "theme", "dark");
}

#[test]
fn session_storage_list_text_output() {
    storage_list_text_output(SESSION, "token", "abc123");
}

fn storage_list_text_output(kind: StorageKind, key: &str, value: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_storage(kind, &sid, &tid, key, value);

    let out = headless(
        &[
            "browser",
            kind.cli_name,
            "list",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} list text", kind.cli_name));
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    let header = lines.first().copied().unwrap_or_default();
    assert!(
        header.starts_with(&format!("[{sid} {tid}]")),
        "header must start with [session tab]: {text}"
    );
    assert!(
        header.contains(&base_url),
        "header must contain url: {text}"
    );
    assert_eq!(lines.get(1), Some(&"1 key"));
    assert_eq!(lines.get(2), Some(&format!("{key}={value}").as_str()));
}

#[test]
fn local_storage_get_json_happy_path() {
    storage_get_json_happy_path(LOCAL, "theme", "dark");
}

#[test]
fn session_storage_get_json_happy_path() {
    storage_get_json_happy_path(SESSION, "token", "abc123");
}

fn storage_get_json_happy_path(kind: StorageKind, key: &str, value: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_storage(kind, &sid, &tid, key, value);

    let out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "get",
            key,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} get json", kind.cli_name));
    let v = parse_json(&out);

    assert_eq!(v["command"], command_name(kind, "get"));
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_tab_context(&v, &sid, &tid, &base_url);
    assert_eq!(v["data"]["storage"], kind.data_name);
    assert_storage_item(&v["data"]["item"], key, value);
}

#[test]
fn local_storage_get_missing_json() {
    storage_get_missing_json(LOCAL, "missing-local-key");
}

#[test]
fn session_storage_get_missing_json() {
    storage_get_missing_json(SESSION, "missing-session-key");
}

fn storage_get_missing_json(kind: StorageKind, key: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "get",
            key,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} get missing json", kind.cli_name));
    let v = parse_json(&out);

    assert_eq!(v["command"], command_name(kind, "get"));
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_tab_context(&v, &sid, &tid, &base_url);
    assert_eq!(v["data"]["storage"], kind.data_name);
    assert!(
        v["data"]["item"].is_null(),
        "missing key should return null item"
    );
}

#[test]
fn local_storage_set_json_happy_path() {
    storage_set_json_happy_path(LOCAL, "theme", "dark");
}

#[test]
fn session_storage_set_json_happy_path() {
    storage_set_json_happy_path(SESSION, "token", "abc123");
}

fn storage_set_json_happy_path(kind: StorageKind, key: &str, value: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    let v = set_storage(kind, &sid, &tid, key, value);
    assert_eq!(v["command"], command_name(kind, "set"));
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_tab_context(&v, &sid, &tid, &base_url);
    assert_eq!(v["data"]["storage"], kind.data_name);
    assert_eq!(v["data"]["action"], "set");
    assert_eq!(v["data"]["affected"], 1);
}

#[test]
fn local_storage_delete_json_happy_path() {
    storage_delete_json_happy_path(LOCAL, "delete-local-key", "gone");
}

#[test]
fn session_storage_delete_json_happy_path() {
    storage_delete_json_happy_path(SESSION, "delete-session-key", "gone");
}

fn storage_delete_json_happy_path(kind: StorageKind, key: &str, value: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_storage(kind, &sid, &tid, key, value);

    let out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "delete",
            key,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} delete json", kind.cli_name));
    let v = parse_json(&out);

    assert_eq!(v["command"], command_name(kind, "delete"));
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_tab_context(&v, &sid, &tid, &base_url);
    assert_eq!(v["data"]["storage"], kind.data_name);
    assert_eq!(v["data"]["action"], "delete");
    assert_eq!(v["data"]["affected"], 1);

    let get_out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "get",
            key,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&get_out, &format!("{} get after delete", kind.cli_name));
    let get_v = parse_json(&get_out);
    assert!(get_v["data"]["item"].is_null(), "key should be deleted");
}

#[test]
fn local_storage_clear_json_happy_path() {
    storage_clear_json_happy_path(LOCAL, "clear-local-key", "gone");
}

#[test]
fn session_storage_clear_json_happy_path() {
    storage_clear_json_happy_path(SESSION, "clear-session-key", "gone");
}

fn storage_clear_json_happy_path(kind: StorageKind, key: &str, value: &str) {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_storage(kind, &sid, &tid, key, value);

    let out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "clear",
            key,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&out, &format!("{} clear json", kind.cli_name));
    let v = parse_json(&out);

    assert_eq!(v["command"], command_name(kind, "clear"));
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_tab_context(&v, &sid, &tid, &base_url);
    assert_eq!(v["data"]["storage"], kind.data_name);
    assert_eq!(v["data"]["action"], "clear");
    assert_eq!(v["data"]["affected"], 1);

    let get_out = headless_json(
        &[
            "browser",
            kind.cli_name,
            "get",
            key,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&get_out, &format!("{} get after clear", kind.cli_name));
    let get_v = parse_json(&get_out);
    assert!(get_v["data"]["item"].is_null(), "key should be cleared");
}

#[test]
fn local_storage_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &[
            "browser",
            "local-storage",
            "list",
            "--session",
            "missing-session",
            "--tab",
            "t1",
        ],
        10,
    );
    assert_failure(&out, "local-storage missing session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser local-storage list");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}

#[test]
fn session_storage_tab_not_found_json() {
    if skip() {
        return;
    }

    let (sid, _tid) = start_session(&url_a());
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "session-storage",
            "list",
            "--session",
            &sid,
            "--tab",
            "missing-tab",
        ],
        10,
    );
    assert_failure(&out, "session-storage missing tab");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser session-storage list");
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], sid);
    assert!(v["context"]["tab_id"].is_null());
    assert_error_envelope(&v, "TAB_NOT_FOUND");
}
