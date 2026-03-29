use std::collections::HashSet;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::snapshot_transform::{self, CursorInfo, SnapshotOptions};

/// Capture accessibility snapshot
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser snapshot --session s1 --tab t1
  actionbook browser snapshot -i --session s1 --tab t1
  actionbook browser snapshot -i -c --session s1 --tab t1
  actionbook browser snapshot -i --depth 3 --session s1 --tab t1
  actionbook browser snapshot --selector \"#main\" --session s1 --tab t1

Flags: -i (interactive only), -c (compact), --cursor (include cursor-interactive).
Elements are labeled with refs (e.g. [ref=e1]) for use in click, fill, etc.")]
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
    /// Additionally include cursor-interactive custom elements (cursor:pointer, onclick, tabindex)
    #[arg(long, default_value_t = false)]
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
    // Detect cursor-interactive elements if --cursor flag set
    let (cursor_elements, cursor_warning) = if cmd.cursor {
        match detect_cursor_elements(&cdp, &target_id).await {
            Ok(map) => (Some(map), None),
            Err(e) => (
                None,
                Some(format!("cursor detection failed: {e}, proceeding without")),
            ),
        }
    } else {
        (None, None)
    };

    let mut nodes = snapshot_transform::parse_ax_tree(
        &cdp_response,
        &options,
        &mut ref_cache,
        scope_backend_ids.as_ref(),
        cursor_elements.as_ref(),
    );

    // Apply token budget truncation (100K tokens max)
    const MAX_TOKENS: usize = 100_000;
    let truncated = {
        let (truncated_nodes, was_truncated) =
            snapshot_transform::truncate_to_tokens(&nodes, MAX_TOKENS);
        if was_truncated {
            nodes = truncated_nodes;
            true
        } else {
            false
        }
    };

    // Store RefCache back (single lock)
    {
        let mut reg = registry.lock().await;
        reg.put_ref_cache(&cmd.session, &cmd.tab, ref_cache);
    }

    // Build output per §10.1
    let output = snapshot_transform::build_output(nodes);

    let mut data = json!({
        "format": "snapshot",
        "content": output.content,
        "nodes": output.nodes,
        "stats": {
            "node_count": output.node_count,
            "interactive_count": output.interactive_count,
        },
        "__ctx_url": url,
        "__ctx_title": title,
    });
    if truncated {
        data["__truncated"] = json!(true);
    }
    if let Some(ref warning) = cursor_warning {
        data["__warnings"] = json!([warning]);
    }
    ActionResult::ok(data)
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

