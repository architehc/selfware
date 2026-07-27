//! DeduplicationAnalyzer: detects exact and near-duplicate code clusters.
//!
//! Exact duplicates are found by SHA-256 hashing each node's content.
//! Near-duplicates are found by comparing normalized line sets with a
//! Jaccard similarity threshold. Node paths may point at single files or
//! directories (directories hash the concatenation of their `.rs` files in
//! sorted order). Nodes whose path cannot be read or yields empty content
//! are skipped.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::Graph;

/// Minimum Jaccard similarity over normalized line sets for two nodes to
/// be reported as near-duplicates.
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// Whether a duplicate pair shares identical content or is merely similar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKind {
    /// Identical content (same SHA-256 hash).
    Exact,
    /// Similar content (Jaccard similarity above threshold).
    Near,
}

/// A pair of nodes flagged as duplicates, with the kind of match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatePair {
    pub first: String,
    pub second: String,
    pub kind: DuplicateKind,
}

pub struct DeduplicationAnalyzer {}

impl DeduplicationAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn find_duplicates(&self, graph: &Graph) -> Result<Vec<DuplicatePair>> {
        let mut hashes: HashMap<String, String> = HashMap::new();
        let mut duplicates = Vec::new();
        // (node id, normalized line set) for near-duplicate comparison.
        let mut line_sets: Vec<(String, HashSet<String>)> = Vec::new();

        for node in &graph.nodes {
            let Some(ref path) = node.path else {
                continue;
            };
            let Some(content) = read_content(Path::new(path)) else {
                continue;
            };

            let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
            if let Some(existing) = hashes.get(&hash) {
                duplicates.push(DuplicatePair {
                    first: existing.clone(),
                    second: node.id.clone(),
                    kind: DuplicateKind::Exact,
                });
                continue;
            }
            hashes.insert(hash, node.id.clone());

            let lines = normalized_lines(&content);
            for (existing_id, existing_lines) in &line_sets {
                if jaccard(existing_lines, &lines) >= SIMILARITY_THRESHOLD {
                    duplicates.push(DuplicatePair {
                        first: existing_id.clone(),
                        second: node.id.clone(),
                        kind: DuplicateKind::Near,
                    });
                    break;
                }
            }
            line_sets.push((node.id.clone(), lines));
        }

        Ok(duplicates)
    }
}

impl Default for DeduplicationAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a node's content: file contents for a file, the concatenation of
/// its `.rs` files (sorted by path) for a directory. `None` when the path
/// does not exist, cannot be read, or yields no content (an empty buffer
/// would hash identically for any two such nodes, producing false
/// duplicates).
fn read_content(path: &Path) -> Option<String> {
    if path.is_file() {
        let content = std::fs::read_to_string(path).ok()?;
        return (!content.is_empty()).then_some(content);
    }
    if path.is_dir() {
        let mut files: Vec<_> = walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
            .map(|e| e.path().to_path_buf())
            .collect();
        files.sort();
        let mut buf = String::new();
        for file in files {
            buf.push_str(&std::fs::read_to_string(file).unwrap_or_default());
        }
        return (!buf.is_empty()).then_some(buf);
    }
    None
}

/// Whitespace-normalized, non-empty lines as a set.
pub(crate) fn normalized_lines(content: &str) -> HashSet<String> {
    content
        .lines()
        .map(|l| l.split_whitespace().collect::<String>())
        .filter(|l| !l.is_empty())
        .collect()
}

pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    intersection / union
}
