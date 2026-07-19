//! Self-evolution context selector.
//!
//! Provides a layered graph view of the codebase, context loading modes,
//! and action execution with git branch isolation.

pub mod actions;
pub mod ast;
pub mod context;
pub mod dedup;
pub mod gate;
pub mod graph;
pub mod graphrag;
pub mod ide;
pub mod r#loop;
pub mod ontology;
pub mod ontology_evolver;
pub mod persona;
pub mod quality;
pub mod server;

pub use actions::{Action, ActionEngine, ActionResult};
pub use ast::{AstAnalyzer, AstNode};
pub use context::{ContextComposer, ContextMode};
pub use dedup::{DeduplicationAnalyzer, DuplicateKind, DuplicatePair};
pub use gate::{GateResult, Gatekeeper};
pub use graph::GraphBuilder;
pub use graphrag::{GraphRag, GroundedFact};
pub use ide::{FileInfo, IdeEngine};
pub use r#loop::{EvolutionLoop, LoopResult};
pub use ontology::{validate_graph, DanglingEdge, OntologyStore, ValidationReport};
pub use ontology_evolver::{OntologyEvolver, OntologyOperation, OntologyProposal, OntologyVersion};
pub use persona::ComponentPersona;
pub use quality::QualityAnalyzer;
pub use server::EvolveServer;

use anyhow::Result;

/// Build the evolve graph from `src/`, persist it via the ontology store,
/// and serve it over HTTP.
pub async fn run_self_evolve(port: u16) -> Result<()> {
    let builder = GraphBuilder::new("src");
    let graph = builder.scan_src()?;
    let server = EvolveServer::new(graph);
    server.save_graph()?;
    server.start(port).await
}

/// A layered graph of code components and concept clusters.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub id: String,
    pub layer: NodeLayer,
    pub path: Option<String>,
    pub tokens: usize,
    pub lines: usize,
    pub files: usize,
    pub coverage: Option<f64>,
    pub dead_code_ratio: Option<f64>,
    pub warning_count: Option<usize>,
    pub complexity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NodeLayer {
    Code,
    Concept,
    Preset,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EdgeType {
    Contains,
    DependsOn,
    Influences,
    Feedback,
    ContextIncluded,
    DuplicateOf,
    SimilarTo,
}

impl Node {
    pub fn code(id: &str, path: &str) -> Self {
        Self {
            id: id.to_string(),
            layer: NodeLayer::Code,
            path: Some(path.to_string()),
            tokens: 0,
            lines: 0,
            files: 0,
            coverage: None,
            dead_code_ratio: None,
            warning_count: None,
            complexity: None,
        }
    }
}
