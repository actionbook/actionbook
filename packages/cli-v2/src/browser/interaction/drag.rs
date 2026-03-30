use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::{element, navigation};
use crate::daemon::cdp_session::{CdpSession, cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

fn default_button() -> String {
    "left".to_string()
}

/// Drag an element to a target element or coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser drag \"#card\" \"#dropzone\" --session s1 --tab t1
  actionbook browser drag \"#slider\" 500,300 --session s1 --tab t1

Drags from the source element to a destination (selector or x,y coordinates).
Fires mousedown, mousemove, and mouseup events to simulate a full drag gesture.")]
pub struct Cmd {
    /// Source element selector
    pub source: String,
    /// Destination element selector or x,y coordinates
    pub destination: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Mouse button (left, right, middle)
    #[arg(long, default_value = "left")]
    #[serde(default = "default_button")]
    pub button: String,
}

pub const COMMAND_NAME: &str = "browser.drag";

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

// ── Destination parsing ─────────────────────────────────────────────

enum DragDestination {
    Coordinates(f64, f64),
    Selector(String),
}

/// Parse the destination arg into coordinates or a selector.
///
/// Same heuristic as click: if the first character is a digit, comma, or
/// minus-digit, treat it as a coordinate attempt and validate strictly.
fn parse_destination(input: &str) -> Result<DragDestination, ActionResult> {
    let trimmed = input.trim();
    let first = trimmed.chars().next().unwrap_or(' ');

    let is_coord_attempt = first.is_ascii_digit()
        || first == ','
        || (first == '-' && trimmed.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));

    if !is_coord_attempt {
        return Ok(DragDestination::Selector(trimmed.to_string()));
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

    Ok(DragDestination::Coordinates(x, y))
}

// ── Execute ────────────────────────────────────────────────────────

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
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

    // Parse destination
    let destination = match parse_destination(&cmd.destination) {
        Ok(d) => d,
        Err(e) => return e,
    };

    // Get CDP session and verify tab
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Enlarge the viewport before resolving coordinates so that elements
    // beyond the default headless viewport (e.g. position:fixed at large
    // offsets) are reachable by both DOM queries and Input.dispatchMouseEvent.
    // Coordinates are then resolved under this final viewport, avoiding the
    // stale-coordinate problem on responsive pages.
    let viewport_set = ensure_viewport(&cdp, &target_id, &destination).await;

    let result = execute_inner(&cdp, &target_id, cmd, &destination, registry).await;

    // Always clear the viewport override
    if viewport_set {
        let _ = cdp
            .execute_on_tab(
                &target_id,
                "Emulation.clearDeviceMetricsOverride",
                json!({}),
            )
            .await;
    }

    result
}

// ── Response builder ───────────────────────────────────────────────

fn build_response(
    source_selector: &str,
    raw_destination: &str,
    destination: &DragDestination,
    url_changed: bool,
    focus_changed: bool,
    post_url: Option<String>,
    post_title: Option<String>,
) -> serde_json::Value {
    let dest_obj = match destination {
        DragDestination::Selector(_) => json!({ "selector": raw_destination }),
        DragDestination::Coordinates(_, _) => json!({ "coordinates": raw_destination }),
    };

    let mut data = json!({
        "action": "drag",
        "target": { "selector": source_selector },
        "destination": dest_obj,
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

/// Ensure the viewport height is large enough for the drag endpoints
/// without changing the viewport width (to avoid flipping responsive
/// breakpoints or triggering resize handlers).
///
/// Returns `true` if the override was applied (and needs clearing).
async fn ensure_viewport(cdp: &CdpSession, target_id: &str, destination: &DragDestination) -> bool {
    // Read the current viewport dimensions
    let current = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({
                "expression": "({w:window.innerWidth,h:window.innerHeight})",
                "returnByValue": true,
            }),
        )
        .await
        .ok();
    let (cur_w, cur_h) = current
        .as_ref()
        .and_then(|v| v.pointer("/result/result/value"))
        .map(|v| {
            (
                v["w"].as_u64().unwrap_or(800),
                v["h"].as_u64().unwrap_or(600),
            )
        })
        .unwrap_or((800, 600));

    let min_h: u64 = match destination {
        DragDestination::Coordinates(_, y) => (*y as u64) + 100,
        DragDestination::Selector(_) => 1200,
    };

    // Only resize if the current viewport is too short
    if cur_h >= min_h {
        return false;
    }

    cdp.execute_on_tab(
        target_id,
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width": cur_w,
            "height": min_h,
            "deviceScaleFactor": 1,
            "mobile": false,
        }),
    )
    .await
    .is_ok()
}

