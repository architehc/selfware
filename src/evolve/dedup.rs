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

use super::{Graph, NodeLayer};

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
        let default_root = crate::tools::workspace_root::current_path();
        let inferred_root = graph
            .nodes
            .iter()
            .filter(|n| {
                !matches!(
                    n.layer,
                    NodeLayer::Symbol | NodeLayer::Concept | NodeLayer::Preset
                )
            })
            .filter_map(|n| n.path.as_deref().map(Path::new))
            .find(|p| p.is_absolute())
            .and_then(|p| p.parent().map(Path::to_path_buf));
        self.find_duplicates_in(graph, inferred_root.as_deref().unwrap_or(&default_root))
    }

    /// Find duplicate file nodes, resolving repository-relative graph paths
    /// against `project_root`.
    ///
    /// Symbol nodes deliberately mirror their parent file's path. Treating
    /// them as independent files both manufactures file↔symbol duplicates and
    /// turns the near-duplicate pass into an O(symbols²) comparison over the
    /// same file content.
    pub fn find_duplicates_in(
        &self,
        graph: &Graph,
        project_root: &Path,
    ) -> Result<Vec<DuplicatePair>> {
        let mut hashes: HashMap<String, String> = HashMap::new();
        let mut duplicates = Vec::new();
        // (node id, normalized line set) for near-duplicate comparison.
        let mut line_sets: Vec<(String, HashSet<String>)> = Vec::new();

        for node in &graph.nodes {
            if matches!(
                node.layer,
                NodeLayer::Symbol | NodeLayer::Concept | NodeLayer::Preset
            ) {
                continue;
            }
            let Some(ref path) = node.path else {
                continue;
            };
            let raw = Path::new(path);
            let rel = if raw.is_absolute() {
                let Ok(stripped) = raw.strip_prefix(project_root) else {
                    continue;
                };
                stripped
            } else {
                raw
            };
            if rel.as_os_str().is_empty()
                || !rel
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                continue;
            }
            let path = project_root.join(rel);
            let Some(content) = read_content(&path) else {
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
    if crate::safety::source_context::path_contains_private_discovery_state(path) {
        return None;
    }
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    if metadata.is_file() {
        let content = super::graph::read_broad_discovery_utf8(path).ok()?;
        return (!content.is_empty()).then_some(content);
    }
    if metadata.is_dir() {
        let mut files: Vec<_> = walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_entry(super::graph::retain_repository_entry)
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && !e.path_is_symlink()
                    && e.path().extension().is_some_and(|x| x == "rs")
            })
            .map(|e| e.path().to_path_buf())
            .collect();
        files.sort();
        let mut buf = String::new();
        for file in files {
            if let Ok(text) = super::graph::read_broad_discovery_utf8(&file) {
                buf.push_str(&text);
            }
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

#[cfg(test)]
mod tests {
    use super::{DeduplicationAnalyzer, DuplicateKind, DuplicatePair};
    use crate::evolve::{Graph, Node};

    #[cfg(any(unix, windows))]
    #[test]
    fn multiply_linked_discovered_file_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let body = "fn hello() { println!(\"hi\"); }\n";
        let secret = outside.path().join("secret.rs");
        std::fs::write(&secret, body).unwrap();
        let linked = dir.path().join("linked.rs");
        std::fs::hard_link(&secret, &linked).unwrap();
        let normal = dir.path().join("normal.rs");
        let twin = dir.path().join("twin.rs");
        std::fs::write(&normal, body).unwrap();
        std::fs::write(&twin, body).unwrap();

        let graph = Graph {
            nodes: vec![
                Node::code("normal", normal.to_str().unwrap()),
                Node::code("twin", twin.to_str().unwrap()),
                Node::code("linked", linked.to_str().unwrap()),
            ],
            edges: vec![],
        };
        let dupes = DeduplicationAnalyzer::new()
            .find_duplicates_in(&graph, dir.path())
            .unwrap();
        assert_eq!(
            dupes,
            vec![DuplicatePair {
                first: "normal".to_string(),
                second: "twin".to_string(),
                kind: DuplicateKind::Exact,
            }]
        );
    }
}
