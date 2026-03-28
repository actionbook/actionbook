//! CDP Accessibility Tree snapshot (borrowed from pinchtab's approach)
//!
//! Uses `Accessibility.getFullAXTree` to get the real browser accessibility tree,
//! then filters, assigns refs (e0, e1...), and formats for AI agent consumption.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::Result;

// ============================================================================
// Typed CDP Accessibility Tree Response Structures (Phase 2b Optimization)
// ============================================================================

/// CDP Accessibility.getFullAXTree response envelope
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct AxTreeResponse {
    pub nodes: Vec<AxNode>,
}

/// Single node in the CDP accessibility tree
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct AxNode {
    #[serde(rename = "nodeId")]
    pub node_id: String,

    #[serde(rename = "backendDOMNodeId", default)]
    pub backend_dom_node_id: Option<i64>,

    #[serde(default)]
    pub ignored: bool,

    pub role: Option<AxValue>,
    pub name: Option<AxValue>,
    pub value: Option<AxValue>,

    #[serde(rename = "childIds", default)]
    pub child_ids: Vec<String>,

    #[serde(default)]
    pub properties: Vec<AxProperty>,
}

/// CDP AXValue structure: { type: "...", value: "..." }
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct AxValue {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub value_type: Option<String>,
    pub value: Option<serde_json::Value>,
}

impl AxValue {
    /// Extract string value from AXValue
    #[allow(dead_code)]
    pub fn as_string(&self) -> String {
        if let Some(ref val) = self.value {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
            if let Some(n) = val.as_i64() {
                return n.to_string();
            }
            // Handle floating-point values (e.g., slider controls, progress bars)
            if let Some(f) = val.as_f64() {
                // Use format! to avoid scientific notation for reasonable ranges
                if f.fract() == 0.0 && f.abs() < 1e10 {
                    return format!("{:.0}", f); // Integer-like floats: 42.0 → "42"
                } else {
                    return f.to_string(); // Keep decimals: 3.14 → "3.14"
                }
            }
            if let Some(b) = val.as_bool() {
                return b.to_string();
            }
        }
        String::new()
    }
}

/// CDP AXProperty structure
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct AxProperty {
    pub name: String,
    pub value: Option<AxValue>,
}

/// A single node in the accessibility tree
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct A11yNode {
    /// Stable reference ID ("e0", "e1", ...) — only set for interactive/named content nodes
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    /// ARIA role (button, link, textbox, etc.)
    pub role: String,
    /// Accessible name
    pub name: String,
    /// Current value (for inputs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Tree depth
    pub depth: usize,
    /// Whether element is disabled
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// Whether element is focused
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
    /// Heading level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    /// Checked state (for checkboxes/radios)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<String>,
    /// Expanded state (for tree items, accordions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    /// Selected state
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
    /// Required state
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// URL (for links)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Backend DOM node ID (for action execution)
    #[serde(rename = "nodeId")]
    pub backend_node_id: i64,
}

/// Cached ref→backendNodeId mapping for action resolution
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RefCache {
    /// "e0" → backend_node_id
    pub refs: HashMap<String, i64>,
    /// Last snapshot nodes
    #[allow(dead_code)]
    pub nodes: Vec<A11yNode>,
    /// Next available ref counter (for appending cursor nodes etc.)
    pub next_ref: usize,
}

/// Snapshot filter options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SnapshotFilter {
    All,
    Interactive,
}

/// Snapshot output format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SnapshotFormat {
    /// Indented tree format: `- role "name" [ref=eN]` (~60-70% fewer tokens than JSON)
    Compact,
    /// Full JSON
    Json,
}

/// Interactive ARIA roles (from pinchtab/snapshot.go)
#[allow(dead_code)]
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "combobox",
    "listbox",
    "option",
    "checkbox",
    "radio",
    "switch",
    "slider",
    "spinbutton",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "tab",
    "treeitem",
];

/// Content roles — get refs only if they have a name
#[allow(dead_code)]
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

/// Roles to skip entirely (noise — text content is already in parent's name)
#[allow(dead_code)]
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

/// Structural roles that may be removed during compact filtering
#[allow(dead_code)]
const STRUCTURAL_ROLES: &[&str] = &[
    "generic",
    "group",
    "list",
    "table",
    "row",
    "rowgroup",
    "grid",
    "treegrid",
    "menu",
    "menubar",
    "toolbar",
    "tablist",
    "tree",
    "directory",
    "document",
    "application",
    "presentation",
    "none",
    "WebArea",
    "RootWebArea",
];

