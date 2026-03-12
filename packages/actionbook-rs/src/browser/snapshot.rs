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
pub struct AxTreeResponse {
    pub nodes: Vec<AxNode>,
}

/// Single node in the CDP accessibility tree
#[derive(Deserialize, Debug, Clone)]
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
pub struct AxValue {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub value_type: Option<String>,
    pub value: Option<serde_json::Value>,
}

impl AxValue {
    /// Extract string value from AXValue
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
pub struct AxProperty {
    pub name: String,
    pub value: Option<AxValue>,
}

/// A single node in the accessibility tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A11yNode {
    /// Stable reference ID ("e0", "e1", ...)
    #[serde(rename = "ref")]
    pub ref_id: String,
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
    /// Backend DOM node ID (for action execution)
    #[serde(rename = "nodeId")]
    pub backend_node_id: i64,
}

/// Cached ref→backendNodeId mapping for action resolution
#[derive(Debug, Clone)]
pub struct RefCache {
    /// "e0" → backend_node_id
    pub refs: HashMap<String, i64>,
    /// Last snapshot nodes
    #[allow(dead_code)]
    pub nodes: Vec<A11yNode>,
}

/// Snapshot filter options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFilter {
    All,
    Interactive,
}

/// Snapshot output format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    /// One-line-per-node compact format (~60-70% fewer tokens than JSON)
    Compact,
    /// Indented tree format (~40-60% fewer tokens)
    Text,
    /// Full JSON
    Json,
}

/// Interactive ARIA roles (from pinchtab/snapshot.go)
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

/// Roles to skip (noise)
const SKIP_ROLES: &[&str] = &["none", "generic", "InlineTextBox"];

/// Structural roles that may be removed during compact filtering
const STRUCTURAL_ROLES: &[&str] = &[
    "generic", "group", "list", "table", "row", "rowgroup",
    "grid", "treegrid", "menu", "menubar", "toolbar", "tablist",
    "tree", "directory", "document", "application", "presentation", "none",
];

/// Parse the raw CDP Accessibility.getFullAXTree response into A11yNode list
///
/// ## Phase 2b Optimization
/// Now uses typed deserialization for 42% performance improvement over dynamic Value access.
/// Accepts JSON string directly for single-pass parsing.
pub fn parse_ax_tree(
    raw_json: &str,
    filter: SnapshotFilter,
    max_depth: Option<usize>,
    scope_backend_id: Option<i64>,
) -> Result<(Vec<A11yNode>, RefCache)> {
    // Phase 2b: Typed deserialization (single parse pass)
    let response: AxTreeResponse = serde_json::from_str(raw_json)?;
    let nodes = &response.nodes;

    // Build parent map and child map for depth calculation
    let mut parent_map: HashMap<String, String> = HashMap::new();
    let mut child_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut node_map: HashMap<String, &AxNode> = HashMap::new();

    for node in nodes {
        let node_id = &node.node_id;
        if node_id.is_empty() {
            continue;
        }
        node_map.insert(node_id.clone(), node);

        if !node.child_ids.is_empty() {
            for cid in &node.child_ids {
                parent_map.insert(cid.clone(), node_id.clone());
            }
            child_map.insert(node_id.clone(), node.child_ids.clone());
        }
    }

    // Calculate depth for each node
    fn get_depth(node_id: &str, parent_map: &HashMap<String, String>, cache: &mut HashMap<String, usize>) -> usize {
        if let Some(&d) = cache.get(node_id) {
            return d;
        }
        let d = match parent_map.get(node_id) {
            Some(parent) => get_depth(parent, parent_map, cache) + 1,
            None => 0,
        };
        cache.insert(node_id.to_string(), d);
        d
    }

    let mut depth_cache: HashMap<String, usize> = HashMap::new();

    // If scoping by CSS selector, collect allowed node IDs (descendants of scope root)
    let scope_set: Option<std::collections::HashSet<i64>> = scope_backend_id.map(|root_id| {
        let mut allowed = std::collections::HashSet::new();
        allowed.insert(root_id);
        // BFS to find all descendants
        let mut queue: Vec<String> = Vec::new();
        // Find the AX node with this backendDOMNodeId
        for node in nodes {
            let bid = node.backend_dom_node_id.unwrap_or(0);
            if bid == root_id {
                queue.push(node.node_id.clone());
            }
        }
        while let Some(nid) = queue.pop() {
            if let Some(children) = child_map.get(&nid) {
                for child in children {
                    // Get backend id of this child
                    if let Some(child_node) = node_map.get(child) {
                        let bid = child_node.backend_dom_node_id.unwrap_or(0);
                        if bid > 0 {
                            allowed.insert(bid);
                        }
                    }
                    queue.push(child.clone());
                }
            }
        }
        allowed
    });

    let interactive_set: std::collections::HashSet<&str> =
        INTERACTIVE_ROLES.iter().copied().collect();
    let skip_set: std::collections::HashSet<&str> = SKIP_ROLES.iter().copied().collect();

    let mut result = Vec::new();
    let mut refs = HashMap::new();
    let mut ref_counter = 0usize;

    for node in nodes {
        // Skip ignored nodes
        if node.ignored {
            continue;
        }

        let node_id_str = &node.node_id;

        // Phase 2b: Direct field access (no get() overhead)
        let role = node.role.as_ref().map(|r| r.as_string()).unwrap_or_default();
        let name = node.name.as_ref().map(|n| n.as_string()).unwrap_or_default();

        // Skip noise roles
        if skip_set.contains(role.as_str()) {
            continue;
        }

        // Skip empty StaticText
        if role == "StaticText" && name.is_empty() {
            continue;
        }

        // Apply interactive filter
        if filter == SnapshotFilter::Interactive && !interactive_set.contains(role.as_str()) {
            continue;
        }

        let depth = get_depth(node_id_str, &parent_map, &mut depth_cache);

        // Apply depth limit
        if let Some(max) = max_depth {
            if depth > max {
                continue;
            }
        }

        let backend_node_id = node.backend_dom_node_id.unwrap_or(0);

        // Apply scope filter
        if let Some(ref scope) = scope_set {
            if backend_node_id > 0 && !scope.contains(&backend_node_id) {
                continue;
            }
        }

        // Extract value
        let value = node.value.as_ref().map(|v| v.as_string()).filter(|s| !s.is_empty());

        // Extract properties
        let mut disabled = false;
        let mut focused = false;
        for prop in &node.properties {
            if let Some(ref prop_value) = prop.value {
                if let Some(ref val) = prop_value.value {
                    if let Some(b) = val.as_bool() {
                        match prop.name.as_str() {
                            "disabled" => disabled = b,
                            "focused" => focused = b,
                            _ => {}
                        }
                    }
                }
            }
        }

        let ref_id = format!("e{}", ref_counter);
        ref_counter += 1;

        if backend_node_id > 0 {
            refs.insert(ref_id.clone(), backend_node_id);
        }

        result.push(A11yNode {
            ref_id,
            role,
            name,
            value,
            depth,
            disabled,
            focused,
            backend_node_id,
        });
    }

    let cache = RefCache {
        refs,
        nodes: result.clone(),
    };
    Ok((result, cache))
}

