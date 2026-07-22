//! Self-evolution context selector.
//!
//! Provides a layered graph view of the codebase, context loading modes,
//! and action execution with git branch isolation.

pub mod actions;
pub mod apply;
pub mod assistant;
pub mod ast;
pub mod context;
pub mod context_selector;
pub mod dead_code;
pub mod dedup;
pub mod deletion;
pub mod fn_dedup;
pub mod diagnostics;
pub mod gate;
pub mod git;
pub mod graph;
pub mod graphrag;
pub mod ide;
pub mod r#loop;
pub mod ontology;
pub mod ontology_evolver;
pub mod persona;
pub mod quality;
pub mod readiness;
pub mod server;
pub mod structure;
pub mod summary;
pub mod xray;

pub use actions::{Action, ActionEngine, ActionResult};
pub use apply::{ApplyRegistry, ApplyRun, ApplyStatus};
pub use ast::{AstAnalyzer, AstNode};
pub use context::{
    ContextComposer, ContextLayerSummary, ContextMode, ContextModeSize, ContextSourceSummary,
    ContextSummary,
};
pub use context_selector::{select as select_context, ContextSelection, SelectedFile, TaskKind};
pub use dead_code::{DeadCodeAnalyzer, DeadSymbol};
pub use dedup::{DeduplicationAnalyzer, DuplicateKind, DuplicatePair};
pub use fn_dedup::{DuplicateFnPair, FnDedupAnalyzer, FnLocation};
pub use gate::{GateResult, Gatekeeper};
pub use graph::GraphBuilder;
pub use graphrag::{GraphRag, GroundedFact};
pub use ide::{DocumentSnapshot, FileClass, FileInfo, IdeEngine, WriteResult};
pub use ontology::{validate_graph, DanglingEdge, OntologyStore, ValidationReport};
pub use ontology_evolver::{OntologyEvolver, OntologyOperation, OntologyProposal, OntologyVersion};
pub use persona::ComponentPersona;
pub use quality::QualityAnalyzer;
pub use r#loop::{EvolutionLoop, LoopResult};
pub use readiness::{GateState, ReadinessGate, ReadinessReport};
pub use server::EvolveServer;
pub use structure::{ClassEntry, FileStructure, Method, StructureAnalyzer};
pub use xray::{ConceptIndex, ConceptRef, ConceptXray, DefinitionSite, RelatedConcept};

use anyhow::Result;

/// Build the evolve graph from `src/`, persist it via the ontology store,
/// and serve it over HTTP.
pub async fn run_self_evolve(port: u16) -> Result<()> {
    run_self_evolve_with_config(port, &crate::config::Config::default()).await
}

/// Run self-evolve with the already resolved CLI configuration so grounded
/// reviews use the same endpoint, model, limits, and protected credential.
pub async fn run_self_evolve_with_config(port: u16, config: &crate::config::Config) -> Result<()> {
    let project_root = std::fs::canonicalize(".")?;
    let builder = GraphBuilder::new(project_root.join("src"));
    let graph = builder.scan_src()?;
    let server = EvolveServer::with_config(graph, &project_root, config)?;
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
    #[serde(rename = "dead_code_annotation_ratio", alias = "dead_code_ratio")]
    pub dead_code_ratio: Option<f64>,
    pub warning_count: Option<usize>,
    pub complexity: Option<f64>,
    #[serde(default)]
    pub inline_test_ranges: usize,
    #[serde(default)]
    pub inline_test_lines: usize,
    #[serde(default)]
    pub inline_test_tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeLayer {
    Code,
    Test,
    Structure,
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
            inline_test_ranges: 0,
            inline_test_lines: 0,
            inline_test_tokens: 0,
        }
    }

    pub fn test(id: &str, path: &str) -> Self {
        let mut node = Self::code(id, path);
        node.layer = NodeLayer::Test;
        node
    }

    pub fn structure(id: &str) -> Self {
        let mut node = Self::code(id, "");
        node.layer = NodeLayer::Structure;
        node.path = None;
        node
    }
}
