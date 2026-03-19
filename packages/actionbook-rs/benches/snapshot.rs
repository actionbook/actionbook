// Snapshot (A11y tree) parsing benchmarks
//
// Tests parse_ax_tree performance comparing:
// - BEFORE Phase 2b: dynamic Value access
// - AFTER Phase 2b: typed deserialization
//
// Key metrics: parse time for small/medium/large AX trees.

use actionbook::browser::snapshot::{parse_ax_tree, SnapshotFilter};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Simplified A11yNode for benchmarking
#[derive(Debug, Clone, Serialize, Deserialize)]
struct A11yNode {
    node_id: String,
    backend_node_id: Option<i64>,
    role: String,
    name: String,
    child_ids: Vec<String>,
}

// Generate synthetic AX tree for testing
fn generate_ax_tree(node_count: usize) -> Value {
    let nodes: Vec<Value> = (0..node_count)
        .map(|i| {
            serde_json::json!({
                "nodeId": format!("node-{}", i),
                "backendDOMNodeId": i as i64,
                "role": {"value": if i % 3 == 0 { "button" } else { "text" }},
                "name": {"value": format!("Element {}", i)},
                "childIds": if i < node_count - 1 {
                    vec![format!("node-{}", i + 1)]
                } else {
                    vec![]
                }
            })
        })
        .collect();

    serde_json::json!({ "nodes": nodes })
}

// Current pattern: dynamic Value access
fn parse_ax_tree_value(ax_tree: &Value) -> Vec<A11yNode> {
    let empty = vec![];
    let nodes = ax_tree
        .get("nodes")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    nodes
        .iter()
        .filter_map(|node| {
            let node_id = node.get("nodeId")?.as_str()?.to_string();
            let backend_node_id = node.get("backendDOMNodeId").and_then(|v| v.as_i64());
            let role = node
                .get("role")
                .and_then(|r| r.get("value"))?
                .as_str()?
                .to_string();
            let name = node
                .get("name")
                .and_then(|n| n.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let child_ids = node
                .get("childIds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Some(A11yNode {
                node_id,
                backend_node_id,
                role,
                name,
                child_ids,
            })
        })
        .collect()
}

// Phase 2a: Typed envelope (outer structure only)
#[derive(Deserialize)]
struct AxTreeResponseEnvelope {
    nodes: Vec<Value>, // Inner still Value
}

fn parse_ax_tree_envelope(json: &str) -> Vec<A11yNode> {
    let response: AxTreeResponseEnvelope = serde_json::from_str(json).unwrap();
    response
        .nodes
        .iter()
        .filter_map(|node| {
            let node_id = node.get("nodeId")?.as_str()?.to_string();
            let backend_node_id = node.get("backendDOMNodeId").and_then(|v| v.as_i64());
            let role = node
                .get("role")
                .and_then(|r| r.get("value"))?
                .as_str()?
                .to_string();
            let name = node
                .get("name")
                .and_then(|n| n.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let child_ids = node
                .get("childIds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Some(A11yNode {
                node_id,
                backend_node_id,
                role,
                name,
                child_ids,
            })
        })
        .collect()
}

// Phase 2b: Fully typed
#[derive(Deserialize)]
struct AxTreeResponseTyped {
    nodes: Vec<AxNodeRaw>,
}

#[derive(Deserialize)]
struct AxNodeRaw {
    #[serde(rename = "nodeId")]
    node_id: String,
    #[serde(rename = "backendDOMNodeId")]
    backend_node_id: Option<i64>,
    role: RoleValue,
    name: Option<NameValue>,
    #[serde(rename = "childIds", default)]
    child_ids: Vec<String>,
}

#[derive(Deserialize)]
struct RoleValue {
    value: String,
}

#[derive(Deserialize)]
struct NameValue {
    value: String,
}

fn parse_ax_tree_typed(json: &str) -> Vec<A11yNode> {
    let response: AxTreeResponseTyped = serde_json::from_str(json).unwrap();
    response
        .nodes
        .iter()
        .map(|node| A11yNode {
            node_id: node.node_id.clone(),
            backend_node_id: node.backend_node_id,
            role: node.role.value.clone(),
            name: node
                .name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_default(),
            child_ids: node.child_ids.clone(),
        })
        .collect()
}

