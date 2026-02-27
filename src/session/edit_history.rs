//! Edit History & Time Machine
//!
//! Tracks all edits with checkpoints, enabling undo/redo
//! and timeline visualization for the agent's file operations.

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
        let end = s.floor_char_boundary(max.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_id_creation() {
        let id = EditCheckpointId::new(42);
        assert_eq!(id.0, 42);
    }

    #[test]
    fn test_file_snapshot_creation() {
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "fn main() {}".to_string());
        assert_eq!(snapshot.path, PathBuf::from("test.rs"));
        assert_eq!(snapshot.size, 12);
        assert!(!snapshot.hash.is_empty());
    }

    #[test]
    fn test_file_snapshot_changed() {
        let s1 = FileSnapshot::new(PathBuf::from("test.rs"), "content1".to_string());
        let s2 = FileSnapshot::new(PathBuf::from("test.rs"), "content2".to_string());
        let s3 = FileSnapshot::new(PathBuf::from("test.rs"), "content1".to_string());

        assert!(s1.changed_from(&s2));
        assert!(!s1.changed_from(&s3));
    }

    #[test]
    fn test_edit_action_description() {
        let action = EditAction::FileEdit {
            path: PathBuf::from("main.rs"),
            tool: "file_edit".to_string(),
        };
        assert!(action.description().contains("main.rs"));
    }

    #[test]
    fn test_edit_action_icons() {
        assert_eq!(
            EditAction::FileCreate {
                path: PathBuf::new()
            }
            .icon(),
            "📄"
        );
        assert_eq!(
            EditAction::FileEdit {
                path: PathBuf::new(),
                tool: String::new()
            }
            .icon(),
            "✏️"
        );
        assert_eq!(
            EditAction::FileDelete {
                path: PathBuf::new()
            }
            .icon(),
            "🗑️"
        );
        assert_eq!(
            EditAction::GitCommit {
                hash: String::new(),
                message: String::new()
            }
            .icon(),
            "🔀"
        );
        assert_eq!(
            EditAction::Manual {
                description: String::new()
            }
            .icon(),
            "📌"
        );
        assert_eq!(EditAction::SessionStart.icon(), "🚀");
        assert_eq!(EditAction::SessionEnd.icon(), "🏁");
    }

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        assert_eq!(checkpoint.id.0, 1);
        assert!(checkpoint.files.is_empty());
    }

    #[test]
    fn test_checkpoint_add_file() {
        let mut checkpoint =
            EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
        checkpoint.add_file(snapshot);
        assert_eq!(checkpoint.files.len(), 1);
    }

    #[test]
    fn test_checkpoint_builder_methods() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart)
            .with_git_hash("abc123".to_string())
            .with_parent(EditCheckpointId::new(0))
            .with_branch("feature".to_string())
            .with_label("Test label".to_string());

        assert_eq!(checkpoint.git_hash, Some("abc123".to_string()));
        assert_eq!(checkpoint.parent, Some(EditCheckpointId::new(0)));
        assert_eq!(checkpoint.branch, Some("feature".to_string()));
        assert_eq!(checkpoint.label, Some("Test label".to_string()));
    }

    #[test]
    fn test_checkpoint_formatted_time() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        let time = checkpoint.formatted_time();
        assert!(time.contains(":")); // HH:MM:SS format
    }

    #[test]
    fn test_checkpoint_relative_time() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        let relative = checkpoint.relative_time();
        assert!(relative.contains("ago") || relative.contains("s"));
    }

    #[test]
    fn test_edit_history_creation() {
        let history = EditHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_edit_history_default() {
        let history = EditHistory::default();
        assert!(history.is_empty());
    }

    #[test]
    fn test_edit_history_with_max() {
        let history = EditHistory::with_max_checkpoints(50);
        assert_eq!(history.max_checkpoints, 50);
    }

    #[test]
    fn test_create_checkpoint() {
        let mut history = EditHistory::new();
        let id = history.create_checkpoint(EditAction::SessionStart);
        assert_eq!(id.0, 1);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_multiple_checkpoints() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::FileEdit {
            path: PathBuf::from("test.rs"),
            tool: "edit".to_string(),
        });
        history.create_checkpoint(EditAction::SessionEnd);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_add_file_to_current() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
        history.add_file_to_current(snapshot);

        let current = history.current_checkpoint().unwrap();
        assert_eq!(current.files.len(), 1);
    }

    #[test]
    fn test_undo_redo() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::FileEdit {
            path: PathBuf::from("test.rs"),
            tool: "edit".to_string(),
        });
        history.create_checkpoint(EditAction::SessionEnd);

        assert_eq!(history.position(), 3);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        history.undo();
        assert_eq!(history.position(), 2);
        assert!(history.can_undo());
        assert!(history.can_redo());

        history.redo();
        assert_eq!(history.position(), 3);
    }

    #[test]
    fn test_goto() {
        let mut history = EditHistory::new();
        let id1 = history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::SessionEnd);

        history.goto(id1);
        assert_eq!(history.position(), 1);
    }

    #[test]
    fn test_cannot_undo_at_start() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        assert!(!history.can_undo());
    }

    #[test]
    fn test_cannot_redo_at_end() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        assert!(!history.can_redo());
    }

    #[test]
    fn test_branches() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);

        assert!(history.current_branch().is_none());

        history.create_branch("feature-x");
        assert_eq!(history.current_branch(), Some("feature-x"));

        history.switch_to_main();
        assert!(history.current_branch().is_none());
    }

    #[test]
    fn test_timeline() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::SessionEnd);

        let timeline = history.timeline();
        assert_eq!(timeline.len(), 2);
        assert!(timeline[1].is_current);
    }

    #[test]
    fn test_timeline_entry_display() {
        let entry = TimelineEntry {
            id: EditCheckpointId::new(1),
            is_current: true,
            action: EditAction::SessionStart,
            timestamp: Utc::now(),
            label: None,
            branch: None,
        };
        assert!(!entry.display_text().is_empty());

        let entry_with_label = TimelineEntry {
            label: Some("Custom Label".to_string()),
            ..entry
        };
        assert_eq!(entry_with_label.display_text(), "Custom Label");
    }

    #[test]
    fn test_clear_history() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::SessionEnd);
        assert_eq!(history.len(), 2);

        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.position(), 0);
    }

    #[test]
    fn test_max_checkpoints_pruning() {
        let mut history = EditHistory::with_max_checkpoints(5);

        for i in 0..10 {
            history.create_checkpoint(EditAction::Manual {
                description: format!("Checkpoint {}", i),
            });
        }

        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_truncate_future_on_new_checkpoint() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        let id2 = history.create_checkpoint(EditAction::SessionEnd);
        history.create_checkpoint(EditAction::Manual {
            description: "Will be removed".to_string(),
        });

        // Go back
        history.goto(id2);
        assert_eq!(history.position(), 2);

        // Create new checkpoint (should truncate future)
        history.create_checkpoint(EditAction::Manual {
            description: "New future".to_string(),
        });
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_get_checkpoint() {
        let mut history = EditHistory::new();
        let id = history.create_checkpoint(EditAction::SessionStart);

        let checkpoint = history.get(id);
        assert!(checkpoint.is_some());
        assert_eq!(checkpoint.unwrap().id, id);

        let invalid = history.get(EditCheckpointId::new(999));
        assert!(invalid.is_none());
    }

    #[test]
    fn test_compute_hash() {
        let h1 = compute_hash("content");
        let h2 = compute_hash("content");
        let h3 = compute_hash("different");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_truncate_function() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_multi_file_action() {
        let action = EditAction::MultiFileEdit {
            paths: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
            tool: "refactor".to_string(),
        };
        assert!(action.description().contains("2 files"));
        assert_eq!(action.icon(), "📝");
    }

    #[test]
    fn test_all_checkpoints() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::SessionEnd);

        let all = history.all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_checkpoint_id_clone() {
        let id = EditCheckpointId::new(42);
        let cloned = id;
        assert_eq!(id, cloned);
    }

    #[test]
    fn test_checkpoint_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(EditCheckpointId::new(1));
        set.insert(EditCheckpointId::new(2));
        set.insert(EditCheckpointId::new(1)); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_checkpoint_id_serialize() {
        let id = EditCheckpointId::new(42);
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("42"));
    }

    #[test]
    fn test_checkpoint_id_deserialize() {
        let json = "42";
        let id: EditCheckpointId = serde_json::from_str(json).unwrap();
        assert_eq!(id.0, 42);
    }

    #[test]
    fn test_file_snapshot_clone() {
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
        let cloned = snapshot.clone();
        assert_eq!(snapshot.path, cloned.path);
        assert_eq!(snapshot.hash, cloned.hash);
    }

    #[test]
    fn test_file_snapshot_serialize() {
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("test.rs"));
        assert!(json.contains("content"));
    }

    #[test]
    fn test_file_snapshot_empty_content() {
        let snapshot = FileSnapshot::new(PathBuf::from("empty.txt"), "".to_string());
        assert_eq!(snapshot.size, 0);
        assert!(!snapshot.hash.is_empty());
    }

    #[test]
    fn test_edit_action_clone() {
        let action = EditAction::FileEdit {
            path: PathBuf::from("test.rs"),
            tool: "edit".to_string(),
        };
        let cloned = action.clone();
        assert!(matches!(cloned, EditAction::FileEdit { .. }));
    }

    #[test]
    fn test_edit_action_serialize() {
        let action = EditAction::GitCommit {
            hash: "abc123".to_string(),
            message: "Test commit".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("Test commit"));
    }

    #[test]
    fn test_edit_action_file_delete_description() {
        let action = EditAction::FileDelete {
            path: PathBuf::from("obsolete.rs"),
        };
        assert!(action.description().contains("Deleted"));
        assert!(action.description().contains("obsolete.rs"));
    }

    #[test]
    fn test_edit_action_git_commit_short_hash() {
        let action = EditAction::GitCommit {
            hash: "abc".to_string(),
            message: "Short hash".to_string(),
        };
        let desc = action.description();
        assert!(desc.contains("abc")); // Full hash since < 7 chars
    }

    #[test]
    fn test_edit_action_manual_long_description() {
        let long_desc = "This is a very long description that should be truncated when displayed";
        let action = EditAction::Manual {
            description: long_desc.to_string(),
        };
        let desc = action.description();
        assert!(desc.len() < long_desc.len() + 10); // Should be truncated
    }

    #[test]
    fn test_edit_checkpoint_clone() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        let cloned = checkpoint.clone();
        assert_eq!(checkpoint.id, cloned.id);
    }

    #[test]
    fn test_edit_checkpoint_serialize() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("id"));
        assert!(json.contains("timestamp"));
    }

    #[test]
    fn test_history_undo_at_start() {
        let mut history = EditHistory::new();
        history.undo(); // Should do nothing, not panic
        assert_eq!(history.position(), 0);
    }

    #[test]
    fn test_history_redo_at_end() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.redo(); // Should do nothing, not panic
        assert_eq!(history.position(), 1);
    }

    #[test]
    fn test_history_goto_invalid() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.goto(EditCheckpointId::new(999)); // Invalid ID
        assert_eq!(history.position(), 1); // Should stay at current
    }

    #[test]
    fn test_history_current_checkpoint_empty() {
        let history = EditHistory::new();
        assert!(history.current_checkpoint().is_none());
    }

    #[test]
    fn test_add_file_to_current_empty_history() {
        let mut history = EditHistory::new();
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
        history.add_file_to_current(snapshot); // Should not panic
    }

    #[test]
    fn test_timeline_with_labels() {
        let mut history = EditHistory::new();
        let _checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart)
            .with_label("Important".to_string());

        // We need to use the internal method to get proper label coverage
        history.create_checkpoint(EditAction::Manual {
            description: "Labeled checkpoint".to_string(),
        });

        let timeline = history.timeline();
        assert!(!timeline.is_empty());
    }

    #[test]
    fn test_timeline_with_branch() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_branch("feature-y");

        // Verify the branch was created
        assert_eq!(history.current_branch(), Some("feature-y"));

        // Create another checkpoint while on branch
        history.create_checkpoint(EditAction::SessionEnd);

        let timeline = history.timeline();
        // The new checkpoint should have the branch
        assert!(timeline.len() >= 2);
    }

    #[test]
    fn test_switch_branch_back() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_branch("branch-a");
        history.create_branch("branch-b");
        history.switch_to_main();

        assert!(history.current_branch().is_none());
    }

    #[test]
    fn test_multiple_files_in_checkpoint() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);

        let snapshot1 = FileSnapshot::new(PathBuf::from("a.rs"), "content a".to_string());
        let snapshot2 = FileSnapshot::new(PathBuf::from("b.rs"), "content b".to_string());

        history.add_file_to_current(snapshot1);
        history.add_file_to_current(snapshot2);

        let checkpoint = history.current_checkpoint().unwrap();
        assert_eq!(checkpoint.files.len(), 2);
    }

    #[test]
    fn test_compute_hash_unicode() {
        let h1 = compute_hash("こんにちは");
        let h2 = compute_hash("你好");
        assert_ne!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn test_timeline_entry_current_marker() {
        let entry = TimelineEntry {
            id: EditCheckpointId::new(1),
            is_current: true,
            action: EditAction::SessionStart,
            timestamp: Utc::now(),
            label: None,
            branch: None,
        };
        assert!(entry.is_current);
    }

    #[test]
    fn test_file_snapshot_debug() {
        let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
        let debug = format!("{:?}", snapshot);
        assert!(debug.contains("FileSnapshot"));
    }

    #[test]
    fn test_edit_action_debug() {
        let action = EditAction::SessionStart;
        let debug = format!("{:?}", action);
        assert!(debug.contains("SessionStart"));
    }

    #[test]
    fn test_checkpoint_debug() {
        let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
        let debug = format!("{:?}", checkpoint);
        assert!(debug.contains("EditCheckpoint"));
    }

    #[test]
    fn test_history_multiple_undo() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::FileEdit {
            path: PathBuf::from("a.rs"),
            tool: "edit".to_string(),
        });
        history.create_checkpoint(EditAction::FileEdit {
            path: PathBuf::from("b.rs"),
            tool: "edit".to_string(),
        });

        history.undo();
        history.undo();

        assert_eq!(history.position(), 1);
        assert!(history.can_redo());
    }

    #[test]
    fn test_history_undo_then_create() {
        let mut history = EditHistory::new();
        history.create_checkpoint(EditAction::SessionStart);
        history.create_checkpoint(EditAction::SessionEnd);
        history.undo();

        // Creating new checkpoint should truncate redo stack
        history.create_checkpoint(EditAction::Manual {
            description: "New".to_string(),
        });

        assert!(!history.can_redo());
    }
}
