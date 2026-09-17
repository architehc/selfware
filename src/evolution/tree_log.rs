//! Durable Attempt Tree Logging
//!
//! Records and indexes evolutionary attempts into a persistent tree structure.
//! Each attempt captures parent-child lineage, semantic branch identification,
//! patch digests, execution metrics, outcome status, and failure classification.
//!
//! This durable log enables offline replay under alternative search policies
//! without rerunning the compiler or evaluation sandboxes (Dream-RSI, arXiv:2609.14858).

use super::FitnessMetrics;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Status of an evolutionary attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    /// Fully evaluated and scored.
    Evaluated,
    /// Failed compilation (`cargo check`).
    CompileFailed,
    /// Failed test suite (`cargo test`).
    TestFailed,
    /// Failed linter (`cargo clippy`).
    ClippyFailed,
    /// Failed code formatting (`cargo fmt`).
    FormatFailed,
    /// Rejected by safety filter (e.g. touches protected paths).
    SafetyRejected,
    /// Patch could not be applied cleanly.
    PatchFailed,
    /// Release build failed.
    BuildFailed,
    /// Execution timed out.
    Timeout,
    /// Unexpected runtime / internal error.
    InternalError,
}

impl std::fmt::Display for AttemptStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evaluated => write!(f, "evaluated"),
            Self::CompileFailed => write!(f, "compile_failed"),
            Self::TestFailed => write!(f, "test_failed"),
            Self::ClippyFailed => write!(f, "clippy_failed"),
            Self::FormatFailed => write!(f, "format_failed"),
            Self::SafetyRejected => write!(f, "safety_rejected"),
            Self::PatchFailed => write!(f, "patch_failed"),
            Self::BuildFailed => write!(f, "build_failed"),
            Self::Timeout => write!(f, "timeout"),
            Self::InternalError => write!(f, "internal_error"),
        }
    }
}

/// Structured failure classification to support trajectory-based search
/// and prevent anti-over-closing (treating repairable implementation slips as fatal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// Repairable syntax or formatting issues (e.g. rustfmt, missing semicolon).
    RepairableSyntax,
    /// Repairable type error or borrow checker mismatch.
    RepairableTypeError,
    /// Repairable test assertion or boundary case mismatch.
    RepairableTestFailure,
    /// Repairable linter / clippy warning.
    RepairableClippy,
    /// Unrecoverable resource exhaustion or timeout.
    UnrecoverableResource,
    /// Safety policy violation (modifying protected files or deleting tests).
    SafetyViolation,
    /// Unrecoverable environment, dependency, or system failure.
    EnvironmentError,
    /// Generic or unclassified failure.
    Unclassified,
}

impl FailureClass {
    /// Whether this failure class represents a repairable implementation slip
    /// rather than a fundamental algorithmic flaw.
    pub fn is_repairable(&self) -> bool {
        matches!(
            self,
            FailureClass::RepairableSyntax
                | FailureClass::RepairableTypeError
                | FailureClass::RepairableTestFailure
                | FailureClass::RepairableClippy
        )
    }
}

/// Compute a SHA256 hex digest for arbitrary bytes.
pub fn compute_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A single node in the evolutionary attempt tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptNode {
    /// Unique attempt identifier.
    pub id: String,
    /// Parent attempt identifier (`None` if this is a root exploration).
    pub parent_id: Option<String>,
    /// Generation index within the evolutionary run.
    pub generation: usize,
    /// Branch identifier grouping related attempts in the same line of inquiry.
    pub branch_id: String,
    /// Identifier of the hypothesis that produced this attempt.
    pub hypothesis_id: String,
    /// Human-readable description of what was attempted.
    pub description: String,
    /// SHA256 hex digest of the tested diff.
    pub diff_sha256: String,
    /// Optional unified diff or patch text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// Path to benchmark report (e.g. SAB), if evaluated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sab_report_path: Option<PathBuf>,
    /// Measured fitness metrics, if evaluation succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<FitnessMetrics>,
    /// Measured composite score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composite_score: Option<f64>,
    /// Tokens consumed by LLM generation/critique, if observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u64>,
    /// Wall-clock evaluation time in milliseconds.
    pub wall_time_ms: u64,
    /// Final status of this attempt.
    pub status: AttemptStatus,
    /// Failure category, if status is not Evaluated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<FailureClass>,
    /// Failure reason or error snippet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// SHA256 of the compiled binary, if built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_sha256: Option<String>,
    /// ISO 8601 / RFC 3339 creation timestamp.
    pub created_at: String,
}

impl AttemptNode {
    /// Returns true if this attempt is an unparented root exploration.
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Returns true if this attempt was successfully evaluated and has a composite score.
    pub fn is_successful_evaluation(&self) -> bool {
        self.status == AttemptStatus::Evaluated && self.composite_score.is_some()
    }

    /// Returns true if this attempt failed but was classified as repairable.
    pub fn is_repairable_failure(&self) -> bool {
        self.failure_class
            .as_ref()
            .is_some_and(|fc| fc.is_repairable())
    }
}

/// Errors occurring during attempt tree logging or loading.
#[derive(Debug, thiserror::Error)]
pub enum TreeLogError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Duplicate attempt id: {0}")]
    DuplicateId(String),
    #[error("Corrupt log line: {0}")]
    CorruptLine(String),
}

/// In-memory indexed collection of evolutionary attempts forming a tree/forest.
#[derive(Debug, Clone, Default)]
pub struct AttemptTree {
    nodes: Vec<AttemptNode>,
    id_to_index: HashMap<String, usize>,
    children_map: HashMap<String, Vec<usize>>,
    branch_map: HashMap<String, Vec<usize>>,
}

