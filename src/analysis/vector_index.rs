//! Vector index for similarity search
//!
//! Brute-force cosine similarity search using a min-heap for top-k queries.
//! Efficient for collections up to ~100k vectors.

use anyhow::{anyhow, Result};
use std::collections::{BinaryHeap, HashSet};

/// Wrapper around `f32` that implements `Ord` via `total_cmp` for use in
/// `BinaryHeap`. This avoids pulling in an external crate like `ordered-float`.
#[derive(Clone, Copy, PartialEq)]
struct OrdF32(f32);

impl Eq for OrdF32 {}

impl PartialOrd for OrdF32 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrdF32 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

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

/// Vector index using simple brute-force search
///
/// This implementation uses linear scan which is efficient for small collections
/// (< 10,000 vectors). For larger collections, consider HNSW or IVF indexing.
pub struct VectorIndex {
    /// Embeddings matrix (row-major)
    pub(crate) embeddings: Vec<Vec<f32>>,
    /// Chunk IDs corresponding to embeddings
    pub(crate) chunk_ids: Vec<String>,
    /// Dimension
    dimension: usize,
}

impl VectorIndex {
    /// Create new index
    pub fn new(dimension: usize) -> Self {
        Self {
            embeddings: Vec::new(),
            chunk_ids: Vec::new(),
            dimension,
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
        self.embeddings.push(embedding);
        self.chunk_ids.push(chunk_id);
        Ok(())
    }

    /// Remove embedding by chunk ID
    pub fn remove(&mut self, chunk_id: &str) {
        if let Some(pos) = self.chunk_ids.iter().position(|id| id == chunk_id) {
            // Use swap_remove for O(1) removal instead of O(N) shift
            self.embeddings.swap_remove(pos);
            self.chunk_ids.swap_remove(pos);
        }
    }

    /// Search for similar embeddings
    ///
    /// Uses a min-heap to efficiently track the top-k results without
    /// sorting the entire result set. Also applies early termination
    /// when all top-k results have similarity > 0.95.
    ///
    /// Because all stored embeddings are L2-normalized at insert time,
    /// cosine similarity is just the dot product (no per-query sqrt needed).
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if query.len() != self.dimension || k == 0 {
            return Vec::new();
        }

        // Normalize the query vector so dot product == cosine similarity.
        let mut normed_query = query.to_vec();
        Self::l2_normalize(&mut normed_query);

        // Min-heap: stores (OrderedFloat(score), index) so the smallest
        // score is at the top, letting us efficiently evict the worst
        // candidate when a better one is found.
        // We use a wrapper to get Ord on f32 via total_cmp.
        let mut heap: BinaryHeap<std::cmp::Reverse<(OrdF32, usize)>> =
            BinaryHeap::with_capacity(k + 1);
        /// Threshold for early termination: if we have k results all above
        /// this similarity, further searching is unlikely to improve results.
        const EARLY_TERM_THRESHOLD: f32 = 0.95;

        for (i, emb) in self.embeddings.iter().enumerate() {
            // Both vectors are unit-length, so dot product == cosine similarity.
            let score = Self::dot_product(&normed_query, emb);

            if heap.len() < k {
                heap.push(std::cmp::Reverse((OrdF32(score), i)));
            } else if let Some(&std::cmp::Reverse((OrdF32(min_score), _))) = heap.peek() {
                // Only consider this vector if it beats the current k-th best
                if score > min_score {
                    heap.pop();
                    heap.push(std::cmp::Reverse((OrdF32(score), i)));
                }
            }

            // Early termination: if we have k results and the worst is
            // already above the threshold, further search is unlikely
            // to meaningfully improve results.
            if heap.len() == k {
                if let Some(&std::cmp::Reverse((OrdF32(min_score), _))) = heap.peek() {
                    if min_score > EARLY_TERM_THRESHOLD {
                        break;
                    }
                }
            }
        }

        // Extract results sorted by score descending
        let mut results: Vec<(String, f32)> = heap
            .into_iter()
            .map(|std::cmp::Reverse((OrdF32(score), i))| (self.chunk_ids[i].clone(), score))
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
    ///
    /// Normalizes both inputs before computing the dot product.
    /// Kept for external callers and tests; the hot search path uses
    /// `dot_product` on pre-normalized vectors instead.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut na = a.to_vec();
        let mut nb = b.to_vec();
        Self::l2_normalize(&mut na);
        Self::l2_normalize(&mut nb);
        Self::dot_product(&na, &nb)
    }

    /// Get index size
    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    /// Clear index
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.chunk_ids.clear();
    }

    /// Verify index integrity, returning a list of issues found.
    ///
    /// Checks for:
    /// - Mismatched embedding dimensions
    /// - NaN or Inf values in vectors
    /// - Duplicate chunk IDs
    /// - Empty embedding vectors
    pub fn verify_index_integrity(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Check for duplicate IDs
        let mut seen_ids = HashSet::new();
        for id in &self.chunk_ids {
            if !seen_ids.insert(id.as_str()) {
                issues.push(format!("Duplicate chunk ID: {}", id));
            }
        }

        // Check each embedding
        for (i, embedding) in self.embeddings.iter().enumerate() {
            let id = self
                .chunk_ids
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("<missing>");

            // Dimension mismatch
            if embedding.len() != self.dimension {
                issues.push(format!(
                    "Dimension mismatch for '{}': expected {}, got {}",
                    id,
                    self.dimension,
                    embedding.len()
                ));
            }

            // Empty vector
            if embedding.is_empty() {
                issues.push(format!("Empty embedding vector for '{}'", id));
                continue;
            }

            // NaN / Inf values
            let has_nan = embedding.iter().any(|v| v.is_nan());
            let has_inf = embedding.iter().any(|v| v.is_infinite());
            if has_nan {
                issues.push(format!("NaN values in embedding for '{}'", id));
            }
            if has_inf {
                issues.push(format!("Inf values in embedding for '{}'", id));
            }
        }

        // Parallel array length mismatch
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

        // NaN, Inf, dimension mismatch, or array length mismatch => Corrupt
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
            // Only duplicates or other minor issues
            IndexHealth::Degraded
        }
    }
}
