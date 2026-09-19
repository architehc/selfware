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
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Status of an evolutionary attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    /// Baseline capability measurement.
    Baseline,
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
    /// Rejected upfront as duplicate of previously failed patch diff.
    DuplicateRejected,
    /// Unexpected runtime / internal error.
    InternalError,
}

impl std::fmt::Display for AttemptStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Baseline => write!(f, "baseline"),
            Self::Evaluated => write!(f, "evaluated"),
            Self::CompileFailed => write!(f, "compile_failed"),
            Self::TestFailed => write!(f, "test_failed"),
            Self::ClippyFailed => write!(f, "clippy_failed"),
            Self::FormatFailed => write!(f, "format_failed"),
            Self::SafetyRejected => write!(f, "safety_rejected"),
            Self::PatchFailed => write!(f, "patch_failed"),
            Self::BuildFailed => write!(f, "build_failed"),
            Self::Timeout => write!(f, "timeout"),
            Self::DuplicateRejected => write!(f, "duplicate_rejected"),
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

/// Extracts the last `n` lines of a string, useful for preserving diagnostic
/// tails without exploding serialized log sizes.
pub fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        text.to_string()
    } else {
        lines[lines.len() - n..].join("\n")
    }
}

/// The exploration action type that produced an attempt node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Open a root exploration candidate (rooted at baseline or active promoted incumbent).
    OpenRoot,
    /// Refine an existing attempt frontier.
    RefineFrontier,
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenRoot => write!(f, "open_root"),
            Self::RefineFrontier => write!(f, "refine_frontier"),
        }
    }
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
    /// Last N lines of execution output (stdout/stderr) for post-mortem analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<String>,
    /// SHA256 of the compiled binary, if built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_sha256: Option<String>,
    /// Base git commit hash when this attempt was made or detached from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    /// Git commit hash created if this attempt was successfully promoted and committed to repo_root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_commit: Option<String>,
    /// Exploration action type that produced this attempt (OpenRoot vs RefineFrontier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_type: Option<ActionType>,
    /// ISO 8601 / RFC 3339 creation timestamp.
    pub created_at: String,
}

