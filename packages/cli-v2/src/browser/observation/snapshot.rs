use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::snapshot_transform::{self, SnapshotOptions};

/// Capture accessibility snapshot
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    #[serde(rename = "session_id")]
    pub session: String,
    /// Tab ID
    #[arg(long)]
    #[serde(rename = "tab_id")]
    pub tab: String,
    /// Include only interactive elements
    #[arg(long, short = 'i', default_value_t = false)]
    #[serde(default)]
    pub interactive: bool,
    /// Compact output, remove empty structural nodes
    #[arg(long, short = 'c', default_value_t = false)]
    #[serde(default)]
    pub compact: bool,
    /// Additionally include cursor-interactive custom elements (P2 — not yet implemented)
    #[arg(long, default_value_t = false, hide = true)]
    #[serde(default)]
    pub cursor: bool,
    /// Limit maximum tree depth
    #[arg(long, short = 'd')]
    #[serde(default)]
    pub depth: Option<u32>,
    /// Limit to a specific subtree by CSS selector
    #[arg(long, short = 's')]
    #[serde(default)]
    pub selector: Option<String>,
}

pub const COMMAND_NAME: &str = "browser.snapshot";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    // SESSION_NOT_FOUND: context must be null per §3.1
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }
    // TAB_NOT_FOUND: context has session_id but tab_id must be null per §3.1
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };
    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data.get("__ctx_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            data.get("__ctx_title")
                .and_then(|v| v.as_str())
                .map(String::from),
        ),
        _ => (None, None),
    };
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id,
        window_id: None,
        url,
        title,
    })
}

pub async fn execute(cmd: &Cmd, registry: &SharedRegistry) -> ActionResult {
    // Reject --selector until P2 implementation (requires CDP DOM.querySelector)
    if cmd.selector.is_some() {
        return ActionResult::fatal("UNSUPPORTED_OPERATION", "--selector is not yet implemented");
    }

    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Fetch the full accessibility tree via CDP
    let cdp_response = match cdp
        .execute_on_tab(&target_id, "Accessibility.getFullAXTree", json!({}))
        .await
    {
        Ok(resp) => resp,
        Err(e) => return crate::daemon::cdp_session::cdp_error_to_result(e, "INTERNAL_ERROR"),
    };

    // Query live url/title from CDP (not registry — avoids stale data after navigation)
    let url = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({"expression": "location.href"}),
        )
        .await
        .ok()
        .and_then(|v| v["result"]["result"]["value"].as_str().map(String::from));
    let title = cdp
        .execute_on_tab(
            &target_id,
            "Runtime.evaluate",
            json!({"expression": "document.title"}),
        )
        .await
        .ok()
        .and_then(|v| v["result"]["result"]["value"].as_str().map(String::from));

    // Get RefCache from registry
    let mut ref_cache = {
        let mut reg = registry.lock().await;
        reg.take_ref_cache(&cmd.session, &cmd.tab)
    };

    let options = SnapshotOptions {
        interactive: cmd.interactive,
        compact: cmd.compact,
        depth: cmd.depth.map(|d| d as usize),
        selector: None,
    };

    // Parse and transform the AX tree
    let nodes = snapshot_transform::parse_ax_tree(&cdp_response, &options, &mut ref_cache);

    // Store RefCache back (single lock)
    {
        let mut reg = registry.lock().await;
        reg.put_ref_cache(&cmd.session, &cmd.tab, ref_cache);
    }

    // Build output per §10.1
    let output = snapshot_transform::build_output(nodes);

    ActionResult::ok(json!({
        "format": "snapshot",
        "content": output.content,
        "nodes": output.nodes,
        "stats": {
            "node_count": output.node_count,
            "interactive_count": output.interactive_count,
        },
        "__ctx_url": url,
        "__ctx_title": title,
    }))
}
