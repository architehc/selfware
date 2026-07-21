//! Edit History & Time Machine
//!
//! Tracks all edits with checkpoints, enabling undo/redo
//! and timeline visualization for the agent's file operations.

#![allow(dead_code, unused_imports, unused_variables)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Unique identifier for a checkpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EditCheckpointId(pub u64);

impl EditCheckpointId {
    /// Create a new checkpoint ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// A snapshot of file content at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Path to the file
    pub path: PathBuf,
    /// Content at this point
    pub content: String,
    /// File size in bytes
    pub size: usize,
    /// Content hash for comparison
    pub hash: String,
}

impl FileSnapshot {
    /// Create a new file snapshot
    pub fn new(path: PathBuf, content: String) -> Self {
        let size = content.len();
        let hash = compute_hash(&content);
        Self {
            path,
            content,
            size,
            hash,
        }
    }

    /// Check if content changed from another snapshot
    pub fn changed_from(&self, other: &FileSnapshot) -> bool {
        self.hash != other.hash
    }
}

/// Action that caused an edit checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditAction {
    /// File was created
    FileCreate { path: PathBuf },
    /// File was edited
    FileEdit { path: PathBuf, tool: String },
    /// File was deleted
    FileDelete { path: PathBuf },
    /// Multiple files changed
    MultiFileEdit { paths: Vec<PathBuf>, tool: String },
    /// Git commit made
    GitCommit { hash: String, message: String },
    /// User manually created checkpoint
    Manual { description: String },
    /// Session started
    SessionStart,
    /// Session ended
    SessionEnd,
}

impl EditAction {
    /// Get a short description of the action
    pub fn description(&self) -> String {
        match self {
            EditAction::FileCreate { path } => {
                format!("Created {}", path.display())
            }
            EditAction::FileEdit { path, tool } => {
                format!("{} edited {}", tool, path.display())
            }
            EditAction::FileDelete { path } => {
                format!("Deleted {}", path.display())
            }
            EditAction::MultiFileEdit { paths, tool } => {
                format!("{} edited {} files", tool, paths.len())
            }
            EditAction::GitCommit { hash, message } => {
                format!(
                    "Commit {}: {}",
                    &hash[..7.min(hash.len())],
                    truncate(message, 30)
                )
            }
            EditAction::Manual { description } => {
                format!("Manual: {}", truncate(description, 40))
            }
            EditAction::SessionStart => "Session started".to_string(),
            EditAction::SessionEnd => "Session ended".to_string(),
        }
    }

    /// Get the icon for this action
    pub fn icon(&self) -> &'static str {
        match self {
            EditAction::FileCreate { .. } => "📄",
            EditAction::FileEdit { .. } => "✏️",
            EditAction::FileDelete { .. } => "🗑️",
            EditAction::MultiFileEdit { .. } => "📝",
            EditAction::GitCommit { .. } => "🔀",
            EditAction::Manual { .. } => "📌",
            EditAction::SessionStart => "🚀",
            EditAction::SessionEnd => "🏁",
        }
    }
}

/// An edit checkpoint in the history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditCheckpoint {
    /// Unique identifier
    pub id: EditCheckpointId,
    /// When this checkpoint was created
    pub timestamp: DateTime<Utc>,
    /// What action caused this checkpoint
    pub action: EditAction,
    /// File snapshots at this point
    pub files: HashMap<PathBuf, FileSnapshot>,
    /// Optional git commit hash
    pub git_hash: Option<String>,
    /// Optional parent checkpoint
    pub parent: Option<EditCheckpointId>,
    /// Optional branch name (for what-if scenarios)
    pub branch: Option<String>,
    /// User-provided label
    pub label: Option<String>,
}

impl EditCheckpoint {
    /// Create a new checkpoint
    pub fn new(id: EditCheckpointId, action: EditAction) -> Self {
        Self {
            id,
            timestamp: Utc::now(),
            action,
            files: HashMap::new(),
            git_hash: None,
            parent: None,
            branch: None,
            label: None,
        }
    }

    /// Add a file snapshot
    pub fn add_file(&mut self, snapshot: FileSnapshot) {
        self.files.insert(snapshot.path.clone(), snapshot);
    }

    /// Set git hash
    pub fn with_git_hash(mut self, hash: String) -> Self {
        self.git_hash = Some(hash);
        self
    }

    /// Set parent checkpoint
    pub fn with_parent(mut self, parent: EditCheckpointId) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Set branch name
    pub fn with_branch(mut self, branch: String) -> Self {
        self.branch = Some(branch);
        self
    }

    /// Set label
    pub fn with_label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }

    /// Get formatted timestamp
    pub fn formatted_time(&self) -> String {
        self.timestamp.format("%H:%M:%S").to_string()
    }

    /// Get relative time (e.g., "2 minutes ago")
    pub fn relative_time(&self) -> String {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.timestamp);

        if duration.num_seconds() < 60 {
            format!("{}s ago", duration.num_seconds())
        } else if duration.num_minutes() < 60 {
            format!("{}m ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{}h ago", duration.num_hours())
        } else {
            format!("{}d ago", duration.num_days())
        }
    }
}

/// The edit history manager
pub struct EditHistory {
    /// All checkpoints
    checkpoints: Vec<EditCheckpoint>,
    /// Current position in history
    current: usize,
    /// Next checkpoint ID to assign
    next_id: u64,
    /// Maximum checkpoints to keep
    max_checkpoints: usize,
    /// Current branch (for what-if scenarios)
    current_branch: Option<String>,
}