/// Detect cursor-interactive elements via JS evaluation + CDP DOM resolution.
/// Returns a map of backendNodeId → CursorInfo for elements with cursor:pointer,
/// onclick, tabindex, or contenteditable that are NOT standard interactive elements.
async fn detect_cursor_elements(
    cdp: &CdpSession,
    target_id: &str,
) -> Result<std::collections::HashMap<i64, CursorInfo>, crate::error::CliError> {
    // Generate a run-unique nonce to avoid colliding with page-owned data-__ab-ci attributes.
    // The nonce is prepended to the index value so cleanup only removes our markers.
    let nonce: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(42);
    let js = format!(
        r#"
(function() {{
    var results = [];
    var nonce = '{nonce}';
    if (!document.body) return results;
    var interactiveRoles = {{
        'button':1,'link':1,'textbox':1,'checkbox':1,'radio':1,'combobox':1,'listbox':1,
        'menuitem':1,'menuitemcheckbox':1,'menuitemradio':1,'option':1,'searchbox':1,
        'slider':1,'spinbutton':1,'switch':1,'tab':1,'treeitem':1
    }};
    var interactiveTags = {{
        'a':1,'button':1,'input':1,'select':1,'textarea':1,'details':1,'summary':1,'iframe':1
    }};
    var allElements = document.body.querySelectorAll('*');
    for (var i = 0; i < allElements.length; i++) {{
        var el = allElements[i];
        if (el.closest && el.closest('[hidden], [aria-hidden="true"]')) continue;
        var tagName = el.tagName.toLowerCase();
        if (interactiveTags[tagName]) continue;
        var role = el.getAttribute('role');
        if (role && interactiveRoles[role.toLowerCase()]) continue;
        var computedStyle = getComputedStyle(el);
        var hasCursorPointer = computedStyle.cursor === 'pointer';
        var hasOnClick = el.hasAttribute('onclick') || el.onclick !== null;
        var tabIndex = el.getAttribute('tabindex');
        var hasTabIndex = tabIndex !== null && tabIndex !== '-1';
        var ce = el.getAttribute('contenteditable');
        var isEditable = ce === '' || ce === 'true';
        if (!hasCursorPointer && !hasOnClick && !hasTabIndex && !isEditable) continue;
        if (hasCursorPointer && !hasOnClick && !hasTabIndex && !isEditable) {{
            var parent = el.parentElement;
            if (parent && getComputedStyle(parent).cursor === 'pointer') continue;
        }}
        var rect = el.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) continue;
        el.setAttribute('data-__ab-ci', nonce + ':' + String(results.length));
        results.push({{
            hasOnClick: hasOnClick,
            hasCursorPointer: hasCursorPointer,
            hasTabIndex: hasTabIndex,
            isEditable: isEditable
        }});
    }}
    return results;
}})()
"#
    );

    let eval_result = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({"expression": js, "returnByValue": true}),
        )
        .await?;

    let elements: Vec<Value> = eval_result["result"]["result"]["value"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if elements.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Resolve backendNodeIds — always clean up our nonce-tagged attributes afterwards
    let nonce_str = nonce.to_string();
    let resolve_result = resolve_cursor_backend_ids(cdp, target_id, &nonce_str).await;

    // Always clean up data-__ab-ci attributes tagged by THIS run (nonce prefix match)
    let cleanup_js = format!(
        "(function(){{var n='{nonce}:';var els=document.querySelectorAll('[data-__ab-ci]');var c=0;for(var i=0;i<els.length;i++){{if(els[i].getAttribute('data-__ab-ci').indexOf(n)===0){{els[i].removeAttribute('data-__ab-ci');c++}}}}return c}})()"
    );
    let _ = cdp
        .execute_on_tab(
            target_id,
            "Runtime.evaluate",
            json!({"expression": cleanup_js, "returnByValue": true}),
        )
        .await;

    let idx_to_backend = resolve_result?;

    // Build result map
    let mut map = std::collections::HashMap::new();
    for (i, elem) in elements.iter().enumerate() {
        if let Some(&bid) = idx_to_backend.get(&i) {
            let has_pointer = elem["hasCursorPointer"].as_bool().unwrap_or(false);
            let has_onclick = elem["hasOnClick"].as_bool().unwrap_or(false);
            let has_tabindex = elem["hasTabIndex"].as_bool().unwrap_or(false);
            let is_editable = elem["isEditable"].as_bool().unwrap_or(false);

            let kind = if has_pointer || has_onclick {
                "clickable"
            } else if is_editable {
                "editable"
            } else {
                "focusable"
            };

            let mut hints = Vec::new();
            if has_pointer {
                hints.push("cursor:pointer".to_string());
            }
            if has_onclick {
                hints.push("onclick".to_string());
            }
            if has_tabindex {
                hints.push("tabindex".to_string());
            }
            if is_editable {
                hints.push("contenteditable".to_string());
            }

            map.insert(
                bid,
                CursorInfo {
                    kind: kind.to_string(),
                    hints,
                },
            );
        }
    }

    Ok(map)
}

/// Resolve data-__ab-ci tagged elements to backendNodeIds via CDP DOM queries.
/// Only matches attributes with our nonce prefix to avoid colliding with page-owned attrs.
async fn resolve_cursor_backend_ids(
    cdp: &CdpSession,
    target_id: &str,
    nonce: &str,
) -> Result<std::collections::HashMap<usize, i64>, crate::error::CliError> {
    let doc = cdp
        .execute_on_tab(target_id, "DOM.getDocument", json!({"depth": 0}))
        .await?;
    let root_node_id = doc["result"]["root"]["nodeId"].as_i64().unwrap_or(0);

    // Query all elements with our nonce-prefixed attribute value
    let selector = format!("[data-__ab-ci^=\"{nonce}:\"]");
    let query = cdp
        .execute_on_tab(
            target_id,
            "DOM.querySelectorAll",
            json!({"nodeId": root_node_id, "selector": selector}),
        )
        .await?;
    let node_ids: Vec<i64> = query["result"]["nodeIds"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();

    let nonce_prefix = format!("{nonce}:");
    let mut idx_to_backend = std::collections::HashMap::new();
    for &dom_node_id in &node_ids {
        if let Ok(desc) = cdp
            .execute_on_tab(
                target_id,
                "DOM.describeNode",
                json!({"nodeId": dom_node_id}),
            )
            .await
        {
            let backend_id = desc["result"]["node"]["backendNodeId"].as_i64();
            let ci_idx = desc["result"]["node"]["attributes"]
                .as_array()
                .and_then(|attrs| {
                    attrs
                        .iter()
                        .enumerate()
                        .find(|(_, v)| v.as_str() == Some("data-__ab-ci"))
                        .and_then(|(i, _)| attrs.get(i + 1))
                        .and_then(|v| v.as_str())
                        // Strip nonce prefix to get the index
                        .and_then(|s| s.strip_prefix(&nonce_prefix))
                        .and_then(|s| s.parse::<usize>().ok())
                });
            if let (Some(bid), Some(idx)) = (backend_id, ci_idx) {
                idx_to_backend.insert(idx, bid);
            }
        }
    }
    Ok(idx_to_backend)
}
