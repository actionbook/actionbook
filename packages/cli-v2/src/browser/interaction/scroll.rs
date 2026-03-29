use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Scroll the page or a container
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser scroll down 500 --session s1 --tab t1
  actionbook browser scroll up 200 --session s1 --tab t1
  actionbook browser scroll top --session s1 --tab t1
  actionbook browser scroll bottom --session s1 --tab t1
  actionbook browser scroll into-view \"#footer\" --session s1 --tab t1
  actionbook browser scroll down 300 --container \"#sidebar\" --session s1 --tab t1

Directions: up, down, left, right (with pixel amount), top, bottom (jump to edge).
Use into-view with a selector to scroll an element into the viewport.
Use --container to scroll within a specific scrollable element.
Use --align with into-view to control alignment (start, center, end, nearest).")]
pub struct Cmd {
    /// Direction or action: up, down, left, right, top, bottom, into-view
    pub direction: String,
    /// Pixels (for directional) or selector (for into-view)
    pub value: Option<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Scroll within a specified container
    #[arg(long)]
    pub container: Option<String>,
    /// Alignment for into-view (start, center, end, nearest)
    #[arg(long)]
    pub align: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.scroll";

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

enum ScrollMode {
    Directional { direction: String, pixels: i64 },
    Edge { direction: String },
    IntoView { selector: String, align: String },
}

fn parse_scroll_mode(cmd: &Cmd) -> Result<ScrollMode, ActionResult> {
    match cmd.direction.as_str() {
        "up" | "down" | "left" | "right" => {
            let pixels_str = cmd.value.as_deref().ok_or_else(|| {
                ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!("'{}' requires a pixel amount", cmd.direction),
                )
            })?;
            let pixels = pixels_str.parse::<i64>().map_err(|_| {
                ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!("invalid pixel value: '{pixels_str}'"),
                )
            })?;
            Ok(ScrollMode::Directional {
                direction: cmd.direction.clone(),
                pixels,
            })
        }
        "top" | "bottom" => Ok(ScrollMode::Edge {
            direction: cmd.direction.clone(),
        }),
        "into-view" => {
            let selector = cmd.value.as_deref().ok_or_else(|| {
                ActionResult::fatal("INVALID_ARGUMENT", "into-view requires a selector")
            })?;
            let align = cmd.align.as_deref().unwrap_or("nearest").to_string();
            if !matches!(align.as_str(), "start" | "center" | "end" | "nearest") {
                return Err(ActionResult::fatal(
                    "INVALID_ARGUMENT",
                    format!("invalid align value: '{align}', expected start|center|end|nearest"),
                ));
            }
            Ok(ScrollMode::IntoView {
                selector: selector.to_string(),
                align,
            })
        }
        other => Err(ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!(
                "invalid scroll direction: '{other}', expected up|down|left|right|top|bottom|into-view"
            ),
        )),
    }
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let mode = match parse_scroll_mode(cmd) {
        Ok(m) => m,
        Err(e) => return e,
    };

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Ensure DOM tree is initialized (required for DOM.requestNode used by XPath resolution)
    let _ = cdp
        .execute_on_tab(&target_id, "DOM.getDocument", json!({}))
        .await;

    // Resolve container to a JS object reference if specified
    let container_object_id = match &cmd.container {
        Some(sel) => match resolve_to_object_id(&cdp, &target_id, sel).await {
            Ok(id) => Some(id),
            Err(e) => return e,
        },
        None => None,
    };

    // Pre-scroll state
    let pre_url = navigation::get_tab_url(&cdp, &target_id).await;
    let pre_focus = get_active_element_id(&cdp, &target_id).await;
    let pre_scroll = get_scroll_position(&cdp, &target_id, container_object_id.as_deref()).await;

    // Execute scroll
    match &mode {
        ScrollMode::Directional { direction, pixels } => {
            if let Err(e) = scroll_directional(
                &cdp,
                &target_id,
                direction,
                *pixels,
                container_object_id.as_deref(),
            )
            .await
            {
                return e;
            }
        }
        ScrollMode::Edge { direction } => {
            if let Err(e) =
                scroll_edge(&cdp, &target_id, direction, container_object_id.as_deref()).await
            {
                return e;
            }
        }
        ScrollMode::IntoView { selector, align } => {
            if let Err(e) = scroll_into_view(&cdp, &target_id, selector, align).await {
                return e;
            }
        }
    }

    // Post-scroll state
    let post_url = navigation::get_tab_url(&cdp, &target_id).await;
    let post_title = navigation::get_tab_title(&cdp, &target_id).await;
    let post_focus = get_active_element_id(&cdp, &target_id).await;
    let post_scroll = get_scroll_position(&cdp, &target_id, container_object_id.as_deref()).await;

    let url_changed = !pre_url.is_empty() && pre_url != post_url;
    let focus_changed = pre_focus != post_focus;
    let scroll_changed = pre_scroll != post_scroll;

    let mut data = json!({
        "action": "scroll",
        "changed": {
            "scroll_changed": scroll_changed,
            "url_changed": url_changed,
            "focus_changed": focus_changed,
        },
        "post_url": post_url,
        "post_title": post_title,
    });

    match &mode {
        ScrollMode::Directional { direction, pixels } => {
            data["direction"] = json!(direction);
            data["pixels"] = json!(pixels);
        }
        ScrollMode::Edge { direction } => {
            data["direction"] = json!(direction);
        }
        ScrollMode::IntoView { selector, align } => {
            data["target"] = json!({ "selector": selector });
            data["align"] = json!(align);
        }
    }

    if let Some(ref container_sel) = cmd.container {
        data["container"] = json!(container_sel);
    }

    ActionResult::ok(data)
}

