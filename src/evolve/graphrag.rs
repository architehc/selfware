//! GraphRAG query layer.
//!
//! Grounds recommendations in the actual code graph by answering natural
//! language queries with facts traceable to real files and line ranges.

use anyhow::Result;

use super::Graph;

/// A fact grounded in a real code element, with a verifiable citation.
#[derive(Debug, Clone)]
pub struct GroundedFact {
    pub text: String,
    pub file: String,
    pub line_range: (usize, usize),
    pub source: String,
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
    /// MVP retrieval: nodes whose id or path contains any query term are
    /// returned as grounded facts citing their source path. Semantic/vector
    /// retrieval can replace this later without changing the API.
    pub fn query(&self, query: &str) -> Result<Vec<GroundedFact>> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 3)
            .map(|w| w.to_lowercase())
            .collect();

        let mut facts = Vec::new();
        for node in &self.graph.nodes {
            let id = node.id.to_lowercase();
            let path = node.path.clone().unwrap_or_default().to_lowercase();
            if terms.iter().any(|t| id.contains(t) || path.contains(t)) {
                facts.push(GroundedFact {
                    text: format!(
                        "{}: {} lines, {} tokens, {} files",
                        node.id, node.lines, node.tokens, node.files
                    ),
                    file: node.path.clone().unwrap_or_else(|| node.id.clone()),
                    line_range: (1, node.lines.max(1)),
                    source: "evolve-graph".to_string(),
                });
            }
        }
        Ok(facts)
    }
}
