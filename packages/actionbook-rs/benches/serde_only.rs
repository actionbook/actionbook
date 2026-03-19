// Pure serialization benchmarks (no I/O)
//
// Tests ONLY serde_json serialization/deserialization performance
// without file I/O overhead.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct MockSessionState {
    profile_name: String,
    browser_path: String,
    active_page_id: Option<String>,
    data: Vec<String>,
}

impl MockSessionState {
    fn new(size: usize) -> Self {
        Self {
            profile_name: "default".to_string(),
            browser_path: "/usr/bin/chrome".to_string(),
            active_page_id: Some("page-123".to_string()),
            data: (0..size).map(|i| format!("item-{}", i)).collect(),
        }
    }
}

// Serialize: String path
fn bench_serialize_string(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialize");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        group.bench_with_input(BenchmarkId::new("to_string_pretty", size), size, |b, _| {
            b.iter(|| {
                black_box(serde_json::to_string_pretty(&state).unwrap());
            });
        });
    }
    group.finish();
}

// Serialize: Bytes path
fn bench_serialize_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialize");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        group.bench_with_input(BenchmarkId::new("to_vec_pretty", size), size, |b, _| {
            b.iter(|| {
                black_box(serde_json::to_vec_pretty(&state).unwrap());
            });
        });
    }
    group.finish();
}

// Deserialize: String path
fn bench_deserialize_string(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        let json_str = serde_json::to_string_pretty(&state).unwrap();

        group.bench_with_input(BenchmarkId::new("from_str", size), size, |b, _| {
            b.iter(|| {
                let _state: MockSessionState = serde_json::from_str(black_box(&json_str)).unwrap();
            });
        });
    }
    group.finish();
}

// Deserialize: Bytes path
fn bench_deserialize_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        let json_bytes = serde_json::to_vec_pretty(&state).unwrap();

        group.bench_with_input(BenchmarkId::new("from_slice", size), size, |b, _| {
            b.iter(|| {
                let _state: MockSessionState =
                    serde_json::from_slice(black_box(&json_bytes)).unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(
    serde_benches,
    bench_serialize_string,
    bench_serialize_bytes,
    bench_deserialize_string,
    bench_deserialize_bytes
);
criterion_main!(serde_benches);
