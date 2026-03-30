//! E2E tests for `browser cookies`.

use crate::harness::{
    SessionGuard, assert_failure, assert_success, headless, headless_json, parse_json, skip,
    stdout_str, url_a,
};

const PRIMARY_COOKIE: &str = "primary_cookie";
const SECONDARY_COOKIE: &str = "secondary_cookie";
const DELETE_COOKIE: &str = "delete_cookie";
const CLEAR_COOKIE: &str = "clear_cookie";
const EXPIRES_TS: &str = "2000000000";

fn start_session(url: &str) -> (String, String) {
    let out = headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "local",
            "--headless",
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

    (sid, tid)
}

fn localhost_url(url: &str) -> String {
    url.replacen("127.0.0.1", "localhost", 1)
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

fn assert_session_context(v: &serde_json::Value, expected_sid: &str) {
    assert!(v["context"].is_object());
    assert_eq!(v["context"]["session_id"], expected_sid);
    assert!(v["context"]["tab_id"].is_null());
}

fn assert_cookie_shape(
    item: &serde_json::Value,
    expected_name: &str,
    expected_value: &str,
    expected_domain_fragment: &str,
    expected_path: &str,
    expected_http_only: bool,
    expected_secure: bool,
    expected_same_site: &str,
) {
    assert_eq!(item["name"], expected_name);
    assert_eq!(item["value"], expected_value);
    assert!(
        item["domain"]
            .as_str()
            .unwrap_or("")
            .contains(expected_domain_fragment),
        "cookie domain must contain {expected_domain_fragment}: got {}",
        item["domain"]
    );
    assert_eq!(item["path"], expected_path);
    assert_eq!(item["http_only"], expected_http_only);
    assert_eq!(item["secure"], expected_secure);
    assert_eq!(item["same_site"], expected_same_site);
    assert!(item["expires"].is_number() || item["expires"].is_null());
}

fn set_cookie(
    sid: &str,
    name: &str,
    value: &str,
    extra_args: &[&str],
    timeout_secs: u64,
) -> serde_json::Value {
    let mut args = vec!["browser", "cookies", "set", name, value, "--session", sid];
    args.extend_from_slice(extra_args);
    let out = headless_json(&args, timeout_secs);
    assert_success(&out, &format!("set cookie {name}"));
    parse_json(&out)
}

#[test]
fn cookies_list_json_happy_path() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    let set_v = set_cookie(
        &sid,
        PRIMARY_COOKIE,
        "alpha",
        &[
            "--domain",
            "127.0.0.1",
            "--path",
            "/",
            "--http-only",
            "--same-site",
            "Lax",
            "--expires",
            EXPIRES_TS,
        ],
        10,
    );
    assert_eq!(set_v["command"], "browser.cookies.set");
    assert_eq!(set_v["data"]["action"], "set");
    assert_eq!(set_v["data"]["affected"], 1);

    let out = headless_json(&["browser", "cookies", "list", "--session", &sid], 10);
    assert_success(&out, "cookies list json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.list");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_session_context(&v, &sid);
    let items = v["data"]["items"]
        .as_array()
        .expect("items must be an array");
    let item = items
        .iter()
        .find(|item| item["name"] == PRIMARY_COOKIE)
        .expect("primary cookie must be listed");
    assert_cookie_shape(
        item,
        PRIMARY_COOKIE,
        "alpha",
        "127.0.0.1",
        "/",
        true,
        false,
        "Lax",
    );
    assert!(
        item["expires"].as_f64().unwrap_or(0.0) >= 2_000_000_000.0,
        "expires should preserve the explicit timestamp: {}",
        item["expires"]
    );
}

#[test]
fn cookies_list_text_output() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_cookie(
        &sid,
        PRIMARY_COOKIE,
        "alpha",
        &["--domain", "127.0.0.1", "--path", "/"],
        10,
    );

    let out = headless(&["browser", "cookies", "list", "--session", &sid], 10);
    assert_success(&out, "cookies list text");
    let text = stdout_str(&out);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.first().copied(), Some(format!("[{sid}]").as_str()));
    assert_eq!(lines.get(1), Some(&"1 cookie"));
    let cookie_line = lines.get(2).copied().unwrap_or_default();
    assert!(
        cookie_line.contains(PRIMARY_COOKIE),
        "missing cookie name: {text}"
    );
    assert!(
        cookie_line.contains("127.0.0.1"),
        "missing cookie domain: {text}"
    );
    assert!(cookie_line.ends_with(" /"), "missing cookie path: {text}");
}

