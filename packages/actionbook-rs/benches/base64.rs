// Base64 encoding benchmarks
//
// Tests screenshot/PDF base64 encoding performance.
// Key metrics: encode/decode time for different payload sizes.

use base64::Engine;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

// Generate random binary data for testing
fn generate_binary_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

fn bench_base64_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("base64_encode");

    // Test different sizes: 100KB, 1MB, 5MB (typical screenshot/PDF sizes)
    for size in [100_000, 1_000_000, 5_000_000].iter() {
        let data = generate_binary_data(*size);
        let size_kb = size / 1000;

        group.bench_with_input(BenchmarkId::new("standard", size_kb), size, |b, _| {
            b.iter(|| {
                base64::engine::general_purpose::STANDARD.encode(black_box(&data))
            });
        });
    }

    group.finish();
}

fn bench_base64_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("base64_decode");

    for size in [100_000, 1_000_000, 5_000_000].iter() {
        let data = generate_binary_data(*size);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
        let size_kb = size / 1000;

        group.bench_with_input(BenchmarkId::new("standard", size_kb), size, |b, _| {
            b.iter(|| {
                base64::engine::general_purpose::STANDARD
                    .decode(black_box(&encoded))
                    .unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(encoding_benches, bench_base64_encode, bench_base64_decode);
criterion_main!(encoding_benches);
