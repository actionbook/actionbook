//! Snapshot data transformation: CDP AX tree → §10.1 spec output.
//!
//! These functions are pure logic (no browser/CDP dependency) and are unit-tested.
//!
//! Contract per api-reference.md §10.1:
//! - `format`: always "snapshot"
//! - `content`: string with `[ref=eN]` labels, one node per line
//! - `nodes`: array with ref/role/name/value fields
//! - `stats`: node_count / interactive_count

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Role classification ───────────────────────────────────────────────────────

/// Roles that are always assigned a [ref=eN] label and count as interactive.
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "checkbox",
    "combobox",
    "link",
    "listbox",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "radio",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "textbox",
    "treeitem",
];

/// Roles that receive a [ref=eN] label only when they have a non-empty accessible name.
const CONTENT_ROLES: &[&str] = &[
    "heading",
    "cell",
    "gridcell",
    "columnheader",
    "rowheader",
    "listitem",
    "article",
    "region",
    "main",
    "navigation",
];

/// Roles that are skipped during rendering; their children are promoted to the
/// same effective depth (transparent / pass-through nodes).
const SKIP_ROLES: &[&str] = &[
    "InlineTextBox",
    "StaticText",
    "LineBreak",
    "ListMarker",
    "strong",
    "emphasis",
    "subscript",
    "superscript",
    "mark",
];

/// Root roles that wrap the page — transparent like SKIP_ROLES.
const ROOT_ROLES: &[&str] = &["RootWebArea", "WebArea"];

// ── Public type definitions ───────────────────────────────────────────────────

/// A normalised accessibility node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AXNode {
    /// Stable reference label, e.g. "e1", "e2", ...  Empty when no ref assigned.
    pub ref_id: String,
    /// ARIA role string (e.g. "button", "textbox")
    pub role: String,
    /// Accessible name
    pub name: String,
    /// Current value (inputs, text areas); empty string if not applicable
    pub value: String,
    /// Whether this node's role is in INTERACTIVE_ROLES
    pub interactive: bool,
    /// Effective tree depth in the rendered output (0 = top-level after unwrapping roots)
    pub depth: usize,
    /// CDP backendDOMNodeId — used for selector-based subtree filtering in execute()
    #[serde(default)]
    pub backend_node_id: Option<i64>,
    /// Children (unused in flat output; reserved for future tree mode)
    pub children: Vec<AXNode>,
}

/// Options that control snapshot output.
#[derive(Debug, Clone, Default)]
pub struct SnapshotOptions {
    /// Include only interactive nodes
    pub interactive: bool,
    /// Remove empty structural nodes
    pub compact: bool,
    /// Maximum tree depth (None = unlimited)
    pub depth: Option<usize>,
    /// CSS selector to limit subtree (None = whole page)
    pub selector: Option<String>,
    /// Resolved selector root backendDOMNodeId — set by execute() after a DOM query.
    /// When Some(id), the flat list is filtered to the subtree rooted at that node.
    pub selector_backend_id: Option<i64>,
}

/// Snapshot output ready to serialise as §10.1 data.
#[derive(Debug, Clone)]
pub struct SnapshotOutput {
    pub content: String,
    pub nodes: Vec<NodeEntry>,
    pub node_count: usize,
    pub interactive_count: usize,
}

/// Flat node entry for the `data.nodes` array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeEntry {
    pub r#ref: String,
    pub role: String,
    pub name: String,
    pub value: String,
}

// ── Role classification helpers ───────────────────────────────────────────────

/// Returns true when the role is in INTERACTIVE_ROLES.
pub fn is_interactive_role(role: &str) -> bool {
    INTERACTIVE_ROLES.contains(&role)
}

/// Returns true when the role is in CONTENT_ROLES.
pub fn is_content_role(role: &str) -> bool {
    CONTENT_ROLES.contains(&role)
}

fn is_skip_role(role: &str) -> bool {
    SKIP_ROLES.contains(&role)
}

fn is_root_role(role: &str) -> bool {
    ROOT_ROLES.contains(&role)
}

// ── Filter functions (also used standalone; all unit-tested) ─────────────────

/// Filter: keep only interactive nodes (where `interactive == true`).
pub fn filter_interactive(nodes: Vec<AXNode>) -> Vec<AXNode> {
    nodes.into_iter().filter(|n| n.interactive).collect()
}

