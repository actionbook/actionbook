use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp::ensure_scheme_or_fatal;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// When to consider navigation complete.
#[derive(Clone, Debug, Default, Serialize, Deserialize, clap::ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WaitUntil {
    /// Return immediately after Page.navigate (old behavior).
    None,
    /// Wait for DOMContentLoaded event (DOM ready, third-party resources may still load).
    #[default]
    Domcontentloaded,
    /// Wait for load event (all resources including images/stylesheets loaded).
    Load,
}

/// Navigate to URL
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser goto https://google.com --session s1 --tab t1
  actionbook browser goto https://example.com/login --session s1 --tab t1 --wait-until domcontentloaded
  actionbook browser goto https://example.com --session s1 --tab t1 --wait-until none

A scheme (https://) is added automatically if omitted.
After navigation, context.url and context.title are updated.

--wait-until controls when the command returns:
  domcontentloaded (default) — wait for DOMContentLoaded (DOM ready, faster)
  load                       — wait for the page load event (all resources)
  none                       — return immediately after navigation starts")]
pub struct Cmd {
    /// Target URL
    pub url: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// When to consider navigation complete
    #[arg(long, value_enum, default_value = "domcontentloaded")]
    #[serde(default)]
    pub wait_until: WaitUntil,
}

pub const COMMAND_NAME: &str = "browser goto";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    // SESSION_NOT_FOUND: context must be null per §3.1
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("to_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("title").and_then(|v| v.as_str()).map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: Some(cmd.tab.clone()),
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let final_url = match ensure_scheme_or_fatal(&cmd.url) {
        Ok(u) => u,
        Err(e) => return e,
    };

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Get from_url before navigation
    let from_url = super::get_tab_url(&cdp, &target_id).await;

    if !target_id.is_empty() {
        // Determine which CDP event to wait for (if any).
        let wait_event = match cmd.wait_until {
            WaitUntil::None => None,
            WaitUntil::Domcontentloaded => Some("Page.domContentEventFired"),
            WaitUntil::Load => Some("Page.loadEventFired"),
        };

        // Get the CDP flat-session ID for event subscription.
        let cdp_session_id = match cdp.get_cdp_session_id(&target_id).await {
            Some(sid) => sid,
            None => {
                return ActionResult::fatal(
                    "INTERNAL_ERROR",
                    format!("no CDP session for target '{target_id}'"),
                );
            }
        };

        // Subscribe to the CDP event BEFORE navigation to avoid missing it.
        let mut event_rx = if let Some(event_name) = wait_event {
            Some(cdp.subscribe_events(&cdp_session_id, event_name).await)
        } else {
            None
        };

        // Page.enable is idempotent — safe to call on every goto.
        // Required for Page.domContentEventFired / Page.loadEventFired events.
        if wait_event.is_some() {
            if let Err(e) = cdp
                .execute_on_tab(&target_id, "Page.enable", json!({}))
                .await
            {
                return cdp_error_to_result(e, "NAVIGATION_FAILED");
            }
        } else {
            let _ = cdp
                .execute_on_tab(&target_id, "Page.enable", json!({}))
                .await;
        }

        // Drain any stale lifecycle events that Page.enable may have replayed
        // for the currently-loaded page. Without this, a residual loadEventFired
        // from the old page could satisfy the wait immediately.
        if let Some(ref mut rx) = event_rx {
            while rx.try_recv().is_ok() {}
        }

        match cdp
            .execute_on_tab(&target_id, "Page.navigate", json!({ "url": final_url }))
            .await
        {
            Err(e) => return cdp_error_to_result(e, "NAVIGATION_FAILED"),
            Ok(v) => {
                if let Some(err_text) = v["result"]["errorText"].as_str()
                    && !err_text.is_empty()
                {
                    return ActionResult::fatal("NAVIGATION_FAILED", err_text.to_string());
                }
            }
        }

        // Wait for the subscribed CDP event.
        // No internal timeout — the global --timeout flag (set in main.rs) controls
        // the overall request deadline. If the user didn't set --timeout, we wait
        // until the browser fires the event (fast for local pages, may need --timeout
        // for heavy external pages).
        if let Some(ref mut rx) = event_rx {
            let _ = rx.recv().await; // None = channel closed (session died), proceed best-effort
        }
    }

    // Get to_url and title after navigation (+ wait)
    let to_url = super::get_tab_url(&cdp, &target_id).await;
    let title = super::get_tab_title(&cdp, &target_id).await;

    // Clear snapshot RefCache — page changed, old backendNodeIds are invalid.
    // Also refresh the registry's TabEntry so downstream consumers (restart
    // preserving open_url, list-sessions summary, etc.) see the navigated URL
    // instead of the stale launch-time URL. Without this, a session restart
    // after a `goto` would rewind the user to whatever page the browser
    // booted on.
    {
        let mut reg = registry.lock().await;
        reg.clear_ref_cache(&cmd.session, &cmd.tab);
        if let Some(entry) = reg.get_mut(&cmd.session)
            && let Some(tab) = entry.tabs.iter_mut().find(|t| t.id.0 == cmd.tab)
        {
            tab.url = to_url.clone();
            tab.title = title.clone();
        }
    }

    ActionResult::ok(json!({
        "kind": "goto",
        "requested_url": cmd.url,
        "from_url": from_url,
        "to_url": to_url,
        "title": title,
    }))
}
