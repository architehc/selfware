//! Vector index for similarity search
//!
//! HNSW (Hierarchical Navigable Small World) index using `hnsw_rs` for
//! O(log N) approximate nearest-neighbour search.
//!
//! NOTE: This module is not currently registered in the crate module tree.
//! The active `VectorIndex` implementation lives inside `vector_store.rs`.
//! This file is kept in sync for reference.

use anyhow::{anyhow, Result};
use std::collections::HashSet;

/// Health status of a vector index
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexHealth {
    /// Index is consistent: no NaN/Inf, no duplicates, dimensions match
    Healthy,
    /// Index has minor issues (e.g., duplicate IDs) but is still usable
    Degraded,
    /// Index is corrupt (e.g., NaN/Inf values, dimension mismatches) and must be rebuilt
    Corrupt,
}

/// Default ef_search value for HNSW queries.
const HNSW_EF_SEARCH: usize = 50;

/// Vector index backed by HNSW graphs for O(log N) search.
///
/// Deletions are soft (filtered from results); call `compact()` to
/// rebuild the graph and reclaim space.
pub struct VectorIndex {
    /// Embeddings (row-major, L2-normalized at insert time).
    pub(crate) embeddings: Vec<Vec<f32>>,
    /// Chunk IDs corresponding to each embedding slot.
    pub(crate) chunk_ids: Vec<String>,
    /// Expected dimension.
    dimension: usize,
    /// HNSW graph, lazily initialised on the first valid insert.
    hnsw: Option<hnsw_rs::hnsw::Hnsw<'static, f32, hnsw_rs::anndists::dist::DistCosine>>,
    /// Soft-deleted slot indices.
    deleted: HashSet<usize>,
}

impl VectorIndex {
    /// Create new index
    pub fn new(dimension: usize) -> Self {
        Self {
            embeddings: Vec::new(),
            chunk_ids: Vec::new(),
            dimension,
            hnsw: None,
            deleted: HashSet::new(),
        }
    }

