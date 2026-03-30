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

/// Wait for network activity to become idle
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser wait network-idle --session s1 --tab t1 --timeout 10000")]
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

pub const COMMAND_NAME: &str = "browser.wait.network-idle";

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
    // Idle stabilisation window: no new requests for this long before we declare idle.
    let idle_window_ms: u64 = 500;
    let start = Instant::now();

    // Resolve the CDP flat-session ID.  `attach()` already called `Network.enable`
    // on this session, so `tab_net_pending` tracks ALL requests since tab attachment —
    // including those that started before this `wait network-idle` call.
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
        var unloaded = imgs.filter(function(i) { return !i.complete; }).length;
        return { ready: true, unloaded_imgs: unloaded };
    })()"#;

    // When pending first reaches 0 (and js_idle), we record the start of the quiet
    // window.  Idle is declared only after idle_window_ms with no new requests.
    let mut quiet_start: Option<Instant> = None;

    loop {
        // Read the live in-flight counter maintained by reader_loop.
        let pending = cdp.network_pending(&cdp_session_id).await;

        // JS fallback: readyState + DOM-attached img.complete.
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

        if pending == 0 && js_idle {
            let quiet_elapsed_ms = match quiet_start {
                None => {
                    quiet_start = Some(Instant::now());
                    0
                }
                Some(qs) => qs.elapsed().as_millis() as u64,
            };
            if quiet_elapsed_ms >= idle_window_ms {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let url = navigation::get_tab_url(&cdp, &target_id).await;
                let title = navigation::get_tab_title(&cdp, &target_id).await;
                return ActionResult::ok(json!({
                    "kind": "network-idle",
                    "satisfied": true,
                    "elapsed_ms": elapsed_ms,
                    "observed_value": { "idle": true },
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
