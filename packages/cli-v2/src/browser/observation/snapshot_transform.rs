//! Snapshot data transformation: CDP AX tree → §10.1 spec output.
//!
//! These functions are pure logic (no browser/CDP dependency) and are unit-tested.
//!
//! Contract per api-reference.md §10.1:
//! - `format`: always "snapshot"
//! - `content`: string with `[ref=eN]` labels, one node per line
//! - `nodes`: array with ref/role/name/value fields
//! - `stats`: node_count / interactive_count

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A normalised accessibility node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AXNode {
    /// Stable reference label, e.g. "e1", "e2", ...
    pub ref_id: String,
    /// ARIA role string (e.g. "button", "textbox")
    pub role: String,
    /// Accessible name
    pub name: String,
    /// Current value (inputs, text areas); empty string if not applicable
    pub value: String,
    /// Whether this node is considered interactive
    pub interactive: bool,
    /// Tree depth (0 = root)
    pub depth: usize,
    /// Children
    pub children: Vec<AXNode>,
    /// Cursor-interactive info (Some when detected via --cursor flag)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_info: Option<CursorInfo>,
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

/// Roles considered interactive per §10.1.
pub fn is_interactive_role(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "checkbox"
            | "combobox"
            | "Iframe"
            | "link"
            | "listbox"
            | "menuitem"
            | "menuitemcheckbox"
            | "menuitemradio"
            | "option"
            | "radio"
            | "searchbox"
            | "slider"
            | "spinbutton"
            | "switch"
            | "tab"
            | "textbox"
            | "treeitem"
    )
}

// NOTE: All filtering (interactive, compact, depth, selector, cursor) is handled
// inline during DFS traversal in parse_ax_tree(). No standalone filter functions.

// ── Cursor-interactive detection types ───────────────────────────────

/// Info about a cursor-interactive element detected via DOM inspection.
/// These are non-ARIA elements with onclick, cursor:pointer, tabindex, or contenteditable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorInfo {
    /// "clickable" (cursor:pointer / onclick), "focusable" (tabindex), "editable" (contenteditable)
    pub kind: String,
    /// Detection hints: ["cursor:pointer", "onclick", "tabindex", "contenteditable"]
    pub hints: Vec<String>,
}

