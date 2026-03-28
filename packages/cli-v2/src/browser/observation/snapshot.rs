use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::get_cdp_and_target;
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::snapshot_transform::{SnapshotOptions, build_output, maybe_truncate, parse_ax_tree};

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
    /// Include only interactive nodes
    #[arg(long, default_value_t = false)]
    #[serde(default)]
    pub interactive: bool,
    /// Remove empty structural nodes
    #[arg(long, default_value_t = false)]
    #[serde(default)]
    pub compact: bool,
    /// Maximum tree depth
    #[arg(long)]
    pub depth: Option<usize>,
    /// CSS selector to limit output to a subtree
    #[arg(long)]
    pub selector: Option<String>,
    /// Highlight cursor position (Phase 2, ignored for now)
    #[arg(long, default_value_t = false)]
    #[serde(default)]
    pub cursor: bool,
}

pub const COMMAND_NAME: &str = "browser.snapshot";

pub fn context(cmd: &Cmd, result: &ActionResult) -> Option<ResponseContext> {
    // SESSION_NOT_FOUND: context must be null per §3.1
    if let ActionResult::Fatal { code, .. } = result
        && code == "SESSION_NOT_FOUND"
    {
        return None;
    }

    // TAB_NOT_FOUND: context has session_id but tab_id must be null
    let tab_id = if let ActionResult::Fatal { code, .. } = result
        && code == "TAB_NOT_FOUND"
    {
        None
    } else {
        Some(cmd.tab.clone())
    };

    let (url, title) = match result {
        ActionResult::Ok { data } => (
            data["__ctx_url"].as_str().map(str::to_string),
            data["__ctx_title"].as_str().map(str::to_string),
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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Fetch the full AX tree
    let ax_response = match cdp
        .execute_on_tab(&target_id, "Accessibility.getFullAXTree", json!({}))
        .await
    {
        Ok(resp) => resp,
        Err(e) => return crate::daemon::cdp_session::cdp_error_to_result(e, "INTERNAL_ERROR"),
    };

    // Resolve optional CSS selector to a backendDOMNodeId
    let selector_backend_id: Option<i64> = if let Some(ref sel) = cmd.selector {
        resolve_selector_backend_id(&cdp, &target_id, sel).await
    } else {
        None
    };

    // Fetch page url and title via Runtime.evaluate
    let (ctx_url, ctx_title) = fetch_url_title(&cdp, &target_id).await;

    // Build options and parse
    let options = SnapshotOptions {
        interactive: cmd.interactive,
        compact: cmd.compact,
        depth: cmd.depth,
        selector_backend_id,
    };

    let nodes = parse_ax_tree(&ax_response, &options);
    let (nodes, truncated) = maybe_truncate(nodes);
    let output = build_output(nodes);

    let mut data = json!({
        "format": "snapshot",
        "content": output.content,
        "nodes": output.nodes,
        "stats": {
            "node_count": output.node_count,
            "interactive_count": output.interactive_count
        }
    });

    if truncated {
        data["__meta_truncated"] = json!(true);
    }
    if let Some(url) = ctx_url {
        data["__ctx_url"] = json!(url);
    }
    if let Some(title) = ctx_title {
        data["__ctx_title"] = json!(title);
    }

    ActionResult::ok(data)
}

/// Resolve a CSS selector to a backendDOMNodeId using CDP DOM domain.
/// Returns None if the selector doesn't match or any CDP call fails.
async fn resolve_selector_backend_id(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
    selector: &str,
) -> Option<i64> {
    // Get document root nodeId
    let doc = cdp
        .execute_on_tab(target_id, "DOM.getDocument", json!({ "depth": 0 }))
        .await
        .ok()?;
    let root_node_id = doc["result"]["root"]["nodeId"].as_i64()?;

    // querySelector
    let qsel = cdp
        .execute_on_tab(
            target_id,
            "DOM.querySelector",
            json!({ "nodeId": root_node_id, "selector": selector }),
        )
        .await
        .ok()?;
    let node_id = qsel["result"]["nodeId"].as_i64()?;
    if node_id == 0 {
        return None; // selector matched nothing
    }

    // Get backendNodeId
    let desc = cdp
        .execute_on_tab(target_id, "DOM.describeNode", json!({ "nodeId": node_id }))
        .await
        .ok()?;
    desc["result"]["node"]["backendNodeId"].as_i64()
}

/// Fetch page URL and title using Runtime.evaluate.
async fn fetch_url_title(
    cdp: &crate::daemon::cdp_session::CdpSession,
    target_id: &str,
) -> (Option<String>, Option<String>) {
    let url = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({ "expression": "document.URL", "returnByValue": true }),
        )
        .await
        .ok()
        .and_then(|v| v["result"]["result"]["value"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty());

    let title = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({ "expression": "document.title", "returnByValue": true }),
        )
        .await
        .ok()
        .and_then(|v| v["result"]["result"]["value"].as_str().map(str::to_string))
        .filter(|s| !s.is_empty());

    (url, title)
}
