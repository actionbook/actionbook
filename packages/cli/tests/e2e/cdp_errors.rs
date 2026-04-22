use crate::harness::{
    SessionGuard, assert_error_envelope, assert_failure, assert_success, headless_json, parse_json,
    skip, start_session, url_slow,
};

#[test]
fn cdp_not_interactable_returns_structured_code() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let setup = headless_json(
        &[
            "browser",
            "eval",
            "document.body.innerHTML = '<button id=\"hidden\" style=\"display:none\">Hidden</button>'",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );
    assert_success(&setup, "install hidden button");

    let out = headless_json(
        &[
            "browser",
            "click",
            "#hidden",
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );

    assert_failure(&out, "hidden click should return a structured CDP code");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser click");
    assert_error_envelope(&v, "CDP_NOT_INTERACTABLE");
    let hint = v["error"]["hint"].as_str().expect("hint string");
    assert!(
        hint.contains("scroll") || hint.contains("visible"),
        "expected not-interactable hint, got {hint:?}"
    );
    assert_eq!(v["error"]["retryable"], false);
    assert!(
        v["error"]["details"]["reason"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "expected non-empty details.reason"
    );
    assert!(
        v["error"]["details"]["cdp_code"].is_i64(),
        "expected integer details.cdp_code"
    );
}

#[test]
fn cdp_nav_timeout_returns_structured_code() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);
    let slow_url = url_slow();

    let out = headless_json(
        &[
            "--timeout",
            "100",
            "browser",
            "goto",
            &slow_url,
            "--session",
            &sid,
            "--tab",
            &tid,
        ],
        10,
    );

    assert_failure(
        &out,
        "slow goto should return a structured nav-timeout code",
    );
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser goto");
    assert_error_envelope(&v, "CDP_NAV_TIMEOUT");
    let hint = v["error"]["hint"].as_str().expect("hint string");
    assert!(
        hint.contains("--timeout") || hint.contains("reachable"),
        "expected nav-timeout hint, got {hint:?}"
    );
    assert_eq!(v["error"]["retryable"], true);
    assert_eq!(v["error"]["details"]["timeout_ms"], 100);
}

#[test]
#[ignore = "TODO(ACT-972): direct-CDP stale-node repro bypassing REF_STALE short-circuit is not yet wired for deterministic E2E"]
fn cdp_node_not_found_returns_structured_code() {
    panic!(
        "TODO(ACT-972): direct-CDP stale-node repro should assert CDP_NODE_NOT_FOUND + snapshot hint + retryable=false"
    );
}

#[test]
#[ignore = "TODO(ACT-972): deterministic target-closed repro needs a dedicated direct-CDP test hook"]
fn cdp_target_closed_returns_structured_code() {
    panic!(
        "TODO(ACT-972): direct-CDP target-close repro should assert CDP_TARGET_CLOSED + retryable=true"
    );
}

#[test]
#[ignore = "TODO(ACT-972): need a deterministic user-facing non-matching CDP error to pin PROTOCOL_ERROR vs CDP_GENERIC in E2E"]
fn cdp_unclassified_falls_back_to_generic() {
    panic!(
        "TODO(ACT-972): add a deterministic fallback E2E once a stable user-facing non-matching CDP error exists"
    );
}
