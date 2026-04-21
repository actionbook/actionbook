use std::time::{Duration, Instant};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const POLL_INTERVAL_MS: u64 = 100;
/// Both the load gate and the post-start network gate must be satisfied
/// continuously for this long before declaring idle.
const IDLE_QUIET_MS: u64 = 500;

/// Wait for the page to settle: readyState=complete, images loaded, and all
/// tracked in-flight fetch/XHR requests finished.
///
/// Persistent connections (WebSocket, SSE/EventSource, favicon, data: URLs)
/// are excluded at the CDP-session level and never block idle.  Orphaned
/// cross-origin iframe requests are evicted after 3 s by the tab-level
/// pending set.  This is an agent-friendly settle signal, not a guarantee of
/// global network silence.
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait network-idle --session s1 --tab t1 --timeout 10000

Notes:
  Waits for all tracked in-flight fetch/XHR requests to settle.
  Persistent connections (WebSocket, SSE) are excluded and do not block.
  Intended as an agent-friendly settle signal, not a guarantee that all
  background activity has stopped.")]
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

pub const COMMAND_NAME: &str = "browser wait network-idle";

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

    let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
        Some(sid) => sid,
        None => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!("no CDP session for target '{target_id}'"),
            );
        }
    };

    // JS guard: readyState must be complete and DOM-attached <img> elements loaded.
    // This ensures the page itself has finished parsing, independent of XHR/fetch traffic.
    let js = r#"(function() {
        if (document.readyState !== 'complete') { return { ready: false, unloaded_imgs: 1 }; }
        var imgs = Array.prototype.slice.call(document.querySelectorAll('img'));
        // Non-lazy images must always be complete.  For loading="lazy" images:
        // Chromium withholds the fetch until the image is within ~2500px of the
        // viewport, so a below-fold lazy image stays .complete===false forever and
        // would block idle.  We exempt lazy images that are truly off-screen (>3000px
        // from the viewport in any direction, safely beyond the Chromium threshold).
        // Lazy images within 3000px are in the "about to load" zone and are treated
        // like non-lazy images — their .complete===false continues to block idle.
        // Once any lazy image finishes loading, .complete becomes true and is no
        // longer counted regardless of position.
        var vh = window.innerHeight, vw = window.innerWidth, m = 3000;
        var unloaded = imgs.filter(function(i) {
          if (i.complete) return false;
          if (i.loading !== 'lazy') return true;
          var r = i.getBoundingClientRect();
          var offscreen = r.bottom < -m || r.top > vh + m || r.right < -m || r.left > vw + m;
          return !offscreen;
        }).length;
        return { ready: true, unloaded_imgs: unloaded };
    })()"#;

    // Wait for tracked in-flight fetch/XHR to drain. Persistent connection
    // types (WebSocket, SSE/EventSource) are excluded at CDP event ingest,
    // so they never appear in the pending set. Orphaned iframe requests are
    // evicted after 3 s by the tab-level pending map.
    let mut quiet_start: Option<Instant> = None;

    loop {
        let pending = cdp.network_pending(&cdp_session_id).await;

        let js_idle = cdp
            .execute_on_tab(
                &target_id,
                "Runtime.evaluate",
                json!({ "expression": js, "returnByValue": true }),
            )
            .await
            .ok()
            .and_then(|v| v.pointer("/result/result/value").cloned())
            .map(|rv| {
                let ready = rv.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
                let unloaded = rv
                    .get("unloaded_imgs")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                ready && unloaded == 0
            })
            .unwrap_or(false);

        if js_idle && pending == 0 {
            if quiet_start.is_none() {
                quiet_start = Some(Instant::now());
            }
            let quiet_elapsed_ms = quiet_start.unwrap().elapsed().as_millis() as u64;
            if quiet_elapsed_ms >= IDLE_QUIET_MS {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let url = navigation::get_tab_url(&cdp, &target_id).await;
                let title = navigation::get_tab_title(&cdp, &target_id).await;
                return ActionResult::ok(json!({
                    "kind": "network-idle",
                    "satisfied": true,
                    "elapsed_ms": elapsed_ms,
                    "observed_value": {
                        "idle": true,
                        "pending": pending,
                    },
                    "__ctx_url": url,
                    "__ctx_title": title,
                }));
            }
        } else {
            quiet_start = None;
        }

        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= timeout_ms {
            return ActionResult::fatal_with_hint(
                "TIMEOUT",
                format!("network did not become idle within {}ms", timeout_ms),
                "check that the page has finished loading or increase --timeout",
            );
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}