#[test]
fn cookies_list_domain_filter_json() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let local_url = localhost_url(&base_url);
    let (sid, tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_cookie(
        &sid,
        PRIMARY_COOKIE,
        "alpha",
        &["--domain", "127.0.0.1", "--path", "/"],
        10,
    );

    let goto_out = headless_json(
        &[
            "browser",
            "goto",
            &local_url,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        30,
    );
    assert_success(&goto_out, "goto localhost");

    set_cookie(
        &sid,
        SECONDARY_COOKIE,
        "beta",
        &["--domain", "localhost", "--path", "/"],
        10,
    );

    let out = headless_json(
        &[
            "browser",
            "cookies",
            "list",
            "--session",
            &sid,
            "--domain",
            "127.0.0.1",
        ],
        10,
    );
    assert_success(&out, "cookies list domain filter");
    let v = parse_json(&out);
    let items = v["data"]["items"]
        .as_array()
        .expect("items must be an array");

    assert!(
        items.iter().any(|item| item["name"] == PRIMARY_COOKIE),
        "filtered list must keep the 127.0.0.1 cookie: {}",
        v["data"]["items"]
    );
    assert!(
        !items.iter().any(|item| item["name"] == SECONDARY_COOKIE),
        "filtered list must exclude the localhost cookie: {}",
        v["data"]["items"]
    );
}

#[test]
fn cookies_get_json_happy_path() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_cookie(
        &sid,
        PRIMARY_COOKIE,
        "alpha",
        &[
            "--domain",
            "127.0.0.1",
            "--path",
            "/",
            "--http-only",
            "--same-site",
            "Lax",
            "--expires",
            EXPIRES_TS,
        ],
        10,
    );

    let out = headless_json(
        &[
            "browser",
            "cookies",
            "get",
            PRIMARY_COOKIE,
            "--session",
            &sid,
        ],
        10,
    );
    assert_success(&out, "cookies get json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.get");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_session_context(&v, &sid);
    assert_cookie_shape(
        &v["data"]["item"],
        PRIMARY_COOKIE,
        "alpha",
        "127.0.0.1",
        "/",
        true,
        false,
        "Lax",
    );
}

#[test]
fn cookies_get_missing_json() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "cookies",
            "get",
            "missing-cookie",
            "--session",
            &sid,
        ],
        10,
    );
    assert_success(&out, "cookies get missing json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.get");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_session_context(&v, &sid);
    assert!(
        v["data"]["item"].is_null(),
        "missing cookie should return null item"
    );
}

#[test]
fn cookies_set_json_happy_path() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    let out = headless_json(
        &[
            "browser",
            "cookies",
            "set",
            PRIMARY_COOKIE,
            "alpha",
            "--session",
            &sid,
            "--domain",
            "127.0.0.1",
            "--path",
            "/",
            "--http-only",
            "--same-site",
            "Lax",
            "--expires",
            EXPIRES_TS,
        ],
        10,
    );
    assert_success(&out, "cookies set json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.set");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_session_context(&v, &sid);
    assert_eq!(v["data"]["action"], "set");
    assert_eq!(v["data"]["affected"], 1);
    assert_eq!(v["data"]["domain"], "127.0.0.1");
}

