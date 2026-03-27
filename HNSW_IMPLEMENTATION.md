# HNSW Index Implementation for O(log N) Cache Search

## Overview

This implementation replaces the previous O(N) linear scan for semantic similarity search in the LLM cache with an HNSW (Hierarchical Navigable Small World) index providing O(log N) approximate nearest neighbor search.

## Algorithm Choice

**Selected Algorithm: HNSW (Hierarchical Navigable Small World)**

- **Crate**: `hnsw_rs` v0.3.4 (already in dependencies)
- **Distance Metric**: `DistDot` (dot product on L2-normalized vectors)
- **Complexity**: 
  - Search: O(log N) vs previous O(N)
  - Insertion: O(log N)
- **Memory**: O(N * M) where M is max connections per node (default: 32)

### Why HNSW over LSH?

| Factor | HNSW | LSH |
|--------|------|-----|
| Query Time | O(log N) | O(1) to O(N) |
| Build Time | O(N log N) | O(N * L) |
| Memory | Moderate | Low to High |
| Accuracy | High (>95%) | Variable |
| Implementation | Simple | Complex |

HNSW was chosen because:
1. It provides consistent high recall (>95%)
2. Already available in the dependency tree
3. Better for small-to-medium datasets (our use case: <10k entries)
4. No hash function tuning required

## Implementation Details

### Data Structures

```rust
pub struct LlmCache {
    config: LlmCacheConfig,
    entries: RwLock<HashMap<String, LlmCacheEntry>>,
    // HNSW index for O(log N) ANN search
    hnsw_index: RwLock<Hnsw<'static, f32, DistDot>>,
    // Maps HNSW DataId -> entry ID
    id_mapping: RwLock<HashMap<usize, String>>,
    // Maps entry ID -> HNSW DataId (for updates/deletion)
    reverse_id_mapping: RwLock<HashMap<String, usize>>,
    next_data_id: AtomicU64,
}
```

### HNSW Configuration

```rust
const HNSW_MAX_CONNECTIONS: usize = 32;  // M parameter (max neighbors per layer)
const HNSW_EF_CONSTRUCTION: usize = 100; // efConstruction (search depth during build)
const HNSW_EF_SEARCH: usize = 50;        // ef (search depth during query)
```

These parameters are optimized for:
- **Cache workloads**: Frequent lookups, occasional insertions
- **Memory efficiency**: Moderate M value (32)
- **Accuracy**: High efConstruction ensures good graph quality

### Search Algorithm (O(log N))

```rust
pub fn lookup(&self, _prompt: &str, embedding: &[f32], context_hash: u64) -> Option<LlmCacheEntry> {
    // 1. L2-normalize query vector
    let normed_query = Self::l2_normalize_vec(embedding);
    
    // 2. HNSW search: O(log N)
    let neighbors = hnsw_index.search(&normed_query, 5, HNSW_EF_SEARCH);
    
    // 3. Filter by threshold and context_hash
    for neighbour in neighbors {
        let similarity = 1.0 - neighbour.distance; // DistDot returns 1 - dot_product
        if similarity >= threshold {
            // Check context_hash and expiration
            // Return best match
        }
    }
}
```

### Insertion Algorithm (O(log N))

```rust
pub fn store(&self, entry: LlmCacheEntry) {
    let data_id = next_data_id.fetch_add(1, Ordering::SeqCst) as usize;
    
    // 1. L2-normalize embedding
    let normed = Self::l2_normalize_vec(&entry.embedding);
    
    // 2. HNSW insert: O(log N)
    hnsw_index.insert((&normed, data_id));
    
    // 3. Update ID mappings
    id_mapping.insert(data_id, entry.id.clone());
    reverse_id_mapping.insert(entry.id.clone(), data_id);
}
```

### Handling Updates and Deletion

HNSW doesn't support true deletion. Instead, we use:

1. **Soft Deletion**: Remove from `id_mapping`, the point stays in HNSW but is unresolvable
2. **Update**: Generate new DataId, insert new point, remove old mapping
3. **Eviction**: Same as deletion - removes from mappings but not from HNSW graph

This approach is suitable for cache workloads where:
- Entries are relatively stable
- Occasional orphaned HNSW points don't significantly impact accuracy
- The cache size is bounded (default: 500 entries)

## Performance Comparison

### Theoretical Complexity

| Operation | Old (Brute Force) | New (HNSW) | Speedup |
|-----------|-------------------|------------|---------|
| Search | O(N * D) | O(log N * D) | N/log N |
| Insert | O(1) | O(log N) | - |
| Memory | O(N * D) | O(N * D + N * M) | ~1.1x |

Where:
- N = number of cached entries
- D = embedding dimension (typically 384-1536)
- M = max connections (32)

### Empirical Results (Estimated)

Based on HNSW benchmarks with 384-dim vectors:

| Cache Size | Brute Force | HNSW | Speedup |
|------------|-------------|------|---------|
| 100 | 10 μs | 2 μs | 5x |
| 500 | 50 μs | 3 μs | 16x |
| 1,000 | 100 μs | 3.5 μs | 28x |
| 5,000 | 500 μs | 4 μs | 125x |

*Note: Actual benchmarks depend on hardware and data distribution*

## Thread Safety

The implementation uses `RwLock` for all shared state:

- `entries: RwLock<HashMap<...>>` - Cache entries
- `hnsw_index: RwLock<Hnsw<...>>` - HNSW graph
- `id_mapping: RwLock<HashMap<...>>` - ID mappings

This allows:
- Multiple concurrent readers (lookup operations)
- Exclusive writers (insert/delete operations)

## Limitations and Future Work

### Current Limitations

1. **No True Deletion**: HNSW doesn't support point deletion; orphaned points accumulate
2. **Single Graph**: All entries in one HNSW structure (no sharding)
3. **No Persistence**: HNSW index not saved to disk (rebuilt on restart)

### Potential Improvements

1. **Periodic Rebuild**: Rebuild HNSW index after significant deletions
2. **Multi-Index**: Partition by context_hash for faster filtering
3. **Approximate Search**: Trade accuracy for speed with lower ef_search
4. **Persistence**: Serialize HNSW graph to disk for faster restarts

## Testing

Run cache tests:
```bash
cargo test --lib -- cache::tests
```

Run all tests:
```bash
cargo test --lib
```

## References

1. Malkov, Y. A., & Yashunin, D. A. (2018). Efficient and robust approximate nearest neighbor search using Hierarchical Navigable Small World graphs. IEEE transactions on pattern analysis and machine intelligence, 42(4), 824-836.

2. `hnsw_rs` crate: https://docs.rs/hnsw_rs

3. `anndists` crate (distance functions): https://docs.rs/anndists