impl EditHistory {
    /// Create a new edit history
    pub fn new() -> Self {
        Self {
            checkpoints: Vec::new(),
            current: 0,
            next_id: 1,
            max_checkpoints: 100,
            current_branch: None,
        }
    }

    /// Create with custom max checkpoints
    pub fn with_max_checkpoints(max: usize) -> Self {
        Self {
            max_checkpoints: max,
            ..Self::new()
        }
    }

    /// Create a new checkpoint
    pub fn create_checkpoint(&mut self, action: EditAction) -> EditCheckpointId {
        let id = EditCheckpointId::new(self.next_id);
        self.next_id += 1;

        let mut checkpoint = EditCheckpoint::new(id, action);

        // Set parent to current checkpoint
        if let Some(parent) = self.current_checkpoint() {
            checkpoint = checkpoint.with_parent(parent.id);
        }

        // Set branch if we're on one
        if let Some(ref branch) = self.current_branch {
            checkpoint = checkpoint.with_branch(branch.clone());
        }

        // Truncate future history if we're not at the end
        if self.current < self.checkpoints.len() {
            self.checkpoints.truncate(self.current);
            self.current = self.current.min(self.checkpoints.len());
        }

        self.checkpoints.push(checkpoint);
        self.current = self.checkpoints.len();

        // Prune old checkpoints if needed
        if self.checkpoints.len() > self.max_checkpoints {
            let remove_count = self.checkpoints.len() - self.max_checkpoints;
            self.checkpoints.drain(0..remove_count);
            self.current = self.current.saturating_sub(remove_count);
        }

        id
    }

    /// Add a file snapshot to the current checkpoint
    pub fn add_file_to_current(&mut self, snapshot: FileSnapshot) {
        if let Some(checkpoint) = self.checkpoints.last_mut() {
            checkpoint.add_file(snapshot);
        }
    }

    /// Get current checkpoint
    pub fn current_checkpoint(&self) -> Option<&EditCheckpoint> {
        if self.current == 0 {
            return None;
        }
        self.checkpoints.get(self.current - 1)
    }

    /// Get checkpoint by ID
    pub fn get(&self, id: EditCheckpointId) -> Option<&EditCheckpoint> {
        self.checkpoints.iter().find(|c| c.id == id)
    }

    /// Can undo?
    pub fn can_undo(&self) -> bool {
        self.current > 1
    }

    /// Can redo?
    pub fn can_redo(&self) -> bool {
        self.current < self.checkpoints.len()
    }

    /// Undo to previous checkpoint
    pub fn undo(&mut self) -> Option<&EditCheckpoint> {
        if self.can_undo() {
            self.current -= 1;
            self.current_checkpoint()
        } else {
            None
        }
    }

    /// Redo to next checkpoint
    pub fn redo(&mut self) -> Option<&EditCheckpoint> {
        if self.can_redo() {
            self.current += 1;
            self.current_checkpoint()
        } else {
            None
        }
    }

    /// Go to a specific checkpoint
    pub fn goto(&mut self, id: EditCheckpointId) -> Option<&EditCheckpoint> {
        if let Some(pos) = self.checkpoints.iter().position(|c| c.id == id) {
            self.current = pos + 1;
            self.current_checkpoint()
        } else {
            None
        }
    }

    /// Get all checkpoints
    pub fn all(&self) -> &[EditCheckpoint] {
        &self.checkpoints
    }

    /// Get checkpoint count
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.current
    }

    /// Create a new branch at current position
    pub fn create_branch(&mut self, name: &str) {
        self.current_branch = Some(name.to_string());
    }

    /// Switch to main branch
    pub fn switch_to_main(&mut self) {
        self.current_branch = None;
    }

    /// Get current branch
    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    /// Get timeline data for visualization
    pub fn timeline(&self) -> Vec<TimelineEntry> {
        self.checkpoints
            .iter()
            .enumerate()
            .map(|(i, c)| TimelineEntry {
                id: c.id,
                is_current: i + 1 == self.current,
                action: c.action.clone(),
                timestamp: c.timestamp,
                label: c.label.clone(),
                branch: c.branch.clone(),
            })
            .collect()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.checkpoints.clear();
        self.current = 0;
        self.next_id = 1;
    }
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Entry in the timeline visualization
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    /// Checkpoint ID
    pub id: EditCheckpointId,
    /// Whether this is the current position
    pub is_current: bool,
    /// Action that caused this checkpoint
    pub action: EditAction,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional label
    pub label: Option<String>,
    /// Branch name if on a branch
    pub branch: Option<String>,
}

impl TimelineEntry {
    /// Get display text for timeline
    pub fn display_text(&self) -> String {
        if let Some(ref label) = self.label {
            label.clone()
        } else {
            self.action.description()
        }
    }
}

/// Compute a SHA-256 hash of content for integrity checking.
///
/// Using a cryptographic hash instead of `DefaultHasher` ensures consistent,
/// deterministic results across Rust versions and platforms.
fn compute_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// Truncate string with ellipsis
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find a valid UTF-8 character boundary
        let mut end = max.saturating_sub(3);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session/edit_history/edit_history_test.rs"]
mod tests;
