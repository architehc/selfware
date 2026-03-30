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
