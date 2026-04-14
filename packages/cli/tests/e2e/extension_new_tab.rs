//! E2E scenarios for extension-mode **new-tab** flows — the paths we just
//! patched or know are still broken.
//!
//! Requires:
//!   RUN_E2E_TESTS=true
//!   RUN_E2E_EXTENSION=true
//!   Chrome running locally with the Actionbook extension loaded and connected
//!   to the bridge at 127.0.0.1:19222 (verify via the extension popup
//!   before running).
//!
//! Not gated into CI — the extension connection and a real Chrome window are
//! not available there.
//!
//! Unix-only because the surrounding harness lsof helper is unix-only.

#![cfg(unix)]

use std::sync::Mutex;
use std::time::Duration;

use crate::harness::{SoloEnv, parse_json, stderr_str, stdout_str, url_a, url_b};

/// Serialize against the global bridge port 19222.
static BRIDGE_PORT_LOCK: Mutex<()> = Mutex::new(());

/// Both env gates must be set, otherwise skip.
fn skip() -> bool {
    std::env::var("RUN_E2E_TESTS")
        .map(|v| v != "true")
        .unwrap_or(true)
        || std::env::var("RUN_E2E_EXTENSION")
            .map(|v| v != "true")
            .unwrap_or(true)
}

fn start_extension(env: &SoloEnv, session_id: &str, open_url: &str) {
    let out = env.headless_json(
        &[
            "browser",
            "start",
            "--mode",
            "extension",
            "--set-session-id",
            session_id,
            "--open-url",
            open_url,
        ],
        30,
    );
    assert!(
        out.status.success(),
        "browser start --mode extension failed — verify Chrome + Actionbook extension are running and popup shows Connected. stderr={}",
        stderr_str(&out)
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 1. Open a single new tab after start, then goto it.
//
// Exercises the fix in `tab/open.rs`:
//   a) extension branch uses Extension.createTab (not Target.createTarget)
//   b) the new tab is register_extension_tab-ed so execute_on_tab can find it
//
// Extension.createTab auto-attaches the new tab, so the very first goto
// against it does not need bridge switching — this should pass today.
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn open_new_tab_then_goto_works() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    let env = SoloEnv::new();

    let session = "ext-open";
    start_extension(&env, session, &url_a());

    let out = env.headless_json(&["browser", "open", &url_b(), "--session", session], 20);
    assert!(
        out.status.success(),
        "browser open (extension) failed: stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v = parse_json(&out);
    assert_eq!(v["ok"], true);
    let new_tab_id = v["data"]["tab"]["tab_id"]
        .as_str()
        .expect("tab_id should be a string")
        .to_string();

    // Goto on the just-opened tab: Extension.createTab auto-attached, so this
    // is also the bridge's currently attached tab.
    let out = env.headless_json(
        &[
            "browser",
            "goto",
            &url_a(),
            "--session",
            session,
            "--tab",
            &new_tab_id,
        ],
        20,
    );
    assert!(
        out.status.success(),
        "goto on the just-opened extension tab failed — register_extension_tab missing? stderr={}",
        stderr_str(&out)
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 2. Multi-tab switching — open a second tab, then operate on the FIRST tab.
//
// Extension.createTab switches the bridge's attachedTabId to the new tab,
// so operating on the original tab now requires the bridge to auto-switch
// via Extension.attachTab before the command. Without that wrapper in
// CdpSession::execute_on_tab, this fails with:
//   "No tab attached. Use Extension.attachTab first."
//
// **Expected to fail until the auto-switch fix lands.**
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn alternate_goto_between_two_tabs_requires_bridge_auto_switch() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    let env = SoloEnv::new();

    let session = "ext-multi";
    start_extension(&env, session, &url_a());
    // t1 is the session's starting tab — currently attached.

    // Open t2 — Extension.createTab switches bridge.attachedTabId to t2.
    let out = env.headless_json(&["browser", "open", &url_b(), "--session", session], 20);
    assert!(out.status.success(), "open second tab failed");
    let t2 = parse_json(&out)["data"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Goto back on t1 — bridge is attached to t2, NOT t1.
    // This is the scenario that currently returns "No tab attached" with the
    // extension-side CDP allowlist.
    let out = env.headless_json(
        &[
            "browser",
            "goto",
            &url_a(),
            "--session",
            session,
            "--tab",
            "t1",
        ],
        20,
    );
    assert!(
        out.status.success(),
        "goto on t1 after bridge switched to t2 failed — execute_on_tab needs an Extension.attachTab wrapper in extension mode. stderr={}",
        stderr_str(&out)
    );

    // And the reverse direction — goto on t2 again should also work, because
    // auto-switch brings bridge back to t2.
    let out = env.headless_json(
        &[
            "browser",
            "goto",
            &url_a(),
            "--session",
            session,
            "--tab",
            &t2,
        ],
        20,
    );
    assert!(
        out.status.success(),
        "goto on t2 after bridge was brought back to t1 failed — auto-switch missing. stderr={}",
        stderr_str(&out)
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 3. batch-new-tab: open multiple tabs at once, then exercise each.
//
// `batch-open` in extension mode loops `Extension.createTab` per URL. The
// bridge's attachedTabId ends up pinned to the LAST created tab, so only
// the last batch tab is directly usable without auto-switch — goto on the
// earlier ones is gated by the same switching gap as test #2.
//
// **Expected to partially fail until auto-switch lands** (last tab OK,
// earlier tabs fail).
// ─────────────────────────────────────────────────────────────────────────
#[test]
fn batch_new_tab_each_tab_usable_after() {
    if skip() {
        return;
    }
    let _g = BRIDGE_PORT_LOCK.lock().unwrap();
    let env = SoloEnv::new();

    let session = "ext-batch";
    start_extension(&env, session, &url_a());

    let out = env.headless_json(
        &[
            "browser",
            "batch-new-tab",
            "--urls",
            &url_a(),
            &url_b(),
            "--session",
            session,
        ],
        30,
    );
    assert!(
        out.status.success(),
        "batch-new-tab (extension) failed: stderr={}",
        stderr_str(&out)
    );
    let v = parse_json(&out);
    let tabs = v["data"]["tabs"]
        .as_array()
        .cloned()
        .expect("data.tabs should be an array");
    assert_eq!(tabs.len(), 2, "expected 2 tabs opened, got {}", tabs.len());

    let tab_ids: Vec<String> = tabs
        .iter()
        .map(|t| t["tab_id"].as_str().unwrap().to_string())
        .collect();

    // Try goto on each. Under the current (no auto-switch) implementation,
    // only the last tab is bridge-attached, so the first goto will fail.
    let mut failures = Vec::new();
    for tid in &tab_ids {
        let out = env.headless_json(
            &[
                "browser",
                "goto",
                &url_a(),
                "--session",
                session,
                "--tab",
                tid,
            ],
            20,
        );
        if !out.status.success() {
            failures.push(format!("tab {tid}: {}", stderr_str(&out)));
        }
        // small delay so consecutive attachTab switches on the bridge settle
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        failures.is_empty(),
        "goto failed on {}/{} batch-opened extension tabs:\n  {}",
        failures.len(),
        tab_ids.len(),
        failures.join("\n  ")
    );
}
