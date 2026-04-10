use std::time::{Duration, Instant};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation as nav_helpers;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const POLL_INTERVAL_MS: u64 = 100;

/// Pages that finished loading within this many ms are treated as "recently
/// navigated" even when no CDP frameNavigated event was observed.  Covers the
/// fast-redirect race where navigation completes before wait-navigation starts.
const RECENTLY_LOADED_GRACE_MS: i64 = 3_000;

/// JS expression that returns the current URL, readyState, and how many ms
/// ago the page's load event fired (null if load has not yet finished or the
/// timing API is unavailable).
const READY_STATE_JS: &str = "(function(){
    var t = window.performance && window.performance.timing;
    var loadAge = (t && t.loadEventEnd > 0) ? (Date.now() - t.loadEventEnd) : null;
    return { url: location.href, ready_state: document.readyState, load_age_ms: loadAge };
})()";

/// Wait for a navigation to complete
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait navigation --session s1 --tab t1 --timeout 10000")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Timeout in milliseconds (default 30000)
    #[arg(long)]
    pub timeout: Option<u64>,
}

pub const COMMAND_NAME: &str = "browser wait navigation";

#[derive(Debug, Clone, PartialEq, Eq)]
enum NavigationSignal {
    FrameNavigated,
    Poll {
        url: String,
        ready_state: String,
        /// Milliseconds since the page's load event fired.  None when the
        /// timing API is unavailable or the page hasn't loaded yet.
        load_age_ms: Option<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NavigationDetector {
    initial_url: String,
    frame_navigated_seen: bool,
    /// Set when we observe a poll with readyState != "complete", meaning the
    /// page is mid-load.  This lets us accept the subsequent "complete" as a
    /// navigation signal even when the URL hasn't changed (already-navigated
    /// case where we caught the page mid-transition).
    loading_seen: bool,
}

impl NavigationDetector {
    fn new(initial_url: String) -> Self {
        Self {
            initial_url,
            frame_navigated_seen: false,
            loading_seen: false,
        }
    }

    /// Feed a signal into the detector.  Returns true when navigation is done
    /// (a navigation event occurred and the page has fully loaded).
    fn observe(&mut self, signal: NavigationSignal) -> bool {
        match signal {
            NavigationSignal::FrameNavigated => {
                self.frame_navigated_seen = true;
                false
            }
            NavigationSignal::Poll {
                url,
                ready_state,
                load_age_ms,
            } => {
                if ready_state != "complete" {
                    self.loading_seen = true;
                    return false;
                }
                // readyState == "complete": accept if any of:
                // 1. A frameNavigated CDP event arrived.
                // 2. We saw the page mid-load (loading_seen).
                // 3. URL moved away from the initial snapshot.
                // 4. The page finished loading very recently — covers the
                //    fast-redirect race where navigation completed before
                //    wait-navigation started and no CDP event was observed.
                let recently_loaded =
                    load_age_ms.is_some_and(|age| age >= 0 && age <= RECENTLY_LOADED_GRACE_MS);
                self.frame_navigated_seen
                    || self.loading_seen
                    || url != self.initial_url
                    || recently_loaded
            }
        }
    }
}

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let timeout_ms = cmd.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);
    let start = Instant::now();

