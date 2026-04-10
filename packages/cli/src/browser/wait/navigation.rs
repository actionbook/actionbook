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

#[derive(Debug, Clone, PartialEq, Eq)]
struct NavigationDetector {
    /// URL recorded by the registry at the time the previous command completed.
    /// If the current URL already differs from this at startup, navigation has
    /// already completed before wait-navigation began.
    prev_url: String,
    frame_navigated_seen: bool,
    loading_seen: bool,
    url_changed_seen: bool,
    /// The URL observed the last time readyState was "complete". Used to confirm
    /// that the page has STABILISED at the new URL before accepting.
    ///
    /// When we first see `url_changed_seen + complete` we record this URL.
    /// Only when the NEXT poll also shows the same URL + complete do we accept.
    /// This prevents accepting the intermediate page in a delayed-redirect chain
    /// (e.g. `/redirect-delayed` loads to complete and then the JS timer fires
    /// to navigate to `/page-b`; we must wait for `/page-b + complete`).
    ///
    /// Reset to None whenever a navigation signal indicates the page is in flux
    /// (frameNavigated event or readyState != "complete").
    last_complete_url: Option<String>,
}

impl NavigationDetector {
    fn new(prev_url: String) -> Self {
        Self {
            prev_url,
            frame_navigated_seen: false,
            loading_seen: false,
            url_changed_seen: false,
            last_complete_url: None,
        }
    }

    /// Feed a signal into the detector.  Returns true when navigation is done
    /// (a navigation event occurred and the page has fully loaded).
    fn observe(&mut self, signal: NavigationSignal) -> bool {
        match signal {
            NavigationSignal::FrameNavigated => {
                self.frame_navigated_seen = true;
                // A new navigation started — any previously recorded stable URL is
                // no longer the final destination.
                self.last_complete_url = None;
                false
            }
            NavigationSignal::Poll { url, ready_state } => {
                if url != self.prev_url {
                    self.url_changed_seen = true;
                }
                if ready_state != "complete" {
                    self.loading_seen = true;
                    // Page is in flux — reset the stability tracker.
                    self.last_complete_url = None;
                    return false;
                }
                // readyState == "complete"
                //
                // Strong signals: a CDP event arrived, or we caught the page mid-load.
                // Accept immediately — no stability confirmation needed.
                if self.frame_navigated_seen || self.loading_seen {
                    return true;
                }
                // Weak signal: URL differs from the registry baseline but we have
                // no in-watch navigation signal yet.  This happens for fast-redirect
                // (navigation completed before wait started) but also for intermediate
                // pages in a delayed-redirect chain.
                //
                // Require URL stability: accept only when this URL appears in two
                // consecutive complete polls.  An intermediate redirect page will
                // change its URL again before the second poll; the final destination
                // will remain stable.
                if self.url_changed_seen {
                    if self.last_complete_url.as_deref() == Some(url.as_str()) {
                        // URL was the same in the previous complete poll → stable.
                        return true;
                    }
                    // First time seeing this URL at complete — record and wait.
                    self.last_complete_url = Some(url);
                    return false;
                }
                // No navigation signal at all — don't accept.
                false
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

    // Read the tab URL that was recorded when the previous command completed.
    // This is the reliable baseline: if the live URL already differs from this,
    // navigation completed between the last command and now (fast-redirect case).
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

    let mut detector = NavigationDetector::new(prev_url);
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

                    if detector.observe(NavigationSignal::Poll {
                        url: current_url.clone(),
                        ready_state: ready_state.clone(),
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

    /// Original #17 test: page was mid-load when watch started (same URL as baseline).
    /// Covers delayed/in-flight redirect caught while loading.
    #[test]
    fn navigation_detector_accepts_already_navigated_final_url_once_load_completes() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/final".to_string());

        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "loading".to_string(),
        }));
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Original #17 test: CDP frameNavigated event arrives, then page reaches complete.
    #[test]
    fn navigation_detector_accepts_frame_navigated_event_then_complete_poll() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/page-a".to_string());

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

    /// Original #17 test: URL matches baseline, no events → must NOT succeed.
    #[test]
    fn navigation_detector_rejects_complete_poll_without_any_navigation_signal() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/page-a".to_string());

        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/page-a".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Fast-redirect case: navigation completed before watch started.
    /// Registry has old URL; first poll already shows final URL at complete.
    /// Requires two consecutive complete polls at the same URL to confirm stability
    /// (guards against intermediate pages in delayed-redirect chains).
    #[test]
    fn navigation_detector_accepts_already_navigated_via_url_baseline_diff_after_stable_polls() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/old".to_string());

        // First complete poll at final URL — recorded but not yet confirmed stable.
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "complete".to_string(),
        }));
        // Second consecutive poll at the same URL — stable → accept.
        assert!(detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/final".to_string(),
            ready_state: "complete".to_string(),
        }));
    }

    /// Delayed-redirect chain: intermediate page reaches complete but then
    /// a frameNavigated event fires for the real destination.
    /// Must NOT accept on the intermediate page.
    #[test]
    fn navigation_detector_rejects_intermediate_page_and_accepts_after_frame_navigated() {
        let mut detector = NavigationDetector::new("http://127.0.0.1/old".to_string());

        // Intermediate page complete — url changed but we record and wait.
        assert!(!detector.observe(NavigationSignal::Poll {
            url: "http://127.0.0.1/redirect-delayed".to_string(),
            ready_state: "complete".to_string(),
        }));
        // JS redirect fires → frameNavigated event.
        assert!(!detector.observe(NavigationSignal::FrameNavigated));
        // Page is now loading the final destination.
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
