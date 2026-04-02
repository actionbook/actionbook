use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::{ClickTarget, TabContext, parse_target};
use crate::browser::interaction::press::key_definition;
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
                    // Resolve, scroll to center, and focus the element
                    let node_id = match ctx.resolve_node(&s).await {
                        Ok(id) => id,
                        Err(e) => return e,
                    };
                    if let Err(e) = ctx.scroll_into_view(node_id).await {
                        return e;
                    }

                    // Check if element is contenteditable via resolved DOM node.
                    // Rich text editors (Slate, ProseMirror, Lark) need a click
                    // to establish the internal cursor, not just DOM.focus.
                    let is_ce = is_contenteditable(&ctx, node_id).await;

                    if is_ce {
                        // Click center to place cursor in rich text editor
                        let frame_id = ctx.resolved_frame_id().map(String::from);
                        if let Ok((cx, cy)) = ctx.get_center(node_id, &s, frame_id.as_deref()).await
                        {
                            let _ = dispatch_mouse_click(&ctx, cx, cy).await;
                        }
                    } else {
                        if let Err(e) = ctx.focus_element(node_id).await {
                            return e;
                        }
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

    // Move cursor to end of existing value so typed text appends.
    // For contentEditable elements, use Selection/Range API instead.
    let _ = ctx
        .execute_on_element(
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const el = document.activeElement; if (el && el.setSelectionRange) { el.setSelectionRange(el.value.length, el.value.length); } else if (el && el.isContentEditable) { const r = document.createRange(); const s = window.getSelection(); r.selectNodeContents(el); r.collapse(false); s.removeAllRanges(); s.addRange(r); } })()",
            }),
        )
        .await;

    // Chrome routes Input.dispatchKeyEvent to the active (foreground) tab,
    // ignoring the CDP sessionId.  Activate our target tab first so key
    // events reach the right page.  Matches Playwright's Page.bringToFront().
    let _ = ctx
        .cdp
        .execute_browser(
            "Target.activateTarget",
            json!({ "targetId": ctx.target_id }),
        )
        .await;

    // Type each character with full CDP key definitions so Chrome triggers
    // native text insertion on both <input> and contenteditable elements.
    for ch in text.chars() {
        let key = ch.to_string();
        let def = key_definition(&key);

        let mut key_down = json!({
            "type": "keyDown",
            "key": key,
            "text": key,
            "unmodifiedText": key,
        });
        if let Some(ref d) = def {
            key_down["code"] = json!(d.code);
            key_down["windowsVirtualKeyCode"] = json!(d.key_code);
            key_down["nativeVirtualKeyCode"] = json!(d.key_code);
        }

        if let Err(e) = ctx
            .cdp
            .execute_on_tab(&ctx.target_id, "Input.dispatchKeyEvent", key_down)
            .await
        {
            return cdp_error_to_result(e, "CDP_ERROR");
        }

        let mut key_up = json!({
            "type": "keyUp",
            "key": key,
        });
        if let Some(ref d) = def {
            key_up["code"] = json!(d.code);
            key_up["windowsVirtualKeyCode"] = json!(d.key_code);
            key_up["nativeVirtualKeyCode"] = json!(d.key_code);
        }

        if let Err(e) = ctx
            .cdp
            .execute_on_tab(&ctx.target_id, "Input.dispatchKeyEvent", key_up)
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

/// Check if a DOM node is contenteditable by resolving it to a JS object.
async fn is_contenteditable(ctx: &TabContext, node_id: i64) -> bool {
    let obj_id = match ctx.resolve_object_id(node_id).await {
        Ok(id) => id,
        Err(_) => return false,
    };
    ctx.execute_on_element(
        "Runtime.callFunctionOn",
        json!({
            "functionDeclaration": "function() { return this.isContentEditable === true; }",
            "objectId": obj_id,
            "returnByValue": true,
        }),
    )
    .await
    .ok()
    .and_then(|v| v.pointer("/result/result/value").and_then(|b| b.as_bool()))
    .unwrap_or(false)
}

/// Click at coordinates to focus the element at that position.
/// Sends mouseMoved first to establish hover state (required by headless Chrome).
async fn dispatch_mouse_click(ctx: &TabContext, x: f64, y: f64) -> Result<(), ActionResult> {
    ctx.cdp
        .execute_on_tab(
            &ctx.target_id,
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