    // Resolve the flat CDP session ID needed for event subscription.
    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(sid) => sid,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!("no CDP session for target '{target_id}'"),
            );
        }
    };

    // Subscribe BEFORE Page.enable to avoid missing events fired during enable.
    let mut event_rx = cdp
        .subscribe_events(&cdp_session_id, "Page.frameNavigated")
        .await;

    // Page.enable is idempotent — required for Page.frameNavigated events.
    let _ = cdp
        .execute_on_tab(&target_id, "Page.enable", json!({}))
        .await;

    // Drain stale events that Page.enable may replay from the already-loaded page.
    while event_rx.try_recv().is_ok() {}

    // Capture the initial URL using location.href (consistent with poll JS below).
    let initial_url = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({"expression": "location.href", "returnByValue": true}),
        )
        .await
        .ok()
        .and_then(|v| v["result"]["result"]["value"].as_str().map(String::from))
        .unwrap_or_default();

    let mut detector = NavigationDetector::new(initial_url);
    let mut poll_interval = tokio::time::interval(Duration::from_millis(POLL_INTERVAL_MS));
    poll_interval.tick().await; // consume the immediate first tick

    loop {
        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= timeout_ms {
            return ActionResult::fatal_with_hint(
                "TIMEOUT",
                format!("navigation not detected within {}ms", timeout_ms),
                "check that navigation is triggered or increase --timeout",
            );
        }

        tokio::select! {
            // Path A: CDP frameNavigated event.
            event = event_rx.recv() => {
                if event.is_none() {
                    // Channel closed — session died; fall through to timeout.
                    continue;
                }
                if detector.observe(NavigationSignal::FrameNavigated) {
                    // Already done (shouldn't happen on FrameNavigated alone, but be safe).
                    let title = nav_helpers::get_tab_title(&cdp, &target_id).await;
                    let url = nav_helpers::get_tab_url(&cdp, &target_id).await;
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    return build_ok(elapsed_ms, &url, &title);
                }
            }

            // Path B: polling fallback.
            _ = poll_interval.tick() => {
                let resp = cdp
                    .execute_on_tab(
                        &target_id,
                        "Runtime.evaluate",
                        json!({ "expression": READY_STATE_JS, "returnByValue": true }),
                    )
                    .await;

                if let Ok(v) = resp
                    && let Some(rv) = v.pointer("/result/result/value")
                {
                    let current_url = rv
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ready_state = rv
                        .get("ready_state")
                        .and_then(|r| r.as_str())
                        .unwrap_or("")
                        .to_string();
                    let load_age_ms = rv.get("load_age_ms").and_then(|v| v.as_i64());

                    if detector.observe(NavigationSignal::Poll {
                        url: current_url.clone(),
                        ready_state: ready_state.clone(),
                        load_age_ms,
                    }) {
                        let title = nav_helpers::get_tab_title(&cdp, &target_id).await;
                        let elapsed_ms = start.elapsed().as_millis() as u64;
                        return build_ok(elapsed_ms, &current_url, &title);
                    }
                }
            }
        }
    }
}

fn build_ok(elapsed_ms: u64, url: &str, title: &str) -> ActionResult {
    ActionResult::ok(json!({
        "kind": "navigation",
        "satisfied": true,
        "elapsed_ms": elapsed_ms,
        "observed_value": {
            "url": url,
            "ready_state": "complete",
        },
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_detector_accepts_already_navigated_final_url_once_load_completes() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/final".to_string());

        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "loading".to_string(),
            load_age_ms: None,
        }));
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "complete".to_string(),
            load_age_ms: None,
        }));
    }

    #[test]
    fn navigation_detector_accepts_frame_navigated_event_then_complete_poll() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/page-a".to_string());

        assert!(!detector.observe(NavigationSignal::FrameNavigated));
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "interactive".to_string(),
            load_age_ms: None,
        }));
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "complete".to_string(),
            load_age_ms: None,
        }));
    }

    #[test]
    fn navigation_detector_rejects_complete_poll_without_any_navigation_signal() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/page-a".to_string());

        // Old load (5 seconds ago) with same URL and no navigation signal → must NOT succeed.
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-a".to_string(),
            ready_state: "complete".to_string(),
            load_age_ms: Some(5_000),
        }));
    }

    #[test]
    fn navigation_detector_accepts_recently_loaded_page_covering_fast_redirect() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/final".to_string());

        // Navigation completed 50 ms before wait-navigation started, so the
        // first poll already shows "complete" at the final URL.  The load-age
        // heuristic must fire here because no frameNavigated event or loading
        // state was observed.
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "complete".to_string(),
            load_age_ms: Some(50),
        }));
    }
}
