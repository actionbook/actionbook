use std::path::Path;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::browser::element::TabContext;
use crate::browser::navigation;
use crate::daemon::cdp_session::cdp_error_to_result;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

/// Upload files to a file input
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser upload \"input[type=file]\" /tmp/photo.png --session s1 --tab t1
  actionbook browser upload @e6 /tmp/a.pdf /tmp/b.pdf --session s1 --tab t1

Accepts a CSS selector, XPath, or snapshot ref (@eN from snapshot output).
Sets files on a <input type=\"file\"> element. Paths must be absolute.
Pass multiple paths for multi-file inputs.")]
pub struct Cmd {
    /// Selector for file input (CSS, XPath, or @ref)
    pub selector: String,
    /// Absolute file paths to upload
    #[arg(required = true, num_args = 1..)]
    pub files: Vec<String>,
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
}

pub const COMMAND_NAME: &str = "browser upload";

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
    // Validate that all file paths are absolute
    for file in &cmd.files {
        if !Path::new(file).is_absolute() {
            return ActionResult::fatal(
                "INVALID_ARGUMENT",
                format!("file path must be absolute: '{file}'"),
            );
        }
    }

    // Get CDP session and verify tab
    let mut ctx = match TabContext::new(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve the file input element
    let node_id = match ctx.resolve_node(&cmd.selector).await {
        Ok(id) => id,
        Err(e) => return e,
    };

    // Set files on the input via DOM.setFileInputFiles
    if let Err(e) = ctx
        .execute_on_element(
            "DOM.setFileInputFiles",
            json!({
                "files": cmd.files,
                "nodeId": node_id,
            }),
        )
        .await
    {
        return cdp_error_to_result(e, "CDP_ERROR");
    }

    let url = navigation::get_tab_url(&ctx.cdp, &ctx.target_id).await;
    let title = navigation::get_tab_title(&ctx.cdp, &ctx.target_id).await;

    ActionResult::ok(json!({
        "action": "upload",
        "target": { "selector": cmd.selector },
        "value_summary": {
            "files": cmd.files,
            "count": cmd.files.len(),
        },
        "post_url": url,
        "post_title": title,
    }))
}
