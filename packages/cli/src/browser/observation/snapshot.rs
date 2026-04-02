use std::collections::{HashMap, HashSet};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::action_result::ActionResult;
use crate::daemon::cdp_session::{CdpSession, get_cdp_and_target};
use crate::daemon::registry::SharedRegistry;
use crate::output::ResponseContext;

use super::snapshot_transform::{self, CursorInfo, SnapshotOptions};

fn cursor_default() -> bool {
    true
}

/// Capture accessibility snapshot
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(after_help = "\
Examples:
  actionbook browser snapshot --session s1 --tab t1
  actionbook browser snapshot -i --session s1 --tab t1
  actionbook browser snapshot -i -c --session s1 --tab t1
  actionbook browser snapshot --depth 3 --session s1 --tab t1
  actionbook browser snapshot --selector \"#main\" --session s1 --tab t1

The default snapshot contains all information including interactive elements,
structural nodes, and cursor-interactive elements. Use additional flags as needed.

Output includes a `path` field pointing to the saved snapshot file.
Elements are labeled with refs (e.g. @e8, @e9). Use @eN to target elements
in other commands: click @e5, fill @e7 \"text\", hover @e3.
Refs are stable across snapshots — if the DOM node stays the same, the ref
stays the same. This lets agents chain commands without re-snapshotting.

Sample output:
  - generic
    - link \"Home\" [ref=e8] url=https://example.com/
    - generic
      - combobox \"Search\" [ref=e9]
      - image \"clear\" [ref=e10] clickable [cursor:pointer]
    - generic
      - link \"Help\" [ref=e11] url=https://example.com/help
        - image \"Help\"")]
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
    /// Include cursor-interactive custom elements (cursor:pointer, onclick, tabindex) — enabled by default
    #[arg(long, default_value_t = true)]
    #[serde(default = "cursor_default")]
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

