//! Self-evolution context selector.
//!
//! Provides a layered graph view of the codebase, context loading modes,
//! and action execution with git branch isolation.

pub mod actions;
// Apply staging rides on shadow worktrees from `evolution::ast_tools`, so it
// exists only with the self-improvement feature; without it the module and
// its server routes are compiled out (keeps no-default-features builds green).
#[cfg(feature = "self-improvement")]
pub mod apply;
pub mod assistant;
pub mod ast;
pub mod clusters;
pub mod context;
pub mod context_fit;
pub mod context_reduce;
pub mod context_selector;
pub mod context_trust;
pub mod dead_code;
pub mod dedup;
pub mod deletion;
pub mod diagnostics;
pub mod envelope;
pub mod expansion;
pub mod fn_dedup;
pub mod gate;
pub mod git;
pub mod graph;
pub mod graph_cache;
pub mod graph_index;
pub mod graphrag;
pub mod ide;
pub mod logical;
pub mod r#loop;
pub mod map;
pub mod module_graph;
pub mod ontology;
pub mod ontology_evolver;
pub mod pair_suggest;
pub mod persona;
pub mod presets;
pub mod quality;
pub mod readiness;
pub mod server;
pub mod skeleton;
pub mod structure;
pub mod summary;
pub mod symbols;
pub mod xray;

pub use actions::{Action, ActionEngine, ActionResult};
#[cfg(feature = "self-improvement")]
pub use apply::{ApplyRegistry, ApplyRun, ApplyStatus};
pub use ast::{AstAnalyzer, AstNode};
pub use clusters::{cluster_of, clustered, ClusteredGraph};
pub use context::{
    ContextComposer, ContextLayerSummary, ContextMode, ContextModeSize, ContextSourceSummary,
    ContextSummary,
};
pub use context_fit::{fit_tier, FitBudget, FitOutcome, RequestedMode, TierMeasurer, TIER_LADDER};
pub use context_reduce::{dedup_context, reduce_source, strip_cfg_test_blocks, strip_comments};
pub use context_selector::{select as select_context, ContextSelection, SelectedFile, TaskKind};
pub use context_trust::{
    analyze_source, scan_injection, GateDecision, InjectionFinding, SourceKind, TrustLevel,
    TrustReport,
};
pub use dead_code::{DeadCodeAnalyzer, DeadSymbol};
pub use dedup::{DeduplicationAnalyzer, DuplicateKind, DuplicatePair};
pub use envelope::{build_envelope, ContextEnvelope, ProjectedDocument};
pub use fn_dedup::{DuplicateFnPair, FnDedupAnalyzer, FnLocation};
pub use gate::{GateResult, Gatekeeper};
pub use graph::GraphBuilder;
pub use graph_cache::shared_graph_index;
pub use graph_index::{GraphIndex, Metric};
pub use graphrag::{GraphRag, GroundedFact};
pub use ide::{DocumentSnapshot, FileClass, FileInfo, IdeEngine, WriteResult};
pub use logical::{build_logical_model, Capability, LogicalEdge, LogicalModel};
pub use map::{
    build_map, expand as expand_component, orientation as workspace_orientation, ComponentCard,
    ContextMap,
};
pub use module_graph::{
    from_lib_rs as parse_module_manifest, module_path, ModuleDecl, ModuleManifest, ReExport,
};
pub use ontology::{validate_graph, DanglingEdge, OntologyStore, ValidationReport};
pub use ontology_evolver::{OntologyEvolver, OntologyOperation, OntologyProposal, OntologyVersion};
pub use pair_suggest::{
    connected_pairs, pair_context, suggest_prompt, ComponentPair, SUGGEST_SYSTEM,
};
pub use persona::ComponentPersona;
pub use presets::{preset, presets, render_prompt as render_preset_prompt, Preset};
pub use quality::QualityAnalyzer;
pub use r#loop::{EvolutionLoop, LoopResult};
pub use readiness::{GateState, ReadinessGate, ReadinessReport};
pub use server::EvolveServer;
pub use skeleton::{extract_rust_skeleton, FileSkeleton, SkeletonItem};
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
    /// Fine-grained content class for perspectives and honest token accounting:
    /// `rust_source`, `python_source`, `javascript_source`, `typescript_source`,
    /// `go_source`, `test`, `data`, `config`, `script`, `markup`, `vendored`,
    /// `generated`, or `other`. Defaults to `rust_source` for older graphs.
    #[serde(default = "default_classification")]
    pub classification: String,
    /// Symbol-level fields — present only on symbol nodes (schema v2).
    /// `Option` + serde defaults keep older YAMLs (file-level only) loading
    /// unchanged; no schema bump is needed because deserialization fills
    /// `None` when the fields are absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_kind: Option<String>,
    /// Id of the parent file node for symbol nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 1-based inclusive (start, end) source lines for symbol nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_range: Option<(usize, usize)>,
}

fn default_classification() -> String {
    "rust_source".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeLayer {
    Code,
    Test,
    Structure,
    Concept,
    Preset,
    /// Non-source repository files (data, config, scripts, vendored, generated).
    /// Excluded from code token tiers so counts reflect real source.
    Auxiliary,
    /// Function/type-level nodes inside a Rust source file (schema v2).
    /// Kept out of file-layer rollups (hotspots, clusters, context tiers) so
    /// file-level queries are unchanged unless a caller asks for symbols.
    Symbol,
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
            classification: default_classification(),
            symbol_kind: None,
            parent_id: None,
            line_range: None,
        }
    }

    pub fn test(id: &str, path: &str) -> Self {
        let mut node = Self::code(id, path);
        node.layer = NodeLayer::Test;
        node.classification = "test".to_string();
        node
    }

    /// A non-source repository file (data, config, script, vendored, generated).
    pub fn auxiliary(id: &str, path: &str, classification: &str) -> Self {
        let mut node = Self::code(id, path);
        node.layer = NodeLayer::Auxiliary;
        node.classification = classification.to_string();
        node
    }

    pub fn structure(id: &str) -> Self {
        let mut node = Self::code(id, "");
        node.layer = NodeLayer::Structure;
        node.path = None;
        node.classification = "structure".to_string();
        node
    }

    /// A symbol-level node inside a Rust source file (schema v2).
    /// `id` is `<parent_id>::<name>`; `path` mirrors the parent file so
    /// path-based lookups (expand, mirror rules) keep working.
    pub fn symbol(
        id: &str,
        parent_id: &str,
        kind: &str,
        path: &str,
        line_range: (usize, usize),
        tokens: usize,
    ) -> Self {
        let mut node = Self::code(id, path);
        node.layer = NodeLayer::Symbol;
        node.classification = "symbol".to_string();
        node.symbol_kind = Some(kind.to_string());
        node.parent_id = Some(parent_id.to_string());
        node.line_range = Some(line_range);
        node.tokens = tokens;
        node.lines = line_range.1.saturating_sub(line_range.0) + 1;
        node
    }
}
