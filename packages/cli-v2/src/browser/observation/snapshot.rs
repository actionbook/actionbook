use std::collections::HashSet;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, get_cdp_and_target};
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
    let (cdp, target_id) = match get_cdp_and_target(registry, &cmd.session, &cmd.tab).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Resolve --selector to a set of backendNodeIds via CDP DOM queries
    let scope_backend_ids = if let Some(ref selector) = cmd.selector {
        match resolve_selector_scope(&cdp, &target_id, selector).await {
            Ok(ids) => Some(ids),
            Err(e) => return e,
        }
    } else {
        None
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
    let url = Some(crate::browser::navigation::get_tab_url(&cdp, &target_id).await)
        .filter(|s| !s.is_empty());
    let title = Some(crate::browser::navigation::get_tab_title(&cdp, &target_id).await)
        .filter(|s| !s.is_empty());

    // Get RefCache from registry
    let mut ref_cache = {
        let mut reg = registry.lock().await;
        reg.take_ref_cache(&cmd.session, &cmd.tab)
    };

    let options = SnapshotOptions {
        interactive: cmd.interactive,
        compact: cmd.compact,
        depth: cmd.depth.map(|d| d as usize),
        selector: cmd.selector.clone(),
    };

    // Parse and transform the AX tree
    let nodes = snapshot_transform::parse_ax_tree(
        &cdp_response,
        &options,
        &mut ref_cache,
        scope_backend_ids.as_ref(),
    );

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

/// Resolve a CSS selector to all backendNodeIds in its subtree.
/// Uses CDP DOM.getDocument → DOM.querySelector → DOM.describeNode(depth=-1).
async fn resolve_selector_scope(
    cdp: &CdpSession,
    target_id: &str,
    selector: &str,
) -> Result<HashSet<i64>, ActionResult> {
    // Get document root
    let doc = cdp
        .execute_on_tab(target_id, "DOM.getDocument", json!({"depth": 0}))
        .await
        .map_err(|e| {
            ActionResult::fatal("INTERNAL_ERROR", format!("DOM.getDocument failed: {e}"))
        })?;
    let root_node_id = doc["result"]["root"]["nodeId"]
        .as_i64()
        .ok_or_else(|| ActionResult::fatal("INTERNAL_ERROR", "no root nodeId"))?;

    // Query selector
    let query = cdp
        .execute_on_tab(
            target_id,
            "DOM.querySelector",
            json!({"nodeId": root_node_id, "selector": selector}),
        )
        .await
        .map_err(|e| {
            ActionResult::fatal(
                "ELEMENT_NOT_FOUND",
                format!("selector '{selector}' query failed: {e}"),
            )
        })?;
    let matched_node_id = query["result"]["nodeId"].as_i64().unwrap_or(0);
    if matched_node_id == 0 {
        return Err(ActionResult::fatal(
            "ELEMENT_NOT_FOUND",
            format!("selector '{selector}' did not match any element"),
        ));
    }

    // Get full subtree to collect all backendNodeIds
    let desc = cdp
        .execute_on_tab(
            target_id,
            "DOM.describeNode",
            json!({"nodeId": matched_node_id, "depth": -1}),
        )
        .await
        .map_err(|e| {
            ActionResult::fatal("INTERNAL_ERROR", format!("DOM.describeNode failed: {e}"))
        })?;

    let mut ids = HashSet::new();
    collect_backend_node_ids(&desc["result"]["node"], &mut ids);
    Ok(ids)
}

/// Recursively collect backendNodeId from a DOM.describeNode result.
fn collect_backend_node_ids(node: &Value, ids: &mut HashSet<i64>) {
    if let Some(id) = node["backendNodeId"].as_i64() {
        ids.insert(id);
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            collect_backend_node_ids(child, ids);
        }
    }
    // Shadow DOM, content documents
    if let Some(shadow) = node["shadowRoots"].as_array() {
        for root in shadow {
            collect_backend_node_ids(root, ids);
        }
    }
    if let Some(content_doc) = node.get("contentDocument") {
        collect_backend_node_ids(content_doc, ids);
    }
}