/// Inner execute logic run under the enlarged viewport.
async fn execute_inner(
    cdp: &CdpSession,
    target_id: &str,
    cmd: &Cmd,
    destination: &DragDestination,
    registry: &SharedRegistry,
) -> ActionResult {
    // Resolve source element to centre coordinates
    let (src_x, src_y) = match element::resolve_element_center(
        cdp,
        target_id,
        &cmd.source,
        registry,
        &cmd.session,
        &cmd.tab,
    )
    .await
    {
        Ok(coords) => coords,
        Err(e) => return e,
    };

    // Resolve destination to coordinates
    let (dst_x, dst_y) = match destination {
        DragDestination::Coordinates(x, y) => (*x, *y),
        DragDestination::Selector(sel) => {
            match element::resolve_element_center(
                cdp,
                target_id,
                sel,
                registry,
                &cmd.session,
                &cmd.tab,
            )
            .await
            {
                Ok(coords) => coords,
                Err(e) => return e,
            }
        }
    };

    // Pre-drag state
    let pre_url = navigation::get_tab_url(cdp, target_id).await;
    let pre_focus = get_active_element_id(cdp, target_id).await;

    // Dispatch drag: mousePressed → mouseMoved → mouseReleased
    if let Err(e) = dispatch_drag(cdp, target_id, src_x, src_y, dst_x, dst_y, &cmd.button).await {
        return e;
    }

    // Store cursor position in registry for cursor-position command
    {
        let mut reg = registry.lock().await;
        reg.set_cursor_position(&cmd.session, &cmd.tab, dst_x, dst_y);
    }

    // Post-drag state
    let post_url = navigation::get_tab_url(cdp, target_id).await;
    let post_title = navigation::get_tab_title(cdp, target_id).await;
    let post_focus = get_active_element_id(cdp, target_id).await;

    let url_changed = !pre_url.is_empty() && pre_url != post_url;
    let focus_changed = pre_focus != post_focus;

    ActionResult::ok(build_response(
        &cmd.source,
        &cmd.destination,
        destination,
        url_changed,
        focus_changed,
        Some(post_url),
        Some(post_title),
    ))
}

/// Dispatch CDP mouse events for the drag gesture.
async fn dispatch_drag(
    cdp: &CdpSession,
    target_id: &str,
    src_x: f64,
    src_y: f64,
    dst_x: f64,
    dst_y: f64,
    button: &str,
) -> Result<(), ActionResult> {
    let buttons_mask = match button {
        "right" => 2,
        "middle" => 4,
        _ => 1, // left
    };

    // 1. mousePressed at source
    cdp.execute_on_tab(
        target_id,
        "Input.dispatchMouseEvent",
        json!({
            "type": "mousePressed",
            "x": src_x,
            "y": src_y,
            "button": button,
            "clickCount": 1,
            "buttons": buttons_mask,
        }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    // 2. mouseMoved along the path
    let steps = 5;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let mx = src_x + (dst_x - src_x) * t;
        let my = src_y + (dst_y - src_y) * t;
        cdp.execute_on_tab(
            target_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseMoved",
                "x": mx,
                "y": my,
                "button": button,
                "buttons": buttons_mask,
            }),
        )
        .await
        .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;
    }

    // 3. mouseReleased at destination
    cdp.execute_on_tab(
        target_id,
        "Input.dispatchMouseEvent",
        json!({
            "type": "mouseReleased",
            "x": dst_x,
            "y": dst_y,
            "button": button,
            "clickCount": 1,
        }),
    )
    .await
    .map_err(|e| cdp_error_to_result(e, "CDP_ERROR"))?;

    Ok(())
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
