//! ContextEnvelope: the content-addressed bundle of tier-projected documents
//! that the evidence paths ship to the model.
//!
//! One envelope = (mode, graph revision, included ids, projected contents).
//! Preview and outbound responses carry its `content_hash`; equality of the
//! hashes proves they describe the same bytes. Projections reuse the exact
//! machinery TierMeasurer measures with (skeleton / reduce_source / map
//! cards), so measured tier size and shipped size converge.

use sha2::{Digest, Sha256};

use super::context_reduce::reduce_source;
use super::map::{build_map, render_card};
use super::skeleton::extract_rust_skeleton;
use super::{ContextMode, Graph, NodeLayer};

/// One included node's content after tier projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedDocument {
    pub id: String,
    pub path: String,
    pub content: String,
    pub tokens: usize,
}

/// The deterministic, hash-addressed context bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEnvelope {
    pub mode: ContextMode,
    pub graph_revision: String,
    pub included: Vec<String>,
    pub documents: Vec<ProjectedDocument>,
    pub total_tokens: usize,
    pub content_hash: String,
}

/// Build the envelope for a mode + included set, rooted at the current
/// directory (matching how the evolve server runs from the project root).
pub fn build_envelope(
    graph: &Graph,
    mode: &ContextMode,
    included: &[String],
    graph_revision: &str,
    read_source: impl Fn(&str) -> Option<String>,
) -> ContextEnvelope {
    build_envelope_with_root(
        graph,
        mode,
        included,
        graph_revision,
        std::path::Path::new("."),
        read_source,
    )
}

/// Build the envelope for a mode + included set.
///
/// `root` is where the Map projection resolves repo-relative paths (it feeds
/// [`build_map`], which reads sources from disk). `read_source` resolves a
/// repo-relative path to fresh file contents for the other tiers; nodes whose
/// id is unknown or whose source cannot be read are skipped (they contribute
/// no document). `included` echoes the request verbatim so callers can detect
/// drops by comparing lengths.
pub fn build_envelope_with_root(
    graph: &Graph,
    mode: &ContextMode,
    included: &[String],
    graph_revision: &str,
    root: &std::path::Path,
    read_source: impl Fn(&str) -> Option<String>,
) -> ContextEnvelope {
    // Map tier: one rendered card per component, matching build_map's content.
    let map_cards = if matches!(mode, ContextMode::Map) {
        Some(build_map(graph, root))
    } else {
        None
    };

    let mut documents = Vec::new();
    for id in included {
        let Some(node) = graph.nodes.iter().find(|n| &n.id == id) else {
            continue;
        };
        if node.layer != NodeLayer::Code && !matches!(mode, ContextMode::FullExtended) {
            continue;
        }
        let Some(rel) = node.path.as_deref() else {
            continue;
        };
        let content = match mode {
            ContextMode::Map => map_cards
                .as_ref()
                .and_then(|m| m.cards.iter().find(|c| &c.component == id))
                .map(render_card),
            ContextMode::Lite | ContextMode::Custom => read_source(rel)
                .filter(|_| rel.ends_with(".rs"))
                .map(|src| extract_rust_skeleton(std::path::Path::new(rel), &src).render()),
            ContextMode::Compact => read_source(rel).map(|src| reduce_source(&src)),
            // Full/FullExtended/Preset ship verbatim source; cfg(test) exclusion
            // for Full stays in the evidence-chunking stage (unchanged behavior).
            _ => read_source(rel),
        };
        let Some(content) = content else {
            continue;
        };
        documents.push(ProjectedDocument {
            id: id.clone(),
            path: rel.to_string(),
            tokens: crate::token_count::estimate_content_tokens(&content),
            content,
        });
    }

    let total_tokens = documents.iter().map(|d| d.tokens).sum();
    let mut hasher = Sha256::new();
    hasher.update(graph_revision.as_bytes());
    hasher.update(mode.name().as_bytes());
    for id in included {
        hasher.update(id.as_bytes());
    }
    for doc in &documents {
        hasher.update(doc.path.as_bytes());
        hasher.update(doc.content.as_bytes());
    }
    let content_hash = format!("{:x}", hasher.finalize());

    ContextEnvelope {
        mode: mode.clone(),
        graph_revision: graph_revision.to_string(),
        included: included.to_vec(),
        documents,
        total_tokens,
        content_hash,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/envelope_test.rs"]
mod envelope_test;
