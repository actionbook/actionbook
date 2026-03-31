use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::{ClickTarget, TabContext, parse_target};
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Directly set the value of an input field
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser fill \"#email\" \"user@example.com\" --session s1 --tab t1
  actionbook browser fill @e4 \"search query\" --session s1 --tab t1
  actionbook browser fill 420,310 \"hello\" --session s1 --tab t1
  actionbook browser fill \"hello\" --session s1 --tab t1

Accepts a CSS selector, XPath, snapshot ref (@eN), or coordinates (x,y).
If selector is omitted, fills the currently focused element (document.activeElement).
Sets the value instantly (no per-character events). Use for standard inputs.
For fields that need keystroke events (autocomplete, validation), use type instead.")]
pub struct Cmd {
    /// Positional args: [selector] value — if one arg, it's the value; if two, first is selector.
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

pub const COMMAND_NAME: &str = "browser fill";

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
    // Parse positional args: [selector] value
    let (selector, value) = match cmd.args.as_slice() {
        [v] => (None, v.as_str()),
        [sel, v] => (Some(sel.as_str()), v.as_str()),
        _ => {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                "fill requires 1 or 2 positional arguments: [selector] value",
            );
        }
    };

    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let target_json: serde_json::Value;

    let object_id = match selector {
        Some(sel) => {
            match parse_target(sel) {
                Ok(ClickTarget::Coordinates(x, y)) => {
                    // Click the coordinates to focus, then fill activeElement
                    if let Err(e) = dispatch_mouse_click(&ctx, x, y).await {
                        return e;
                    }
                    target_json = json!({ "coordinates": sel });
                    match get_active_element_object_id(&ctx).await {
                        Ok(oid) => oid,
                        Err(e) => return e,
                    }
                }
                Ok(ClickTarget::Selector(s)) => {
                    target_json = json!({ "selector": s });
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
                    match ctx.resolve_object_id(node_id).await {
                        Ok(oid) => oid,
                        Err(e) => return e,
                    }
                }
                Err(e) => return e,
            }
        }
        None => {
            // No selector — fill current activeElement
            target_json = json!({});
            match get_active_element_object_id(&ctx).await {
                Ok(oid) => oid,
                Err(e) => return e,
            }
        }
    };

    // Set value directly via JS and dispatch an input event (no key events)
    let value_json = serde_json::to_string(&value).unwrap_or_default();
    let fill_fn = format!(
        r#"function() {{
            const proto = this instanceof HTMLTextAreaElement
                ? HTMLTextAreaElement.prototype
                : HTMLInputElement.prototype;
            const nativeSet = Object.getOwnPropertyDescriptor(proto, 'value');
            if (nativeSet && nativeSet.set) {{
                nativeSet.set.call(this, {value_json});
            }} else {{
                this.value = {value_json};
            }}
            this.dispatchEvent(new Event('input', {{ bubbles: true }}));
            return 'ok';
        }}"#
    );

    let resp = match ctx
        .execute_on_element(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": fill_fn,
                "returnByValue": true,
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return cdp_error_to_result(e, "CDP_ERROR"),
    };

    let result_str = resp
        .pointer("/result/result/value")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if result_str != "ok" {
        return ActionResult::fatal("CDP_ERROR", format!("fill failed: {result_str}"));
    }

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "fill",
        "target": target_json,
        "value_summary": { "text_length": value.chars().count() },
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

/// Get the CDP objectId for document.activeElement; error if none is focused.
async fn get_active_element_object_id(ctx: &TabContext) -> Result<String, ActionResult> {
    let resp = ctx
        .cdp
        .execute_on_tab(
            &ctx.target_id,
            "Runtime.evaluate",
            json!({
                "expression": "document.activeElement && document.activeElement !== document.body ? document.activeElement : null",
                "returnByValue": false,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    let object_id = resp
        .pointer("/result/result/objectId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if object_id.is_empty() {
        return Err(ActionResult::fatal_with_hint(
            "NO_FOCUSED_ELEMENT",
            "no element is currently focused",
            "click on an input field first, or pass a selector/coordinates as the first argument",
        ));
    }

    Ok(object_id)
}