/// Resolve a selector to a CDP JS object ID via element::resolve_node + DOM.resolveNode.
async fn resolve_to_object_id(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
) -> Result<String, ActionResult> {
    let node_id = element::resolve_node(cdp, target_id, selector).await?;
    let resp = cdp
        .execute_on_tab(target_id, "DOM.resolveNode", json!({ "nodeId": node_id }))
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    resp.pointer("/result/object/objectId")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ActionResult::fatal("CDP_ERROR", "DOM.resolveNode did not return objectId"))
}

/// Scroll by pixel amount in a given direction.
async fn scroll_directional(
    cdp: &CdpSession,
    target_id: &str,
    direction: &str,
    pixels: i64,
    container_object_id: Option<&str>,
) -> Result<(), ActionResult> {
    let (dx, dy) = match direction {
        "up" => (0, -pixels),
        "down" => (0, pixels),
        "left" => (-pixels, 0),
        "right" => (pixels, 0),
        _ => unreachable!(),
    };

    if let Some(object_id) = container_object_id {
        call_fn_on(
            cdp,
            target_id,
            object_id,
            &format!("function() {{ this.scrollBy({dx}, {dy}); return 'ok'; }}"),
        )
        .await
    } else {
        eval_js(
            cdp,
            target_id,
            &format!("(() => {{ window.scrollBy({dx}, {dy}); return 'ok'; }})()"),
        )
        .await
    }
}

/// Scroll to top or bottom edge.
async fn scroll_edge(
    cdp: &CdpSession,
    target_id: &str,
    direction: &str,
    container_object_id: Option<&str>,
) -> Result<(), ActionResult> {
    if let Some(object_id) = container_object_id {
        let fn_body = match direction {
            "top" => "function() { this.scrollTop = 0; this.scrollLeft = 0; return 'ok'; }",
            "bottom" => "function() { this.scrollTop = this.scrollHeight; return 'ok'; }",
            _ => unreachable!(),
        };
        call_fn_on(cdp, target_id, object_id, fn_body).await
    } else {
        let js = match direction {
            "top" => "(() => { window.scrollTo(0, 0); return 'ok'; })()",
            "bottom" => {
                "(() => { window.scrollTo(0, document.documentElement.scrollHeight); return 'ok'; })()"
            }
            _ => unreachable!(),
        };
        eval_js(cdp, target_id, js).await
    }
}

/// Scroll an element into view with alignment.
async fn scroll_into_view(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
    align: &str,
) -> Result<(), ActionResult> {
    let object_id = resolve_to_object_id(cdp, target_id, selector).await?;

    let block = match align {
        "start" => "start",
        "center" => "center",
        "end" => "end",
        _ => "nearest",
    };

    call_fn_on(
        cdp,
        target_id,
        &object_id,
        &format!(
            "function() {{ this.scrollIntoView({{ block: '{block}', inline: 'nearest', behavior: 'instant' }}); return 'ok'; }}"
        ),
    )
    .await
}

/// Execute a function on a resolved JS object.
async fn call_fn_on(
    cdp: &CdpSession,
    target_id: &str,
    object_id: &str,
    function_declaration: &str,
) -> Result<(), ActionResult> {
    cdp.execute_on_tab(
        target_id,
        "Runtime.callFunctionOn",
        json!({
            "objectId": object_id,
            "functionDeclaration": function_declaration,
            "returnByValue": true,
        }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    Ok(())
}

/// Evaluate JS and check the result.
async fn eval_js(cdp: &CdpSession, target_id: &str, expression: &str) -> Result<(), ActionResult> {
    cdp.execute_on_tab(
        target_id,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
        }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    Ok(())
}

/// Get scroll position for change detection.
async fn get_scroll_position(
    cdp: &CdpSession,
    target_id: &str,
    container_object_id: Option<&str>,
) -> (f64, f64) {
    if let Some(object_id) = container_object_id {
        let resp = cdp
            .execute_on_tab(
                target_id,
                "Runtime.callFunctionOn",
                json!({
                    "objectId": object_id,
                    "functionDeclaration": "function() { return JSON.stringify({x:this.scrollLeft,y:this.scrollTop}); }",
                    "returnByValue": true,
                }),
            )
            .await
            .ok();
        parse_scroll_json(resp)
    } else {
        let resp = cdp
            .execute_on_tab(
                target_id,
                "Runtime.evaluate",
                json!({
                    "expression": "JSON.stringify({x:window.scrollX,y:window.scrollY})",
                    "returnByValue": true,
                }),
            )
            .await
            .ok();
        parse_scroll_json(resp)
    }
}

fn parse_scroll_json(resp: Option<serde_json::Value>) -> (f64, f64) {
    resp.and_then(|v| {
        let s = v.pointer("/result/result/value").and_then(|v| v.as_str())?;
        let parsed: serde_json::Value = serde_json::from_str(s).ok()?;
        let x = parsed.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = parsed.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        Some((x, y))
    })
    .unwrap_or((0.0, 0.0))
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