impl AttemptTree {
    /// Create an empty attempt tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree contains zero nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// All nodes in order of insertion.
    pub fn nodes(&self) -> &[AttemptNode] {
        &self.nodes
    }

    /// Add a node to the tree. Returns error if the ID already exists.
    pub fn add_node(&mut self, node: AttemptNode) -> Result<(), TreeLogError> {
        if self.id_to_index.contains_key(&node.id) {
            return Err(TreeLogError::DuplicateId(node.id));
        }

        let idx = self.nodes.len();
        let id = node.id.clone();
        let parent_id = node.parent_id.clone();
        let branch_id = node.branch_id.clone();

        self.nodes.push(node);
        self.id_to_index.insert(id, idx);

        if let Some(pid) = parent_id {
            self.children_map.entry(pid).or_default().push(idx);
        }

        self.branch_map.entry(branch_id).or_default().push(idx);
        Ok(())
    }

    /// Look up a node by its unique ID.
    pub fn get(&self, id: &str) -> Option<&AttemptNode> {
        self.id_to_index
            .get(id)
            .and_then(|&idx| self.nodes.get(idx))
    }

    /// Look up a node by internal index.
    pub fn get_by_index(&self, idx: usize) -> Option<&AttemptNode> {
        self.nodes.get(idx)
    }

    /// Get all root attempts (attempts without a parent).
    pub fn roots(&self) -> Vec<&AttemptNode> {
        self.nodes.iter().filter(|n| n.is_root()).collect()
    }

    /// Get all direct children of an attempt.
    pub fn children(&self, parent_id: &str) -> Vec<&AttemptNode> {
        self.children_map
            .get(parent_id)
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|&idx| self.nodes.get(idx))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all attempts in a specific semantic branch.
    pub fn branch_nodes(&self, branch_id: &str) -> Vec<&AttemptNode> {
        self.branch_map
            .get(branch_id)
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|&idx| self.nodes.get(idx))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all unique branch IDs present in the tree.
    pub fn branches(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.branch_map.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Find the node with the highest composite score across the entire tree.
    pub fn best_node(&self) -> Option<&AttemptNode> {
        self.nodes
            .iter()
            .filter(|n| n.composite_score.is_some())
            .max_by(|a, b| {
                a.composite_score
                    .unwrap_or(0.0)
                    .partial_cmp(&b.composite_score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Find the node with the highest composite score within a specific branch.
    pub fn best_in_branch(&self, branch_id: &str) -> Option<&AttemptNode> {
        self.branch_nodes(branch_id)
            .into_iter()
            .filter(|n| n.composite_score.is_some())
            .max_by(|a, b| {
                a.composite_score
                    .unwrap_or(0.0)
                    .partial_cmp(&b.composite_score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Partition the tree into (discovery_tree, held_out_tree) by splitting semantic branches.
    /// Ensures that discovery and held-out validation never share branches or nodes.
    pub fn split_held_out(&self, validation_fraction: f64) -> (Self, Self) {
        let frac = validation_fraction.clamp(0.0, 1.0);
        let branches = self.branches();
        if branches.len() > 1 {
            let val_count =
                ((branches.len() as f64 * frac).round() as usize).clamp(1, branches.len() - 1);
            let split_idx = branches.len() - val_count;
            let disc_branches: std::collections::HashSet<String> =
                branches[..split_idx].iter().cloned().collect();
            let val_branches: std::collections::HashSet<String> =
                branches[split_idx..].iter().cloned().collect();

            let mut disc_tree = Self::new();
            let mut val_tree = Self::new();

            for node in &self.nodes {
                if disc_branches.contains(&node.branch_id) {
                    let _ = disc_tree.add_node(node.clone());
                } else if val_branches.contains(&node.branch_id) {
                    let _ = val_tree.add_node(node.clone());
                }
            }
            (disc_tree, val_tree)
        } else {
            let val_count =
                ((self.nodes.len() as f64 * frac).round() as usize).min(self.nodes.len());
            let split_idx = self.nodes.len().saturating_sub(val_count);

            let mut disc_tree = Self::new();
            let mut val_tree = Self::new();

            for node in &self.nodes[..split_idx] {
                let _ = disc_tree.add_node(node.clone());
            }
            for node in &self.nodes[split_idx..] {
                let _ = val_tree.add_node(node.clone());
            }
            (disc_tree, val_tree)
        }
    }

    /// Load an attempt tree from a JSONL file.
    pub fn load_from_jsonl(path: &Path) -> Result<Self, TreeLogError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut tree = Self::new();

        for (line_num, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let node: AttemptNode = serde_json::from_str(trimmed).map_err(|e| {
                TreeLogError::CorruptLine(format!("line {}: {}: raw: {}", line_num + 1, e, trimmed))
            })?;
            tree.add_node(node)?;
        }

        Ok(tree)
    }

    /// Atomically write the entire tree to a JSONL file via a staging file.
    pub fn save_to_jsonl(&self, target_path: &Path) -> Result<(), TreeLogError> {
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = target_path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        {
            let mut file = File::create(&tmp_path)?;
            for node in &self.nodes {
                let serialized = serde_json::to_string(node)?;
                writeln!(file, "{}", serialized)?;
            }
            file.sync_all()?;
        }

        std::fs::rename(&tmp_path, target_path)?;
        Ok(())
    }

    /// Append a single node to an existing or new JSONL file, syncing data to disk.
    pub fn append_node_to_jsonl(path: &Path, node: &AttemptNode) -> Result<(), TreeLogError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        let serialized = serde_json::to_string(node)?;
        writeln!(file, "{}", serialized)?;
        file.sync_data()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/tree_log/tree_log_test.rs"]
mod tests;
