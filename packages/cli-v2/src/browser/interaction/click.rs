use std::time::Duration;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::{ClickTarget, TabContext, parse_target};
use crate::browser::navigation;
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

fn default_button() -> String {
    "left".to_string()
}

fn default_count() -> u32 {
    1
}

/// Click an element or coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser click \"#submit\" --session s1 --tab t1
  actionbook browser click @e5 --session s1 --tab t1
  actionbook browser click 420,310 --session s1 --tab t1
  actionbook browser click \"a.link\" --new-tab --session s1 --tab t1
  actionbook browser click \"#item\" --count 2 --session s1 --tab t1

Accepts a CSS selector, XPath, snapshot ref (@eN), or x,y coordinates.
Refs come from snapshot output (e.g. [ref=e5]).
Use --count 2 for double-click. Use --new-tab to open links in a new tab.")]
pub struct Cmd {
    /// CSS selector, XPath, @ref, or x,y coordinates
    pub selector: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Open link in new tab
    #[arg(long)]
    #[serde(default)]
    pub new_tab: bool,
    /// Mouse button (left, right, middle)
    #[arg(long, default_value = "left")]
    #[serde(default = "default_button")]
    pub button: String,
    /// Click count (2 = double-click)
    #[arg(long, default_value_t = 1)]
    #[serde(default = "default_count")]
    pub count: u32,
}

pub const COMMAND_NAME: &str = "browser click";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    // SESSION_NOT_FOUND: context must be null per §3.1
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("post_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            data.get("post_title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
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

// ── Execute ────────────────────────────────────────────────────────

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Validate count
    if cmd.count == 0 {
        return ActionResult::fatal("INVALID_ARGUMENT", "count must be at least 1");
    }

    // Validate button
    if !matches!(cmd.button.as_str(), "left" | "right" | "middle") {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!(
                "invalid button: '{}', expected left|right|middle",
                cmd.button
            ),
        );
    }

    // Parse target
    let target = match parse_target(&cmd.selector) {
        Ok(t) => t,
        Err(e) => return e,
    };

    // Get CDP session and verify tab
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve element to (x, y) coordinates
    let (x, y) = match &target {
        ClickTarget::Coordinates(cx, cy) => (*cx, *cy),
        ClickTarget::Selector(sel) => match ctx.resolve_center(sel).await {
            Ok((_node_id, cx, cy)) => (cx, cy),
            Err(e) => return e,
        },
    };

    // Handle --new-tab: if the target is a link, open href in a new tab
    if cmd.new_tab
        && let Some(href) = get_element_href(&mut ctx, &target, x, y).await
    {
        return match open_in_new_tab(&ctx.cdp, &href, ctx.session_id(), ctx.registry()).await {
            Ok(()) => {
                let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
                let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;
                ActionResult::ok(build_response(
                    &cmd.selector,
                    &target,
                    false,
                    false,
                    Some(url),
                    Some(title),
                ))
            }
            Err(e) => e,
        };
    }

    // Pre-click state
    let pre_url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let pre_focus = get_active_element_id(&ctx.cdp, &ctx.target_id).await;

    // Dispatch click events
    if let Err(e) = dispatch_click(&ctx.cdp, &ctx.target_id, x, y, &cmd.button, cmd.count).await {
        return e;
    }

    // Store cursor position in registry for cursor-position command
    {
        let mut reg = ctx.registry().lock().await;
        reg.set_cursor_position(ctx.session_id(), ctx.tab_id(), x, y);
    }

    // Wait for potential navigation: poll for URL change with early exit.
    // Check at short intervals so fast navigations aren't delayed, but
    // keep polling long enough for slow navigations (SPA routers, redirects).
    let post_url = wait_for_navigation(&ctx.cdp, &ctx.target_id, &pre_url).await;
    let post_title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;
    let post_focus = get_active_element_id(&ctx.cdp, &ctx.target_id).await;

    let url_changed = !pre_url.is_empty() && pre_url != post_url;
    let focus_changed = pre_focus != post_focus;

    ActionResult::ok(build_response(
        &cmd.selector,
        &target,
        url_changed,
        focus_changed,
        Some(post_url),
        Some(post_title),
    ))
}

// ── Response builder ───────────────────────────────────────────────

fn build_response(
    raw_input: &str,
    target: &ClickTarget,
    url_changed: bool,
    focus_changed: bool,
    post_url: Option<String>,
    post_title: Option<String>,
) -> serde_json::Value {
    let target_obj = match target {
        ClickTarget::Selector(_) => json!({ "selector": raw_input }),
        ClickTarget::Coordinates(_, _) => json!({ "coordinates": raw_input }),
    };

    let mut data = json!({
        "action": "click",
        "target": target_obj,
        "changed": {
            "url_changed": url_changed,
            "focus_changed": focus_changed,
        },
    });

    if let Some(url) = post_url {
        data["post_url"] = json!(url);
    }
    if let Some(title) = post_title {
        data["post_title"] = json!(title);
    }

    data
}

// ── CDP helpers ────────────────────────────────────────────────────