/// Filter: remove empty structural nodes (role is generic/none and name is empty).
pub fn filter_compact(nodes: Vec<AXNode>) -> Vec<AXNode> {
    nodes
        .into_iter()
        .filter(|n| {
            // Keep if has a meaningful role or non-empty name
            !matches!(n.role.as_str(), "generic" | "none" | "") || !n.name.is_empty()
        })
        .collect()
}

/// Filter: keep only nodes up to the given maximum depth.
pub fn apply_depth(nodes: Vec<AXNode>, max_depth: usize) -> Vec<AXNode> {
    nodes.into_iter().filter(|n| n.depth <= max_depth).collect()
}

/// Filter: keep only nodes belonging to the subtree rooted at `selector`.
///
/// CSS selector → AX subtree matching requires a nodeId lookup via CDP, which is
/// performed in `execute()` before calling this function. The matching node IDs are
/// passed in as `allowed_ref_ids`. An empty set means selector matched nothing —
/// return an empty list.
///
/// When called from `parse_ax_tree` without DOM context, `allowed_ref_ids` is empty
/// and the filter is a no-op (all nodes returned) until execute() wires the subtree.
pub fn apply_selector(nodes: Vec<AXNode>, allowed_ref_ids: &[String]) -> Vec<AXNode> {
    if allowed_ref_ids.is_empty() {
        // No selector resolved yet (pure parse context) — return all nodes unchanged.
        return nodes;
    }
    nodes
        .into_iter()
        .filter(|n| allowed_ref_ids.contains(&n.ref_id))
        .collect()
}

/// Filter: keep the subtree rooted at the node with the given `backend_node_id`.
///
/// Walks the flat DFS-ordered list: finds the root by backendNodeId, then collects
/// the root node and all following nodes at a strictly greater depth.  If the root
/// is not found (e.g. it is a transparent wrapper node), all nodes are returned unchanged.
pub fn filter_selector_subtree(nodes: Vec<AXNode>, root_backend_id: i64) -> Vec<AXNode> {
    let root_pos = nodes
        .iter()
        .position(|n| n.backend_node_id == Some(root_backend_id));

    let Some(root_pos) = root_pos else {
        // Selector root not visible in AX tree (transparent wrapper) — return all
        return nodes;
    };

    let root_depth = nodes[root_pos].depth;
    let mut result = Vec::new();
    for (i, n) in nodes.into_iter().enumerate() {
        if i < root_pos {
            continue;
        }
        if i == root_pos || n.depth > root_depth {
            result.push(n);
        } else {
            break; // back to same or shallower depth — subtree ends
        }
    }
    result
}

// ── Content rendering ─────────────────────────────────────────────────────────

/// Render a flat node list to `content` string with `[ref=eN]` labels.
/// Format per §10.1: `- role "name" [ref=eN]` with depth-based indentation.
/// Nodes without a ref label render as `- role "name" []`.
pub fn render_content(nodes: &[AXNode]) -> String {
    let mut lines = Vec::new();
    for node in nodes {
        let indent = "  ".repeat(node.depth);
        let mut line = format!(
            "{indent}- {} \"{}\" [ref={}]",
            node.role, node.name, node.ref_id
        );
        if !node.value.is_empty() {
            line.push_str(&format!(" value=\"{}\"", node.value));
        }
        lines.push(line);
    }
    lines.join("\n")
}

// ── Stats ─────────────────────────────────────────────────────────────────────

/// Build stats from a flat node list.
pub fn build_stats(nodes: &[AXNode]) -> (usize, usize) {
    let node_count = nodes.len();
    let interactive_count = nodes.iter().filter(|n| n.interactive).count();
    (node_count, interactive_count)
}

// ── Truncation ────────────────────────────────────────────────────────────────

/// Token budget: ~100 000 chars ≈ 25 000 tokens (4 chars/token for compact text).
const MAX_CONTENT_CHARS: usize = 100_000;

/// If `nodes` would produce content exceeding the token budget, truncate and
/// return `(truncated_nodes, true)`.  Otherwise return `(nodes, false)`.
pub fn maybe_truncate(nodes: Vec<AXNode>) -> (Vec<AXNode>, bool) {
    let estimated: usize = nodes.iter().map(|n| n.role.len() + n.name.len() + 20).sum();
    if estimated <= MAX_CONTENT_CHARS {
        return (nodes, false);
    }
    // Binary-search for the largest prefix that fits
    let mut budget = MAX_CONTENT_CHARS;
    let mut kept = Vec::new();
    for n in nodes {
        let cost = n.role.len() + n.name.len() + 20;
        if cost > budget {
            return (kept, true);
        }
        budget -= cost;
        kept.push(n);
    }
    (kept, false)
}

