//! GraphRAG query layer.
//!
//! Grounds recommendations in the actual code graph by answering natural
//! language queries with facts traceable to real files and line ranges.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::Graph;

/// A fact grounded in a real code element, with a verifiable citation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedFact {
    pub text: String,
    pub file: String,
    pub line_range: (usize, usize),
    pub source: String,
    /// Exact, unnumbered source lines covered by `line_range`.
    #[serde(default)]
    pub excerpt: String,
    /// SHA-256 of the complete file bytes used to create this fact.
    #[serde(default)]
    pub content_hash: String,
    /// Stable identity for the file hash and cited range.
    #[serde(default)]
    pub evidence_id: String,
}

/// Query layer over the evolve graph.
pub struct GraphRag {
    graph: Graph,
}

impl GraphRag {
    pub fn new(graph: Graph) -> Self {
        Self { graph }
    }

    /// Answer a natural-language query with facts grounded in graph nodes.
    ///
    /// Nodes whose id or path contains a query term are candidates. A
    /// candidate becomes a fact only when its source is a readable UTF-8 file;
    /// graph-only metadata never substitutes for source evidence.
    pub fn query(&self, query: &str) -> Result<Vec<GroundedFact>> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 3)
            .map(|w| w.to_lowercase())
            .collect();

        let mut facts = Vec::new();
        for node in &self.graph.nodes {
            let id = node.id.to_lowercase();
            let Some(path) = node.path.as_deref() else {
                continue;
            };
            let normalized_path = path.to_lowercase();
            let matched_terms = terms
                .iter()
                .filter(|term| {
                    id.contains(term.as_str()) || normalized_path.contains(term.as_str())
                })
                .cloned()
                .collect::<Vec<_>>();
            if matched_terms.is_empty() {
                continue;
            }

            let Some(snapshot) = SourceSnapshot::read(path) else {
                continue;
            };
            let Some((start_line, end_line, excerpt)) = snapshot.excerpt(&matched_terms) else {
                continue;
            };
            let evidence_id = evidence_id(path, &snapshot.content_hash, start_line, end_line);
            facts.push(GroundedFact {
                text: format!(
                    "{} maps to {} at lines {}-{}",
                    node.id, path, start_line, end_line
                ),
                file: path.to_string(),
                line_range: (start_line, end_line),
                source: "workspace_snapshot".to_string(),
                excerpt,
                content_hash: snapshot.content_hash,
                evidence_id,
            });
        }
        facts.sort_by(|left, right| {
            left.file
                .cmp(&right.file)
                .then_with(|| left.line_range.cmp(&right.line_range))
                .then_with(|| left.evidence_id.cmp(&right.evidence_id))
        });
        Ok(facts)
    }
}

struct SourceSnapshot {
    content: String,
    content_hash: String,
}

impl SourceSnapshot {
    fn read(path: &str) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        if !std::fs::metadata(path).ok()?.is_file() {
            return None;
        }
        let content_hash = hex::encode(Sha256::digest(&bytes));
        let content = String::from_utf8(bytes).ok()?;
        Some(Self {
            content,
            content_hash,
        })
    }

    fn excerpt(&self, matched_terms: &[String]) -> Option<(usize, usize, String)> {
        const CONTEXT_LINES: usize = 2;

        let mut lines = self.content.split('\n').collect::<Vec<_>>();
        if self.content.ends_with('\n') {
            lines.pop();
        }
        if lines.is_empty() {
            return None;
        }

        let matching_line = lines.iter().position(|line| {
            let lowercase = line.to_lowercase();
            matched_terms
                .iter()
                .any(|term| lowercase.contains(term.as_str()))
        });
        let center = matching_line.unwrap_or(0);
        let start = center.saturating_sub(CONTEXT_LINES);
        let end = (center + CONTEXT_LINES + 1).min(lines.len());
        Some((start + 1, end, lines[start..end].join("\n")))
    }
}

fn evidence_id(path: &str, content_hash: &str, start_line: usize, end_line: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"graphrag-evidence\0");
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(content_hash.as_bytes());
    hasher.update(b"\0");
    hasher.update(format!("{start_line}:{end_line}").as_bytes());
    format!("GF-{}", hex::encode(hasher.finalize()))
}
