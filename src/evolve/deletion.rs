//! Non-mutating source-deletion impact previews.
//!
//! This module intentionally has no unlink operation. It creates an immutable
//! action proposal tied to an exact graph revision and file content hash. A
//! later executor can consume that envelope only after complete impact
//! evidence, confirmation, checkpointing, and verification are available.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

use super::ide::IdeEngine;
use super::{Edge, Graph};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionTarget {
    pub logical_node_id: String,
    pub path: String,
    pub content_hash: String,
    pub graph_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactEdge {
    pub direction: String,
    pub from: String,
    pub to: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStep {
    pub order: usize,
    pub command: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionPreview {
    pub action_id: String,
    pub lifecycle: String,
    pub action: String,
    pub target: DeletionTarget,
    pub inbound: Vec<ImpactEdge>,
    pub outbound: Vec<ImpactEdge>,
    pub blockers: Vec<String>,
    pub evidence_completeness: String,
    pub removed_lines: usize,
    pub verification: Vec<VerificationStep>,
    pub executable: bool,
}

pub fn preview_deletion(graph: &Graph, ide: &IdeEngine, node_id: &str) -> Result<DeletionPreview> {
    let node = graph
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .with_context(|| format!("unknown graph node: {node_id}"))?;
    let path = node
        .path
        .as_deref()
        .context("selected node has no source path")?;
    let document = ide.read_graph_document(path)?;
    let graph_revision = graph_revision(graph)?;
    let mut blockers = Vec::new();
    if Path::new(path).extension().is_none_or(|ext| ext != "rs") {
        blockers.push("only exact Rust file nodes can be staged for source deletion".to_string());
    }

    let inbound = graph
        .edges
        .iter()
        .filter(|edge| edge.to == node_id)
        .map(|edge| impact_edge("inbound", edge))
        .collect::<Vec<_>>();
    let outbound = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node_id)
        .map(|edge| impact_edge("outbound", edge))
        .collect::<Vec<_>>();
    if !inbound.is_empty() {
        blockers.push(format!(
            "{} inbound graph relationship(s) require review",
            inbound.len()
        ));
    }
    blockers.push(
        "reference completeness is not yet proven by rust-analyzer or compiler metadata"
            .to_string(),
    );

    Ok(DeletionPreview {
        action_id: Uuid::new_v4().to_string(),
        lifecycle: "proposed".to_string(),
        action: "preview_source_deletion".to_string(),
        target: DeletionTarget {
            logical_node_id: node_id.to_string(),
            path: document.path,
            content_hash: document.hash,
            graph_revision,
        },
        inbound,
        outbound,
        blockers,
        evidence_completeness: "partial".to_string(),
        removed_lines: document.lines,
        verification: vec![
            VerificationStep {
                order: 1,
                command: "cargo check --all-targets".to_string(),
                purpose: "resolve compile-time dependents".to_string(),
            },
            VerificationStep {
                order: 2,
                command: "cargo clippy --all-targets".to_string(),
                purpose: "surface dead imports and secondary diagnostics".to_string(),
            },
            VerificationStep {
                order: 3,
                command: "cargo test --test evolve".to_string(),
                purpose: "verify the self-evolve behavior boundary".to_string(),
            },
        ],
        executable: false,
    })
}

fn impact_edge(direction: &str, edge: &Edge) -> ImpactEdge {
    ImpactEdge {
        direction: direction.to_string(),
        from: edge.from.clone(),
        to: edge.to.clone(),
        edge_type: format!("{:?}", edge.edge_type),
    }
}

fn graph_revision(graph: &Graph) -> Result<String> {
    let bytes = serde_json::to_vec(graph)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn require_executable(preview: &DeletionPreview) -> Result<()> {
    if !preview.executable || !preview.blockers.is_empty() {
        bail!("source deletion is not executable: impact evidence is incomplete");
    }
    Ok(())
}