// ── Core parse function ───────────────────────────────────────────────────────

/// Parse CDP `Accessibility.getFullAXTree` response into a flat `AXNode` list.
///
/// The CDP response has shape:
/// ```json
/// { "result": { "nodes": [ { "nodeId": "1", "role": {"value":"button"}, ... } ] } }
/// ```
///
/// Implementation (per agent-browser reference):
/// 1. Index nodes by `nodeId` into a HashMap.
/// 2. Find root nodes (not referenced by any `childIds`).
/// 3. DFS traversal: transparent nodes (ignored / SKIP_ROLES / ROOT_ROLES) promote
///    their children to the same effective depth.
/// 4. Assign `[ref=eN]` to interactive roles (always) and content roles (when named).
/// 5. Apply filters from `options` (interactive, compact, depth, selector_backend_id).
pub fn parse_ax_tree(response: &Value, options: &SnapshotOptions) -> Vec<AXNode> {
    let nodes_json = match response["result"]["nodes"].as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return vec![],
    };

    // ── Parse raw node data ───────────────────────────────────────────────────
    struct RawNode {
        node_id: String,
        backend_dom_node_id: Option<i64>,
        ignored: bool,
        role: String,
        name: String,
        value: String,
        child_ids: Vec<String>,
    }

    let raw: Vec<RawNode> = nodes_json
        .iter()
        .map(|n| {
            let role = n["role"]["value"].as_str().unwrap_or("generic").to_string();
            let name = n["name"]["value"].as_str().unwrap_or("").to_string();
            let value = n["value"]["value"].as_str().unwrap_or("").to_string();
            let ignored = n["ignored"].as_bool().unwrap_or(false);
            let backend_dom_node_id = n["backendDOMNodeId"].as_i64();
            let node_id = n["nodeId"].as_str().unwrap_or("").to_string();
            let child_ids = n["childIds"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            RawNode {
                node_id,
                backend_dom_node_id,
                ignored,
                role,
                name,
                value,
                child_ids,
            }
        })
        .collect();

    // ── Build index + find roots ──────────────────────────────────────────────
    let id_to_idx: HashMap<&str, usize> = raw
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.node_id.is_empty())
        .map(|(i, n)| (n.node_id.as_str(), i))
        .collect();

    // All nodeIds that appear as a child of some other node
    let mut is_child: HashSet<&str> = HashSet::new();
    for n in &raw {
        for child_id in &n.child_ids {
            is_child.insert(child_id.as_str());
        }
    }

    // Roots: non-empty nodeId, not referenced as a child
    let roots: Vec<usize> = raw
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.node_id.is_empty() && !is_child.contains(n.node_id.as_str()))
        .map(|(i, _)| i)
        .collect();

    // ── DFS traversal ─────────────────────────────────────────────────────────
    // Stack entries: (node_idx, effective_depth)
    // effective_depth accounts for transparent nodes that don't contribute to depth.
    let mut result: Vec<AXNode> = Vec::new();
    let mut ref_counter = 0usize;

    // Push roots in reverse so first root is processed first
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&i| (i, 0)).collect();

    while let Some((idx, eff_depth)) = stack.pop() {
        let node = &raw[idx];

        // Depth limit — stop descending when at or beyond max depth
        if let Some(max_d) = options.depth
            && eff_depth > max_d
        {
            continue;
        }

        // Transparent nodes: skip self, promote children to same effective depth
        let is_transparent = node.ignored || is_skip_role(&node.role) || is_root_role(&node.role);

        if is_transparent {
            for child_id in node.child_ids.iter().rev() {
                if let Some(&child_idx) = id_to_idx.get(child_id.as_str()) {
                    stack.push((child_idx, eff_depth));
                }
            }
            continue;
        }

        let interactive = is_interactive_role(&node.role);
        let has_content_ref = is_content_role(&node.role) && !node.name.is_empty();

        // In --interactive mode: skip non-interactive nodes (flatten children to same depth)
        if options.interactive && !interactive {
            for child_id in node.child_ids.iter().rev() {
                if let Some(&child_idx) = id_to_idx.get(child_id.as_str()) {
                    stack.push((child_idx, eff_depth));
                }
            }
            continue;
        }

        // Assign ref to interactive nodes and to named content nodes
        let has_ref = interactive || has_content_ref;
        let ref_id = if has_ref {
            ref_counter += 1;
            format!("e{ref_counter}")
        } else {
            String::new()
        };

        result.push(AXNode {
            ref_id,
            role: node.role.clone(),
            name: node.name.clone(),
            value: node.value.clone(),
            interactive,
            depth: eff_depth,
            backend_node_id: node.backend_dom_node_id,
            children: vec![],
        });

        // Push children (reversed so they are processed in document order)
        for child_id in node.child_ids.iter().rev() {
            if let Some(&child_idx) = id_to_idx.get(child_id.as_str()) {
                stack.push((child_idx, eff_depth + 1));
            }
        }
    }

    // ── Post-DFS filters ──────────────────────────────────────────────────────

    // Selector subtree filter (applied before compact so structural parents are preserved)
    if let Some(root_backend_id) = options.selector_backend_id {
        result = filter_selector_subtree(result, root_backend_id);
    }

    // Compact: remove empty structural nodes
    if options.compact {
        result = filter_compact(result);
    }

    result
}

