//! EvolutionLoop: re-analyzes the code graph after actions are applied.

use anyhow::Result;

use super::Graph;

/// Outcome of a single evolution loop iteration.
#[derive(Debug)]
pub struct LoopResult {
    pub reanalyzed: bool,
    pub updated_nodes: usize,
}

pub struct EvolutionLoop {
    graph: Graph,
}

impl EvolutionLoop {
    pub fn new(graph: Graph) -> Self {
        Self { graph }
    }

    pub async fn run_once(&mut self) -> Result<LoopResult> {
        // Re-scan src and update graph
        let builder = super::GraphBuilder::new("src");
        self.graph = builder.scan_src()?;
        Ok(LoopResult {
            reanalyzed: true,
            updated_nodes: self.graph.nodes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_evolution_loop_reanalyzes_after_action() {
        let graph = Graph { nodes: vec![], edges: vec![] };
        let mut loop_ = EvolutionLoop::new(graph);
        let result = loop_.run_once().await.unwrap();
        assert!(result.reanalyzed);
    }
}