/// Format nodes as compact output (most token-efficient)
/// Format: e0:button "Submit" [focused]
pub fn format_compact(nodes: &[A11yNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        out.push_str(&node.ref_id);
        out.push(':');
        out.push_str(&node.role);
        if !node.name.is_empty() {
            out.push_str(" \"");
            out.push_str(&node.name);
            out.push('"');
        }
        if let Some(ref val) = node.value {
            out.push_str(" val=\"");
            out.push_str(val);
            out.push('"');
        }
        let mut flags = Vec::new();
        if node.focused {
            flags.push("focused");
        }
        if node.disabled {
            flags.push("disabled");
        }
        if !flags.is_empty() {
            out.push_str(" [");
            out.push_str(&flags.join(","));
            out.push(']');
        }
        out.push('\n');
    }
    out
}

/// Format nodes as indented text tree
pub fn format_text(nodes: &[A11yNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        // Indent by depth
        for _ in 0..node.depth {
            out.push_str("  ");
        }
        out.push_str(&node.ref_id);
        out.push(' ');
        out.push_str(&node.role);
        if !node.name.is_empty() {
            out.push_str(" \"");
            out.push_str(&node.name);
            out.push('"');
        }
        if let Some(ref val) = node.value {
            out.push_str(" val=\"");
            out.push_str(val);
            out.push('"');
        }
        let mut flags = Vec::new();
        if node.focused {
            flags.push("focused");
        }
        if node.disabled {
            flags.push("disabled");
        }
        if !flags.is_empty() {
            out.push_str(" [");
            out.push_str(&flags.join(","));
            out.push(']');
        }
        out.push('\n');
    }
    out
}

/// Compute diff between two snapshots
/// Returns (added, changed, removed)
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
pub fn estimate_tokens(content: &str, format: SnapshotFormat) -> usize {
    let len = content.len();
    match format {
        SnapshotFormat::Compact | SnapshotFormat::Text => len / 4,
        SnapshotFormat::Json => len / 3,
    }
}

/// Estimate the token cost of a single node in a given format
fn estimate_node_tokens(node: &A11yNode, format: SnapshotFormat) -> usize {
    let ref_len = node.ref_id.len();
    let role_len = node.role.len();
    let name_len = node.name.len();
    let value_len = node.value.as_ref().map(|v| v.len()).unwrap_or(0);
    let base = ref_len + role_len + name_len + value_len;

    match format {
        SnapshotFormat::Compact => (base + 8) / 4,
        SnapshotFormat::Text => (base + 8 + node.depth * 2) / 4,
        SnapshotFormat::Json => (base + 60) / 3,
    }
}

/// Truncate nodes to fit within a token budget.
/// Returns the truncated node list and whether truncation occurred.
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