/// Parse the raw CDP Accessibility.getFullAXTree response into A11yNode list.
///
/// Builds a proper tree from CDP nodes, then renders with recursive traversal.
/// Ignored nodes' children are promoted. RootWebArea/WebArea are unwrapped.
/// Only interactive roles and named content roles get refs (eN).
#[allow(dead_code)]
pub fn parse_ax_tree(
    raw: serde_json::Value,
    filter: SnapshotFilter,
    max_depth: Option<usize>,
    scope_backend_id: Option<i64>,
) -> Result<(Vec<A11yNode>, RefCache)> {
    let response: AxTreeResponse = serde_json::from_value(raw)?;
    let ax_nodes = &response.nodes;

    let interactive_set: HashSet<&str> = INTERACTIVE_ROLES.iter().copied().collect();
    let content_set: HashSet<&str> = CONTENT_ROLES.iter().copied().collect();
    let skip_set: HashSet<&str> = SKIP_ROLES.iter().copied().collect();

    // Index AX nodes by nodeId
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, node) in ax_nodes.iter().enumerate() {
        if !node.node_id.is_empty() {
            id_to_idx.insert(node.node_id.clone(), i);
        }
    }

    // Build child map from childIds
    let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, node) in ax_nodes.iter().enumerate() {
        let mut children = Vec::new();
        for cid in &node.child_ids {
            if let Some(&child_idx) = id_to_idx.get(cid) {
                children.push(child_idx);
            }
        }
        if !children.is_empty() {
            children_map.insert(i, children);
        }
    }

    // Find root nodes (nodes that are not children of any other node)
    let mut is_child = vec![false; ax_nodes.len()];
    for children in children_map.values() {
        for &c in children {
            is_child[c] = true;
        }
    }
    let root_indices: Vec<usize> = (0..ax_nodes.len()).filter(|&i| !is_child[i]).collect();

    // If scoping by CSS selector, collect allowed backend node IDs
    let scope_set: Option<HashSet<i64>> = scope_backend_id.map(|root_id| {
        let mut allowed = HashSet::new();
        allowed.insert(root_id);
        // BFS from the scope root
        let mut queue: Vec<usize> = Vec::new();
        for (i, node) in ax_nodes.iter().enumerate() {
            if node.backend_dom_node_id == Some(root_id) {
                queue.push(i);
            }
        }
        while let Some(idx) = queue.pop() {
            if let Some(children) = children_map.get(&idx) {
                for &child_idx in children {
                    let bid = ax_nodes[child_idx].backend_dom_node_id.unwrap_or(0);
                    if bid > 0 {
                        allowed.insert(bid);
                    }
                    queue.push(child_idx);
                }
            }
        }
        allowed
    });

    // Recursive tree rendering
    let mut result = Vec::new();
    let mut refs = HashMap::new();
    let mut ref_counter = 0usize;

    #[allow(clippy::too_many_arguments)]
    fn render(
        ax_nodes: &[AxNode],
        children_map: &HashMap<usize, Vec<usize>>,
        idx: usize,
        depth: usize,
        filter: SnapshotFilter,
        max_depth: Option<usize>,
        scope_set: &Option<HashSet<i64>>,
        interactive_set: &HashSet<&str>,
        content_set: &HashSet<&str>,
        skip_set: &HashSet<&str>,
        result: &mut Vec<A11yNode>,
        refs: &mut HashMap<String, i64>,
        ref_counter: &mut usize,
    ) {
        let node = &ax_nodes[idx];
        let role = node
            .role
            .as_ref()
            .map(|r| r.as_string())
            .unwrap_or_default();
        let name = node
            .name
            .as_ref()
            .map(|n| n.as_string())
            .unwrap_or_default();

        // Ignored nodes: skip self but render children at same depth
        if node.ignored && role != "RootWebArea" {
            if let Some(children) = children_map.get(&idx) {
                for &child_idx in children {
                    render(
                        ax_nodes,
                        children_map,
                        child_idx,
                        depth,
                        filter,
                        max_depth,
                        scope_set,
                        interactive_set,
                        content_set,
                        skip_set,
                        result,
                        refs,
                        ref_counter,
                    );
                }
            }
            return;
        }

        // RootWebArea / WebArea: unwrap (render children, skip self)
        if role == "RootWebArea" || role == "WebArea" {
            if let Some(children) = children_map.get(&idx) {
                for &child_idx in children {
                    render(
                        ax_nodes,
                        children_map,
                        child_idx,
                        depth,
                        filter,
                        max_depth,
                        scope_set,
                        interactive_set,
                        content_set,
                        skip_set,
                        result,
                        refs,
                        ref_counter,
                    );
                }
            }
            return;
        }

        // Skip noise roles (but render their children)
        if skip_set.contains(role.as_str()) {
            if let Some(children) = children_map.get(&idx) {
                for &child_idx in children {
                    render(
                        ax_nodes,
                        children_map,
                        child_idx,
                        depth,
                        filter,
                        max_depth,
                        scope_set,
                        interactive_set,
                        content_set,
                        skip_set,
                        result,
                        refs,
                        ref_counter,
                    );
                }
            }
            return;
        }

        // Depth limit
        if let Some(max) = max_depth {
            if depth > max {
                return;
            }
        }

        let backend_node_id = node.backend_dom_node_id.unwrap_or(0);

        // Scope filter: skip self but still render children (ancestors are outside scope)
        if let Some(ref scope) = scope_set {
            if backend_node_id > 0 && !scope.contains(&backend_node_id) {
                if let Some(children) = children_map.get(&idx) {
                    for &child_idx in children {
                        render(
                            ax_nodes,
                            children_map,
                            child_idx,
                            depth,
                            filter,
                            max_depth,
                            scope_set,
                            interactive_set,
                            content_set,
                            skip_set,
                            result,
                            refs,
                            ref_counter,
                        );
                    }
                }
                return;
            }
        }

        // Determine if this node should get a ref
        let is_interactive = interactive_set.contains(role.as_str());
        let is_content = content_set.contains(role.as_str());
        let should_ref = is_interactive || (is_content && !name.is_empty());

        // Interactive filter: skip non-interactive but render children
        if filter == SnapshotFilter::Interactive && !is_interactive {
            if let Some(children) = children_map.get(&idx) {
                for &child_idx in children {
                    render(
                        ax_nodes,
                        children_map,
                        child_idx,
                        depth,
                        filter,
                        max_depth,
                        scope_set,
                        interactive_set,
                        content_set,
                        skip_set,
                        result,
                        refs,
                        ref_counter,
                    );
                }
            }
            return;
        }

        // Extract value
        let value = node
            .value
            .as_ref()
            .map(|v| v.as_string())
            .filter(|s| !s.is_empty());

        // Extract properties
        let mut disabled = false;
        let mut focused = false;
        let mut level = None;
        let mut checked = None;
        let mut expanded = None;
        let mut selected = false;
        let mut required = false;
        let mut url = None;
        for prop in &node.properties {
            if let Some(ref prop_value) = prop.value {
                if let Some(ref val) = prop_value.value {
                    match prop.name.as_str() {
                        "disabled" => disabled = val.as_bool().unwrap_or(false),
                        "focused" => focused = val.as_bool().unwrap_or(false),
                        "level" => level = val.as_i64(),
                        "checked" => {
                            checked = match val {
                                serde_json::Value::String(s) => Some(s.clone()),
                                serde_json::Value::Bool(b) => Some(b.to_string()),
                                _ => None,
                            }
                        }
                        "expanded" => expanded = val.as_bool(),
                        "selected" => selected = val.as_bool().unwrap_or(false),
                        "required" => required = val.as_bool().unwrap_or(false),
                        "url" => {
                            if let Some(s) = val.as_str() {
                                if !s.is_empty() {
                                    url = Some(s.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Assign ref only for actionable nodes
        let ref_id = if should_ref {
            let rid = format!("e{}", *ref_counter);
            *ref_counter += 1;
            if backend_node_id > 0 {
                refs.insert(rid.clone(), backend_node_id);
            }
            Some(rid)
        } else {
            None
        };

        result.push(A11yNode {
            ref_id,
            role,
            name,
            value,
            depth,
            disabled,
            focused,
            level,
            checked,
            expanded,
            selected,
            required,
            url,
            backend_node_id,
        });

        // Render children
        if let Some(children) = children_map.get(&idx) {
            for &child_idx in children {
                render(
                    ax_nodes,
                    children_map,
                    child_idx,
                    depth + 1,
                    filter,
                    max_depth,
                    scope_set,
                    interactive_set,
                    content_set,
                    skip_set,
                    result,
                    refs,
                    ref_counter,
                );
            }
        }
    }

    for &root_idx in &root_indices {
        render(
            ax_nodes,
            &children_map,
            root_idx,
            0,
            filter,
            max_depth,
            &scope_set,
            &interactive_set,
            &content_set,
            &skip_set,
            &mut result,
            &mut refs,
            &mut ref_counter,
        );
    }

    let cache = RefCache {
        refs,
        nodes: result.clone(),
        next_ref: ref_counter,
    };
    Ok((result, cache))
}

/// Remove leaf nodes that are structural with no name, no ref, no value.
/// These are empty `<div>`/`<span>` wrappers that add no information.
#[allow(dead_code)]
pub fn remove_empty_leaves(nodes: &[A11yNode]) -> Vec<A11yNode> {
    let structural_set: HashSet<&str> = STRUCTURAL_ROLES.iter().copied().collect();
    let mut has_child = vec![false; nodes.len()];

    // Mark which nodes have children (next node with deeper depth)
    for i in 0..nodes.len() {
        if i + 1 < nodes.len() && nodes[i + 1].depth > nodes[i].depth {
            has_child[i] = true;
        }
    }

    nodes
        .iter()
        .enumerate()
        .filter(|(i, node)| {
            // Keep non-structural nodes
            if !structural_set.contains(node.role.as_str()) {
                return true;
            }
            // Keep if it has a ref, name, or value
            if node.ref_id.is_some() || !node.name.is_empty() || node.value.is_some() {
                return true;
            }
            // Keep if it has children
            has_child[*i]
        })
        .map(|(_, node)| node.clone())
        .collect()
}

/// Format nodes as indented tree (agent-browser style)
///
/// Format: `- role "name" [disabled, ref=eN]: value`
/// Attributes are combined in a single `[...]` block. Value uses `: value` suffix.
#[allow(dead_code)]
pub fn format_compact(nodes: &[A11yNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        for _ in 0..node.depth {
            out.push_str("  ");
        }
        out.push_str("- ");
        out.push_str(&node.role);
        if !node.name.is_empty() {
            out.push_str(" \"");
            out.push_str(&node.name);
            out.push('"');
        }

        // Collect attributes into a single [...] block
        let mut attrs = Vec::new();
        if let Some(ref checked) = node.checked {
            attrs.push(format!("checked={}", checked));
        }
        if let Some(expanded) = node.expanded {
            attrs.push(format!("expanded={}", expanded));
        }
        if node.selected {
            attrs.push("selected".to_string());
        }
        if node.disabled {
            attrs.push("disabled".to_string());
        }
        if node.required {
            attrs.push("required".to_string());
        }
        if node.focused {
            attrs.push("focused".to_string());
        }
        if let Some(ref rid) = node.ref_id {
            attrs.push(format!("ref={}", rid));
        }
        if !attrs.is_empty() {
            out.push_str(" [");
            out.push_str(&attrs.join(", "));
            out.push(']');
        }

        // Value shown with ": value" suffix (if different from name)
        if let Some(ref val) = node.value {
            if !val.is_empty() && val != &node.name {
                out.push_str(": ");
                out.push_str(val);
            }
        }
        out.push('\n');

        // Metadata: /url: for links
        if let Some(ref url) = node.url {
            for _ in 0..=node.depth {
                out.push_str("  ");
            }
            out.push_str("- /url: ");
            out.push_str(url);
            out.push('\n');
        }
    }
    out
}

/// Compute diff between two snapshots
/// Returns (added, changed, removed)
#[allow(dead_code)]
pub fn diff_snapshots(
    prev: &[A11yNode],
    curr: &[A11yNode],
) -> (Vec<A11yNode>, Vec<A11yNode>, Vec<A11yNode>) {
    fn node_key(n: &A11yNode) -> String {
        format!("{}:{}:{}", n.role, n.name, n.backend_node_id)
    }

    let prev_map: HashMap<String, &A11yNode> = prev.iter().map(|n| (node_key(n), n)).collect();
    let curr_map: HashMap<String, &A11yNode> = curr.iter().map(|n| (node_key(n), n)).collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    // Find added and changed
    for (key, node) in &curr_map {
        match prev_map.get(key) {
            None => added.push((*node).clone()),
            Some(prev_node) => {
                if node.value != prev_node.value
                    || node.focused != prev_node.focused
                    || node.disabled != prev_node.disabled
                {
                    changed.push((*node).clone());
                }
            }
        }
    }

    // Find removed
    for (key, node) in &prev_map {
        if !curr_map.contains_key(key) {
            removed.push((*node).clone());
        }
    }

    (added, changed, removed)
}

/// Estimate token count for output
#[allow(dead_code)]
pub fn estimate_tokens(content: &str, format: SnapshotFormat) -> usize {
    let len = content.len();
    match format {
        SnapshotFormat::Compact => len / 4,
        SnapshotFormat::Json => len / 3,
    }
}

/// Estimate the token cost of a single node in a given format
#[allow(dead_code)]
fn estimate_node_tokens(node: &A11yNode, format: SnapshotFormat) -> usize {
    let ref_len = node.ref_id.as_ref().map(|r| r.len()).unwrap_or(0);
    let role_len = node.role.len();
    let name_len = node.name.len();
    let value_len = node.value.as_ref().map(|v| v.len()).unwrap_or(0);
    let base = ref_len + role_len + name_len + value_len;

    match format {
        SnapshotFormat::Compact => (base + 12 + node.depth * 2) / 4,
        SnapshotFormat::Json => (base + 60) / 3,
    }
}

/// Truncate nodes to fit within a token budget.
/// Returns the truncated node list and whether truncation occurred.
#[allow(dead_code)]
pub fn truncate_to_tokens(
    nodes: &[A11yNode],
    max_tokens: usize,
    format: SnapshotFormat,
) -> (Vec<A11yNode>, bool) {
    let mut total = 0usize;
    let mut result = Vec::new();

    for node in nodes {
        let cost = estimate_node_tokens(node, format);
        if total + cost > max_tokens {
            return (result, true);
        }
        total += cost;
        result.push(node.clone());
    }

    (result, false)
}

/// Compact tree: keep only nodes with [ref=] or values, plus their ancestors.
/// Matches agent-browser's compact_tree behavior.
#[allow(dead_code)]
pub fn compact_tree_nodes(nodes: &[A11yNode]) -> Vec<A11yNode> {
    let mut keep = vec![false; nodes.len()];

    for i in 0..nodes.len() {
        if nodes[i].ref_id.is_some() || nodes[i].value.is_some() {
            keep[i] = true;
            // Mark true ancestor chain: walk backwards, tracking the next
            // ancestor depth we need to find (parent = depth - 1)
            let mut need_depth = nodes[i].depth;
            for j in (0..i).rev() {
                if need_depth == 0 {
                    break;
                }
                if nodes[j].depth == need_depth - 1 {
                    keep[j] = true;
                    need_depth = nodes[j].depth;
                }
            }
        }
    }

    nodes
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, node)| node.clone())
        .collect()
}

/// A cursor-interactive element detected via DOM inspection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CursorElement {
    pub selector: String,
    pub text: String,
    pub tag_name: String,
    pub has_onclick: bool,
    pub has_cursor_pointer: bool,
    pub has_tabindex: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(ref_id: Option<&str>, role: &str, name: &str, depth: usize) -> A11yNode {
        A11yNode {
            ref_id: ref_id.map(|s| s.to_string()),
            role: role.to_string(),
            name: name.to_string(),
            value: None,
            depth,
            disabled: false,
            focused: false,
            level: None,
            checked: None,
            expanded: None,
            selected: false,
            required: false,
            url: None,
            backend_node_id: 0,
        }
    }

    #[test]
    fn format_compact_basic_tree() {
        let nodes = vec![
            make_node(None, "heading", "Title", 0),
            make_node(None, "navigation", "Primary", 1),
            make_node(Some("e0"), "link", "Home", 2),
            make_node(Some("e1"), "link", "About", 2),
        ];
        let output = format_compact(&nodes);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "- heading \"Title\"");
        assert_eq!(lines[1], "  - navigation \"Primary\"");
        assert_eq!(lines[2], "    - link \"Home\" [ref=e0]");
        assert_eq!(lines[3], "    - link \"About\" [ref=e1]");
    }

    #[test]
    fn format_compact_with_value() {
        let mut node = make_node(Some("e0"), "textbox", "Search", 0);
        node.value = Some("hello".to_string());
        let output = format_compact(&[node]);
        assert_eq!(output, "- textbox \"Search\" [ref=e0]: hello\n");
    }

    #[test]
    fn format_compact_value_same_as_name_hidden() {
        let mut node = make_node(Some("e0"), "textbox", "hello", 0);
        node.value = Some("hello".to_string());
        let output = format_compact(&[node]);
        // Value same as name → not shown
        assert_eq!(output, "- textbox \"hello\" [ref=e0]\n");
    }

    #[test]
    fn format_compact_with_all_attrs() {
        let mut node = make_node(Some("e0"), "checkbox", "Accept", 0);
        node.checked = Some("true".to_string());
        node.disabled = true;
        node.required = true;
        let output = format_compact(&[node]);
        assert_eq!(
            output,
            "- checkbox \"Accept\" [checked=true, disabled, required, ref=e0]\n"
        );
    }

    #[test]
    fn format_compact_heading_no_level() {
        let mut node = make_node(None, "heading", "Title", 0);
        node.level = Some(1);
        let output = format_compact(&[node]);
        // level is not shown in output
        assert_eq!(output, "- heading \"Title\"\n");
    }

    #[test]
    fn format_compact_no_name_no_ref() {
        let node = make_node(None, "main", "", 1);
        let output = format_compact(&[node]);
        assert_eq!(output, "  - main\n");
    }

    #[test]
    fn format_compact_empty_nodes() {
        let output = format_compact(&[]);
        assert_eq!(output, "");
    }

    #[test]
    fn format_compact_deep_nesting() {
        let node = make_node(Some("e9"), "button", "OK", 5);
        let output = format_compact(&[node]);
        assert_eq!(output, "          - button \"OK\" [ref=e9]\n");
    }

    #[test]
    fn estimate_tokens_compact() {
        let content = "- heading \"Title\"\n  - link \"Home\" [ref=e0]\n";
        let tokens = estimate_tokens(content, SnapshotFormat::Compact);
        assert_eq!(tokens, content.len() / 4);
    }

    #[test]
    fn estimate_node_tokens_includes_depth() {
        let shallow = make_node(Some("e0"), "button", "OK", 0);
        let deep = make_node(Some("e1"), "button", "OK", 5);
        let t_shallow = estimate_node_tokens(&shallow, SnapshotFormat::Compact);
        let t_deep = estimate_node_tokens(&deep, SnapshotFormat::Compact);
        assert!(t_deep > t_shallow, "deeper node should cost more tokens");
    }

    #[test]
    fn snapshot_format_has_no_text_variant() {
        let compact = SnapshotFormat::Compact;
        let json = SnapshotFormat::Json;
        assert_ne!(compact, json);
    }

    #[test]
    fn truncate_respects_budget() {
        let nodes: Vec<A11yNode> = (0..100)
            .map(|i| make_node(Some(&format!("e{}", i)), "button", "X", 0))
            .collect();
        let (truncated, did_truncate) = truncate_to_tokens(&nodes, 10, SnapshotFormat::Compact);
        assert!(did_truncate);
        assert!(truncated.len() < 100);
        assert!(!truncated.is_empty());
    }

    #[test]
    fn truncate_no_truncation_when_budget_sufficient() {
        let nodes = vec![make_node(Some("e0"), "button", "OK", 0)];
        let (result, did_truncate) = truncate_to_tokens(&nodes, 10000, SnapshotFormat::Compact);
        assert!(!did_truncate);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn diff_snapshots_detects_changes() {
        let prev = vec![
            make_node(Some("e0"), "button", "Submit", 0),
            make_node(Some("e1"), "link", "Home", 0),
        ];
        let mut changed_node = make_node(Some("e0"), "button", "Submit", 0);
        changed_node.disabled = true;
        let curr = vec![changed_node, make_node(Some("e2"), "link", "New", 0)];
        let (added, changed, removed) = diff_snapshots(&prev, &curr);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].name, "New");
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].name, "Submit");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].name, "Home");
    }

    #[test]
    fn compact_tree_keeps_ref_nodes_and_ancestors() {
        let nodes = vec![
            make_node(None, "banner", "", 0), // ancestor of ref'd node → keep
            make_node(None, "navigation", "", 1), // ancestor of ref'd node → keep
            make_node(Some("e0"), "link", "Home", 2), // has ref → keep
            make_node(None, "group", "", 0),  // no ref descendant → remove
            make_node(None, "paragraph", "", 1), // no ref → remove
        ];
        let compacted = compact_tree_nodes(&nodes);
        assert_eq!(compacted.len(), 3);
        assert_eq!(compacted[0].role, "banner");
        assert_eq!(compacted[1].role, "navigation");
        assert_eq!(compacted[2].role, "link");
    }

    #[test]
    fn remove_empty_leaves_keeps_non_structural_nodes() {
        let nodes = vec![
            make_node(None, "button", "Click me", 0),
            make_node(None, "link", "Home", 1),
        ];
        let result = remove_empty_leaves(&nodes);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn remove_empty_leaves_removes_empty_structural_nodes() {
        let nodes = vec![
            // "generic" is a structural role; no name, no ref, no children → should be removed
            make_node(None, "generic", "", 0),
            make_node(None, "button", "Click", 0),
        ];
        let result = remove_empty_leaves(&nodes);
        // "generic" with no name/ref/children → removed; button kept
        assert!(result.iter().any(|n| n.role == "button"));
    }

    #[test]
    fn remove_empty_leaves_keeps_structural_nodes_with_children() {
        let nodes = vec![
            // "generic" has children (next node has deeper depth)
            make_node(None, "generic", "", 0),
            make_node(Some("e0"), "link", "Home", 1),
        ];
        let result = remove_empty_leaves(&nodes);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "generic");
    }

    #[test]
    fn diff_snapshots_detects_added_nodes() {
        let prev = vec![make_node(Some("e0"), "button", "Submit", 0)];
        let curr = vec![
            make_node(Some("e0"), "button", "Submit", 0),
            make_node(Some("e1"), "link", "Home", 0),
        ];
        let (added, changed, removed) = diff_snapshots(&prev, &curr);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].role, "link");
        assert!(changed.is_empty());
        assert!(removed.is_empty());
    }

    #[test]
    fn diff_snapshots_detects_removed_nodes() {
        let prev = vec![
            make_node(Some("e0"), "button", "Submit", 0),
            make_node(Some("e1"), "link", "Home", 0),
        ];
        let curr = vec![make_node(Some("e0"), "button", "Submit", 0)];
        let (added, changed, removed) = diff_snapshots(&prev, &curr);
        assert!(added.is_empty());
        assert!(changed.is_empty());
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].role, "link");
    }

    #[test]
    fn diff_snapshots_empty_input_yields_empty_output() {
        let (added, changed, removed) = diff_snapshots(&[], &[]);
        assert!(added.is_empty());
        assert!(changed.is_empty());
        assert!(removed.is_empty());
    }

    #[test]
    fn parse_ax_tree_basic_flat_tree() {
        let raw = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "backendDOMNodeId": 100,
                    "ignored": false,
                    "role": { "type": "role", "value": "button" },
                    "name": { "type": "computedString", "value": "Submit" },
                    "childIds": [],
                    "properties": []
                }
            ]
        });
        let (nodes, cache) = parse_ax_tree(raw, SnapshotFilter::Interactive, None, None).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].role, "button");
        assert_eq!(nodes[0].name, "Submit");
        // e0 should be in the RefCache's refs map
        assert!(cache.refs.contains_key("e0"));
    }

    #[test]
    fn parse_ax_tree_ignored_nodes_skipped() {
        let raw = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "backendDOMNodeId": 101,
                    "ignored": true,
                    "role": { "type": "role", "value": "none" },
                    "name": { "type": "computedString", "value": "" },
                    "childIds": [],
                    "properties": []
                }
            ]
        });
        let (nodes, _cache) = parse_ax_tree(raw, SnapshotFilter::Interactive, None, None).unwrap();
        // Ignored nodes should be filtered out
        assert!(nodes.is_empty());
    }

    #[test]
    fn estimate_tokens_compact_vs_json() {
        let content = "a".repeat(400);
        let compact_tokens = estimate_tokens(&content, SnapshotFormat::Compact);
        let json_tokens = estimate_tokens(&content, SnapshotFormat::Json);
        // Compact divides by 4, JSON divides by 3 → compact should have fewer tokens
        assert_eq!(compact_tokens, 100);
        assert_eq!(json_tokens, 133); // floor(400/3)
    }

    #[test]
    fn truncate_to_tokens_allows_all_nodes_when_budget_large() {
        let nodes = vec![
            make_node(Some("e0"), "button", "A", 0),
            make_node(Some("e1"), "link", "B", 0),
        ];
        let (result, truncated) = truncate_to_tokens(&nodes, 10_000, SnapshotFormat::Compact);
        assert_eq!(result.len(), 2);
        assert!(!truncated);
    }

    #[test]
    fn compact_tree_empty_input_returns_empty() {
        let result = compact_tree_nodes(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn format_compact_with_url_field() {
        let mut node = make_node(Some("e0"), "link", "Homepage", 0);
        node.url = Some("https://example.com".to_string());
        let output = format_compact(&[node]);
        assert!(output.contains("link \"Homepage\" [ref=e0]"));
        assert!(output.contains("/url: https://example.com"));
    }

    #[test]
    fn format_compact_with_expanded_and_selected() {
        let mut node = make_node(Some("e0"), "treeitem", "Node", 0);
        node.expanded = Some(true);
        node.selected = true;
        let output = format_compact(&[node]);
        assert!(output.contains("expanded=true"));
        assert!(output.contains("selected"));
    }

    #[test]
    fn format_compact_with_focused_attr() {
        let mut node = make_node(Some("e0"), "textbox", "Search", 0);
        node.focused = true;
        let output = format_compact(&[node]);
        assert_eq!(output, "- textbox \"Search\" [focused, ref=e0]\n");
    }

    #[test]
    fn ax_value_as_string_handles_various_types() {
        // String value
        let av = AxValue {
            value_type: Some("string".to_string()),
            value: Some(serde_json::Value::String("hello".to_string())),
        };
        assert_eq!(av.as_string(), "hello");

        // Integer value
        let av = AxValue {
            value_type: Some("integer".to_string()),
            value: Some(serde_json::json!(42i64)),
        };
        assert_eq!(av.as_string(), "42");

        // Boolean value
        let av = AxValue {
            value_type: Some("boolean".to_string()),
            value: Some(serde_json::json!(true)),
        };
        assert_eq!(av.as_string(), "true");

        // None value
        let av = AxValue {
            value_type: None,
            value: None,
        };
        assert_eq!(av.as_string(), "");
    }

    #[test]
    fn ax_value_as_string_formats_float_variants() {
        let integer_like = AxValue {
            value_type: Some("number".to_string()),
            value: Some(serde_json::json!(42.0)),
        };
        assert_eq!(integer_like.as_string(), "42");

        let decimal = AxValue {
            value_type: Some("number".to_string()),
            value: Some(serde_json::json!(3.25)),
        };
        assert_eq!(decimal.as_string(), "3.25");
    }

    #[test]
    fn parse_ax_tree_promotes_wrappers_and_extracts_properties() {
        let raw = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "backendDOMNodeId": 1,
                    "ignored": false,
                    "role": { "type": "role", "value": "RootWebArea" },
                    "name": { "type": "computedString", "value": "Root" },
                    "childIds": ["2", "3", "9", "14", "missing"],
                    "properties": []
                },
                {
                    "nodeId": "2",
                    "backendDOMNodeId": 2,
                    "ignored": true,
                    "role": { "type": "role", "value": "generic" },
                    "name": { "type": "computedString", "value": "" },
                    "childIds": ["4"],
                    "properties": []
                },
                {
                    "nodeId": "3",
                    "backendDOMNodeId": 3,
                    "ignored": false,
                    "role": { "type": "role", "value": "WebArea" },
                    "name": { "type": "computedString", "value": "Inner" },
                    "childIds": ["5"],
                    "properties": []
                },
                {
                    "nodeId": "4",
                    "backendDOMNodeId": 4,
                    "ignored": false,
                    "role": { "type": "role", "value": "button" },
                    "name": { "type": "computedString", "value": "Submit" },
                    "value": { "type": "string", "value": "submit" },
                    "childIds": [],
                    "properties": [
                        { "name": "disabled", "value": { "type": "boolean", "value": true } },
                        { "name": "focused", "value": { "type": "boolean", "value": true } },
                        { "name": "required", "value": { "type": "boolean", "value": true } },
                        { "name": "checked", "value": { "type": "boolean", "value": true } },
                        { "name": "expanded", "value": { "type": "boolean", "value": false } }
                    ]
                },
                {
                    "nodeId": "5",
                    "ignored": false,
                    "role": { "type": "role", "value": "heading" },
                    "name": { "type": "computedString", "value": "Title" },
                    "childIds": ["6"],
                    "properties": [
                        { "name": "level", "value": { "type": "integer", "value": 2 } }
                    ]
                },
                {
                    "nodeId": "6",
                    "backendDOMNodeId": 6,
                    "ignored": false,
                    "role": { "type": "role", "value": "link" },
                    "name": { "type": "computedString", "value": "Nested link" },
                    "childIds": [],
                    "properties": [
                        { "name": "url", "value": { "type": "string", "value": "https://example.com/title" } }
                    ]
                },
                {
                    "nodeId": "9",
                    "backendDOMNodeId": 9,
                    "ignored": false,
                    "role": { "type": "role", "value": "StaticText" },
                    "name": { "type": "computedString", "value": "ignored" },
                    "childIds": ["10", "11"],
                    "properties": []
                },
                {
                    "nodeId": "10",
                    "backendDOMNodeId": 10,
                    "ignored": false,
                    "role": { "type": "role", "value": "link" },
                    "name": { "type": "computedString", "value": "Docs" },
                    "childIds": [],
                    "properties": [
                        { "name": "selected", "value": { "type": "boolean", "value": true } },
                        { "name": "url", "value": { "type": "string", "value": "https://example.com/docs" } }
                    ]
                },
                {
                    "nodeId": "11",
                    "backendDOMNodeId": 11,
                    "ignored": false,
                    "role": { "type": "role", "value": "checkbox" },
                    "name": { "type": "computedString", "value": "Choice" },
                    "childIds": [],
                    "properties": [
                        { "name": "checked", "value": { "type": "string", "value": "mixed" } },
                        { "name": "url", "value": { "type": "string", "value": "" } }
                    ]
                },
                {
                    "nodeId": "14",
                    "backendDOMNodeId": 14,
                    "ignored": false,
                    "role": { "type": "role", "value": "generic" },
                    "name": { "type": "computedString", "value": "" },
                    "childIds": ["15"],
                    "properties": []
                },
                {
                    "nodeId": "15",
                    "backendDOMNodeId": 15,
                    "ignored": false,
                    "role": { "type": "role", "value": "link" },
                    "name": { "type": "computedString", "value": "Scoped only" },
                    "childIds": ["16"],
                    "properties": []
                },
                {
                    "nodeId": "16",
                    "backendDOMNodeId": 16,
                    "ignored": false,
                    "role": { "type": "role", "value": "button" },
                    "name": { "type": "computedString", "value": "Too deep" },
                    "childIds": [],
                    "properties": []
                }
            ]
        });

        let (nodes, cache) = parse_ax_tree(raw, SnapshotFilter::All, Some(1), None).unwrap();
        let button = nodes.iter().find(|node| node.name == "Submit").unwrap();
        assert_eq!(button.depth, 0);
        assert_eq!(button.value.as_deref(), Some("submit"));
        assert!(button.disabled);
        assert!(button.focused);
        assert!(button.required);
        assert_eq!(button.checked.as_deref(), Some("true"));
        assert_eq!(button.expanded, Some(false));

        let heading = nodes.iter().find(|node| node.name == "Title").unwrap();
        assert_eq!(heading.level, Some(2));
        assert!(heading.ref_id.is_some());
        assert!(!cache.refs.contains_key(heading.ref_id.as_ref().unwrap()));

        let nested_link = nodes
            .iter()
            .find(|node| node.name == "Nested link")
            .unwrap();
        assert_eq!(nested_link.depth, 1);
        assert_eq!(
            nested_link.url.as_deref(),
            Some("https://example.com/title")
        );

        let docs_link = nodes.iter().find(|node| node.name == "Docs").unwrap();
        assert!(docs_link.selected);
        assert_eq!(docs_link.url.as_deref(), Some("https://example.com/docs"));

        let checkbox = nodes.iter().find(|node| node.name == "Choice").unwrap();
        assert_eq!(checkbox.checked.as_deref(), Some("mixed"));
        assert_eq!(checkbox.url, None);

        assert!(nodes.iter().all(|node| node.name != "Too deep"));
        assert!(cache.refs.contains_key("e0"));
        assert!(cache.next_ref >= cache.refs.len());
    }

    #[test]
    fn parse_ax_tree_scope_filter_keeps_scoped_descendant_only() {
        let raw = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "backendDOMNodeId": 1,
                    "ignored": false,
                    "role": { "type": "role", "value": "RootWebArea" },
                    "name": { "type": "computedString", "value": "Root" },
                    "childIds": ["2", "14"],
                    "properties": []
                },
                {
                    "nodeId": "2",
                    "backendDOMNodeId": 2,
                    "ignored": false,
                    "role": { "type": "role", "value": "generic" },
                    "name": { "type": "computedString", "value": "" },
                    "childIds": ["3"],
                    "properties": []
                },
                {
                    "nodeId": "3",
                    "backendDOMNodeId": 3,
                    "ignored": false,
                    "role": { "type": "role", "value": "button" },
                    "name": { "type": "computedString", "value": "Outside scope" },
                    "childIds": [],
                    "properties": []
                },
                {
                    "nodeId": "14",
                    "backendDOMNodeId": 14,
                    "ignored": false,
                    "role": { "type": "role", "value": "generic" },
                    "name": { "type": "computedString", "value": "" },
                    "childIds": ["15"],
                    "properties": []
                },
                {
                    "nodeId": "15",
                    "backendDOMNodeId": 15,
                    "ignored": false,
                    "role": { "type": "role", "value": "link" },
                    "name": { "type": "computedString", "value": "Scoped only" },
                    "childIds": ["16"],
                    "properties": []
                },
                {
                    "nodeId": "16",
                    "backendDOMNodeId": 16,
                    "ignored": false,
                    "role": { "type": "role", "value": "button" },
                    "name": { "type": "computedString", "value": "Too deep" },
                    "childIds": [],
                    "properties": []
                }
            ]
        });

        let (nodes, cache) =
            parse_ax_tree(raw, SnapshotFilter::Interactive, Some(0), Some(15)).unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "Scoped only");
        assert_eq!(nodes[0].depth, 0);
        assert!(cache.refs.values().all(|backend_id| *backend_id == 15));
    }

    #[test]
    fn remove_empty_leaves_keeps_structural_nodes_with_value() {
        let mut generic = make_node(None, "generic", "", 0);
        generic.value = Some("summary".to_string());
        let result = remove_empty_leaves(&[generic]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value.as_deref(), Some("summary"));
    }

    #[test]
    fn estimate_node_tokens_json_and_compact_tree_root_break() {
        let mut json_node = make_node(Some("e1"), "textbox", "Search", 3);
        json_node.value = Some("query".to_string());
        let json_cost = estimate_node_tokens(&json_node, SnapshotFormat::Json);
        let compact_cost = estimate_node_tokens(&json_node, SnapshotFormat::Compact);
        assert!(json_cost > compact_cost);

        let mut second_root = make_node(None, "main", "", 0);
        second_root.value = Some("kept".to_string());
        let compacted = compact_tree_nodes(&[make_node(None, "banner", "", 0), second_root]);
        assert_eq!(compacted.len(), 1);
        assert_eq!(compacted[0].role, "main");
    }
}
