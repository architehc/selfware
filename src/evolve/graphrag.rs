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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evolve::Node;

    fn test_graph() -> Graph {
        let mut agent = Node::code("agent", "src/agent");
        agent.lines = 100;
        agent.tokens = 400;
        agent.files = 3;
        Graph {
            nodes: vec![agent, Node::code("config", "src/config")],
            edges: vec![],
        }
    }

    #[test]
    fn test_graphrag_returns_grounded_facts() {
        let graph = Graph {
            nodes: vec![],
            edges: vec![],
        };
        let rag = GraphRag::new(graph);
        let facts = rag.query("What is the agent module?").unwrap();
        assert!(facts.is_empty()); // no nodes yet
    }

    #[test]
    fn test_graphrag_grounds_matching_nodes() {
        let rag = GraphRag::new(test_graph());
        let facts = rag.query("What does the agent module do?").unwrap();
        // A stub that always returns empty would fail this assertion.
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].file, "src/agent");
        assert!(facts[0].text.contains("agent"));
    }

    #[test]
    fn test_graphrag_no_match_returns_empty() {
        let rag = GraphRag::new(test_graph());
        let facts = rag.query("nonexistent component xyzzy").unwrap();
        assert!(facts.is_empty());
    }
}
