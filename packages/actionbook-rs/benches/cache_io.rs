// Cache I/O benchmarks
//
// Tests session state and snapshot cache serialization performance.
// Key metrics: save/load time for different payload sizes.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::{Deserialize, Serialize};
use std::fs;
use tempfile::tempdir;

// Mock SessionState for benchmarking
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

// Benchmark current implementation (string path)
fn bench_save_session_string(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.json");

    let mut group = c.benchmark_group("save_session_state");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        group.bench_with_input(BenchmarkId::new("string", size), size, |b, _| {
            b.iter(|| {
                let content = serde_json::to_string_pretty(&state).unwrap();
                fs::write(&path, content).unwrap();
            });
        });
    }
    group.finish();
}

// Benchmark optimized implementation (bytes path)
fn bench_save_session_bytes(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.json");

    let mut group = c.benchmark_group("save_session_state");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        group.bench_with_input(BenchmarkId::new("bytes", size), size, |b, _| {
            b.iter(|| {
                // FIXED: Use to_vec_pretty for fair comparison
                let bytes = serde_json::to_vec_pretty(&state).unwrap();
                fs::write(&path, bytes).unwrap();
            });
        });
    }
    group.finish();
}

// Benchmark current implementation (load with string)
fn bench_load_session_string(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.json");

    let mut group = c.benchmark_group("load_session_state");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        let content = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&path, &content).unwrap();

        group.bench_with_input(BenchmarkId::new("string", size), size, |b, _| {
            b.iter(|| {
                let content = fs::read_to_string(&path).unwrap();
                let _state: MockSessionState = serde_json::from_str(&content).unwrap();
            });
        });
    }
    group.finish();
}

// Benchmark optimized implementation (load with bytes)
fn bench_load_session_bytes(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.json");

    let mut group = c.benchmark_group("load_session_state");
    for size in [10, 100, 1000].iter() {
        let state = MockSessionState::new(*size);
        // FIXED: Use to_vec_pretty to match string benchmark format
        let bytes = serde_json::to_vec_pretty(&state).unwrap();
        fs::write(&path, &bytes).unwrap();

        group.bench_with_input(BenchmarkId::new("bytes", size), size, |b, _| {
            b.iter(|| {
                let bytes = fs::read(&path).unwrap();
                let _state: MockSessionState = serde_json::from_slice(&bytes).unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(
    cache_benches,
    bench_save_session_string,
    bench_save_session_bytes,
    bench_load_session_string,
    bench_load_session_bytes
);
criterion_main!(cache_benches);
