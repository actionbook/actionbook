use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::{ClickTarget, TabContext, parse_target};
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Type text character by character
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser type \"#search\" \"hello world\" --session s1 --tab t1
  actionbook browser type @e4 \"hello world\" --session s1 --tab t1
  actionbook browser type 420,310 \"hello\" --session s1 --tab t1
  actionbook browser type \"hello\" --session s1 --tab t1

Accepts a CSS selector, XPath, snapshot ref (@eN), or coordinates (x,y).
If selector is omitted, types into the currently focused element (document.activeElement).
Types each character individually, firing keydown/keypress/keyup events.
Use for fields with autocomplete, live validation, or input listeners.
For simple value setting without events, use fill instead.")]
pub struct Cmd {
    /// Positional args: [selector] text — if one arg, it's the text; if two, first is selector.
    #[arg(num_args = 1..=2)]
    pub args: Vec<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser type";

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
    // Parse positional args: [selector] text
    let (selector, text) = match cmd.args.as_slice() {
        [v] => (None, v.as_str()),
        [sel, v] => (Some(sel.as_str()), v.as_str()),
        _ => {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                "type requires 1 or 2 positional arguments: [selector] text",
            );
        }
    };

    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let target_json: serde_json::Value;

    match selector {
        Some(sel) => {
            match parse_target(sel) {
                Ok(ClickTarget::Coordinates(x, y)) => {
                    // Click the coordinates to focus
                    if let Err(e) = dispatch_mouse_click(&ctx, x, y).await {
                        return e;
                    }
                    target_json = json!({ "coordinates": sel });
                }
                Ok(ClickTarget::Selector(s)) => {
                    target_json = json!({ "selector": s.clone() });
                    // Resolve and focus the element
                    let node_id = match ctx.resolve_node(&s).await {
                        Ok(id) => id,
                        Err(e) => return e,
                    };
                    if let Err(e) = ctx
                        .execute_on_element("DOM.focus", json!({ "nodeId": node_id }))
                        .await
                    {
                        return cdp_error_to_result(e, "CDP_ERROR");
                    }
                }
                Err(e) => return e,
            }
        }
        None => {
            // No selector — ensure something is focused
            let is_focused = ctx
                .cdp
                .execute_on_tab(
                    &ctx.target_id,
                    "Runtime.evaluate",
                    json!({
                        "expression": "document.activeElement && document.activeElement !== document.body",
                        "returnByValue": true,
                    }),
                )
                .await
                .ok()
                .and_then(|v| v.pointer("/result/result/value").and_then(|b| b.as_bool()))
                .unwrap_or(false);

            if !is_focused {
                return ActionResult::fatal_with_hint(
                    "NO_FOCUSED_ELEMENT",
                    "no element is currently focused",
                    "click on an input field first, or pass a selector/coordinates as the first argument",
                );
            }
            target_json = json!({});
        }
    }

    // Move cursor to end of existing value so typed text appends
    let _ = ctx
        .execute_on_element(
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const el = document.activeElement; if (el && el.setSelectionRange) el.setSelectionRange(el.value.length, el.value.length); })()",
            }),
        )
        .await;

    // Type each character: keyDown (with text insertion) + keyUp
    for ch in text.chars() {
        let key = ch.to_string();

        if let Err(e) = ctx
            .cdp
            .execute_on_tab(
                &ctx.target_id,
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyDown",
                    "key": key,
                    "text": key,
                }),
            )
            .await
        {
            return cdp_error_to_result(e, "CDP_ERROR");
        }

        if let Err(e) = ctx
            .cdp
            .execute_on_tab(
                &ctx.target_id,
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyUp",
                    "key": key,
                }),
            )
            .await
        {
            return cdp_error_to_result(e, "CDP_ERROR");
        }
    }

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "type",
        "target": target_json,
        "value_summary": { "text_length": text.chars().count() },
        "post_url": url,
        "post_title": title,
    }))
}

/// Click at coordinates to focus the element at that position.
async fn dispatch_mouse_click(ctx: &TabContext, x: f64, y: f64) -> Result<(), ActionResult> {
    for event_type in &["mousePressed", "mouseReleased"] {
        ctx.cdp
            .execute_on_tab(
                &ctx.target_id,
                "Input.dispatchMouseEvent",
                json!({
                    "type": event_type,
                    "x": x,
                    "y": y,
                    "button": "left",
                    "clickCount": 1,
                    "buttons": 1,
                }),
            )
            .await
            .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    }
    Ok(())
}