impl AttemptNode {
    /// Returns true if this attempt is an unparented root exploration or an OpenRoot action.
    pub fn is_root(&self) -> bool {
        self.action_type == Some(ActionType::OpenRoot) || self.parent_id.is_none()
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
    #[error("Insufficient independent branches for held-out validation: {0} branch(es) found (minimum 2 required)")]
    InsufficientBranchesForHeldOut(usize),
    #[error("Tree has orphaned node '{node_id}' referencing missing parent '{parent_id}'")]
    OrphanedNode { node_id: String, parent_id: String },
    #[error("Insufficient independent ancestry groups for held-out validation: {0} group(s) found (minimum 2 required)")]
    InsufficientAncestryGroupsForHeldOut(usize),
    #[error("Attempts log file not found: {0}")]
    LogFileNotFound(String),
    #[error("Node '{0}' not found in attempts log")]
    NodeNotFound(String),
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

    /// Check if a node with `id` exists in the tree.
    pub fn has_node(&self, id: &str) -> bool {
        self.id_to_index.contains_key(id)
    }

    /// Validates that every non-root node in the tree has its parent present in the tree.
    /// Fails closed if any node is orphaned.
    pub fn validate_ancestry(&self) -> Result<(), TreeLogError> {
        for node in &self.nodes {
            if let Some(ref pid) = node.parent_id {
                if !self.has_node(pid) {
                    return Err(TreeLogError::OrphanedNode {
                        node_id: node.id.clone(),
                        parent_id: pid.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Partitions all nodes into disjoint connected components (independent ancestry groups)
    /// based on parent-child edges. Whole lineages stay strictly together.
    pub fn ancestry_groups(&self) -> Vec<Vec<String>> {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for node in &self.nodes {
            adj.entry(node.id.clone()).or_default();
            if let Some(ref pid) = node.parent_id {
                if self.has_node(pid) {
                    adj.entry(node.id.clone()).or_default().push(pid.clone());
                    adj.entry(pid.clone()).or_default().push(node.id.clone());
                }
            }
        }

        let mut visited = HashSet::new();
        let mut groups = Vec::new();

        for node in &self.nodes {
            if visited.insert(node.id.clone()) {
                let mut component = Vec::new();
                let mut queue = std::collections::VecDeque::new();
                queue.push_back(node.id.clone());
                component.push(node.id.clone());

                while let Some(curr) = queue.pop_front() {
                    if let Some(neighbors) = adj.get(&curr) {
                        for neighbor in neighbors {
                            if visited.insert(neighbor.clone()) {
                                component.push(neighbor.clone());
                                queue.push_back(neighbor.clone());
                            }
                        }
                    }
                }
                groups.push(component);
            }
        }

        groups
    }

    /// Computes the set of all node IDs reachable by traversing downwards from roots.
    pub fn reachable_node_ids(&self) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        for root in self.roots() {
            reachable.insert(root.id.clone());
            queue.push_back(root.id.clone());
        }

        while let Some(curr) = queue.pop_front() {
            if let Some(children_indices) = self.children_map.get(&curr) {
                for &child_idx in children_indices {
                    if let Some(child) = self.nodes.get(child_idx) {
                        if reachable.insert(child.id.clone()) {
                            queue.push_back(child.id.clone());
                        }
                    }
                }
            }
        }
        reachable
    }

    /// Traces a node up to the first child below common_root (the lineage root branch).
    fn find_lineage_root_branch(&self, start_id: &str, common_root_id: &str) -> Option<String> {
        let mut curr_id = start_id.to_string();
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(curr_id.clone()) {
                return None;
            }
            let &idx = self.id_to_index.get(&curr_id)?;
            let node = &self.nodes[idx];
            match &node.parent_id {
                Some(pid) if pid == common_root_id => {
                    return Some(node.branch_id.clone());
                }
                Some(pid) => {
                    curr_id = pid.clone();
                }
                None => {
                    return Some(node.branch_id.clone());
                }
            }
        }
    }

    /// Partition the tree into (discovery_tree, held_out_tree) ensuring that each partition
    /// forms a valid, fully connected tree/forest where every node is reachable from a root node
    /// within that tree. Whole lineages stay strictly together, and control anchors are excluded.
    pub fn split_held_out(&self, validation_fraction: f64) -> Result<(Self, Self), TreeLogError> {
        self.validate_ancestry()?;

        let roots: Vec<&AttemptNode> = self
            .roots()
            .into_iter()
            .filter(|r| r.branch_id != "control")
            .collect();
        let frac = validation_fraction.clamp(0.05, 0.50);

        if roots.len() >= 2 {
            // Case 1: Multiple independent root components (forest)
            let val_count =
                ((roots.len() as f64 * frac).round() as usize).clamp(1, roots.len() - 1);
            let split_idx = roots.len() - val_count;
            let disc_roots: HashSet<String> =
                roots[..split_idx].iter().map(|r| r.id.clone()).collect();
            let val_roots: HashSet<String> =
                roots[split_idx..].iter().map(|r| r.id.clone()).collect();

            let mut disc_tree = Self::new();
            let mut val_tree = Self::new();

            for node in &self.nodes {
                if node.branch_id == "control" {
                    continue;
                }
                let mut curr = node;
                let mut visited = HashSet::new();
                visited.insert(curr.id.clone());
                while let Some(ref pid) = curr.parent_id {
                    if !visited.insert(pid.clone()) {
                        break;
                    }
                    if let Some(&idx) = self.id_to_index.get(pid) {
                        curr = &self.nodes[idx];
                    } else {
                        break;
                    }
                }
                if disc_roots.contains(&curr.id) {
                    let _ = disc_tree.add_node(node.clone());
                } else if val_roots.contains(&curr.id) {
                    let _ = val_tree.add_node(node.clone());
                }
            }

            disc_tree.validate_ancestry()?;
            val_tree.validate_ancestry()?;
            return Ok((disc_tree, val_tree));
        }

        // Case 2: Single common root (e.g. baseline node) with multiple exploratory branches
        if let Some(common_root) = roots.first() {
            let mut branch_to_lineage: HashMap<String, String> = HashMap::new();
            let mut lineages: Vec<String> = Vec::new();

            for node in &self.nodes {
                if node.id == common_root.id || node.branch_id == "control" {
                    continue;
                }
                if !branch_to_lineage.contains_key(&node.branch_id) {
                    if let Some(l_root) = self.find_lineage_root_branch(&node.id, &common_root.id) {
                        branch_to_lineage.insert(node.branch_id.clone(), l_root.clone());
                        if !lineages.contains(&l_root) {
                            lineages.push(l_root);
                        }
                    }
                }
            }
            lineages.sort();

            if lineages.len() < 2 {
                return Err(TreeLogError::InsufficientBranchesForHeldOut(lineages.len()));
            }

            let val_count =
                ((lineages.len() as f64 * frac).round() as usize).clamp(1, lineages.len() - 1);
            let split_idx = lineages.len() - val_count;

            let disc_lineages: HashSet<String> = lineages[..split_idx].iter().cloned().collect();
            let val_lineages: HashSet<String> = lineages[split_idx..].iter().cloned().collect();

            let mut disc_tree = Self::new();
            let mut val_tree = Self::new();

            // Common root is present in both trees to provide the reachable root ancestor
            disc_tree.add_node((*common_root).clone())?;
            val_tree.add_node((*common_root).clone())?;

            for node in &self.nodes {
                if node.id == common_root.id || node.branch_id == "control" {
                    continue;
                }
                if let Some(l_root) = branch_to_lineage.get(&node.branch_id) {
                    if disc_lineages.contains(l_root) {
                        let _ = disc_tree.add_node(node.clone());
                    } else if val_lineages.contains(l_root) {
                        let _ = val_tree.add_node(node.clone());
                    }
                }
            }

            disc_tree.validate_ancestry()?;
            val_tree.validate_ancestry()?;

            let disc_reachable = disc_tree.reachable_node_ids();
            if disc_reachable.len() != disc_tree.nodes.len() {
                return Err(TreeLogError::OrphanedNode {
                    node_id: "discovery partition contains unreachable nodes".into(),
                    parent_id: "missing".into(),
                });
            }
            let val_reachable = val_tree.reachable_node_ids();
            if val_reachable.len() != val_tree.nodes.len() {
                return Err(TreeLogError::OrphanedNode {
                    node_id: "validation partition contains unreachable nodes".into(),
                    parent_id: "missing".into(),
                });
            }

            return Ok((disc_tree, val_tree));
        }

        Err(TreeLogError::InsufficientBranchesForHeldOut(0))
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

    /// Update the committed git commit hash of a node in the durable log.
    pub fn record_committed_commit(
        path: &Path,
        id: &str,
        commit_hash: &str,
    ) -> Result<(), TreeLogError> {
        if !path.exists() {
            return Err(TreeLogError::LogFileNotFound(path.display().to_string()));
        }
        let content = std::fs::read_to_string(path)?;
        let mut updated_lines = Vec::new();
        let mut modified = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(mut node) = serde_json::from_str::<AttemptNode>(trimmed) {
                if node.id == id {
                    node.committed_commit = Some(commit_hash.to_string());
                    updated_lines.push(serde_json::to_string(&node)?);
                    modified = true;
                    continue;
                }
            }
            updated_lines.push(trimmed.to_string());
        }

        if !modified {
            return Err(TreeLogError::NodeNotFound(id.to_string()));
        }

        let tmp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        {
            let mut file = File::create(&tmp_path)?;
            for line in updated_lines {
                writeln!(file, "{}", line)?;
            }
            file.sync_all()?;
        }
        std::fs::rename(&tmp_path, path)?;

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/tree_log/tree_log_test.rs"]
mod tests;