/// Check whether the element at the target position has an `href`.
///
/// For selectors, resolves via `ctx.resolve_node` (supports CSS, XPath,
/// @eN refs) and then inspects the node. For coordinates, uses
/// `document.elementFromPoint`.
async fn get_element_href(
    ctx: &mut TabContext,
    target: &ClickTarget,
    x: f64,
    y: f64,
) -> Option<String> {
    match target {
        ClickTarget::Selector(sel) => {
            let node_id = ctx.resolve_node(sel).await.ok()?;
            let object_id = ctx.resolve_object_id(node_id).await.ok()?;
            let eval = ctx
                .execute_on_element(
                    "Runtime.callFunctionOn",
                    json!({
                        "objectId": object_id,
                        "functionDeclaration": "function() { if (this.tagName === 'A' && this.href) return this.href; const link = this.closest && this.closest('a[href]'); return link ? link.href : null; }",
                        "returnByValue": true,
                    }),
                )
                .await
                .ok()?;
            eval.pointer("/result/result/value")
                .and_then(|v| v.as_str())
                .map(String::from)
        }
        ClickTarget::Coordinates(_, _) => {
            let js = format!(
                r#"(() => {{
                    const el = document.elementFromPoint({x}, {y});
                    if (!el) return null;
                    if (el.tagName === 'A' && el.href) return el.href;
                    const link = el.closest('a[href]');
                    return link ? link.href : null;
                }})()"#
            );
            ctx.cdp
                .execute_on_tab(
                    &ctx.target_id,
                    "Runtime.evaluate",
                    json!({ "expression": js, "returnByValue": true }),
                )
                .await
                .ok()
                .and_then(|v| {
                    v.pointer("/result/result/value")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
        }
    }
}

/// Create a new tab pointing to `url` and register it in the session.
async fn open_in_new_tab(
    cdp: &CdpSession,
    url: &str,
    session_id: &str,
    registry: &SharedRegistry,
) -> Result<(), ActionResult> {
    // Get stealth_ua from session so the new tab gets the same stealth injection.
    let stealth_ua = {
        let reg = registry.lock().await;
        reg.get(session_id).and_then(|e| e.stealth_ua.clone())
    };
    let resp = cdp
        .execute_browser("Target.createTarget", json!({ "url": url }))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let new_target_id = resp
        .pointer("/result/targetId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ActionResult::fatal("CDP_ERROR", "Target.createTarget did not return targetId")
        })?
        .to_string();

    // Attach — rollback on failure.
    // Pass stealth_ua so new tabs get the same stealth injection.
    if let Err(e) = cdp.attach(&new_target_id, stealth_ua.as_deref()).await {
        let _ = cdp
            .execute_browser("Target.closeTarget", json!({ "targetId": new_target_id }))
            .await;
        return Err(cdp_error_to_result(e, "CDP_ERROR"));
    }

    // Register in the session's tab list
    let mut reg = registry.lock().await;
    match reg.get_mut(session_id) {
        Some(entry) => {
            entry.push_tab(new_target_id, url.to_string(), String::new());
        }
        None => {
            // Session vanished concurrently — detach and close the orphan
            drop(reg);
            let _ = cdp.detach(&new_target_id).await;
            let _ = cdp
                .execute_browser("Target.closeTarget", json!({ "targetId": new_target_id }))
                .await;
            return Err(ActionResult::fatal(
                "SESSION_NOT_FOUND",
                format!("session '{session_id}' was closed during tab creation"),
            ));
        }
    }

    Ok(())
}

/// Dispatch CDP mouse events for click(s).
async fn dispatch_click(
    cdp: &CdpSession,
    target_id: &str,
    x: f64,
    y: f64,
    button: &str,
    count: u32,
) -> Result<(), ActionResult> {
    let buttons_mask = match button {
        "right" => 2,
        "middle" => 4,
        _ => 1, // left
    };

    for click_count in 1..=count {
        cdp.execute_on_tab(
            target_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": button,
                "clickCount": click_count,
                "buttons": buttons_mask,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

        cdp.execute_on_tab(
            target_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": button,
                "clickCount": click_count,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    }

    Ok(())
}

/// Poll for a URL change after a click.
///
/// Returns the final URL. If the URL changes within the polling window,
/// returns immediately. Otherwise returns after the timeout with
/// whatever URL the page currently has.
///
/// Intervals are short (50 ms) so fast navigations resolve quickly.
/// Total timeout (2 s) covers SPA routers and JS redirects without
/// the unconditional 500 ms penalty of a fixed sleep.
async fn wait_for_navigation(cdp: &CdpSession, target_id: &str, pre_url: &str) -> String {
    const POLL_INTERVAL: Duration = Duration::from_millis(50);
    const TIMEOUT: Duration = Duration::from_millis(2000);

    let deadline = tokio::time::Instant::now() + TIMEOUT;
    // Brief initial pause: give the browser a moment to start navigation
    // before the first poll so we don't immediately read stale state.
    tokio::time::sleep(Duration::from_millis(100)).await;

    loop {
        let current = navigation::get_tab_url(cdp, target_id).await;
        if !pre_url.is_empty() && current != pre_url {
            return current;
        }
        if tokio::time::Instant::now() >= deadline {
            return current;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Snapshot of the active element for focus-change detection.
async fn get_active_element_id(cdp: &CdpSession, target_id: &str) -> String {
    cdp.execute_on_tab(
        target_id,
        "Runtime.evaluate",
        json!({
            "expression": "(() => { const a = document.activeElement; return a ? a.tagName + '#' + (a.id || '') : ''; })()",
            "returnByValue": true,
        }),
    )
    .await
    .ok()
    .and_then(|v| {
        v.pointer("/result/result/value")
            .and_then(|v| v.as_str())
            .map(String::from)
    })
    .unwrap_or_default()
}
