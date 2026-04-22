use crate::harness::{
    SessionGuard, assert_error_envelope, assert_failure, parse_json, skip, start_session, url_slow,
};

#[test]
fn global_timeout_applies_to_browser_commands() {
    if skip() {
        return;
    }

    let (sid, tid) = start_session("about:blank");
    let _guard = SessionGuard::new(&sid);

    let slow_url = url_slow();
    let out = crate::harness::headless_json(
        &[
            "--timeout",
            "50",
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

    assert_failure(&out, "global timeout on goto");
    let v = parse_json(&out);
    assert_eq!(v["command"], "browser goto");
    assert_error_envelope(&v, "CDP_NAV_TIMEOUT");
    assert_eq!(v["error"]["retryable"], true);
    assert_eq!(v["context"]["session_id"], sid);
    assert_eq!(v["context"]["tab_id"], tid);
}