// OLD Value-based implementation with FULL filtering logic (for fair comparison)
// This replicates the pre-Phase-2b parse_ax_tree implementation
fn parse_ax_tree_value_full(json: &str) -> Vec<A11yNode> {
    use std::collections::HashMap;

    // Parse to Value first (old approach)
    let ax_tree: Value = serde_json::from_str(json).unwrap();
    let empty = vec![];
    let nodes = ax_tree
        .get("nodes")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    // Build parent map for depth calculation (same as real implementation)
    let mut parent_map: HashMap<String, String> = HashMap::new();
    let mut child_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut node_map: HashMap<String, &Value> = HashMap::new();

    for node in nodes {
        let node_id = match node.get("nodeId").and_then(|n| n.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };

        if node_id.is_empty() {
            continue;
        }

        node_map.insert(node_id.clone(), node);

        if let Some(child_ids) = node.get("childIds").and_then(|c| c.as_array()) {
            let children: Vec<String> = child_ids
                .iter()
                .filter_map(|c| c.as_str().map(|s| s.to_string()))
                .collect();

            if !children.is_empty() {
                for cid in &children {
                    parent_map.insert(cid.clone(), node_id.clone());
                }
                child_map.insert(node_id.clone(), children);
            }
        }
    }

    // Depth calculation function (same as real implementation)
    fn get_depth(
        node_id: &str,
        parent_map: &HashMap<String, String>,
        cache: &mut HashMap<String, usize>,
    ) -> usize {
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

    // Parse with filtering (same as real implementation, no scope filter for benchmark)
    nodes
        .iter()
        .filter_map(|node| {
            // Skip ignored nodes
            if node
                .get("ignored")
                .and_then(|i| i.as_bool())
                .unwrap_or(false)
            {
                return None;
            }

            let node_id_str = node.get("nodeId")?.as_str()?;

            // OLD pattern: Value access with get()
            let role = node
                .get("role")
                .and_then(|r| r.get("value"))?
                .as_str()?
                .to_string();

            let name = node
                .get("name")
                .and_then(|n| n.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let backend_node_id = node.get("backendDOMNodeId").and_then(|v| v.as_i64());

            let child_ids = node
                .get("childIds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Some(A11yNode {
                node_id: node_id_str.to_string(),
                backend_node_id,
                role,
                name,
                child_ids,
            })
        })
        .collect()
}

// Benchmarks
fn bench_parse_ax_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_ax_tree");

    for size in [10, 100, 500].iter() {
        let ax_tree_value = generate_ax_tree(*size);
        let ax_tree_json = serde_json::to_string(&ax_tree_value).unwrap();

        // Current: Value access (FAIR: includes JSON parsing)
        group.bench_with_input(BenchmarkId::new("value_fair", size), size, |b, _| {
            b.iter(|| {
                let ax_tree: Value = serde_json::from_str(black_box(&ax_tree_json)).unwrap();
                parse_ax_tree_value(&ax_tree)
            });
        });

        // Current: Value access (UNFAIR: pre-parsed, for comparison)
        group.bench_with_input(BenchmarkId::new("value_unfair", size), size, |b, _| {
            b.iter(|| parse_ax_tree_value(black_box(&ax_tree_value)));
        });

        // Phase 2a: Envelope
        group.bench_with_input(BenchmarkId::new("envelope", size), size, |b, _| {
            b.iter(|| parse_ax_tree_envelope(black_box(&ax_tree_json)));
        });

        // Phase 2b: Fully typed (local implementation)
        group.bench_with_input(BenchmarkId::new("typed_local", size), size, |b, _| {
            b.iter(|| parse_ax_tree_typed(black_box(&ax_tree_json)));
        });

        // OLD: Value-based with full filtering logic (BEFORE Phase 2b)
        group.bench_with_input(BenchmarkId::new("real_value", size), size, |b, _| {
            b.iter(|| parse_ax_tree_value_full(black_box(&ax_tree_json)));
        });

        // Phase 2b: Real implementation (AFTER optimization) — accepts Value directly
        group.bench_with_input(BenchmarkId::new("real_typed", size), size, |b, _| {
            b.iter(|| {
                let value: serde_json::Value =
                    serde_json::from_str(black_box(&ax_tree_json)).unwrap();
                parse_ax_tree(black_box(value), SnapshotFilter::All, None, None).unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(snapshot_benches, bench_parse_ax_tree);
criterion_main!(snapshot_benches);
