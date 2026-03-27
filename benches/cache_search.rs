//! Benchmark for HNSW-based O(log N) cache search vs O(N) brute force
//!
//! This benchmark compares:
//! - HNSW approximate nearest neighbor search: O(log N)
//! - Linear scan (brute force): O(N)

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

// Simulate the old O(N) brute force search
fn brute_force_search(
    embeddings: &[(String, Vec<f32>)],
    query: &[f32],
    threshold: f32,
) -> Option<String> {
    let mut best_match: Option<(String, f32)> = None;

    for (id, entry_embedding) in embeddings.iter() {
        let similarity = cosine_similarity(query, entry_embedding);
        if similarity >= threshold
            && (best_match.is_none() || similarity > best_match.as_ref().unwrap().1)
        {
            best_match = Some((id.clone(), similarity));
        }
    }

    best_match.map(|(id, _)| id)
}

// Cosine similarity calculation (same as in cache.rs)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot_product / (norm_a * norm_b)
    } else {
        0.0
    }
}

// L2 normalize a vector
fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// Generate random normalized embedding
fn random_embedding(dim: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::rng();
    let mut v: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect();
    l2_normalize(&mut v);
    v
}

fn benchmark_cache_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_search");

    // Test with different cache sizes
    let cache_sizes = vec![100, 500, 1000, 5000];
    let dim = 384; // Common embedding dimension
    let threshold = 0.85;

    for size in cache_sizes {
        // Generate test data
        let mut embeddings: Vec<(String, Vec<f32>)> = Vec::with_capacity(size);
        for i in 0..size {
            embeddings.push((format!("entry-{}", i), random_embedding(dim)));
        }

        // Query vector (normalized)
        let mut query = random_embedding(dim);
        l2_normalize(&mut query);

        // Benchmark brute force O(N) search
        group.bench_with_input(BenchmarkId::new("brute_force", size), &size, |b, _| {
            b.iter(|| {
                brute_force_search(
                    black_box(&embeddings),
                    black_box(&query),
                    black_box(threshold),
                )
            });
        });

        // Benchmark HNSW O(log N) search
        // Note: We need to create the HNSW index outside the bench function
        use hnsw_rs::anndists::dist::distances::DistDot;
        use hnsw_rs::hnsw::{Hnsw, Neighbour};

        let hnsw = Hnsw::new(32, size.max(100), 16, 100, DistDot {});
        let mut id_mapping: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();

        // Insert all embeddings into HNSW
        for (i, (id, embedding)) in embeddings.iter().enumerate() {
            let mut normed = embedding.clone();
            l2_normalize(&mut normed);
            hnsw.insert((&normed, i));
            id_mapping.insert(i, id.clone());
        }

        group.bench_with_input(BenchmarkId::new("hnsw_index", size), &size, |b, _| {
            b.iter(|| {
                let normed_query: Vec<f32> = query.clone();
                let neighbors: Vec<Neighbour> = hnsw.search(&normed_query, 1, 50);

                let mut best: Option<String> = None;
                for neighbour in neighbors {
                    let similarity = 1.0 - neighbour.distance;
                    if similarity >= threshold {
                        if let Some(entry_id) = id_mapping.get(&neighbour.d_id) {
                            best = Some(entry_id.clone());
                            break;
                        }
                    }
                }
                best
            });
        });
    }

    group.finish();
}

fn benchmark_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_insertion");

    let dim = 384;
    let sizes = vec![100, 500, 1000];

    for size in sizes {
        // Generate test embeddings
        let embeddings: Vec<Vec<f32>> = (0..size).map(|_| random_embedding(dim)).collect();

        // Benchmark HNSW insertion
        use hnsw_rs::anndists::dist::distances::DistDot;
        use hnsw_rs::hnsw::Hnsw;

        group.bench_with_input(BenchmarkId::new("hnsw_insert", size), &size, |b, _| {
            b.iter(|| {
                let hnsw = Hnsw::new(32, size.max(100), 16, 100, DistDot {});
                for (i, mut emb) in embeddings.clone().into_iter().enumerate() {
                    l2_normalize(&mut emb);
                    hnsw.insert((&emb, i));
                }
            });
        });

        // Benchmark Vec insertion (baseline)
        group.bench_with_input(BenchmarkId::new("vec_insert", size), &size, |b, _| {
            b.iter(|| {
                let mut vec: Vec<(String, Vec<f32>)> = Vec::with_capacity(size);
                for (i, emb) in embeddings.clone().into_iter().enumerate() {
                    vec.push((format!("entry-{}", i), emb));
                }
                black_box(vec);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, benchmark_cache_search, benchmark_insertion);
criterion_main!(benches);
