use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Focus an element
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser focus \"#email\" --session s1 --tab t1
  actionbook browser focus @e4 --session s1 --tab t1

Sets keyboard focus on the element. Use before press to send keys to a specific field.
Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).")]
pub struct Cmd {
    /// Selector (CSS, XPath, or @ref)
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

pub const COMMAND_NAME: &str = "browser focus";

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
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve selector to a DOM node
    let node_id = match ctx.resolve_node(&cmd.selector).await {
        Ok(id) => id,
        Err(e) => return e,
    };

    // Scroll element to viewport center before focusing
    if let Err(e) = ctx.scroll_into_view(node_id).await {
        return e;
    }

    // Stash a reference to the current activeElement, focus the target,
    // then compare with === for true element identity (not a lossy string).
    if let Err(e) = ctx
        .execute_on_element(
            "Runtime.evaluate",
            json!({
                "expression": "window.__ab_pre_focus = document.activeElement",
            }),
        )
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Focus the element via DOM.focus (CDP-level focus)
    if let Err(e) = ctx
        .execute_on_element("DOM.focus", json!({ "nodeId": node_id }))
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Also call .focus() via JS to durably update document.activeElement in headless Chrome.
    // DOM.focus alone does not reliably update activeElement in headless environments.
    let resolve = ctx
        .execute_on_element("DOM.resolveNode", json!({ "nodeId": node_id }))
        .await;
    if let Ok(ref resolved) = resolve
        && let Some(obj_id) = resolved
            .pointer("/result/object/objectId")
            .and_then(|v| v.as_str())
    {
        let _ = ctx
            .execute_on_element(
                "Runtime.callFunctionOn",
                json!({
                    "functionDeclaration": "function() { this.focus(); }",
                    "objectId": obj_id,
                }),
            )
            .await;
    }

    // Compare pre/post active element by reference identity
    let focus_changed = ctx
        .execute_on_element(
            "Runtime.evaluate",
            json!({
                "expression": "document.activeElement !== window.__ab_pre_focus",
                "returnByValue": true,
            }),
        )
        .await
        .ok()
        .and_then(|v| v.pointer("/result/result/value").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    // Clean up the temporary global
    let _ = ctx
        .execute_on_element(
            "Runtime.evaluate",
            json!({ "expression": "delete window.__ab_pre_focus" }),
        )
        .await;

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "focus",
        "target": { "selector": cmd.selector },
        "changed": {
            "url_changed": false,
            "focus_changed": focus_changed,
        },
        "post_url": url,
        "post_title": title,
    }))
}
