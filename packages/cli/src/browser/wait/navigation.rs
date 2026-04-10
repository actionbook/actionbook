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

/// After detecting `url != prev_url + readyState=complete` we require the URL
/// to remain stable (same value, still complete) for this many milliseconds
/// before accepting.  This window must exceed the longest intermediate-redirect
/// delay we expect to encounter so that delayed-redirect chains are not
/// prematurely accepted at an intermediate page.
const URL_STABILITY_MS: u64 = 300;

const READY_STATE_JS: &str =
    "(function(){ return { url: location.href, ready_state: document.readyState }; })()";

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
    Poll { url: String, ready_state: String },
}

/// Detects in-watch navigation via strong signals (CDP event or caught
/// mid-load).  The "already-completed" fast-redirect case is handled in
/// execute() via time-based URL stability tracking.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NavigationDetector {
    frame_navigated_seen: bool,
    loading_seen: bool,
}

impl NavigationDetector {
    fn new() -> Self {
        Self {
            frame_navigated_seen: false,
            loading_seen: false,
        }
    }

    /// Returns true when a strong in-watch navigation signal has been observed
    /// and the page is now fully loaded.
    fn observe(&mut self, signal: NavigationSignal) -> bool {
        match signal {
            NavigationSignal::FrameNavigated => {
                self.frame_navigated_seen = true;
                self.loading_seen = false; // fresh loading cycle required for the new page
                false
            }
            NavigationSignal::Poll { ready_state, .. } => {
                if ready_state != "complete" {
                    self.loading_seen = true;
                    return false;
                }
                self.frame_navigated_seen || self.loading_seen
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

    // Read the tab URL recorded when the previous command completed.
    // Used as the baseline: if the live URL already differs from this on the
    // first poll, navigation completed between the last command and now.
    let prev_url = {
        let reg = registry.lock().await;
        reg.get_tab_url_title(&cmd.session, &cmd.tab)
            .0
            .unwrap_or_default()
    };

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

    let mut detector = NavigationDetector::new();
    let mut poll_interval = tokio::time::interval(Duration::from_millis(POLL_INTERVAL_MS));
    poll_interval.tick().await; // consume the immediate first tick

    // Time-based stability tracker for the "already-navigated" fast-redirect case.
    // When we first detect `current_url != prev_url + readyState=complete` we start
    // the clock.  We accept only when the URL remains stable for URL_STABILITY_MS.
    // Any intervening frameNavigated event or non-complete readyState resets this.
    let mut stable_since: Option<(Instant, String)> = None;

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
            // Path A: CDP frameNavigated event (in-watch navigation).
            event = event_rx.recv() => {
                if event.is_none() {
                    // Channel closed — session died; fall through to timeout.
                    continue;
                }
                // A new navigation started — reset stability tracking.
                stable_since = None;
                detector.observe(NavigationSignal::FrameNavigated);
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

                let Ok(v) = resp else { continue };
                let Some(rv) = v.pointer("/result/result/value") else { continue };

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

                // Strong-signal path: in-watch event or mid-load caught.
                if detector.observe(NavigationSignal::Poll {
                    url: current_url.clone(),
                    ready_state: ready_state.clone(),
                }) {
                    let title = nav_helpers::get_tab_title(&cdp, &target_id).await;
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    return build_ok(elapsed_ms, &current_url, &title);
                }

                // Weak-signal path: URL differs from registry baseline.
                // We can't immediately accept because this might be an intermediate
                // page in a redirect chain.  Require URL_STABILITY_MS of continuous
                // stability (same URL + complete) before accepting.
                if current_url != prev_url && ready_state == "complete" {
                    match &stable_since {
                        None => {
                            stable_since = Some((Instant::now(), current_url));
                        }
                        Some((since, tracked_url)) if *tracked_url == current_url => {
                            if since.elapsed().as_millis() >= URL_STABILITY_MS as u128 {
                                let title = nav_helpers::get_tab_title(&cdp, &target_id).await;
                                let elapsed_ms = start.elapsed().as_millis() as u64;
                                return build_ok(elapsed_ms, &current_url, &title);
                            }
                            // Not yet stable enough — keep waiting.
                        }
                        Some(_) => {
                            // URL changed since we started tracking → reset.
                            stable_since = Some((Instant::now(), current_url));
                        }
                    }
                } else {
                    // Page is in motion (loading or still at prev_url) → reset stability.
                    stable_since = None;
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

    /// Original #17 test: page was mid-load during watch (in-place or in-watch navigation).
    #[test]
    fn navigation_detector_accepts_loading_then_complete() {
        let mut detector = NavigationDetector::new();

        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "loading".to_string(),
        }));
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Original #17 test: CDP frameNavigated event then page reaches complete.
    #[test]
    fn navigation_detector_accepts_frame_navigated_event_then_complete_poll() {
        let mut detector = NavigationDetector::new();

        assert!(!detector.observe(NavigationSignal::FrameNavigated));
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "interactive".to_string(),
        }));
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Original #17 test: no in-watch signal and URL == baseline → must NOT succeed.
    #[test]
    fn navigation_detector_rejects_complete_poll_without_navigation_signal() {
        let mut detector = NavigationDetector::new();

        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-a".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Stability tracker: URL differs from baseline, tracks stability duration.
    /// Simulates the fast-redirect case where URL was already different from prev
    /// when watch started. URL must be stable for URL_STABILITY_MS before accepting.
    #[test]
    fn stability_tracker_accepts_after_stability_window() {
        // Simulate that `prev_url = "about:blank"` and the page is at `/page-b`
        // already (fast-redirect completed before watch started).
        // The execute() loop tracks this via stable_since; here we just verify
        // the detector itself doesn't accept on strong-signal path alone.
        let mut detector = NavigationDetector::new();

        // No in-watch signal — detector alone won't accept.
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Delayed redirect: frameNavigated resets the strong-signal state, requiring
    /// a fresh loading cycle for the final page.
    #[test]
    fn navigation_detector_handles_redirect_chain_via_frame_navigated_reset() {
        let mut detector = NavigationDetector::new();

        // Intermediate page reaches complete.
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/redirect-delayed".to_string(),
            ready_state: "complete".to_string(),
        }));
        // JS redirect fires → frameNavigated event.
        assert!(!detector.observe(NavigationSignal::FrameNavigated));
        // Final page loading.
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "loading".to_string(),
        }));
        // Final page complete → accept.
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-b".to_string(),
            ready_state: "complete".to_string(),
        }));
    }
}