pub const COMMAND_NAME: &str = "browser snapshot";

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
        None, // main frame
    );

    // Expand 1 level of iframe content (only from main frame, no recursion)
    expand_iframes(&cdp, &target_id, &mut nodes, &mut ref_cache, &options).await;

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

    // Write snapshot content to a file in the session data directory.
    let session_data_dir = crate::config::session_data_dir(&cmd.session);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let snapshot_path = session_data_dir.join(format!("snapshot_{ts}.txt"));
    let snapshot_path_str = snapshot_path.to_string_lossy().to_string();

    if let Err(e) = std::fs::write(&snapshot_path, &output.content) {
        return ActionResult::fatal(
            "ARTIFACT_WRITE_FAILED",
            format!("failed to write snapshot to {snapshot_path_str}: {e}"),
        );
    }

    let mut data = json!({
        "format": "snapshot",
        "path": snapshot_path_str,
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

// ── iframe expansion helpers ──────────────────────────────────────

/// Resolve the child frame ID for an iframe element given its backendNodeId.
/// Uses DOM.describeNode to get contentDocument.frameId.
async fn resolve_iframe_frame_id(
    cdp: &CdpSession,
    target_id: &str,
    backend_node_id: i64,
) -> Result<String, String> {
    let describe = cdp
        .execute_on_tab(
            target_id,
            "DOM.describeNode",
            json!({ "backendNodeId": backend_node_id, "depth": 1 }),
        )
        .await
        .map_err(|e| format!("DOM.describeNode failed: {e}"))?;

    // Try contentDocument.frameId first (standard for iframes)
    if let Some(frame_id) = describe
        .pointer("/result/node/contentDocument/frameId")
        .and_then(|v| v.as_str())
    {
        return Ok(frame_id.to_string());
    }

    // Fallback: the node itself may have a frameId
    describe
        .pointer("/result/node/frameId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "could not resolve iframe frame ID".to_string())
}

/// Fetch the accessibility tree for a child frame.
/// Cross-origin iframes (found in iframe_sessions) use their dedicated CDP session.
/// Same-origin iframes use the parent session with a frameId parameter.
async fn fetch_iframe_ax_tree(
    cdp: &CdpSession,
    target_id: &str,
    frame_id: &str,
    iframe_sessions: &HashMap<String, String>,
) -> Result<Value, String> {
    if let Some(iframe_sid) = iframe_sessions.get(frame_id) {
        // Cross-origin: use dedicated iframe CDP session (no frameId param needed)
        cdp.execute("Accessibility.getFullAXTree", json!({}), Some(iframe_sid))
            .await
            .map_err(|e| format!("iframe AX tree (cross-origin) failed: {e}"))
    } else {
        // Same-origin: use parent session with frameId parameter
        cdp.execute_on_tab(
            target_id,
            "Accessibility.getFullAXTree",
            json!({ "frameId": frame_id }),
        )
        .await
        .map_err(|e| format!("iframe AX tree (same-origin) failed: {e}"))
    }
}

/// Enable DOM and Accessibility domains on newly discovered iframe sessions.
/// Called before querying iframe AX trees.
async fn enable_iframe_sessions(cdp: &CdpSession) {
    let pending = cdp.drain_pending_iframe_enables().await;
    for sid in &pending {
        let _ = cdp.execute("DOM.enable", json!({}), Some(sid)).await;
        let _ = cdp
            .execute("Accessibility.enable", json!({}), Some(sid))
            .await;
    }
}

/// Expand 1 level of iframe content into the snapshot node list.
/// For each Iframe node with a ref, resolves its child frame, fetches the AX tree,
/// and inserts child nodes right after the Iframe node with depth += iframe_depth + 1.
async fn expand_iframes(
    cdp: &CdpSession,
    target_id: &str,
    nodes: &mut Vec<snapshot_transform::AXNode>,
    ref_cache: &mut snapshot_transform::RefCache,
    options: &SnapshotOptions,
) {
    // Enable any pending iframe sessions first
    enable_iframe_sessions(cdp).await;

    let iframe_sessions = cdp.iframe_sessions().await;

    // Collect iframe nodes to expand (index, ref_id, depth)
    // We collect first to avoid borrow issues during mutation.
    let iframe_info: Vec<(usize, String, usize)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.role == "Iframe" && !n.ref_id.is_empty())
        .map(|(i, n)| (i, n.ref_id.clone(), n.depth))
        .collect();

    // Process in reverse order so insertion indices stay valid
    for (idx, ref_id, iframe_depth) in iframe_info.into_iter().rev() {
        let backend_node_id = match ref_cache.backend_node_id_for_ref(&ref_id) {
            Some(bid) if bid > 0 => bid,
            _ => continue,
        };

        // Resolve child frame ID
        let child_frame_id = match resolve_iframe_frame_id(cdp, target_id, backend_node_id).await {
            Ok(fid) => fid,
            Err(_) => continue, // silently skip inaccessible iframes
        };

        // Fetch child AX tree
        let child_response =
            match fetch_iframe_ax_tree(cdp, target_id, &child_frame_id, &iframe_sessions).await {
                Ok(resp) => resp,
                Err(_) => continue, // silently skip
            };

        // Parse child tree with frame_id for RefCache isolation
        let mut child_nodes = snapshot_transform::parse_ax_tree(
            &child_response,
            options,
            ref_cache,
            None, // no selector scope for iframe content
            None, // no cursor detection in iframes (main frame only)
            Some(&child_frame_id),
        );

        if child_nodes.is_empty() {
            continue;
        }

        // Adjust depth: child nodes should be nested under the Iframe node
        let depth_offset = iframe_depth + 1;
        for child in &mut child_nodes {
            child.depth += depth_offset;
        }

        // Insert right after the Iframe node
        let insert_at = idx + 1;
        // Splice child nodes into the flat list
        let tail = nodes.split_off(insert_at);
        nodes.extend(child_nodes);
        nodes.extend(tail);
    }
}

// ── Selector scope helpers ────────────────────────────────────────

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
