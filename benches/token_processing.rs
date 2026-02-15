//! Token processing benchmarks
//!
//! Measures performance of token counting and streaming operations.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Simulate token counting for a string
fn count_tokens(text: &str) -> usize {
    // Simple whitespace tokenization for benchmarking
    // Real implementation would use tiktoken or similar
    text.split_whitespace().count()
}

/// Simulate processing a batch of tokens
fn process_token_batch(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|t| t.to_uppercase()).collect()
}

fn token_counting_benchmark(c: &mut Criterion) {
    let short_text = "Hello, world! This is a test.";
    let medium_text = "The quick brown fox jumps over the lazy dog. ".repeat(100);
    let long_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(1000);

    let mut group = c.benchmark_group("token_counting");

    group.bench_with_input(
        BenchmarkId::new("short", short_text.len()),
        &short_text,
        |b, text| b.iter(|| count_tokens(black_box(text))),
    );

    group.bench_with_input(
        BenchmarkId::new("medium", medium_text.len()),
        &medium_text,
        |b, text| b.iter(|| count_tokens(black_box(text))),
    );

    group.bench_with_input(
        BenchmarkId::new("long", long_text.len()),
        &long_text,
        |b, text| b.iter(|| count_tokens(black_box(text))),
    );

    group.finish();
}

fn token_batch_benchmark(c: &mut Criterion) {
    let small_batch: Vec<&str> = (0..10).map(|_| "token").collect();
    let medium_batch: Vec<&str> = (0..100).map(|_| "token").collect();
    let large_batch: Vec<&str> = (0..1000).map(|_| "token").collect();

    let mut group = c.benchmark_group("token_batch");

    group.bench_with_input(
        BenchmarkId::new("small", small_batch.len()),
        &small_batch,
        |b, batch| b.iter(|| process_token_batch(black_box(batch))),
    );

    group.bench_with_input(
        BenchmarkId::new("medium", medium_batch.len()),
        &medium_batch,
        |b, batch| b.iter(|| process_token_batch(black_box(batch))),
    );

    group.bench_with_input(
        BenchmarkId::new("large", large_batch.len()),
        &large_batch,
        |b, batch| b.iter(|| process_token_batch(black_box(batch))),
    );

    group.finish();
}

criterion_group!(benches, token_counting_benchmark, token_batch_benchmark);
criterion_main!(benches);