#[test]
fn cookies_actions_text_output() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    let set_out = headless(
        &[
            "browser",
            "cookies",
            "set",
            PRIMARY_COOKIE,
            "alpha",
            "--session",
            &sid,
            "--domain",
            "127.0.0.1",
            "--path",
            "/",
        ],
        10,
    );
    assert_success(&set_out, "cookies set text");
    let set_text = stdout_str(&set_out);
    assert!(
        set_text.contains(&format!("[{sid}]")),
        "set text must contain session header: {set_text}"
    );
    assert!(
        set_text.contains("ok browser.cookies.set"),
        "set text must contain ok line: {set_text}"
    );

    let delete_out = headless(
        &[
            "browser",
            "cookies",
            "delete",
            PRIMARY_COOKIE,
            "--session",
            &sid,
        ],
        10,
    );
    assert_success(&delete_out, "cookies delete text");
    let delete_text = stdout_str(&delete_out);
    assert!(delete_text.contains(&format!("[{sid}]")));
    assert!(delete_text.contains("ok browser.cookies.delete"));

    set_cookie(
        &sid,
        CLEAR_COOKIE,
        "gamma",
        &["--domain", "127.0.0.1", "--path", "/"],
        10,
    );

    let clear_out = headless(
        &[
            "browser",
            "cookies",
            "clear",
            "--session",
            &sid,
            "--domain",
            "127.0.0.1",
        ],
        10,
    );
    assert_success(&clear_out, "cookies clear text");
    let clear_text = stdout_str(&clear_out);
    assert!(clear_text.contains(&format!("[{sid}]")));
    assert!(clear_text.contains("ok browser.cookies.clear"));
}

#[test]
fn cookies_delete_json_happy_path() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_cookie(
        &sid,
        DELETE_COOKIE,
        "gone",
        &["--domain", "127.0.0.1", "--path", "/"],
        10,
    );

    let out = headless_json(
        &[
            "browser",
            "cookies",
            "delete",
            DELETE_COOKIE,
            "--session",
            &sid,
        ],
        10,
    );
    assert_success(&out, "cookies delete json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.delete");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_session_context(&v, &sid);
    assert_eq!(v["data"]["action"], "delete");
    assert_eq!(v["data"]["affected"], 1);

    let get_out = headless_json(
        &[
            "browser",
            "cookies",
            "get",
            DELETE_COOKIE,
            "--session",
            &sid,
        ],
        10,
    );
    assert_success(&get_out, "cookies get after delete");
    let get_v = parse_json(&get_out);
    assert!(get_v["data"]["item"].is_null(), "cookie should be deleted");
}

#[test]
fn cookies_clear_json_happy_path() {
    if skip() {
        return;
    }

    let base_url = url_a();
    let (sid, _tid) = start_session(&base_url);
    let _guard = SessionGuard::new(&sid);

    set_cookie(
        &sid,
        CLEAR_COOKIE,
        "gamma",
        &["--domain", "127.0.0.1", "--path", "/"],
        10,
    );

    let out = headless_json(
        &[
            "browser",
            "cookies",
            "clear",
            "--session",
            &sid,
            "--domain",
            "127.0.0.1",
        ],
        10,
    );
    assert_success(&out, "cookies clear json");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.clear");
    assert_eq!(v["ok"], true);
    assert!(v["error"].is_null());
    assert_meta(&v);
    assert_session_context(&v, &sid);
    assert_eq!(v["data"]["action"], "clear");
    assert!(v["data"]["affected"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(v["data"]["domain"], "127.0.0.1");

    let list_out = headless_json(
        &[
            "browser",
            "cookies",
            "list",
            "--session",
            &sid,
            "--domain",
            "127.0.0.1",
        ],
        10,
    );
    assert_success(&list_out, "cookies list after clear");
    let list_v = parse_json(&list_out);
    let items = list_v["data"]["items"].as_array().unwrap();
    assert!(
        !items.iter().any(|item| item["name"] == CLEAR_COOKIE),
        "cleared cookie must be absent from filtered list: {}",
        list_v["data"]["items"]
    );
}

#[test]
fn cookies_session_not_found_json() {
    if skip() {
        return;
    }

    let out = headless_json(
        &["browser", "cookies", "list", "--session", "missing-session"],
        10,
    );
    assert_failure(&out, "cookies missing session");
    let v = parse_json(&out);

    assert_eq!(v["command"], "browser.cookies.list");
    assert!(v["context"].is_null());
    assert_error_envelope(&v, "SESSION_NOT_FOUND");
}
