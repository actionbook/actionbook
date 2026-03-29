use std::time::Duration;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::{SharedRegistry, TabEntry};
use crate::output::ResponseContext;
use crate::types::TabId;

fn default_button() -> String {
    "left".to_string()
}

fn default_count() -> u32 {
    1
}

/// Click an element or coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// CSS selector or x,y coordinates
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

pub const COMMAND_NAME: &str = "browser.click";

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

// ── Target parsing ─────────────────────────────────────────────────

enum ClickTarget {
    Coordinates(f64, f64),
    Selector(String),
}

/// Parse the positional arg into coordinates or a CSS selector.
///
/// Heuristic: if the first character is a digit, comma, or minus-digit,
/// treat it as a coordinate attempt and validate strictly. Otherwise it
/// is a CSS selector.
fn parse_target(input: &str) -> Result<ClickTarget, ActionResult> {
    let trimmed = input.trim();
    let first = trimmed.chars().next().unwrap_or(' ');

    let is_coord_attempt = first.is_ascii_digit()
        || first == ','
        || (first == '-' && trimmed.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));

    if !is_coord_attempt {
        return Ok(ClickTarget::Selector(trimmed.to_string()));
    }

    let parts: Vec<&str> = trimmed.splitn(2, ',').collect();
    if parts.len() != 2 {
        return Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid coordinates: '{input}'"),
        ));
    }

    let x = parts[0].trim().parse::<f64>().map_err(|_| {
        ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid coordinates: '{input}'"),
        )
    })?;
    let y = parts[1].trim().parse::<f64>().map_err(|_| {
        ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!("invalid coordinates: '{input}'"),
        )
    })?;

    Ok(ClickTarget::Coordinates(x, y))
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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve element to (x, y) coordinates
    let (x, y) = match &target {
        ClickTarget::Coordinates(cx, cy) => (*cx, *cy),
        ClickTarget::Selector(sel) => {
            match element::resolve_element_center(&cdp, &target_id, sel).await {
                Ok(coords) => coords,
                Err(e) => return e,
            }
        }
    };

    // Handle --new-tab: if the target is a link, open href in a new tab
    if cmd.new_tab
        && let Some(href) = get_element_href(&cdp, &target_id, &target, x, y).await
    {
        return match open_in_new_tab(&cdp, &href, &cmd.session, registry).await {
            Ok(()) => {
                let url = navigation::get_tab_url(&cdp, &target_id).await;
                let title = navigation::get_tab_title(&cdp, &target_id).await;
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
    let pre_url = navigation::get_tab_url(&cdp, &target_id).await;
    let pre_focus = get_active_element_id(&cdp, &target_id).await;

    // Dispatch click events
    if let Err(e) = dispatch_click(&cdp, &target_id, x, y, &cmd.button, cmd.count).await {
        return e;
    }

    // Store cursor position in registry for cursor-position command
    {
        let mut reg = registry.lock().await;
        reg.set_cursor_position(&cmd.session, &cmd.tab, x, y);
    }

    // Wait for potential navigation: poll for URL change with early exit.
    // Check at short intervals so fast navigations aren't delayed, but
    // keep polling long enough for slow navigations (SPA routers, redirects).
    let post_url = wait_for_navigation(&cdp, &target_id, &pre_url).await;
    let post_title = navigation::get_tab_title(&cdp, &target_id).await;
    let post_focus = get_active_element_id(&cdp, &target_id).await;

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
/// For selectors, resolves via `element::resolve_node` (supports CSS, XPath,
/// future @eN refs) and then inspects the node. For coordinates, uses
/// `document.elementFromPoint`.
async fn get_element_href(
    cdp: &CdpSession,
    target_id: &str,
    target: &ClickTarget,
    x: f64,
    y: f64,
) -> Option<String> {
    match target {
        ClickTarget::Selector(sel) => {
            let node_id = element::resolve_node(cdp, target_id, sel).await.ok()?;
            // Resolve the DOM node to a JS object, then check for href.
            let resp = cdp
                .execute_on_tab(target_id, "DOM.resolveNode", json!({ "nodeId": node_id }))
                .await
                .ok()?;
            let object_id = resp
                .pointer("/result/object/objectId")
                .and_then(|v| v.as_str())?;
            let eval = cdp
                .execute_on_tab(
                    target_id,
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
            cdp.execute_on_tab(
                target_id,
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

    // Attach — rollback on failure
    if let Err(e) = cdp.attach(&new_target_id).await {
        let _ = cdp
            .execute_browser("Target.closeTarget", json!({ "targetId": new_target_id }))
            .await;
        return Err(cdp_error_to_result(e, "CDP_ERROR"));
    }

    // Register in the session's tab list
    let mut reg = registry.lock().await;
    match reg.get_mut(session_id) {
        Some(entry) => {
            entry.tabs.push(TabEntry {
                id: TabId(new_target_id),
                url: url.to_string(),
                title: String::new(),
            });
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
