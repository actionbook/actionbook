use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Hover over an element
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser hover \"#menu-item\" --session s1 --tab t1
  actionbook browser hover \"a.dropdown-toggle\" --session s1 --tab t1

Moves the mouse over the element to trigger hover states (tooltips, dropdowns, etc.).
Accepts a CSS selector, XPath, or snapshot ref (@eN).")]
pub struct Cmd {
    /// CSS selector, XPath, or snapshot ref
    pub selector: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.hover";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
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

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve selector to a DOM node (handles CSS, XPath, snapshot refs)
    let node_id = match ctx.resolve_node(&cmd.selector).await {
        Ok(id) => id,
        Err(e) => return e,
    };

    // Scroll element into view
    if let Err(e) = ctx.scroll_into_view(node_id).await {
        return e;
    }

    // Get a JS object reference for the resolved node
    let object_id = match ctx.resolve_object_id(node_id).await {
        Ok(id) => id,
        Err(e) => return e,
    };

    // Dispatch mouseenter, mouseover, and mousemove on the element via JS.
    // CDP Input.dispatchMouseEvent with mouseMoved does not reliably produce
    // the full set of DOM hover events in headless Chrome.
    let hover_resp = match ctx
        .cdp
        .execute_on_tab(
            &ctx.target_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": r#"function() {
                    const rect = this.getBoundingClientRect();
                    const cx = rect.left + rect.width / 2;
                    const cy = rect.top + rect.height / 2;
                    const shared = { clientX: cx, clientY: cy, screenX: cx, screenY: cy, view: window };
                    this.dispatchEvent(new MouseEvent('mouseenter', { ...shared, bubbles: false }));
                    this.dispatchEvent(new MouseEvent('mouseover', { ...shared, bubbles: true }));
                    this.dispatchEvent(new MouseEvent('mousemove', { ...shared, bubbles: true }));
                    return JSON.stringify({ x: cx, y: cy });
                }"#,
                "returnByValue": true,
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return cdp_error_to_result(e, "CDP_ERROR"),
    };

    let result_str = hover_resp
        .pointer("/result/result/value")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if result_str.is_empty() {
        return ActionResult::fatal("CDP_ERROR", "hover dispatch failed: empty result");
    }

    // Parse and store cursor position from the hover coordinates
    if let Ok(coords) = serde_json::from_str::<serde_json::Value>(result_str)
        && let (Some(x), Some(y)) = (
            coords.get("x").and_then(|v| v.as_f64()),
            coords.get("y").and_then(|v| v.as_f64()),
        )
    {
        let mut reg = ctx.registry().lock().await;
        reg.set_cursor_position(ctx.session_id(), ctx.tab_id(), x, y);
    }

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "hover",
        "target": { "selector": cmd.selector },
        "changed": {
            "url_changed": false,
            "focus_changed": false,
        },
        "post_url": url,
        "post_title": title,
    }))
}
