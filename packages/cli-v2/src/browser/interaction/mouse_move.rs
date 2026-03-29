use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::navigation;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Move the mouse to absolute coordinates
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Coordinates in x,y format
    pub coordinates: String,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.mouse-move";

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

fn parse_coordinates(input: &str) -> Result<(f64, f64), ActionResult> {
    let trimmed = input.trim();
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

    Ok((x, y))
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Parse coordinates
    let (x, y) = match parse_coordinates(&cmd.coordinates) {
        Ok(coords) => coords,
        Err(e) => return e,
    };

    // Get CDP session and verify tab
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Pre-move state
    let pre_url = navigation::get_tab_url(&cdp, &target_id).await;
    let pre_focus = get_active_element_id(&cdp, &target_id).await;

    // Dispatch mouseMoved event
    if let Err(e) = cdp
        .execute_on_tab(
            &target_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseMoved",
                "x": x,
                "y": y,
            }),
        )
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    // Store cursor position in registry for cursor-position command
    {
        let mut reg = registry.lock().await;
        reg.set_cursor_position(&cmd.session, &cmd.tab, x, y);
    }

    // Post-move state
    let post_url = navigation::get_tab_url(&cdp, &target_id).await;
    let post_title = navigation::get_tab_title(&cdp, &target_id).await;
    let post_focus = get_active_element_id(&cdp, &target_id).await;

    let url_changed = !pre_url.is_empty() && pre_url != post_url;
    let focus_changed = pre_focus != post_focus;

    ActionResult::ok(json!({
        "action": "mouse-move",
        "target": { "coordinates": cmd.coordinates },
        "changed": {
            "url_changed": url_changed,
            "focus_changed": focus_changed,
        },
        "post_url": post_url,
        "post_title": post_title,
    }))
}

/// Snapshot of the active element for focus-change detection.
async fn get_active_element_id(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
) -> String {
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
