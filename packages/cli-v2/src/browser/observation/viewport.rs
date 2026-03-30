use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{cdp_error_to_result, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Get current viewport dimensions
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser viewport --session s1 --tab t1

Returns width and height in pixels (e.g. 1440x900).")]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser.viewport";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let url = match result {
        ActionResult::Ok { data } => data
            .get("__ctx_url")
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title: None,
    })
}

/// Format viewport dimensions as `{width}x{height}` string.
pub fn format_viewport(width: u64, height: u64) -> String {
    format!("{width}x{height}")
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let url = crate::browser::navigation::get_tab_url(&cdp, &target_id).await;

    let resp = match cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({
                "expression": "({width:window.innerWidth,height:window.innerHeight})",
                "returnByValue": true
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return cdp_error_to_result(e, "INTERNAL_ERROR"),
    };

    let obj = &resp["result"]["result"]["value"];
    let width = obj.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
    let height = obj.get("height").and_then(|v| v.as_u64()).unwrap_or(0);

    ActionResult::ok(json!({
        "width": width,
        "height": height,
        "__ctx_url": url,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_viewport_standard() {
        assert_eq!(format_viewport(1440, 900), "1440x900");
    }

    #[test]
    fn test_format_viewport_hd() {
        assert_eq!(format_viewport(1920, 1080), "1920x1080");
    }

    #[test]
    fn test_format_viewport_small() {
        assert_eq!(format_viewport(375, 667), "375x667");
    }

    #[test]
    fn test_format_viewport_square() {
        assert_eq!(format_viewport(800, 800), "800x800");
    }

    #[test]
    fn test_format_viewport_zero_height() {
        assert_eq!(format_viewport(1280, 0), "1280x0");
    }
}
