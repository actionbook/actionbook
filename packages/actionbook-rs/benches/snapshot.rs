// Snapshot (A11y tree) parsing benchmarks
//
// Tests parse_ax_tree performance with different approaches:
// - Current: dynamic Value access
// - Phase 2a: typed envelope
// - Phase 2b: fully typed
//
// Key metrics: parse time for small/medium/large AX trees.

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
            let backend_node_id = node
                .get("backendDOMNodeId")
                .and_then(|v| v.as_i64());
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

// Benchmarks
fn bench_parse_ax_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_ax_tree");

    for size in [10, 100, 500].iter() {
        let ax_tree_value = generate_ax_tree(*size);
        let ax_tree_json = serde_json::to_string(&ax_tree_value).unwrap();

        // Current: Value access
        group.bench_with_input(BenchmarkId::new("value", size), size, |b, _| {
            b.iter(|| parse_ax_tree_value(black_box(&ax_tree_value)));
        });

        // Phase 2a: Envelope
        group.bench_with_input(BenchmarkId::new("envelope", size), size, |b, _| {
            b.iter(|| parse_ax_tree_envelope(black_box(&ax_tree_json)));
        });

        // Phase 2b: Fully typed
        group.bench_with_input(BenchmarkId::new("typed", size), size, |b, _| {
            b.iter(|| parse_ax_tree_typed(black_box(&ax_tree_json)));
        });
    }

    group.finish();
}

criterion_group!(snapshot_benches, bench_parse_ax_tree);
criterion_main!(snapshot_benches);
