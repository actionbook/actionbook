use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::{ClickTarget, TabContext, parse_target};
use crate::browser::navigation;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::click::{dispatch_click, get_active_element_id};

fn default_button() -> String {
    "left".to_string()
}

fn default_count() -> u32 {
    1
}

/// Click multiple elements in sequence
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser batch-click \"#cookie-banner\" \"#accept\" --session s1 --tab t1
  actionbook browser batch-click @e3 @e7 @e12 --session s1 --tab t1
  actionbook browser batch-click \".step-1\" \".step-2\" \".step-3\" --session s1 --tab t1

Clicks each selector in order. Stops on the first failure.
Accepts CSS selectors, XPath, snapshot refs (@eN), or x,y coordinates.")]
pub struct Cmd {
    /// Selectors to click (CSS, XPath, @ref, or x,y coordinates)
    #[arg(required = true, num_args = 1..)]
    pub selectors: Vec<String>,
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
    /// Click count per element (2 = double-click)
    #[arg(long, default_value_t = 1)]
    #[serde(default = "default_count")]
    pub count: u32,
    /// Delay between clicks in milliseconds
    #[arg(long, default_value_t = 0)]
    #[serde(default)]
    pub delay: u64,
}

pub const COMMAND_NAME: &str = "browser batch-click";

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
    // Validate inputs
    if cmd.selectors.is_empty() {
        return ActionResult::fatal("INVALID_ARGUMENT", "at least one selector is required");
    }
    if cmd.count == 0 {
        return ActionResult::fatal("INVALID_ARGUMENT", "count must be at least 1");
    }
    if !matches!(cmd.button.as_str(), "left" | "right" | "middle") {
        return ActionResult::fatal(
            "INVALID_ARGUMENT",
            format!(
                "invalid button: '{}', expected left|right|middle",
                cmd.button
            ),
        );
    }

    // Parse all targets upfront so we fail fast on invalid selectors
    let mut targets: Vec<(String, ClickTarget)> = Vec::with_capacity(cmd.selectors.len());
    for sel in &cmd.selectors {
        match parse_target(sel) {
            Ok(t) => targets.push((sel.clone(), t)),
            Err(e) => return e,
        }
    }

    // Get CDP session
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let pre_url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let pre_focus = get_active_element_id(&ctx.cdp, &ctx.target_id).await;

    let mut results: Vec<serde_json::Value> = Vec::with_capacity(targets.len());
    let total = targets.len();

    for (i, (raw_selector, target)) in targets.iter().enumerate() {
        // Resolve element coordinates
        let (x, y) = match target {
            ClickTarget::Coordinates(cx, cy) => (*cx, *cy),
            ClickTarget::Selector(sel) => match ctx.resolve_center(sel).await {
                Ok((_node_id, cx, cy)) => (cx, cy),
                Err(e) => {
                    // Record partial results up to the failure
                    let mut data = json!({
                        "action": "batch-click",
                        "total": total,
                        "clicked": i,
                        "results": results,
                        "failed_at": { "index": i, "selector": raw_selector },
                    });
                    if let ActionResult::Fatal { code, message, .. } = &e {
                        data["failed_at"]["error"] = json!({ "code": code, "message": message });
                    }
                    add_post_state(&ctx, &mut data).await;
                    return ActionResult::fatal_with_details(
                        "BATCH_CLICK_PARTIAL",
                        format!(
                            "failed at selector {} of {}: '{}'",
                            i + 1,
                            total,
                            raw_selector
                        ),
                        "Check the details.results array for successful clicks before the failure",
                        data,
                    );
                }
            },
        };

        // Dispatch click
        if let Err(e) = dispatch_click(&ctx.cdp, &ctx.target_id, x, y, &cmd.button, cmd.count).await
        {
            let mut data = json!({
                "action": "batch-click",
                "total": total,
                "clicked": i,
                "results": results,
                "failed_at": { "index": i, "selector": raw_selector },
            });
            if let ActionResult::Fatal { code, message, .. } = &e {
                data["failed_at"]["error"] = json!({ "code": code, "message": message });
            }
            add_post_state(&ctx, &mut data).await;
            return ActionResult::fatal_with_details(
                "BATCH_CLICK_PARTIAL",
                format!(
                    "click failed at selector {} of {}: '{}'",
                    i + 1,
                    total,
                    raw_selector
                ),
                "Check the details.results array for successful clicks before the failure",
                data,
            );
        }

        // Store cursor position
        {
            let mut reg = ctx.registry().lock().await;
            reg.set_cursor_position(ctx.session_id(), ctx.tab_id(), x, y);
        }

        // Record result for this click
        let target_obj = match target {
            ClickTarget::Selector(_) => json!({ "selector": raw_selector }),
            ClickTarget::Coordinates(_, _) => json!({ "coordinates": raw_selector }),
        };
        results.push(json!({
            "index": i,
            "target": target_obj,
            "x": x as i64,
            "y": y as i64,
        }));

        // Inter-click delay (skip after last click)
        if cmd.delay > 0 && i + 1 < total {
            tokio::time::sleep(std::time::Duration::from_millis(cmd.delay)).await;
        }
    }

    // Post state
    let post_url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let post_title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;
    let post_focus = get_active_element_id(&ctx.cdp, &ctx.target_id).await;
    let url_changed = !pre_url.is_empty() && pre_url != post_url;
    let focus_changed = pre_focus != post_focus;

    ActionResult::ok(json!({
        "action": "batch-click",
        "total": total,
        "clicked": total,
        "results": results,
        "changed": {
            "url_changed": url_changed,
            "focus_changed": focus_changed,
        },
        "post_url": post_url,
        "post_title": post_title,
    }))
}

async fn add_post_state(ctx: &TabContext, data: &mut serde_json::Value) {
    let post_url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let post_title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;
    data["post_url"] = json!(post_url);
    data["post_title"] = json!(post_title);
}
