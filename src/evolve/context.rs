//! ContextComposer: manages which graph components are in the active context.
//!
//! Explicit loading modes:
//! - `Lite`: nothing loaded (default).
//! - `Full`: all graph nodes loaded.
//! - `FullExtended`: all graph nodes loaded (extended metadata is resolved
//!   downstream; the included set is the same as `Full`).
//! - `Preset(name)`: a single named preset loaded.

use super::Graph;

/// Rough tokens-per-line factor used when a node has no measured token count.
const TOKENS_PER_LINE: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum ContextMode {
    Lite,
    Full,
    FullExtended,
    Preset(String),
}

pub struct ContextComposer {
    graph: Graph,
    #[allow(dead_code)] // Read by downstream tasks (server, personas).
    mode: ContextMode,
    included: Vec<String>,
}

impl ContextComposer {
    pub fn new(graph: Graph) -> Self {
        Self {
            graph,
            mode: ContextMode::Lite,
            included: Vec::new(),
        }
    }

    pub fn set_mode(&mut self, mode: ContextMode) {
        self.mode = mode.clone();
        self.included = match mode {
            ContextMode::Lite => Vec::new(),
            ContextMode::Full => self.graph.nodes.iter().map(|n| n.id.clone()).collect(),
            ContextMode::FullExtended => self.graph.nodes.iter().map(|n| n.id.clone()).collect(),
            ContextMode::Preset(name) => vec![name],
        };
    }

    /// Estimated token cost of the currently included nodes.
    ///
    /// Uses the measured `tokens` count when available; otherwise falls back
    /// to a `lines`-based heuristic (with a minimum of 1 per included node),
    /// so included content never estimates to zero.
    pub fn estimate_tokens(&self) -> usize {
        self.graph
            .nodes
            .iter()
            .filter(|n| self.included.contains(&n.id))
            .map(|n| estimate_node_tokens(n.tokens, n.lines))
            .sum()
    }

    pub fn included_nodes(&self) -> Vec<String> {
        self.included.clone()
    }
}

fn estimate_node_tokens(tokens: usize, lines: usize) -> usize {
    if tokens > 0 {
        tokens
    } else {
        (lines * TOKENS_PER_LINE).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Graph, Node};
    use super::*;

    #[test]
    fn test_context_composer_full_mode_includes_all_code() {
        let graph = Graph {
            nodes: vec![
                Node::code("agent", "src/agent"),
                Node::code("tools", "src/tools"),
            ],
            edges: vec![],
        };
        let mut composer = ContextComposer::new(graph);
        composer.set_mode(ContextMode::Full);
        assert!(composer.estimate_tokens() > 0);
        assert!(composer.included_nodes().len() == 2);
    }

    #[test]
    fn test_modes_and_token_estimation() {
        let mut with_tokens = Node::code("agent", "src/agent");
        with_tokens.tokens = 500;
        let mut with_lines = Node::code("tools", "src/tools");
        with_lines.lines = 20;
        let graph = Graph {
            nodes: vec![with_tokens, with_lines],
            edges: vec![],
        };

        // Lite (default) includes nothing.
        let mut composer = ContextComposer::new(graph);
        assert!(composer.included_nodes().is_empty());
        assert_eq!(composer.estimate_tokens(), 0);

        // Full sums measured tokens with the lines-based fallback (20 * 10).
        composer.set_mode(ContextMode::Full);
        assert_eq!(composer.estimate_tokens(), 700);

        // Preset includes only the named entry.
        composer.set_mode(ContextMode::Preset("agent".to_string()));
        assert_eq!(composer.included_nodes(), vec!["agent".to_string()]);
        assert_eq!(composer.estimate_tokens(), 500);
    }
}
