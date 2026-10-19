//! Code analysis module
//!
//! This module contains code analysis and search functionality including:
//! - Code graph visualization
//! - Error analysis
//! - BM25 search
//! - Vector storage
//! - Technical debt tracking
//! - Deterministic repository inventory for reviews

pub mod analyzer;
pub mod bm25;
pub mod code_graph;
pub mod repo_inventory;
pub mod tech_debt;
pub mod vector_store;
pub mod workspace_graph;

pub use analyzer::{AnalyzedError, ErrorAnalyzer, ErrorCategory, FixSuggestion, RawError};
pub use bm25::{BM25Index, BM25Result};
pub use code_graph::{
    CodeGraph, EdgeType as CodeEdgeType, GraphEdge, GraphNode, GraphRenderer, NodeType,
    OutputFormat,
};
pub use repo_inventory::{RepoInventory, CENTRALITY_METRIC};
pub use tech_debt::{DebtItem, DebtSeverity, DebtTracker, DebtType};
pub use vector_store::{SearchResult as VectorSearchResult, VectorStore};
pub use workspace_graph::WorkspaceGraphOptions;
