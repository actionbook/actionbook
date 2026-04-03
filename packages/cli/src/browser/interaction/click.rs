use std::time::Duration;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

/// Click one or more elements or coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser click \"#submit\" --session s1 --tab t1
  actionbook browser click @e5 --session s1 --tab t1
  actionbook browser click 420,310 --session s1 --tab t1
  actionbook browser click \"a.link\" --new-tab --session s1 --tab t1
  actionbook browser click \"#item\" --count 2 --session s1 --tab t1
  actionbook browser click \"#close-banner\" \"#main-btn\" \"#confirm\" --session s1 --tab t1

Accepts a CSS selector, XPath, snapshot ref (@eN), or x,y coordinates.
When multiple selectors are provided, they are clicked sequentially in order.
Refs come from snapshot output (e.g. [ref=e5]).
Use --count 2 for double-click. Use --new-tab to open links in a new tab.")]
pub struct Cmd {
    /// CSS selector, XPath, @ref, or x,y coordinates (one or more)
    #[arg(num_args(1..))]
    pub selectors: Vec<String>,
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

/// Click a single selector with the given context and command params.
async fn execute_single_click(
    selector: &str,
    cmd: &Cmd,
    ctx: &mut TabContext,
) -> ActionResult {
    // Parse target
    let target = match parse_target(selector) {
        Ok(t) => t,
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
        && let Some(href) = get_element_href(ctx, &target, x, y).await
    {
        return match open_in_new_tab(&ctx.cdp, &href, ctx.session_id(), ctx.registry()).await {
            Ok(()) => {
                let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
                let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;
                ActionResult::ok(build_response(
                    selector,
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

    // Pre-click state: one evaluate for url + focus
    let (pre_url, pre_focus) = get_tab_state(&ctx.cdp, &ctx.target_id).await;

    // Dispatch click events
    if let Err(e) = dispatch_click(&ctx.cdp, &ctx.target_id, x, y, &cmd.button, cmd.count).await {
        return e;
    }

    // Store cursor position in registry for cursor-position command
    {
        let mut reg = ctx.registry().lock().await;
        reg.set_cursor_position(ctx.session_id(), ctx.tab_id(), x, y);
    }

    // Post-click state: wait for JS to settle, then one evaluate for url + title + focus
    let (post_url, post_title, post_focus) =
        wait_and_get_post_state(&ctx.cdp, &ctx.target_id, &pre_url).await;

    let url_changed = !pre_url.is_empty() && pre_url != post_url;
    let focus_changed = pre_focus != post_focus;

    ActionResult::ok(build_response(
        selector,
        &target,
        url_changed,
        focus_changed,
        Some(post_url),
        Some(post_title),
    ))
}

// ── Execute ────────────────────────────────────────────────────────

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Validate selectors
    if cmd.selectors.is_empty() {
        return ActionResult::fatal("INVALID_ARGUMENT", "at least one selector required");
    }

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

    // Get CDP session and verify tab
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Single selector: same response shape as before (backwards compat)
    if cmd.selectors.len() == 1 {
        return execute_single_click(&cmd.selectors[0], cmd, &mut ctx).await;
    }

    // Batch: sequential, fail-fast
    let mut results = Vec::new();
    for (i, selector) in cmd.selectors.iter().enumerate() {
        match execute_single_click(selector, cmd, &mut ctx).await {
            ActionResult::Ok { data } => {
                results.push(json!({
                    "index": i,
                    "selector": selector,
                    "url_changed": data["changed"]["url_changed"],
                    "focus_changed": data["changed"]["focus_changed"],
                    "post_url": data.get("post_url"),
                    "post_title": data.get("post_title"),
                }));
            }
            _err => {
                return ActionResult::fatal_with_details(
                    "BATCH_CLICK_ERROR",
                    format!("click failed at index {i} (selector: {selector})"),
                    format!(
                        "completed {}/{}, retry from index {i}",
                        results.len(),
                        cmd.selectors.len()
                    ),
                    json!({
                        "failed_index": i,
                        "failed_selector": selector,
                        "completed": results.len()
                    }),
                );
            }
        }
    }

    // Final state from last result
    let last = results.last().cloned().unwrap_or(Value::Null);
    let mut data = json!({
        "action": "click",
        "clicks": results.len(),
        "results": results,
    });
    if let Some(url) = last.get("post_url").and_then(|v| v.as_str()) {
        data["post_url"] = json!(url);
    }
    if let Some(title) = last.get("post_title").and_then(|v| v.as_str()) {
        data["post_title"] = json!(title);
    }
    ActionResult::ok(data)
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

    // Move mouse to the target coordinates first to establish pointer hover state.
    // SPA frameworks (React Router, Vue Router, Next.js) attach click listeners
    // that only fire after the browser has registered a hover via mouseMoved.
    // Without this, Chrome does not synthesise the click event correctly and the
    // SPA router's navigation handler never triggers. Matches Playwright behaviour.
    cdp.execute_on_tab(
        target_id,
        "Input.dispatchMouseEvent",
        json!({
            "type": "mouseMoved",
            "x": x,
            "y": y,
            "button": "none",
            "buttons": 0,
        }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

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

/// Wait for JS to settle after a click, then fetch url + title + focus in one evaluate.
///
/// Strategy:
///   1. sleep(50ms) — give the browser time to start processing the click event
///   2. One Runtime.evaluate (blocks until JS main thread is free)
///      - URL unchanged → DOM-only interaction (expand/toggle), return immediately (~100ms total)
///      - URL changed   → navigation started, poll until URL stabilises (max 2s)
async fn wait_and_get_post_state(
    cdp: &CdpSession,
    target_id: &str,
    pre_url: &str,
) -> (String, String, String) {
    // Give the browser time to start processing the click event.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let state = get_full_tab_state(cdp, target_id).await;

    // URL unchanged → DOM-only interaction, no navigation pending.
    // The evaluate above already waited for the JS main thread to finish.
    if pre_url.is_empty() || state.0 == pre_url {
        return state;
    }

    // URL changed → navigation in progress; poll until the URL stabilises.
    const POLL_INTERVAL: Duration = Duration::from_millis(50);
    const TIMEOUT: Duration = Duration::from_millis(2000);
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    let mut current = state;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let next = get_full_tab_state(cdp, target_id).await;
        if next.0 == current.0 || tokio::time::Instant::now() >= deadline {
            return next;
        }
        current = next;
    }
}

/// Fetch url + title + active-element in one evaluate round-trip.
async fn get_full_tab_state(cdp: &CdpSession, target_id: &str) -> (String, String, String) {
    let result = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const a = document.activeElement; return JSON.stringify({ url: document.URL, title: document.title, focus: a ? a.tagName + '#' + (a.id || '') : '' }); })()",
                "returnByValue": true,
            }),
        )
        .await
        .ok()
        .and_then(|v| {
            v.pointer("/result/result/value")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        });

    let url = result.as_ref().and_then(|v| v["url"].as_str()).unwrap_or_default().to_string();
    let title = result.as_ref().and_then(|v| v["title"].as_str()).unwrap_or_default().to_string();
    let focus = result.as_ref().and_then(|v| v["focus"].as_str()).unwrap_or_default().to_string();
    (url, title, focus)
}

/// Fetch url + active-element in one evaluate, saving one JS-main-thread round-trip.
async fn get_tab_state(cdp: &CdpSession, target_id: &str) -> (String, String) {
    let result = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const a = document.activeElement; return JSON.stringify({ url: document.URL, focus: a ? a.tagName + '#' + (a.id || '') : '' }); })()",
                "returnByValue": true,
            }),
        )
        .await
        .ok()
        .and_then(|v| {
            v.pointer("/result/result/value")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        });

    let url = result
        .as_ref()
        .and_then(|v| v["url"].as_str())
        .unwrap_or_default()
        .to_string();
    let focus = result
        .as_ref()
        .and_then(|v| v["focus"].as_str())
        .unwrap_or_default()
        .to_string();
    (url, focus)
}