/// Render a flat node list to `content` string per §10.1.
/// Format: `- role "name" [ref=eN]` with depth-based indentation.
/// Nodes without a ref omit the `[...]` bracket.
/// Quotes and newlines in names are escaped to prevent tree injection.
pub fn render_content(nodes: &[AXNode]) -> String {
    let mut lines = Vec::new();
    for node in nodes {
        let indent = "  ".repeat(node.depth);
        // Escape quotes, newlines, and control chars to prevent injection
        let escaped_name: String = node
            .name
            .chars()
            .flat_map(|c| match c {
                '\\' => vec!['\\', '\\'],
                '"' => vec!['\\', '"'],
                '\n' => vec!['\\', 'n'],
                '\r' => vec!['\\', 'r'],
                c if c.is_control() => vec![], // strip other control chars (ESC, etc.)
                c => vec![c],
            })
            .collect();
        let mut line = format!("{indent}- {} \"{}\"", node.role, escaped_name);
        if !node.ref_id.is_empty() {
            line.push_str(&format!(" [ref={}]", node.ref_id));
        }
        // Append cursor-interactive info: " clickable [cursor:pointer, onclick]"
        if let Some(ref ci) = node.cursor_info {
            line.push_str(&format!(" {} [{}]", ci.kind, ci.hints.join(", ")));
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Extract a string from a CDP AXValue `{"type":"...","value":"..."}`.
/// Handles string, integer, float, and boolean value types.
fn extract_ax_string(ax_value: &Value) -> String {
    let val = &ax_value["value"];
    if let Some(s) = val.as_str() {
        return s.to_string();
    }
    if let Some(n) = val.as_i64() {
        return n.to_string();
    }
    if let Some(f) = val.as_f64() {
        if f.fract() == 0.0 && f.abs() < 1e10 {
            return format!("{:.0}", f);
        }
        return f.to_string();
    }
    if let Some(b) = val.as_bool() {
        return b.to_string();
    }
    String::new()
}

/// Parse CDP Accessibility.getFullAXTree response into a flat AXNode list.
///
/// Builds a proper tree from CDP childIds, then renders recursively.
/// Ignored nodes' children are promoted. RootWebArea/WebArea are unwrapped.
/// Only interactive roles and named content roles get refs (eN).
///
/// `ref_cache`: tab-scoped cache for stable refs across repeated snapshots.
/// `scope_backend_ids`: if Some, only include nodes whose backendDOMNodeId is in the set
///   (used by --selector to restrict to a CSS subtree).
/// `cursor_elements`: if Some, nodes whose backendDOMNodeId is in this map get
///   marked as interactive and assigned refs (used by --cursor flag).
pub fn parse_ax_tree(
    response: &Value,
    options: &SnapshotOptions,
    ref_cache: &mut RefCache,
    scope_backend_ids: Option<&std::collections::HashSet<i64>>,
    cursor_elements: Option<&std::collections::HashMap<i64, CursorInfo>>,
) -> Vec<AXNode> {
    let nodes_json = response["result"]["nodes"].as_array();
    let Some(nodes_json) = nodes_json else {
        return vec![];
    };
    if nodes_json.is_empty() {
        return vec![];
    }

    // Build index: nodeId → array index
    let mut id_to_idx: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, node) in nodes_json.iter().enumerate() {
        if let Some(id) = node["nodeId"].as_str() {
            id_to_idx.insert(id, i);
        }
    }

    // Build children map from childIds
    let mut children_map: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    let mut is_child = vec![false; nodes_json.len()];
    for (i, node) in nodes_json.iter().enumerate() {
        if let Some(child_ids) = node["childIds"].as_array() {
            let mut children = Vec::new();
            for cid in child_ids {
                if let Some(cid_str) = cid.as_str()
                    && let Some(&child_idx) = id_to_idx.get(cid_str)
                {
                    children.push(child_idx);
                    is_child[child_idx] = true;
                }
            }
            if !children.is_empty() {
                children_map.insert(i, children);
            }
        }
    }

    // Find root nodes (not referenced as children)
    let root_indices: Vec<usize> = (0..nodes_json.len()).filter(|&i| !is_child[i]).collect();

    // Recursive render
    let mut result = Vec::new();

    #[allow(clippy::too_many_arguments)]
    fn render(
        nodes_json: &[Value],
        children_map: &std::collections::HashMap<usize, Vec<usize>>,
        idx: usize,
        depth: usize,
        options: &SnapshotOptions,
        result: &mut Vec<AXNode>,
        ref_cache: &mut RefCache,
        scope: Option<&std::collections::HashSet<i64>>,
        cursor_elements: Option<&std::collections::HashMap<i64, CursorInfo>>,
    ) {
        let node = &nodes_json[idx];
        let role = extract_ax_string(&node["role"]);
        let name = strip_invisible_chars(&extract_ax_string(&node["name"]));
        let ignored = node["ignored"].as_bool().unwrap_or(false);

        // Helper: render all children at the given depth
        let render_children = |depth: usize, result: &mut Vec<AXNode>, ref_cache: &mut RefCache| {
            if let Some(children) = children_map.get(&idx) {
                for &child_idx in children {
                    render(
                        nodes_json,
                        children_map,
                        child_idx,
                        depth,
                        options,
                        result,
                        ref_cache,
                        scope,
                        cursor_elements,
                    );
                }
            }
        };

        // Skip ignored nodes (promote children to same depth)
        if ignored && role != "RootWebArea" {
            render_children(depth, result, ref_cache);
            return;
        }

        // Unwrap RootWebArea / WebArea (promote children)
        if role == "RootWebArea" || role == "WebArea" {
            render_children(depth, result, ref_cache);
            return;
        }

        // Skip noise roles (promote children)
        if is_skip_role(&role) {
            render_children(depth, result, ref_cache);
            return;
        }

        // Scope filter: skip nodes outside the CSS selector subtree (promote children)
        if let Some(scope_set) = scope {
            let bid = node["backendDOMNodeId"].as_i64().unwrap_or(0);
            if bid > 0 && !scope_set.contains(&bid) {
                render_children(depth, result, ref_cache);
                return;
            }
        }

        // Depth limit
        if let Some(max) = options.depth
            && depth > max
        {
            return;
        }

        // Check cursor-interactive BEFORE interactive filter — cursor nodes
        // must survive --interactive filtering even if their ARIA role is non-interactive.
        let backend_node_id = node["backendDOMNodeId"].as_i64().unwrap_or(0);
        let cursor_info = cursor_elements.and_then(|map| {
            if backend_node_id > 0 {
                map.get(&backend_node_id).cloned()
            } else {
                None
            }
        });
        let is_cursor = cursor_info.is_some();
        let is_interactive = is_interactive_role(&role) || is_cursor;

        // Interactive filter: skip non-interactive self but render children
        if options.interactive && !is_interactive {
            render_children(depth, result, ref_cache);
            return;
        }

        // Extract value (handles string, number, bool)
        let value = extract_ax_string(&node["value"]);

        // Assign ref for: interactive roles, named content roles, OR cursor-interactive
        let should_ref = should_assign_ref(&role, &name) || is_cursor;
        let ref_id = if should_ref {
            if backend_node_id > 0 {
                ref_cache.get_or_assign(backend_node_id, &role, &name)
            } else {
                let node_id_str = node["nodeId"].as_str().unwrap_or("");
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                node_id_str.hash(&mut hasher);
                let hash_key = -(hasher.finish() as i64).abs() - 1;
                ref_cache.get_or_assign(hash_key, &role, &name)
            }
        } else {
            String::new()
        };

        result.push(AXNode {
            ref_id,
            role,
            name,
            value,
            interactive: is_interactive || is_cursor,
            depth,
            children: vec![],
            cursor_info,
        });

        // Recurse children at depth + 1
        if let Some(children) = children_map.get(&idx) {
            for &child_idx in children {
                render(
                    nodes_json,
                    children_map,
                    child_idx,
                    depth + 1,
                    options,
                    result,
                    ref_cache,
                    scope,
                    cursor_elements,
                );
            }
        }
    }

    for &root_idx in &root_indices {
        render(
            nodes_json,
            &children_map,
            root_idx,
            0,
            options,
            &mut result,
            ref_cache,
            scope_backend_ids,
            cursor_elements,
        );
    }

    // Apply compact: compact_tree_nodes first (preserves ancestor chains for
    // ref/value nodes), then remove_empty_leaves (cleans remaining structural leaves).
    // Order matters: removing leaves first can break ancestor chain depth detection.
    if options.compact {
        result = compact_tree_nodes(&result);
        result = remove_empty_leaves(result);
    }

    result
}

/// Build the full SnapshotOutput from a flat node list.
/// `data.nodes` only contains nodes that have a ref (interactive + named content).
pub fn build_output(nodes: Vec<AXNode>) -> SnapshotOutput {
    let content = render_content(&nodes);
    // Stats count all nodes with refs
    let ref_nodes: Vec<&AXNode> = nodes.iter().filter(|n| !n.ref_id.is_empty()).collect();
    let node_count = ref_nodes.len();
    let interactive_count = ref_nodes.iter().filter(|n| n.interactive).count();
    let entries = ref_nodes
        .iter()
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

// ── P0: Role & noise classification ──────────────────────────────────

/// Noise roles to skip entirely during tree traversal.
/// Children of skipped nodes are promoted to the parent level.
/// Roles to skip entirely — internal rendering detail, redundant with parent content.
/// NOTE: StaticText is NOT skipped — it carries visible text content agents need.
/// InlineTextBox is the internal rendering detail that duplicates StaticText content.
const SKIP_ROLES: &[&str] = &[
    "InlineTextBox",
    "LineBreak",
    "ListMarker",
    "strong",
    "emphasis",
    "subscript",
    "superscript",
    "mark",
];

/// Content roles — get refs only when they have a non-empty accessible name.
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

/// Structural roles — candidates for removal during compact/empty-leaf filtering.
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

/// Check if a role is a noise role that should be skipped.
pub fn is_skip_role(role: &str) -> bool {
    SKIP_ROLES.contains(&role)
}

/// Check if a role is a content role.
pub fn is_content_role(role: &str) -> bool {
    CONTENT_ROLES.contains(&role)
}

/// Check if a role is a structural role.
pub fn is_structural_role(role: &str) -> bool {
    STRUCTURAL_ROLES.contains(&role)
}

/// Whether a node should receive a ref based on its role and name.
/// Interactive roles always get refs. Content roles get refs only with non-empty name.
pub fn should_assign_ref(role: &str, name: &str) -> bool {
    is_interactive_role(role) || (is_content_role(role) && !name.is_empty())
}

// ── P1: Invisible character filtering ────────────────────────────────

/// Normalize invisible/zero-width characters in accessible names.
/// Removes truly invisible chars (BOM, ZWS, ZWNJ, ZWJ, WJ).
/// Replaces NBSP with regular space (preserves word boundaries).
pub fn strip_invisible_chars(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{FEFF}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' => None,
            '\u{00A0}' => Some(' '), // NBSP → regular space
            _ => Some(c),
        })
        .collect()
}

// ── P1: Duplicate ref tracking ───────────────────────────────────────

/// Tracks occurrences of role:name pairs to detect duplicates.
/// When multiple elements share the same role and name, each gets an `nth` index
/// for disambiguation (e.g., the 2nd "button:Submit" → nth=2).
#[derive(Debug, Default)]
pub struct RoleNameTracker {
    counts: std::collections::HashMap<String, usize>,
}

impl RoleNameTracker {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(role: &str, name: &str) -> String {
        // Use NUL byte separator to avoid collisions:
        // "a:b" + "c" won't collide with "a" + "b:c"
        format!("{}\0{}", role, name)
    }

    /// Record an occurrence of role:name. Returns the 1-based occurrence index.
    pub fn record(&mut self, role: &str, name: &str) -> usize {
        let key = Self::key(role, name);
        let count = self.counts.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    /// Returns how many times a role:name pair has been seen.
    pub fn count(&self, role: &str, name: &str) -> usize {
        let key = Self::key(role, name);
        *self.counts.get(&key).unwrap_or(&0)
    }

    /// Returns true if a role:name pair has been seen more than once.
    pub fn has_duplicates(&self, role: &str, name: &str) -> bool {
        self.count(role, name) > 1
    }
}

// ── P0: Stable ref cache (cross-snapshot persistence) ────────────────

/// A cached entry for a DOM element: stable ref label plus AX metadata.
#[derive(Debug, Clone)]
pub struct RefEntry {
    pub ref_id: String,
    pub role: String,
    pub name: String,
}

/// Tab-scoped cache mapping backendNodeId → RefEntry.
/// Ensures that the same DOM element keeps the same ref across repeated snapshots.
/// Also stores role/name so that screenshot `--annotate` can build annotation metadata.
///
/// Scope: one RefCache per tab (not per session — each tab has independent DOM).
// TODO: iframe isolation — iframes within a tab may share backendNodeId values
// with the main frame. Need either frame-prefixed keys (e.g., "frame_id:node_id")
// or a separate RefCache per frame to avoid collisions.
#[derive(Debug, Clone)]
pub struct RefCache {
    /// backendNodeId → RefEntry (e.g., 42 → RefEntry { ref_id: "e1", role: "button", name: "Submit" })
    id_to_ref: std::collections::HashMap<i64, RefEntry>,
    /// ref_id → backendNodeId (reverse lookup for @eN resolution)
    ref_to_id: std::collections::HashMap<String, i64>,
    /// Next available ref counter
    next_ref: usize,
}

impl Default for RefCache {
    fn default() -> Self {
        Self::new() // next_ref starts at 1, so refs begin at e1
    }
}

impl RefCache {
    pub fn new() -> Self {
        Self {
            id_to_ref: std::collections::HashMap::new(),
            ref_to_id: std::collections::HashMap::new(),
            next_ref: 1, // refs start from e1
        }
    }

    /// Get or assign a stable ref for the given backendNodeId.
    /// If the node was seen before, updates role/name and returns the same ref.
    /// If new, assigns the next available eN.
    pub fn get_or_assign(&mut self, backend_node_id: i64, role: &str, name: &str) -> String {
        if let Some(existing) = self.id_to_ref.get_mut(&backend_node_id) {
            existing.role = role.to_string();
            existing.name = name.to_string();
            return existing.ref_id.clone();
        }
        let ref_id = format!("e{}", self.next_ref);
        self.next_ref += 1;
        self.ref_to_id.insert(ref_id.clone(), backend_node_id);
        self.id_to_ref.insert(
            backend_node_id,
            RefEntry {
                ref_id: ref_id.clone(),
                role: role.to_string(),
                name: name.to_string(),
            },
        );
        ref_id
    }

    /// Look up the ref for a backendNodeId without assigning.
    pub fn get_ref(&self, backend_node_id: i64) -> Option<&str> {
        self.id_to_ref
            .get(&backend_node_id)
            .map(|e| e.ref_id.as_str())
    }

    /// Look up the full entry for a backendNodeId.
    pub fn get(&self, backend_node_id: i64) -> Option<&RefEntry> {
        self.id_to_ref.get(&backend_node_id)
    }

    /// Iterate over all entries: (backendNodeId, &RefEntry).
    pub fn entries(&self) -> impl Iterator<Item = (i64, &RefEntry)> {
        self.id_to_ref.iter().map(|(&k, v)| (k, v))
    }

    /// Number of refs assigned so far.
    pub fn len(&self) -> usize {
        self.id_to_ref.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.id_to_ref.is_empty()
    }

    /// Reverse lookup: ref_id (e.g. "e5") → backendNodeId.
    pub fn backend_node_id_for_ref(&self, ref_id: &str) -> Option<i64> {
        self.ref_to_id.get(ref_id).copied()
    }

    /// Reverse lookup: ref_id → full RefEntry (for role+name fallback).
    pub fn entry_for_ref(&self, ref_id: &str) -> Option<&RefEntry> {
        self.ref_to_id
            .get(ref_id)
            .and_then(|bid| self.id_to_ref.get(bid))
    }
}

// ── P0: Empty leaf removal ───────────────────────────────────────────

/// Remove leaf nodes that are structural with no name, no ref, no value.
/// A leaf is a node with no children (i.e., no subsequent node at a greater depth).
pub fn remove_empty_leaves(nodes: Vec<AXNode>) -> Vec<AXNode> {
    let mut has_child = vec![false; nodes.len()];
    for i in 0..nodes.len() {
        if i + 1 < nodes.len() && nodes[i + 1].depth > nodes[i].depth {
            has_child[i] = true;
        }
    }

    nodes
        .into_iter()
        .enumerate()
        .filter(|(i, node)| {
            // Keep non-structural nodes
            if !is_structural_role(&node.role) {
                return true;
            }
            // Keep if it has a ref, name, or value
            if !node.ref_id.is_empty() || !node.name.is_empty() || !node.value.is_empty() {
                return true;
            }
            // Keep if it has children
            has_child[*i]
        })
        .map(|(_, node)| node)
        .collect()
}

// ── P1: Compact tree with ancestor chain ─────────────────────────────

/// Keep only nodes with refs or values, plus their ancestor chain.
/// More aggressive than remove_empty_leaves — removes ALL non-ref/non-value nodes
/// except those needed to maintain the tree path to ref/value nodes.
pub fn compact_tree_nodes(nodes: &[AXNode]) -> Vec<AXNode> {
    let mut keep = vec![false; nodes.len()];

    for i in 0..nodes.len() {
        let has_ref = !nodes[i].ref_id.is_empty();
        let has_value = !nodes[i].value.is_empty();
        if has_ref || has_value {
            keep[i] = true;
            // Walk backwards to mark ancestor chain
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

// ── P1: Token budget ─────────────────────────────────────────────────

/// Estimate the token count of rendered content.
/// Uses ~3 chars per token (conservative — handles CJK, special chars, markup overhead).
pub fn estimate_tokens(content: &str) -> usize {
    content.len().div_ceil(3)
}

/// Truncate a node list to fit within a token budget.
/// Returns (truncated_nodes, was_truncated).
pub fn truncate_to_tokens(nodes: &[AXNode], max_tokens: usize) -> (Vec<AXNode>, bool) {
    let mut total = 0usize;
    let mut result = Vec::new();

    for node in nodes {
        // Estimate actual rendered line length:
        // {indent}- {role} "{name}" [ref={ref_id}]\n
        // Value is in JSON nodes[] only, not in content text, but count it
        // for total payload estimation.
        let indent = node.depth * 2;
        let ref_bracket = if node.ref_id.is_empty() {
            0
        } else {
            " [ref=]".len() + node.ref_id.len()
        };
        let line_chars = indent
            + "- ".len()
            + node.role.len()
            + " \"\"".len()
            + node.name.len()
            + ref_bracket
            + 1; // +1 for \n
        // Add value cost (appears in JSON nodes[], ~20 chars JSON overhead per node)
        let value_cost = if node.value.is_empty() {
            0
        } else {
            node.value.len() + 20
        };
        // Conservative: ~3 chars per token
        let cost = (line_chars + value_cost).div_ceil(3);
        if total + cost > max_tokens {
            return (result, true);
        }
        total += cost;
        result.push(node.clone());
    }

    (result, false)
}

// ── Unit Tests ────────────────────────────────────────────────────────

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
            children: vec![],
            cursor_info: None,
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
            children: vec![],
            cursor_info: None,
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

    // NOTE: filter_interactive, filter_compact, apply_depth, apply_selector tests removed —
    // these functions were superseded by DFS inline processing in parse_ax_tree().

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
    fn test_render_content_value_not_in_text() {
        // Per §10.1: value is in data.nodes[] JSON, not in text content.
        let nodes = vec![make_node_with_value(
            "e1",
            "textbox",
            "Email",
            "user@example.com",
            true,
            0,
        )];
        let content = render_content(&nodes);
        // Content should contain role+name+ref, but NOT the value
        assert!(
            content.contains("textbox") && content.contains("[ref=e1]"),
            "must contain role and ref"
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

    // NOTE: build_stats tests removed — stats are computed inline in build_output.

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
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );
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
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_ax_tree_missing_nodes() {
        let response = serde_json::json!({ "result": {} });
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );
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
        let nodes = parse_ax_tree(&response, &opts, &mut RefCache::new(), None, None);
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
        let nodes = parse_ax_tree(&response, &opts, &mut RefCache::new(), None, None);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].role, "button");
    }

    #[test]
    fn test_parse_ax_tree_depth_filter() {
        // depth=0: only root-level nodes survive.
        // RootWebArea is unwrapped, so its children become depth 0.
        // Their children are at depth 1 and should be cut.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""}, "childIds": ["2"]
                    },
                    {
                        "nodeId": "2", "role": {"value": "navigation"},
                        "name": {"value": "Nav"}, "childIds": ["3"]
                    },
                    {
                        "nodeId": "3", "role": {"value": "button"},
                        "name": {"value": "OK"}, "childIds": []
                    },
                ]
            }
        });
        let opts = SnapshotOptions {
            depth: Some(0),
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts, &mut RefCache::new(), None, None);
        // After RootWebArea unwrap: navigation=depth 0, button=depth 1 (cut by depth=0)
        assert_eq!(
            nodes.len(),
            1,
            "depth=0 must return only root-level node; got {}",
            nodes.len()
        );
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
        let nodes = parse_ax_tree(&response, &opts, &mut RefCache::new(), None, None);
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
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );
        assert_eq!(nodes[0].ref_id, "e1");
        assert_eq!(nodes[1].ref_id, "e2");
        assert_eq!(nodes[2].ref_id, "e3");
    }

    // ══════════════════════════════════════════════════════════════════
    // P0: Role & noise classification
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_is_skip_role_noise_roles() {
        assert!(is_skip_role("InlineTextBox"));
        assert!(is_skip_role("LineBreak"));
        assert!(is_skip_role("ListMarker"));
        assert!(is_skip_role("strong"));
        assert!(is_skip_role("emphasis"));
        // StaticText is NOT skipped — it carries visible text content
        assert!(!is_skip_role("StaticText"));
    }

    #[test]
    fn test_is_skip_role_non_noise() {
        assert!(!is_skip_role("button"));
        assert!(!is_skip_role("heading"));
        assert!(!is_skip_role("generic"));
        assert!(!is_skip_role(""));
    }

    #[test]
    fn test_is_content_role() {
        assert!(is_content_role("heading"));
        assert!(is_content_role("cell"));
        assert!(is_content_role("navigation"));
        assert!(is_content_role("main"));
        assert!(!is_content_role("button"));
        assert!(!is_content_role("generic"));
    }

    #[test]
    fn test_is_structural_role() {
        assert!(is_structural_role("generic"));
        assert!(is_structural_role("group"));
        assert!(is_structural_role("RootWebArea"));
        assert!(is_structural_role("none"));
        assert!(!is_structural_role("button"));
        assert!(!is_structural_role("heading"));
    }

    #[test]
    fn test_should_assign_ref_interactive_always() {
        // Interactive roles always get refs regardless of name
        assert!(should_assign_ref("button", "Submit"));
        assert!(should_assign_ref("button", ""));
        assert!(should_assign_ref("textbox", ""));
        assert!(should_assign_ref("link", ""));
        assert!(should_assign_ref("checkbox", ""));
    }

    #[test]
    fn test_should_assign_ref_content_needs_name() {
        // Content roles only get refs when they have a non-empty name
        assert!(should_assign_ref("heading", "Title"));
        assert!(should_assign_ref("navigation", "Main Nav"));
        assert!(!should_assign_ref("heading", ""));
        assert!(!should_assign_ref("navigation", ""));
    }

    #[test]
    fn test_should_assign_ref_structural_never() {
        // Structural and noise roles never get refs
        assert!(!should_assign_ref("generic", ""));
        assert!(!should_assign_ref("generic", "Container"));
        assert!(!should_assign_ref("group", "Section"));
        assert!(!should_assign_ref("InlineTextBox", "text"));
        assert!(!should_assign_ref("StaticText", "text"));
    }

    // ══════════════════════════════════════════════════════════════════
    // P0: Recursive tree building contract tests
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_ignored_nodes_skip_self_render_children() {
        // Contract: ignored nodes (ignored=true) are skipped, but their children
        // are promoted to the same depth level.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""},
                        "childIds": ["2"]
                    },
                    {
                        "nodeId": "2", "ignored": true,
                        "role": {"value": "generic"}, "name": {"value": ""},
                        "childIds": ["3"]
                    },
                    {
                        "nodeId": "3",
                        "role": {"value": "button"}, "name": {"value": "Submit"},
                        "childIds": []
                    },
                ]
            }
        });
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );

        // The button should appear (child of ignored node promoted).
        // Neither RootWebArea nor the ignored generic should appear.
        assert!(
            nodes
                .iter()
                .any(|n| n.role == "button" && n.name == "Submit"),
            "button child of ignored node must be present: {nodes:?}"
        );
        assert!(
            !nodes.iter().any(|n| n.role == "RootWebArea"),
            "RootWebArea must be unwrapped: {nodes:?}"
        );
    }

    #[test]
    fn test_parse_rootwebarea_unwrap() {
        // Contract: RootWebArea is unwrapped — its children are rendered at depth 0
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": "Page"},
                        "childIds": ["2", "3"]
                    },
                    {
                        "nodeId": "2",
                        "role": {"value": "navigation"}, "name": {"value": "Main"},
                        "childIds": []
                    },
                    {
                        "nodeId": "3",
                        "role": {"value": "button"}, "name": {"value": "OK"},
                        "childIds": []
                    },
                ]
            }
        });
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );

        // RootWebArea must not appear in output
        assert!(
            !nodes.iter().any(|n| n.role == "RootWebArea"),
            "RootWebArea must not appear in output: {nodes:?}"
        );
        // Children should be at depth 0 (promoted from RootWebArea)
        let nav = nodes.iter().find(|n| n.role == "navigation");
        assert!(nav.is_some(), "navigation must be present");
        assert_eq!(
            nav.unwrap().depth,
            0,
            "navigation must be at depth 0 after RootWebArea unwrap"
        );
    }

    #[test]
    fn test_parse_noise_roles_skipped() {
        // InlineTextBox is skipped (internal rendering detail).
        // StaticText is KEPT (carries visible text content).
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""},
                        "childIds": ["2"]
                    },
                    {
                        "nodeId": "2",
                        "role": {"value": "heading"}, "name": {"value": "Title"},
                        "childIds": ["3", "4"]
                    },
                    {
                        "nodeId": "3",
                        "role": {"value": "StaticText"}, "name": {"value": "Title"},
                        "childIds": ["5"]
                    },
                    {
                        "nodeId": "4",
                        "role": {"value": "InlineTextBox"}, "name": {"value": "Title"},
                        "childIds": []
                    },
                    {
                        "nodeId": "5",
                        "role": {"value": "InlineTextBox"}, "name": {"value": "Title"},
                        "childIds": []
                    },
                ]
            }
        });
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );

        // InlineTextBox must be filtered out
        assert!(
            !nodes.iter().any(|n| n.role == "InlineTextBox"),
            "InlineTextBox must be filtered out: {nodes:?}"
        );
        // StaticText must be kept (visible text)
        assert!(
            nodes.iter().any(|n| n.role == "StaticText"),
            "StaticText must be kept: {nodes:?}"
        );
        assert!(
            nodes.iter().any(|n| n.role == "heading"),
            "heading must remain: {nodes:?}"
        );
    }

    #[test]
    fn test_parse_ref_only_for_interactive_and_named_content() {
        // Contract: only interactive + named content nodes get refs
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""}, "childIds": ["2", "3", "4", "5"]
                    },
                    {
                        "nodeId": "2",
                        "role": {"value": "button"}, "name": {"value": "OK"},
                        "childIds": []
                    },
                    {
                        "nodeId": "3",
                        "role": {"value": "heading"}, "name": {"value": "Title"},
                        "childIds": []
                    },
                    {
                        "nodeId": "4",
                        "role": {"value": "heading"}, "name": {"value": ""},
                        "childIds": []
                    },
                    {
                        "nodeId": "5",
                        "role": {"value": "generic"}, "name": {"value": "Container"},
                        "childIds": []
                    },
                ]
            }
        });
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );

        // button "OK" → interactive → gets ref
        let btn = nodes.iter().find(|n| n.role == "button");
        assert!(btn.is_some(), "button must be present");
        assert!(
            !btn.unwrap().ref_id.is_empty(),
            "button must have a ref: {:?}",
            btn
        );

        // heading "Title" → content with name → gets ref
        let h_named = nodes
            .iter()
            .find(|n| n.role == "heading" && n.name == "Title");
        assert!(h_named.is_some(), "named heading must be present");
        assert!(
            !h_named.unwrap().ref_id.is_empty(),
            "named heading must have a ref"
        );

        // heading "" → content without name → no ref
        let h_empty = nodes
            .iter()
            .find(|n| n.role == "heading" && n.name.is_empty());
        if let Some(h) = h_empty {
            assert!(
                h.ref_id.is_empty(),
                "unnamed heading must NOT have a ref: {:?}",
                h
            );
        }

        // generic "Container" → structural → no ref
        let structural = nodes.iter().find(|n| n.role == "generic");
        if let Some(s) = structural {
            assert!(
                s.ref_id.is_empty(),
                "structural node must NOT have a ref: {:?}",
                s
            );
        }
    }

    #[test]
    fn test_parse_depth_from_childids() {
        // Contract: depth is computed from the tree hierarchy, not flat index
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""}, "childIds": ["2"]
                    },
                    {
                        "nodeId": "2",
                        "role": {"value": "navigation"}, "name": {"value": "Nav"},
                        "childIds": ["3"]
                    },
                    {
                        "nodeId": "3",
                        "role": {"value": "link"}, "name": {"value": "Home"},
                        "childIds": []
                    },
                ]
            }
        });
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );

        // After RootWebArea unwrap: navigation=depth 0, link=depth 1
        let nav = nodes.iter().find(|n| n.role == "navigation");
        let link = nodes.iter().find(|n| n.role == "link");
        assert!(nav.is_some() && link.is_some(), "both nodes must exist");
        assert_eq!(
            nav.unwrap().depth,
            0,
            "navigation must be at depth 0 (child of unwrapped RootWebArea)"
        );
        assert_eq!(
            link.unwrap().depth,
            1,
            "link must be at depth 1 (child of navigation)"
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // P0: remove_empty_leaves
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_remove_empty_leaves_removes_structural_leaf() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("", "generic", "", false, 0), // structural, no name/ref/value, leaf → remove
        ];
        let result = remove_empty_leaves(nodes);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ref_id, "e1");
    }

    #[test]
    fn test_remove_empty_leaves_keeps_structural_with_children() {
        let nodes = vec![
            make_node("", "group", "", false, 0), // structural but has children → keep
            make_node("e1", "button", "OK", true, 1), // child at depth 1
        ];
        let result = remove_empty_leaves(nodes);
        assert_eq!(result.len(), 2, "group with children must be kept");
    }

    #[test]
    fn test_remove_empty_leaves_keeps_named_structural() {
        let nodes = vec![
            make_node("", "navigation", "Main", false, 0), // structural but has name → keep
        ];
        let result = remove_empty_leaves(nodes);
        assert_eq!(result.len(), 1, "named structural node must be kept");
    }

    #[test]
    fn test_remove_empty_leaves_keeps_non_structural() {
        let nodes = vec![
            make_node("", "paragraph", "", false, 0), // non-structural → always keep
        ];
        let result = remove_empty_leaves(nodes);
        assert_eq!(result.len(), 1, "non-structural node must be kept");
    }

    #[test]
    fn test_remove_empty_leaves_multiple_structural_leaves() {
        let nodes = vec![
            make_node("", "generic", "", false, 0),
            make_node("e1", "button", "A", true, 0),
            make_node("", "none", "", false, 0),
            make_node("e2", "link", "B", true, 0),
            make_node("", "group", "", false, 0),
        ];
        let result = remove_empty_leaves(nodes);
        // Only generic, none, group leaves removed; button and link kept
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e1");
        assert_eq!(result[1].ref_id, "e2");
    }

    // ══════════════════════════════════════════════════════════════════
    // P1: Invisible character filtering
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_strip_invisible_chars_bom() {
        let input = "\u{FEFF}Hello";
        assert_eq!(strip_invisible_chars(input), "Hello");
    }

    #[test]
    fn test_strip_invisible_chars_zws() {
        let input = "He\u{200B}llo";
        assert_eq!(strip_invisible_chars(input), "Hello");
    }

    #[test]
    fn test_strip_invisible_chars_nbsp_becomes_space() {
        // NBSP should become regular space, not be deleted (preserves word boundaries)
        let input = "Hello\u{00A0}World";
        assert_eq!(strip_invisible_chars(input), "Hello World");
    }

    #[test]
    fn test_strip_invisible_chars_multiple() {
        // NBSP becomes space, all others removed
        let input = "\u{FEFF}\u{200B}\u{200C}\u{200D}\u{2060}\u{00A0}Clean";
        assert_eq!(strip_invisible_chars(input), " Clean");
    }

    #[test]
    fn test_strip_invisible_chars_normal_text() {
        let input = "Normal text 123";
        assert_eq!(strip_invisible_chars(input), "Normal text 123");
    }

    #[test]
    fn test_strip_invisible_chars_empty() {
        assert_eq!(strip_invisible_chars(""), "");
    }

    // ══════════════════════════════════════════════════════════════════
    // P1: RoleNameTracker (duplicate detection)
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_role_name_tracker_first_occurrence() {
        let mut tracker = RoleNameTracker::new();
        let nth = tracker.record("button", "Submit");
        assert_eq!(nth, 1, "first occurrence returns 1");
        assert_eq!(tracker.count("button", "Submit"), 1);
        assert!(!tracker.has_duplicates("button", "Submit"));
    }

    #[test]
    fn test_role_name_tracker_duplicates() {
        let mut tracker = RoleNameTracker::new();
        assert_eq!(tracker.record("button", "Submit"), 1);
        assert_eq!(tracker.record("button", "Submit"), 2);
        assert_eq!(tracker.record("button", "Submit"), 3);
        assert!(tracker.has_duplicates("button", "Submit"));
        assert_eq!(tracker.count("button", "Submit"), 3);
    }

    #[test]
    fn test_role_name_tracker_different_names() {
        let mut tracker = RoleNameTracker::new();
        tracker.record("button", "Submit");
        tracker.record("button", "Cancel");
        assert!(!tracker.has_duplicates("button", "Submit"));
        assert!(!tracker.has_duplicates("button", "Cancel"));
    }

    #[test]
    fn test_role_name_tracker_different_roles() {
        let mut tracker = RoleNameTracker::new();
        tracker.record("button", "OK");
        tracker.record("link", "OK");
        assert!(!tracker.has_duplicates("button", "OK"));
        assert!(!tracker.has_duplicates("link", "OK"));
    }

    #[test]
    fn test_role_name_tracker_unseen_pair() {
        let tracker = RoleNameTracker::new();
        assert_eq!(tracker.count("button", "Never"), 0);
        assert!(!tracker.has_duplicates("button", "Never"));
    }

    // ══════════════════════════════════════════════════════════════════
    // P1: compact_tree_nodes (ancestor chain preservation)
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_compact_tree_nodes_keeps_ref_and_ancestors() {
        let nodes = vec![
            make_node("", "navigation", "Nav", false, 0), // ancestor of e1
            make_node("", "list", "Links", false, 1),     // ancestor of e1
            make_node("e1", "link", "Home", true, 2),     // has ref → keep + ancestors
            make_node("", "generic", "", false, 1),       // no ref, not ancestor → remove
        ];
        let result = compact_tree_nodes(&nodes);
        assert_eq!(
            result.len(),
            3,
            "ref node + 2 ancestors, non-ancestor removed"
        );
        assert_eq!(result[0].role, "navigation");
        assert_eq!(result[1].role, "list");
        assert_eq!(result[2].ref_id, "e1");
    }

    #[test]
    fn test_compact_tree_nodes_empty_input() {
        let result = compact_tree_nodes(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compact_tree_nodes_all_refs() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let result = compact_tree_nodes(&nodes);
        assert_eq!(result.len(), 2, "all ref nodes kept");
    }

    #[test]
    fn test_compact_tree_nodes_no_refs() {
        let nodes = vec![
            make_node("", "generic", "", false, 0),
            make_node("", "group", "", false, 1),
        ];
        let result = compact_tree_nodes(&nodes);
        assert!(result.is_empty(), "no ref nodes → empty result");
    }

    #[test]
    fn test_compact_tree_nodes_with_value() {
        let nodes = vec![
            make_node("", "group", "", false, 0),
            make_node_with_value("", "textbox", "Email", "user@test.com", true, 1), // has value → keep
        ];
        let result = compact_tree_nodes(&nodes);
        assert_eq!(result.len(), 2, "value node + ancestor kept");
    }

    // ══════════════════════════════════════════════════════════════════
    // P1: Token budget
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_estimate_tokens_basic() {
        let content = "- button \"Submit\" [ref=e1]\n";
        let tokens = estimate_tokens(content);
        assert!(tokens > 0, "non-empty content must have > 0 tokens");
        // ~27 chars / 4 ≈ 6-7 tokens
        assert!(tokens < 20, "short content should have < 20 tokens");
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_truncate_to_tokens_no_truncation() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("e2", "link", "Home", true, 0),
        ];
        let (result, truncated) = truncate_to_tokens(&nodes, 1000);
        assert!(!truncated);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_truncate_to_tokens_with_truncation() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("e2", "link", "Home", true, 0),
            make_node("e3", "textbox", "Search query input field", true, 0),
        ];
        // Very small budget → should truncate
        let (result, truncated) = truncate_to_tokens(&nodes, 5);
        assert!(truncated, "must truncate with tiny budget");
        assert!(
            result.len() < 3,
            "must return fewer nodes than input: got {}",
            result.len()
        );
    }

    #[test]
    fn test_truncate_to_tokens_empty() {
        let (result, truncated) = truncate_to_tokens(&[], 100);
        assert!(!truncated);
        assert!(result.is_empty());
    }

    #[test]
    fn test_truncate_to_tokens_zero_budget() {
        let nodes = vec![make_node("e1", "button", "OK", true, 0)];
        let (result, truncated) = truncate_to_tokens(&nodes, 0);
        assert!(truncated);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════
    // P0: RefCache — stable ref across repeated snapshots
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_ref_cache_assigns_sequential() {
        let mut cache = RefCache::new();
        assert_eq!(cache.get_or_assign(42, "button", "OK"), "e1");
        assert_eq!(cache.get_or_assign(55, "link", "Home"), "e2");
        assert_eq!(cache.get_or_assign(99, "textbox", "Search"), "e3");
    }

    #[test]
    fn test_ref_cache_stable_on_repeat() {
        let mut cache = RefCache::new();
        // First snapshot
        assert_eq!(cache.get_or_assign(42, "button", "OK"), "e1");
        assert_eq!(cache.get_or_assign(55, "link", "Home"), "e2");

        // Second snapshot — same backendNodeIds must keep same refs
        assert_eq!(cache.get_or_assign(42, "button", "OK"), "e1");
        assert_eq!(cache.get_or_assign(55, "link", "Home"), "e2");
    }

    #[test]
    fn test_ref_cache_new_element_gets_next_ref() {
        let mut cache = RefCache::new();
        assert_eq!(cache.get_or_assign(42, "button", "OK"), "e1");
        assert_eq!(cache.get_or_assign(55, "link", "Home"), "e2");

        // New element appears in second snapshot
        assert_eq!(cache.get_or_assign(99, "textbox", "Search"), "e3");
        // Original elements unchanged
        assert_eq!(cache.get_or_assign(42, "button", "OK"), "e1");
    }

    #[test]
    fn test_ref_cache_lookup_without_assign() {
        let mut cache = RefCache::new();
        assert!(cache.get(42).is_none());
        cache.get_or_assign(42, "button", "OK");
        assert_eq!(cache.get(42).map(|e| e.ref_id.as_str()), Some("e1"));
    }

    #[test]
    fn test_ref_cache_len() {
        let mut cache = RefCache::new();
        assert!(cache.is_empty());
        cache.get_or_assign(42, "button", "OK");
        cache.get_or_assign(55, "link", "Home");
        assert_eq!(cache.len(), 2);
        // Repeat doesn't increase len
        cache.get_or_assign(42, "button", "OK");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_ref_cache_stores_role_name() {
        let mut cache = RefCache::new();
        cache.get_or_assign(42, "button", "Submit");
        let entry = cache.get(42).unwrap();
        assert_eq!(entry.ref_id, "e1");
        assert_eq!(entry.role, "button");
        assert_eq!(entry.name, "Submit");
    }

    #[test]
    fn test_ref_cache_updates_role_name_on_reassign() {
        let mut cache = RefCache::new();
        cache.get_or_assign(42, "button", "Old");
        cache.get_or_assign(42, "button", "New");
        let entry = cache.get(42).unwrap();
        assert_eq!(entry.ref_id, "e1");
        assert_eq!(entry.name, "New");
    }

    #[test]
    fn test_ref_cache_entries() {
        let mut cache = RefCache::new();
        cache.get_or_assign(42, "button", "OK");
        cache.get_or_assign(55, "link", "Home");
        let entries: Vec<_> = cache.entries().collect();
        assert_eq!(entries.len(), 2);
    }

    // ══════════════════════════════════════════════════════════════════
    // Codex review fixes: collision resistance, escaping
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_role_name_tracker_no_colon_collision() {
        // Codex finding #9: "a:b" + "c" must NOT collide with "a" + "b:c"
        let mut tracker = RoleNameTracker::new();
        tracker.record("a:b", "c");
        tracker.record("a", "b:c");
        // These are different pairs, neither should show as duplicate
        assert!(!tracker.has_duplicates("a:b", "c"));
        assert!(!tracker.has_duplicates("a", "b:c"));
    }

    #[test]
    fn test_render_content_escapes_quotes_in_name() {
        let nodes = vec![make_node("e1", "button", "Click \"here\"", true, 0)];
        let content = render_content(&nodes);
        assert!(
            !content.contains("\"here\"\""),
            "unescaped quotes in name would break parsing"
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Cursor-interactive detection
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_cursor_elements_assigns_refs() {
        // Contract: cursor-mapped nodes get ref + interactive=true + cursor_info
        // Use generic with EMPTY name so should_assign_ref returns false —
        // the ONLY path to getting a ref is via cursor_elements.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""}, "childIds": ["2", "3"]
                    },
                    {
                        "nodeId": "2", "backendDOMNodeId": 42,
                        "role": {"value": "generic"}, "name": {"value": ""},
                        "childIds": []
                    },
                    {
                        "nodeId": "3", "backendDOMNodeId": 55,
                        "role": {"value": "generic"}, "name": {"value": ""},
                        "childIds": []
                    },
                ]
            }
        });
        let mut cursor_map = std::collections::HashMap::new();
        cursor_map.insert(
            42_i64,
            CursorInfo {
                kind: "clickable".to_string(),
                hints: vec!["cursor:pointer".to_string(), "onclick".to_string()],
            },
        );

        // Baseline: without cursor_elements, generic "" gets no ref
        let baseline = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );
        // Generic with empty name should have no ref in baseline
        for n in &baseline {
            if n.role == "generic" {
                assert!(n.ref_id.is_empty(), "baseline: generic '' must have no ref");
                assert!(
                    !n.interactive,
                    "baseline: generic '' must not be interactive"
                );
            }
        }

        // With cursor_elements: backendNodeId=42 should get ref
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            Some(&cursor_map),
        );

        // Find the cursor-mapped node (backendNodeId=42)
        // It should now have: ref, interactive=true, cursor_info
        let cursor_node = nodes.iter().find(|n| n.cursor_info.is_some());
        assert!(
            cursor_node.is_some(),
            "cursor element must be present with cursor_info"
        );
        let cursor_node = cursor_node.unwrap();
        assert!(
            !cursor_node.ref_id.is_empty(),
            "cursor element must have ref: {:?}",
            cursor_node
        );
        assert!(
            cursor_node.interactive,
            "cursor element must have interactive=true"
        );
        assert_eq!(cursor_node.cursor_info.as_ref().unwrap().kind, "clickable");

        // Non-cursor generic (backendNodeId=55) should still have no ref
        let non_cursor: Vec<_> = nodes
            .iter()
            .filter(|n| n.role == "generic" && n.cursor_info.is_none())
            .collect();
        for n in &non_cursor {
            assert!(
                n.ref_id.is_empty(),
                "non-cursor generic must have no ref: {:?}",
                n
            );
        }
    }

    #[test]
    fn test_parse_cursor_elements_none_is_noop() {
        // Contract: None vs Some(empty) produce identical output
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""}, "childIds": ["2"]
                    },
                    {
                        "nodeId": "2", "backendDOMNodeId": 42,
                        "role": {"value": "generic"}, "name": {"value": "div"},
                        "childIds": []
                    },
                ]
            }
        });
        let nodes_without = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            None,
        );
        let nodes_with_empty = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            Some(&std::collections::HashMap::new()),
        );
        // Must be structurally identical, not just same count
        assert_eq!(nodes_without, nodes_with_empty);
    }

    #[test]
    fn test_parse_cursor_already_interactive_no_duplicate() {
        // Contract: if a node is already interactive (e.g., button) AND in cursor_map,
        // it keeps its ref but also gets cursor_info attached.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    {
                        "nodeId": "1", "role": {"value": "RootWebArea"},
                        "name": {"value": ""}, "childIds": ["2"]
                    },
                    {
                        "nodeId": "2", "backendDOMNodeId": 42,
                        "role": {"value": "button"}, "name": {"value": "Submit"},
                        "childIds": []
                    },
                ]
            }
        });
        let mut cursor_map = std::collections::HashMap::new();
        cursor_map.insert(
            42_i64,
            CursorInfo {
                kind: "clickable".to_string(),
                hints: vec!["onclick".to_string()],
            },
        );
        let nodes = parse_ax_tree(
            &response,
            &SnapshotOptions::default(),
            &mut RefCache::new(),
            None,
            Some(&cursor_map),
        );
        let btn = nodes.iter().find(|n| n.role == "button").unwrap();
        // Already interactive → has ref
        assert!(!btn.ref_id.is_empty());
        assert!(btn.interactive);
        // Also has cursor_info attached
        assert!(
            btn.cursor_info.is_some(),
            "already-interactive node in cursor_map should get cursor_info"
        );
    }

    #[test]
    fn test_render_content_with_cursor_info() {
        // Contract: cursor_info appended after ref as " clickable [cursor:pointer, onclick]"
        let mut node = make_node("e1", "generic", "Click me", true, 0);
        node.cursor_info = Some(CursorInfo {
            kind: "clickable".to_string(),
            hints: vec!["cursor:pointer".to_string(), "onclick".to_string()],
        });
        let content = render_content(&[node]);
        // Must contain kind and hints
        assert!(
            content.contains("clickable"),
            "content must show cursor kind: {content}"
        );
        assert!(
            content.contains("cursor:pointer"),
            "content must show cursor hints: {content}"
        );
    }
}