    /// Add embedding to index.
    ///
    /// The embedding is L2-normalized at insert time so that cosine similarity
    /// reduces to a simple dot product during search.
    pub fn add(&mut self, chunk_id: String, mut embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(anyhow!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            ));
        }

        Self::l2_normalize(&mut embedding);

        let idx = self.embeddings.len();
        let is_finite = embedding.iter().all(|v| v.is_finite());
        if is_finite {
            let hnsw = self.hnsw.get_or_insert_with(|| {
                hnsw_rs::hnsw::Hnsw::new(
                    16,
                    256,
                    16,
                    200,
                    hnsw_rs::anndists::dist::DistCosine,
                )
            });
            hnsw.insert((&embedding, idx));
        }

        self.embeddings.push(embedding);
        self.chunk_ids.push(chunk_id);
        Ok(())
    }

    /// Remove embedding by chunk ID (soft delete).
    pub fn remove(&mut self, chunk_id: &str) {
        if let Some(pos) = self.chunk_ids.iter().enumerate()
            .filter(|(i, _)| !self.deleted.contains(i))
            .find(|(_, id)| id.as_str() == chunk_id)
            .map(|(i, _)| i)
        {
            self.deleted.insert(pos);
        }
    }

    /// Search for similar embeddings using HNSW.
    ///
    /// Returns `(chunk_id, cosine_similarity)` pairs sorted by
    /// descending similarity.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if query.len() != self.dimension || k == 0 {
            return Vec::new();
        }

        let hnsw = match self.hnsw.as_ref() {
            Some(h) => h,
            None => return Vec::new(),
        };

        let mut normed_query = query.to_vec();
        Self::l2_normalize(&mut normed_query);

        let extra = self.deleted.len().min(k * 2);
        let ef = HNSW_EF_SEARCH.max(k + extra);
        let neighbours = hnsw.search(&normed_query, k + extra, ef);

        let mut results: Vec<(String, f32)> = neighbours
            .into_iter()
            .filter(|n| !self.deleted.contains(&n.d_id))
            .map(|n| {
                let similarity = 1.0 - n.distance;
                (self.chunk_ids[n.d_id].clone(), similarity)
            })
            .take(k)
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Dot product between two vectors.
    #[inline]
    fn dot_product(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// L2-normalize a vector in place.  Zero vectors are left unchanged.
    fn l2_normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// Cosine similarity between two arbitrary vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut na = a.to_vec();
        let mut nb = b.to_vec();
        Self::l2_normalize(&mut na);
        Self::l2_normalize(&mut nb);
        Self::dot_product(&na, &nb)
    }

    /// Get index size (live entries only)
    pub fn len(&self) -> usize {
        self.embeddings.len() - self.deleted.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear index
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.chunk_ids.clear();
        self.deleted.clear();
        self.hnsw = None;
    }

    /// Verify index integrity, returning a list of issues found.
    pub fn verify_index_integrity(&self) -> Vec<String> {
        let mut issues = Vec::new();

        let mut seen_ids = HashSet::new();
        for (i, id) in self.chunk_ids.iter().enumerate() {
            if self.deleted.contains(&i) {
                continue;
            }
            if !seen_ids.insert(id.as_str()) {
                issues.push(format!("Duplicate chunk ID: {}", id));
            }
        }

        for (i, embedding) in self.embeddings.iter().enumerate() {
            if self.deleted.contains(&i) {
                continue;
            }
            let id = self
                .chunk_ids
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("<missing>");

            if embedding.len() != self.dimension {
                issues.push(format!(
                    "Dimension mismatch for '{}': expected {}, got {}",
                    id, self.dimension, embedding.len()
                ));
            }

            if embedding.is_empty() {
                issues.push(format!("Empty embedding vector for '{}'", id));
                continue;
            }

            if embedding.iter().any(|v| v.is_nan()) {
                issues.push(format!("NaN values in embedding for '{}'", id));
            }
            if embedding.iter().any(|v| v.is_infinite()) {
                issues.push(format!("Inf values in embedding for '{}'", id));
            }
        }

        if self.embeddings.len() != self.chunk_ids.len() {
            issues.push(format!(
                "Array length mismatch: {} embeddings vs {} chunk_ids",
                self.embeddings.len(),
                self.chunk_ids.len()
            ));
        }

        issues
    }

    /// Check overall health of the index.
    pub fn check_health(&self) -> IndexHealth {
        let issues = self.verify_index_integrity();
        if issues.is_empty() {
            return IndexHealth::Healthy;
        }

        let has_corrupt = issues.iter().any(|issue| {
            issue.contains("NaN")
                || issue.contains("Inf")
                || issue.contains("Dimension mismatch")
                || issue.contains("Array length mismatch")
                || issue.contains("Empty embedding")
        });

        if has_corrupt {
            IndexHealth::Corrupt
        } else {
            IndexHealth::Degraded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== new() ====================

    #[test]
    fn test_new_is_empty() {
        let index = VectorIndex::new(128);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_new_with_dimension_1() {
        let index = VectorIndex::new(1);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_new_search_returns_empty() {
        let index = VectorIndex::new(4);
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 10);
        assert!(results.is_empty());
    }

    // ==================== add() ====================

    #[test]
    fn test_add_increases_len() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn test_add_dimension_mismatch_returns_error() {
        let mut index = VectorIndex::new(4);
        let result = index.add("a".to_string(), vec![1.0, 0.0, 0.0]); // 3 dims
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dimension mismatch"));
        assert!(err.contains("expected 4"));
        assert!(err.contains("got 3"));
    }

    #[test]
    fn test_add_dimension_mismatch_zero_dim() {
        let mut index = VectorIndex::new(4);
        let result = index.add("a".to_string(), vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_normalizes_embedding() {
        // [3,4,0,0] normalizes to [0.6, 0.8, 0, 0] (norm = 5)
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![3.0, 4.0, 0.0, 0.0])
            .unwrap();
        // Search with the normalized version should yield similarity ~1.0
        let results = index.search(&[0.6, 0.8, 0.0, 0.0], 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "a");
        assert!(
            (results[0].1 - 1.0).abs() < 0.01,
            "Expected similarity ~1.0 for normalized match, got {}",
            results[0].1
        );
    }

    #[test]
    fn test_add_multiple_normalizes_all() {
        let mut index = VectorIndex::new(2);
        // [3,4] => [0.6, 0.8]
        index.add("a".to_string(), vec![3.0, 4.0]).unwrap();
        // [6,8] => [0.6, 0.8] (same direction)
        index.add("b".to_string(), vec![6.0, 8.0]).unwrap();
        // Both normalize to same vector, so searching should find both with sim ~1.0
        let results = index.search(&[3.0, 4.0], 2);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, sim)| (sim - 1.0).abs() < 0.01));
    }

    #[test]
    fn test_add_zero_vector() {
        // Zero vector: l2_normalize leaves it as zero (norm == 0).
        // is_finite is true, so it's inserted into HNSW.
        let mut index = VectorIndex::new(4);
        index
            .add("zero".to_string(), vec![0.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 1);
        // Should not be flagged by integrity check (finite, correct dim)
        let issues = index.verify_index_integrity();
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_add_negative_values() {
        let mut index = VectorIndex::new(3);
        index
            .add("neg".to_string(), vec![-1.0, -2.0, -3.0])
            .unwrap();
        assert_eq!(index.len(), 1);
        let issues = index.verify_index_integrity();
        assert!(issues.is_empty());
    }

    // ==================== remove() ====================

    #[test]
    fn test_remove_decreases_len() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 2);
        index.remove("a");
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_remove_nonexistent_is_noop() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 1);
        index.remove("nonexistent");
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_remove_on_empty_index_is_noop() {
        let mut index = VectorIndex::new(4);
        index.remove("anything");
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_remove_already_removed_is_noop() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index.remove("a");
        assert_eq!(index.len(), 0);
        // Second remove should not panic or change state
        index.remove("a");
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_remove_duplicate_ids_removes_one_at_a_time() {
        // Adding two entries with same ID; remove should remove first active one.
        let mut index = VectorIndex::new(4);
        index
            .add("dup".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("dup".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 2);
        index.remove("dup");
        assert_eq!(index.len(), 1);
        index.remove("dup");
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_remove_then_readd() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index.remove("a");
        assert_eq!(index.len(), 0);
        // Adding back should work and increase len
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 1);
    }

    // ==================== search() ====================

    #[test]
    fn test_search_wrong_dimension_returns_empty() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        let results = index.search(&[1.0, 0.0, 0.0], 5); // 3 dims, not 4
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_k_zero_returns_empty() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_index_returns_empty() {
        let index = VectorIndex::new(8);
        let results = index.search(&[1.0; 8], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_returns_at_most_k() {
        let mut index = VectorIndex::new(4);
        for i in 0..30 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.add(format!("v{}", i), v).unwrap();
        }
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 5);
        assert!(results.len() <= 5);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_exact_match_high_similarity() {
        let mut index = VectorIndex::new(8);
        // Add enough vectors to make HNSW reliable
        for i in 0..20 {
            let mut v = vec![0.0; 8];
            v[i % 8] = 1.0;
            index.add(format!("v{}", i), v).unwrap();
        }
        // Search for exact match of a vector aligned with axis 3
        let mut query = vec![0.0; 8];
        query[3] = 1.0;
        let results = index.search(&query, 3);
        assert!(!results.is_empty());
        // Top result should have very high similarity
        assert!(
            results[0].1 > 0.95,
            "Expected top similarity > 0.95, got {}",
            results[0].1
        );
    }

    #[test]
    fn test_search_results_sorted_descending() {
        let mut index = VectorIndex::new(4);
        for i in 0..30 {
            let mut v = vec![0.0; 4];
            v[0] = 1.0 - (i as f32) * 0.01; // decreasing alignment with [1,0,0,0]
            v[1] = (i as f32) * 0.01;
            index.add(format!("v{}", i), v).unwrap();
        }
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 10);
        assert!(
            results.len() > 1,
            "Expected at least 2 results, got {}",
            results.len()
        );
        for i in 1..results.len() {
            assert!(
                results[i - 1].1 >= results[i].1,
                "Results not sorted descending at index {}: {} >= {}",
                i,
                results[i - 1].1,
                results[i].1
            );
        }
    }

    #[test]
    fn test_search_excludes_deleted_entries() {
        let mut index = VectorIndex::new(8);
        for i in 0..20 {
            let mut v = vec![0.0; 8];
            v[i % 8] = 1.0;
            index.add(format!("v{}", i), v).unwrap();
        }
        // Remove a specific entry and ensure it doesn't appear in results
        index.remove("v3");
        let mut query = vec![0.0; 8];
        query[3] = 1.0;
        let results = index.search(&query, 20);
        assert!(!results.is_empty());
        assert!(
            !results.iter().any(|(id, _)| id == "v3"),
            "Deleted entry 'v3' should not appear in search results"
        );
    }

    #[test]
    fn test_search_k_larger_than_live_count() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        // Request more than available; should return at most the live count
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 100);
        assert!(results.len() <= 2);
    }

    #[test]
    fn test_search_returns_chunk_ids() {
        let mut index = VectorIndex::new(4);
        index
            .add("alpha".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("beta".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        index
            .add("gamma".to_string(), vec![0.9, 0.1, 0.0, 0.0])
            .unwrap();
        index
            .add("delta".to_string(), vec![0.1, 0.9, 0.0, 0.0])
            .unwrap();
        index
            .add("epsilon".to_string(), vec![0.8, 0.2, 0.0, 0.0])
            .unwrap();
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 3);
        assert!(!results.is_empty());
        // The top result for query [1,0,0,0] should be "alpha" (exact match)
        assert_eq!(results[0].0, "alpha");
        // All returned IDs should be valid chunk IDs we inserted
        let valid_ids: std::collections::HashSet<&str> =
            ["alpha", "beta", "gamma", "delta", "epsilon"].iter().copied().collect();
        assert!(results.iter().all(|(id, _)| valid_ids.contains(id.as_str())));
    }

    // ==================== cosine_similarity() ====================

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![1.0, 2.0, 3.0, 4.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "Expected 1.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_scale_invariant() {
        // Cosine similarity is scale-invariant
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![100.0, 0.0, 0.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "Expected 1.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Expected 0.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6, "Expected -1.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_known_value_45_degrees() {
        // cos(45°) = 1/√2 ≈ 0.7071
        // a = [1, 1], b = [1, 0] => angle = 45°
        let a = vec![1.0, 1.0];
        let b = vec![1.0, 0.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!(
            (sim - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5,
            "Expected ~0.7071, got {}",
            sim
        );
    }

    #[test]
    fn test_cosine_similarity_zero_vector_returns_zero() {
        // Zero vector: norm is 0, l2_normalize leaves it unchanged,
        // dot product with anything is 0
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Expected 0.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_both_zero_vectors() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![0.0, 0.0, 0.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Expected 0.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        // The function to_vec's both inputs, so different lengths should not panic.
        // With mismatched lengths, zip stops at the shorter one.
        // a = [1, 0] => normalized [1, 0]
        // b = [1, 0, 0] => normalized [1, 0, 0]
        // dot product = 1*1 + 0*0 = 1.0
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = VectorIndex::cosine_similarity(&a, &b);
        // Both normalize to unit vectors with dot product 1.0
        assert!((sim - 1.0).abs() < 1e-5, "Expected ~1.0, got {}", sim);
    }

    #[test]
    fn test_cosine_similarity_high_dimensional() {
        // Two vectors in 100-dim space with known angle
        let mut a = vec![0.0; 100];
        let mut b = vec![0.0; 100];
        a[0] = 1.0;
        // b is a unit vector with b[0] = 0.5, so cos(angle) = a.b / (|a||b|) = 0.5
        b[0] = 0.5;
        b[1] = 0.75_f32.sqrt(); // sqrt(1 - 0.25) to keep |b| = 1
        let sim = VectorIndex::cosine_similarity(&a, &b);
        assert!((sim - 0.5).abs() < 1e-5, "Expected 0.5, got {}", sim);
    }

    // ==================== len() and is_empty() ====================

    #[test]
    fn test_len_reflects_live_entries() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        index
            .add("c".to_string(), vec![0.0, 0.0, 1.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 3);
        index.remove("b");
        assert_eq!(index.len(), 2);
        index.remove("a");
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_is_empty_transitions() {
        let mut index = VectorIndex::new(4);
        assert!(index.is_empty());
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert!(!index.is_empty());
        index.remove("a");
        assert!(index.is_empty());
    }

    // ==================== clear() ====================

    #[test]
    fn test_clear_empties_index() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert!(!index.is_empty());
        index.clear();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_clear_allows_reuse() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index.clear();
        // Should be able to add and search again after clear
        index
            .add("b".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 1);
        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "b");
    }

    #[test]
    fn test_clear_on_empty_index() {
        let mut index = VectorIndex::new(4);
        index.clear(); // should not panic
        assert!(index.is_empty());
    }

    // ==================== verify_index_integrity() ====================

    #[test]
    fn test_verify_integrity_healthy_index() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        let issues = index.verify_index_integrity();
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_verify_integrity_empty_index() {
        let index = VectorIndex::new(4);
        let issues = index.verify_index_integrity();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_verify_integrity_detects_duplicate_ids() {
        let mut index = VectorIndex::new(4);
        index
            .add("dup".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("dup".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        let issues = index.verify_index_integrity();
        assert_eq!(issues.len(), 1, "Expected 1 issue, got: {:?}", issues);
        assert!(issues[0].contains("Duplicate chunk ID: dup"));
    }

    #[test]
    fn test_verify_integrity_deleted_duplicate_not_flagged() {
        // If one of the duplicate IDs is deleted, it should not be flagged
        let mut index = VectorIndex::new(4);
        index
            .add("dup".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("dup".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        index.remove("dup"); // removes first active one
        // Now only one active "dup" remains
        let issues = index.verify_index_integrity();
        assert!(
            issues.is_empty(),
            "Deleted duplicate should not be flagged, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_verify_integrity_detects_nan() {
        // NaN embedding: l2_normalize leaves NaN (norm=NaN, NaN > 0 is false).
        // is_finite is false so not in HNSW, but stored in embeddings.
        let mut index = VectorIndex::new(2);
        index
            .add("nan_vec".to_string(), vec![f32::NAN, 0.0])
            .unwrap();
        let issues = index.verify_index_integrity();
        assert!(
            issues.iter().any(|i| i.contains("NaN")),
            "Expected NaN issue, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_verify_integrity_detects_inf_as_nan_after_normalization() {
        // Inf embedding: l2_normalize computes norm=Inf, then Inf/Inf=NaN.
        // So after normalization the embedding contains NaN.
        let mut index = VectorIndex::new(2);
        index
            .add("inf_vec".to_string(), vec![f32::INFINITY, 0.0])
            .unwrap();
        let issues = index.verify_index_integrity();
        assert!(
            !issues.is_empty(),
            "Expected issues for Inf embedding, got: {:?}",
            issues
        );
    }

    // ==================== check_health() ====================

    #[test]
    fn test_check_health_healthy() {
        let mut index = VectorIndex::new(4);
        index
            .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.check_health(), IndexHealth::Healthy);
    }

    #[test]
    fn test_check_health_empty_is_healthy() {
        let index = VectorIndex::new(4);
        assert_eq!(index.check_health(), IndexHealth::Healthy);
    }

    #[test]
    fn test_check_health_degraded_for_duplicates() {
        let mut index = VectorIndex::new(4);
        index
            .add("dup".to_string(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .add("dup".to_string(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.check_health(), IndexHealth::Degraded);
    }

    #[test]
    fn test_check_health_corrupt_for_nan() {
        let mut index = VectorIndex::new(2);
        index
            .add("nan_vec".to_string(), vec![f32::NAN, 0.0])
            .unwrap();
        assert_eq!(index.check_health(), IndexHealth::Corrupt);
    }

    #[test]
    fn test_check_health_corrupt_for_inf() {
        let mut index = VectorIndex::new(2);
        index
            .add("inf_vec".to_string(), vec![f32::INFINITY, 0.0])
            .unwrap();
        assert_eq!(index.check_health(), IndexHealth::Corrupt);
    }

    // ==================== IndexHealth enum ====================

    #[test]
    fn test_index_health_equality_and_inequality() {
        assert_eq!(IndexHealth::Healthy, IndexHealth::Healthy);
        assert_eq!(IndexHealth::Degraded, IndexHealth::Degraded);
        assert_eq!(IndexHealth::Corrupt, IndexHealth::Corrupt);
        assert_ne!(IndexHealth::Healthy, IndexHealth::Degraded);
        assert_ne!(IndexHealth::Degraded, IndexHealth::Corrupt);
        assert_ne!(IndexHealth::Healthy, IndexHealth::Corrupt);
    }

    #[test]
    fn test_index_health_copy_and_clone() {
        let h = IndexHealth::Healthy;
        let h_copy = h; // Copy semantics
        assert_eq!(h, h_copy);
        let h_clone = h.clone();
        assert_eq!(h, h_clone);
    }
}