/// Build the full SnapshotOutput from a flat node list.
pub fn build_output(nodes: Vec<AXNode>) -> SnapshotOutput {
    let content = render_content(&nodes);
    let (node_count, interactive_count) = build_stats(&nodes);
    let entries = nodes
        .iter()
        .filter(|n| !n.ref_id.is_empty())
        .map(|n| NodeEntry {
            r#ref: n.ref_id.clone(),
            role: n.role.clone(),
            name: n.name.clone(),
            value: n.value.clone(),
        })
        .collect();
    SnapshotOutput {
        content,
        nodes: entries,
        node_count,
        interactive_count,
    }
}

// ── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(ref_id: &str, role: &str, name: &str, interactive: bool, depth: usize) -> AXNode {
        AXNode {
            ref_id: ref_id.to_string(),
            role: role.to_string(),
            name: name.to_string(),
            value: String::new(),
            interactive,
            depth,
            backend_node_id: None,
            children: vec![],
        }
    }

    fn make_node_with_value(
        ref_id: &str,
        role: &str,
        name: &str,
        value: &str,
        interactive: bool,
        depth: usize,
    ) -> AXNode {
        AXNode {
            ref_id: ref_id.to_string(),
            role: role.to_string(),
            name: name.to_string(),
            value: value.to_string(),
            interactive,
            depth,
            backend_node_id: None,
            children: vec![],
        }
    }

    // ── is_interactive_role ──────────────────────────────────────────

    #[test]
    fn test_interactive_roles() {
        assert!(is_interactive_role("button"));
        assert!(is_interactive_role("textbox"));
        assert!(is_interactive_role("link"));
        assert!(is_interactive_role("checkbox"));
        assert!(is_interactive_role("combobox"));
    }

    #[test]
    fn test_non_interactive_roles() {
        assert!(!is_interactive_role("generic"));
        assert!(!is_interactive_role("none"));
        assert!(!is_interactive_role("heading"));
        assert!(!is_interactive_role("paragraph"));
        assert!(!is_interactive_role(""));
    }

    // ── filter_interactive ───────────────────────────────────────────

    #[test]
    fn test_filter_interactive_keeps_only_interactive() {
        let nodes = vec![
            make_node("e1", "button", "Submit", true, 0),
            make_node("e2", "heading", "Title", false, 0),
            make_node("e3", "textbox", "Search", true, 0),
            make_node("e4", "paragraph", "Text", false, 0),
        ];
        let result = filter_interactive(nodes);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e1");
        assert_eq!(result[1].ref_id, "e3");
    }

    #[test]
    fn test_filter_interactive_empty_list() {
        let result = filter_interactive(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_interactive_all_non_interactive() {
        let nodes = vec![
            make_node("e1", "heading", "Title", false, 0),
            make_node("e2", "paragraph", "Text", false, 0),
        ];
        let result = filter_interactive(nodes);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_interactive_all_interactive() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("e2", "link", "Home", true, 0),
        ];
        let result = filter_interactive(nodes.clone());
        assert_eq!(result.len(), 2);
    }

    // ── filter_compact ───────────────────────────────────────────────

    #[test]
    fn test_filter_compact_removes_empty_structural() {
        let nodes = vec![
            make_node("e1", "generic", "", false, 0), // empty structural — remove
            make_node("e2", "button", "OK", true, 0), // has name — keep
            make_node("e3", "none", "", false, 0),    // empty structural — remove
            make_node("e4", "generic", "Container", false, 0), // has name — keep
        ];
        let result = filter_compact(nodes);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e2");
        assert_eq!(result[1].ref_id, "e4");
    }

    #[test]
    fn test_filter_compact_keeps_meaningful_nodes() {
        let nodes = vec![
            make_node("e1", "heading", "Title", false, 0),
            make_node("e2", "paragraph", "", false, 0), // paragraph with no name — keep (has role)
        ];
        let result = filter_compact(nodes);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_compact_empty_list() {
        let result = filter_compact(vec![]);
        assert!(result.is_empty());
    }

    // ── apply_depth ──────────────────────────────────────────────────

    #[test]
    fn test_apply_depth_limits_to_max() {
        let nodes = vec![
            make_node("e1", "generic", "root", false, 0),
            make_node("e2", "button", "OK", true, 1),
            make_node("e3", "link", "Home", true, 2),
            make_node("e4", "button", "Deep", true, 3),
        ];
        let result = apply_depth(nodes, 1);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].depth, 0);
        assert_eq!(result[1].depth, 1);
    }

    #[test]
    fn test_apply_depth_zero_returns_root_only() {
        let nodes = vec![
            make_node("e1", "generic", "root", false, 0),
            make_node("e2", "button", "OK", true, 1),
        ];
        let result = apply_depth(nodes, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ref_id, "e1");
    }

    #[test]
    fn test_apply_depth_large_keeps_all() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "button", "B", true, 5),
            make_node("e3", "button", "C", true, 10),
        ];
        let result = apply_depth(nodes, 100);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_apply_depth_empty_list() {
        let result = apply_depth(vec![], 5);
        assert!(result.is_empty());
    }

    // ── apply_selector ───────────────────────────────────────────────

    #[test]
    fn test_apply_selector_empty_allowed_ids_returns_all() {
        // No DOM context: allowed_ref_ids is empty → no-op, all nodes returned.
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let result = apply_selector(nodes, &[]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_selector_filters_to_allowed_ids() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
            make_node("e3", "heading", "C", false, 0),
        ];
        let allowed = vec!["e1".to_string(), "e3".to_string()];
        let result = apply_selector(nodes, &allowed);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e1");
        assert_eq!(result[1].ref_id, "e3");
    }

    #[test]
    fn test_apply_selector_no_match_returns_empty() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let allowed = vec!["e99".to_string()];
        let result = apply_selector(nodes, &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_selector_empty_list() {
        let result = apply_selector(vec![], &["e1".to_string()]);
        assert!(result.is_empty());
    }

    // ── render_content ───────────────────────────────────────────────

    #[test]
    fn test_render_content_basic() {
        let nodes = vec![
            make_node("e1", "textbox", "Search", true, 0),
            make_node("e2", "button", "Google Search", true, 0),
        ];
        let content = render_content(&nodes);
        assert!(content.contains("[ref=e1]"), "must contain [ref=e1]");
        assert!(content.contains("[ref=e2]"), "must contain [ref=e2]");
        assert!(content.contains("textbox"), "must contain role");
        assert!(content.contains("Search"), "must contain name");
    }

    #[test]
    fn test_render_content_indentation() {
        let nodes = vec![
            make_node("e1", "generic", "Container", false, 0),
            make_node("e2", "button", "OK", true, 1),
            make_node("e3", "button", "Cancel", true, 2),
        ];
        let content = render_content(&nodes);
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        // depth 0: no indent
        assert!(lines[0].starts_with("- "), "depth 0 has no indent");
        // depth 1: 2 spaces
        assert!(lines[1].starts_with("  - "), "depth 1 has 2-space indent");
        // depth 2: 4 spaces
        assert!(lines[2].starts_with("    - "), "depth 2 has 4-space indent");
    }

    #[test]
    fn test_render_content_includes_value() {
        let nodes = vec![make_node_with_value(
            "e1",
            "textbox",
            "Email",
            "user@example.com",
            true,
            0,
        )];
        let content = render_content(&nodes);
        assert!(
            content.contains("value=\"user@example.com\""),
            "must include value when present"
        );
    }

    #[test]
    fn test_render_content_empty_list() {
        let content = render_content(&[]);
        assert!(content.is_empty(), "empty node list produces empty content");
    }

    #[test]
    fn test_render_content_ref_labels_format() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e42", "link", "B", true, 0),
        ];
        let content = render_content(&nodes);
        assert!(content.contains("[ref=e1]"));
        assert!(content.contains("[ref=e42]"));
    }

    // ── build_stats ──────────────────────────────────────────────────

    #[test]
    fn test_build_stats_counts_correctly() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("e2", "heading", "Title", false, 0),
            make_node("e3", "textbox", "Search", true, 0),
            make_node("e4", "paragraph", "Text", false, 0),
        ];
        let (node_count, interactive_count) = build_stats(&nodes);
        assert_eq!(node_count, 4);
        assert_eq!(interactive_count, 2);
    }

    #[test]
    fn test_build_stats_empty_list() {
        let (node_count, interactive_count) = build_stats(&[]);
        assert_eq!(node_count, 0);
        assert_eq!(interactive_count, 0);
    }

    #[test]
    fn test_build_stats_all_interactive() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let (node_count, interactive_count) = build_stats(&nodes);
        assert_eq!(node_count, 2);
        assert_eq!(interactive_count, 2);
    }

    #[test]
    fn test_build_stats_none_interactive() {
        let nodes = vec![
            make_node("e1", "heading", "Title", false, 0),
            make_node("e2", "paragraph", "Text", false, 0),
        ];
        let (node_count, interactive_count) = build_stats(&nodes);
        assert_eq!(node_count, 2);
        assert_eq!(interactive_count, 0);
    }

    // ── build_output ─────────────────────────────────────────────────

    #[test]
    fn test_build_output_complete() {
        let nodes = vec![
            make_node("e1", "textbox", "Search", true, 0),
            make_node("e2", "button", "Go", true, 0),
        ];
        let output = build_output(nodes);
        assert_eq!(output.node_count, 2);
        assert_eq!(output.interactive_count, 2);
        assert!(output.content.contains("[ref=e1]"));
        assert!(output.content.contains("[ref=e2]"));
        assert_eq!(output.nodes.len(), 2);
        assert_eq!(output.nodes[0].r#ref, "e1");
        assert_eq!(output.nodes[0].role, "textbox");
        assert_eq!(output.nodes[0].name, "Search");
    }

    #[test]
    fn test_build_output_node_entries_have_required_fields() {
        let nodes = vec![make_node_with_value(
            "e1",
            "textbox",
            "Email",
            "test@test.com",
            true,
            0,
        )];
        let output = build_output(nodes);
        let entry = &output.nodes[0];
        assert_eq!(entry.r#ref, "e1");
        assert_eq!(entry.role, "textbox");
        assert_eq!(entry.name, "Email");
        assert_eq!(entry.value, "test@test.com");
    }

    // ── parse_ax_tree ────────────────────────────────────────────────

    #[test]
    fn test_parse_ax_tree_basic() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "Submit"} },
                    { "nodeId": "2", "role": {"value": "textbox"}, "name": {"value": "Email"} },
                ]
            }
        });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].ref_id, "e1");
        assert_eq!(nodes[0].role, "button");
        assert_eq!(nodes[0].name, "Submit");
        assert!(nodes[0].interactive);
        assert_eq!(nodes[1].ref_id, "e2");
        assert_eq!(nodes[1].role, "textbox");
    }

    #[test]
    fn test_parse_ax_tree_empty_response() {
        let response = serde_json::json!({ "result": { "nodes": [] } });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_ax_tree_missing_nodes() {
        let response = serde_json::json!({ "result": {} });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_ax_tree_interactive_filter() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "Submit"} },
                    { "nodeId": "2", "role": {"value": "heading"}, "name": {"value": "Title"} },
                    { "nodeId": "3", "role": {"value": "link"}, "name": {"value": "Home"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            interactive: true,
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        // --interactive: only interactive roles (button + link); heading excluded
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.interactive));
    }

    #[test]
    fn test_parse_ax_tree_compact_filter() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "generic"}, "name": {"value": ""} },
                    { "nodeId": "2", "role": {"value": "button"}, "name": {"value": "OK"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            compact: true,
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].role, "button");
    }

    #[test]
    fn test_parse_ax_tree_depth_filter() {
        // With real depth computation: roots have depth=0, their children depth=1, etc.
        // Two flat nodes (no childIds) both appear as roots → both get depth=0.
        // A depth=Some(0) filter should keep only nodes at depth 0.
        // Since these two nodes ARE the roots (depth=0), both are kept.
        // This test verifies that depth filtering works correctly for root-level nodes.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "A"} },
                    { "nodeId": "2", "role": {"value": "button"}, "name": {"value": "B"} },
                    { "nodeId": "3", "role": {"value": "button"}, "name": {"value": "C"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            depth: Some(0),
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        // All 3 nodes are roots (no childIds, so all unparented), all at depth=0.
        // depth=Some(0) keeps nodes where depth <= 0, i.e. all 3.
        assert_eq!(
            nodes.len(),
            3,
            "three root nodes at depth=0 all survive depth=Some(0) filter"
        );
    }

    #[test]
    fn test_parse_ax_tree_depth_filter_with_children() {
        // Verify real depth propagation: parent at depth=0, child at depth=1.
        // depth=Some(0) should keep only the parent.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "navigation"}, "name": {"value": "Nav"}, "childIds": ["2", "3"] },
                    { "nodeId": "2", "role": {"value": "link"}, "name": {"value": "Home"} },
                    { "nodeId": "3", "role": {"value": "link"}, "name": {"value": "About"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            depth: Some(0),
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        // navigation is root (depth=0), links are children (depth=1).
        // depth=Some(0) keeps only navigation.
        assert_eq!(nodes.len(), 1, "depth=0 keeps only root; got {:?}", nodes);
        assert_eq!(nodes[0].role, "navigation");
    }

    #[test]
    fn test_parse_ax_tree_selector_option_accepted() {
        // selector filtering via apply_selector() requires DOM context (nodeId lookup)
        // which is wired in execute(), not in parse_ax_tree. This UT verifies parse_ax_tree
        // accepts the selector option without panicking (no-op in pure parse context).
        // Pure apply_selector() contract is tested separately (test_apply_selector_*).
        // E2E subtree-limiting behaviour is covered by snap_selector_flag_limits_subtree.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "OK"} },
                    { "nodeId": "2", "role": {"value": "link"}, "name": {"value": "Home"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            selector: Some("body".to_string()),
            ..Default::default()
        };
        // Must not panic; actual subtree filtering handled in execute() via CDP node lookup
        let nodes = parse_ax_tree(&response, &opts);
        assert!(
            nodes.len() <= 2,
            "selector option must not expand node list"
        );
    }

    #[test]
    fn test_parse_ax_tree_assigns_sequential_refs() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "A"} },
                    { "nodeId": "2", "role": {"value": "button"}, "name": {"value": "B"} },
                    { "nodeId": "3", "role": {"value": "button"}, "name": {"value": "C"} },
                ]
            }
        });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert_eq!(nodes[0].ref_id, "e1");
        assert_eq!(nodes[1].ref_id, "e2");
        assert_eq!(nodes[2].ref_id, "e3");
    }

    // ── filter_selector_subtree ───────────────────────────────────────

    #[test]
    fn test_filter_selector_subtree_finds_root_and_descendants() {
        let mut root = make_node("e1", "navigation", "Nav", false, 0);
        root.backend_node_id = Some(10);
        let mut child1 = make_node("e2", "link", "Home", true, 1);
        child1.backend_node_id = Some(11);
        let mut child2 = make_node("e3", "link", "About", true, 1);
        child2.backend_node_id = Some(12);
        let mut sibling = make_node("e4", "button", "Submit", true, 0);
        sibling.backend_node_id = Some(20);

        let nodes = vec![root, child1, child2, sibling];
        let result = filter_selector_subtree(nodes, 10);
        // Should include nav root + its 2 children but not the sibling button
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].ref_id, "e1");
        assert_eq!(result[1].ref_id, "e2");
        assert_eq!(result[2].ref_id, "e3");
    }

    #[test]
    fn test_filter_selector_subtree_root_not_found_returns_all() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let result = filter_selector_subtree(nodes.clone(), 999);
        assert_eq!(result.len(), 2, "unknown backend_id → return all");
    }
}