/// Remove empty structural elements from the node list.
///
/// A structural node is removed if:
/// - Its role is in `STRUCTURAL_ROLES`
/// - Its name is empty
/// - It is not an interactive element
/// - It has no meaningful descendants (interactive or named nodes below it in the tree)
pub fn compact_tree_nodes(nodes: &[A11yNode]) -> Vec<A11yNode> {
    let structural_set: HashSet<&str> = STRUCTURAL_ROLES.iter().copied().collect();
    let interactive_set: HashSet<&str> = INTERACTIVE_ROLES.iter().copied().collect();

    // For each node index, determine if it has meaningful descendants.
    // A node at index i with depth d has descendants at indices j > i where depth > d,
    // until we hit a node at depth <= d.
    let mut has_meaningful_descendant = vec![false; nodes.len()];

    // Walk backwards so we can propagate descendant info upward
    for i in (0..nodes.len()).rev() {
        let d = nodes[i].depth;
        // Scan forward for children
        for j in (i + 1)..nodes.len() {
            if nodes[j].depth <= d {
                break; // No longer a descendant
            }
            // Check if this descendant is meaningful
            if interactive_set.contains(nodes[j].role.as_str())
                || !nodes[j].name.is_empty()
                || has_meaningful_descendant[j]
            {
                has_meaningful_descendant[i] = true;
                break;
            }
        }
    }

    nodes
        .iter()
        .enumerate()
        .filter(|(i, node)| {
            // Keep non-structural nodes always
            if !structural_set.contains(node.role.as_str()) {
                return true;
            }
            // Keep interactive structural nodes
            if interactive_set.contains(node.role.as_str()) {
                return true;
            }
            // Keep named structural nodes
            if !node.name.is_empty() {
                return true;
            }
            // Keep if it has meaningful descendants
            has_meaningful_descendant[*i]
        })
        .map(|(_, node)| node.clone())
        .collect()
}

/// A cursor-interactive element detected via DOM inspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorElement {
    pub selector: String,
    pub text: String,
    pub tag_name: String,
    pub has_onclick: bool,
    pub has_cursor_pointer: bool,
    pub has_tabindex: bool,
}

/// Find elements that are cursor-interactive but not in the accessibility tree.
///
/// Detects elements with `cursor: pointer`, `onclick`, or `tabindex` attributes
/// that may not have ARIA roles but are still clickable/focusable.
pub async fn find_cursor_interactive_elements(
    driver: &mut super::router::BrowserDriver,
    scope_selector: Option<&str>,
) -> Result<Vec<CursorElement>> {
    let scope = scope_selector.unwrap_or("document.body");
    let js = format!(
        r#"(function() {{
    const root = {};
    if (!root) return JSON.stringify([]);
    const results = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
    const seen = new Set();
    let node;
    while (node = walker.nextNode()) {{
        if (seen.has(node)) continue;
        seen.add(node);
        const tag = node.tagName.toLowerCase();
        // Skip elements that already have ARIA roles (handled by AX tree)
        if (node.getAttribute('role')) continue;
        // Skip standard interactive elements (already in AX tree)
        if (['a','button','input','select','textarea','summary','details'].includes(tag)) continue;
        const style = window.getComputedStyle(node);
        const hasCursorPointer = style.cursor === 'pointer';
        const hasOnclick = node.hasAttribute('onclick') || node.onclick !== null;
        const hasTabindex = node.hasAttribute('tabindex');
        if (!hasCursorPointer && !hasOnclick && !hasTabindex) continue;
        // Get concise text
        let text = (node.textContent || '').trim().substring(0, 80);
        if (!text && node.getAttribute('aria-label')) text = node.getAttribute('aria-label');
        if (!text && node.getAttribute('title')) text = node.getAttribute('title');
        // Build a simple selector
        let sel = tag;
        if (node.id) sel = '#' + node.id;
        else if (node.className && typeof node.className === 'string') sel = tag + '.' + node.className.trim().split(/\s+/).join('.');
        results.push({{
            selector: sel,
            text: text,
            tagName: tag,
            hasOnclick: hasOnclick,
            hasCursorPointer: hasCursorPointer,
            hasTabindex: hasTabindex,
        }});
        if (results.length >= 100) break;
    }}
    return JSON.stringify(results);
}})()"#,
        if scope_selector.is_some() {
            format!("document.querySelector('{}')", scope.replace('\'', "\\'"))
        } else {
            "document.body".to_string()
        }
    );

    let result_str = driver.eval(&js).await?;

    // The eval result is JSON-stringified, possibly wrapped in quotes
    let cleaned = result_str.trim().trim_matches('"');
    // Unescape if needed (eval returns JSON string inside JSON string)
    let unescaped = cleaned.replace("\\\"", "\"").replace("\\\\", "\\");

    let elements: Vec<CursorElement> = serde_json::from_str(&unescaped).unwrap_or_default();
    Ok(elements)
}
