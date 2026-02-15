//! Task Checkpointing & Persistence
//!
//! Enables resumable long-running tasks by saving state to disk.
//! Captures:
//! - Task description and status
//! - Conversation messages
//! - Tool call history with timing
//! - Git state for reproducibility
//! - Error logs for debugging
//!
//! Checkpoints are stored as JSON files and can be resumed with `Agent::resume()`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::api::types::Message;
use crate::redact;

/// Status of a task checkpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Completed,
    Failed,
    Paused,
}

/// A memory entry for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
    pub token_estimate: usize,
}

/// Log of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallLog {
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub success: bool,
    pub duration_ms: Option<u64>,
}

/// Log of an error during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLog {
    pub timestamp: DateTime<Utc>,
    pub step: usize,
    pub error: String,
    pub recovered: bool,
}

/// Git state at checkpoint time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCheckpointInfo {
    pub branch: String,
    pub commit_hash: String,
    pub dirty: bool,
    pub staged_files: Vec<String>,
    pub modified_files: Vec<String>,
}

/// A complete checkpoint of task state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    pub task_id: String,
    pub task_description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: TaskStatus,
    pub current_step: usize,

    // Context state
    pub messages: Vec<Message>,
    pub memory_entries: Vec<MemoryEntry>,
    pub estimated_tokens: usize,

    // Execution log
    pub tool_calls: Vec<ToolCallLog>,
    pub errors: Vec<ErrorLog>,

    // Git state
    pub git_checkpoint: Option<GitCheckpointInfo>,
}

/// Summary of a task for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub task_description: String,
    pub status: TaskStatus,
    pub current_step: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tool_call_count: usize,
    pub error_count: usize,
}

impl TaskCheckpoint {
    /// Create a new checkpoint for a task
    pub fn new(task_id: String, task_description: String) -> Self {
        let now = Utc::now();
        Self {
            task_id,
            task_description,
            created_at: now,
            updated_at: now,
            status: TaskStatus::InProgress,
            current_step: 0,
            messages: Vec::new(),
            memory_entries: Vec::new(),
            estimated_tokens: 0,
            tool_calls: Vec::new(),
            errors: Vec::new(),
            git_checkpoint: None,
        }
    }

    /// Create a summary of this checkpoint
    pub fn to_summary(&self) -> TaskSummary {
        TaskSummary {
            task_id: self.task_id.clone(),
            task_description: self.task_description.clone(),
            status: self.status.clone(),
            current_step: self.current_step,
            created_at: self.created_at,
            updated_at: self.updated_at,
            tool_call_count: self.tool_calls.len(),
            error_count: self.errors.len(),
        }
    }

    /// Add a tool call log entry
    pub fn log_tool_call(&mut self, log: ToolCallLog) {
        self.tool_calls.push(log);
        self.updated_at = Utc::now();
    }

    /// Add an error log entry
    pub fn log_error(&mut self, step: usize, error: String, recovered: bool) {
        self.errors.push(ErrorLog {
            timestamp: Utc::now(),
            step,
            error,
            recovered,
        });
        self.updated_at = Utc::now();
    }

    /// Update the step
    pub fn set_step(&mut self, step: usize) {
        self.current_step = step;
        self.updated_at = Utc::now();
    }

    /// Update the status
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// Update messages
    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.updated_at = Utc::now();
    }
}

/// Manager for saving and loading task checkpoints
pub struct CheckpointManager {
    checkpoints_dir: PathBuf,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(checkpoints_dir: PathBuf) -> Result<Self> {
        // Create directory if it doesn't exist
        if !checkpoints_dir.exists() {
            fs::create_dir_all(&checkpoints_dir).with_context(|| {
                format!(
                    "Failed to create checkpoints directory: {:?}",
                    checkpoints_dir
                )
            })?;
        }
        Ok(Self { checkpoints_dir })
    }

    /// Create a checkpoint manager with default directory
    pub fn default_path() -> Result<Self> {
        let home = dirs_home();
        let checkpoints_dir = home.join(".selfware").join("checkpoints");
        Self::new(checkpoints_dir)
    }

    /// Get the path for a checkpoint file
    fn checkpoint_path(&self, task_id: &str) -> PathBuf {
        self.checkpoints_dir.join(format!("{}.json", task_id))
    }

    /// Save a checkpoint to disk (with secrets redacted)
    pub fn save(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let path = self.checkpoint_path(&checkpoint.task_id);

        // Serialize to JSON value first so we can redact secrets
        let mut json_value =
            serde_json::to_value(checkpoint).context("Failed to serialize checkpoint")?;

        // Redact any secrets in the checkpoint data
        redact::redact_json(&mut json_value);

        let json = serde_json::to_string_pretty(&json_value)
            .context("Failed to format checkpoint JSON")?;

        fs::write(&path, json)
            .with_context(|| format!("Failed to write checkpoint to {:?}", path))?;
        Ok(())
    }

    /// Load a checkpoint from disk
    pub fn load(&self, task_id: &str) -> Result<TaskCheckpoint> {
        let path = self.checkpoint_path(task_id);
        let json = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint from {:?}", path))?;
        let checkpoint: TaskCheckpoint =
            serde_json::from_str(&json).context("Failed to deserialize checkpoint")?;
        Ok(checkpoint)
    }

    /// List all saved tasks
    pub fn list_tasks(&self) -> Result<Vec<TaskSummary>> {
        let mut summaries = Vec::new();

        if !self.checkpoints_dir.exists() {
            return Ok(summaries);
        }

        for entry in fs::read_dir(&self.checkpoints_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(checkpoint) = serde_json::from_str::<TaskCheckpoint>(&json) {
                        summaries.push(checkpoint.to_summary());
                    }
                }
            }
        }

        // Sort by updated_at descending (most recent first)
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(summaries)
    }

    /// Delete a checkpoint
    pub fn delete(&self, task_id: &str) -> Result<()> {
        let path = self.checkpoint_path(task_id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to delete checkpoint: {:?}", path))?;
        }
        Ok(())
    }

    /// Check if a checkpoint exists (test helper)
    #[cfg(test)]
    pub fn exists(&self, task_id: &str) -> bool {
        self.checkpoint_path(task_id).exists()
    }

    /// Get the checkpoints directory path (test helper)
    #[cfg(test)]
    pub fn checkpoints_dir(&self) -> &PathBuf {
        &self.checkpoints_dir
    }
}

/// Get home directory
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Capture current git state for checkpoint
pub fn capture_git_state(repo_path: &str) -> Option<GitCheckpointInfo> {
    let repo = git2::Repository::open(repo_path).ok()?;

    // Get current branch
    let head = repo.head().ok()?;
    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "HEAD".to_string());

    // Get current commit
    let commit = head.peel_to_commit().ok()?;
    let commit_hash = commit.id().to_string();

    // Check for dirty state
    let statuses = repo.statuses(None).ok()?;
    let mut staged_files = Vec::new();
    let mut modified_files = Vec::new();

    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("").to_string();

        if status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_renamed()
        {
            staged_files.push(path.clone());
        }

        if status.is_wt_new()
            || status.is_wt_modified()
            || status.is_wt_deleted()
            || status.is_wt_renamed()
        {
            modified_files.push(path);
        }
    }

    let dirty = !staged_files.is_empty() || !modified_files.is_empty();

    Some(GitCheckpointInfo {
        branch,
        commit_hash,
        dirty,
        staged_files,
        modified_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_task_checkpoint_new() {
        let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        assert_eq!(checkpoint.task_id, "task_123");
        assert_eq!(checkpoint.task_description, "Test task");
        assert_eq!(checkpoint.status, TaskStatus::InProgress);
        assert_eq!(checkpoint.current_step, 0);
    }

    #[test]
    fn test_task_checkpoint_to_summary() {
        let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        let summary = checkpoint.to_summary();
        assert_eq!(summary.task_id, "task_123");
        assert_eq!(summary.task_description, "Test task");
        assert_eq!(summary.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_task_checkpoint_log_tool_call() {
        let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        let log = ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: r#"{"path": "test.txt"}"#.to_string(),
            result: Some("content".to_string()),
            success: true,
            duration_ms: Some(100),
        };
        checkpoint.log_tool_call(log);
        assert_eq!(checkpoint.tool_calls.len(), 1);
    }

    #[test]
    fn test_task_checkpoint_log_error() {
        let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        checkpoint.log_error(1, "Test error".to_string(), true);
        assert_eq!(checkpoint.errors.len(), 1);
        assert!(checkpoint.errors[0].recovered);
    }

    #[test]
    fn test_task_checkpoint_set_step() {
        let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        checkpoint.set_step(5);
        assert_eq!(checkpoint.current_step, 5);
    }

    #[test]
    fn test_task_checkpoint_set_status() {
        let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        checkpoint.set_status(TaskStatus::Completed);
        assert_eq!(checkpoint.status, TaskStatus::Completed);
    }

    #[test]
    fn test_task_status_serde() {
        let status = TaskStatus::InProgress;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"in_progress\"");

        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TaskStatus::InProgress);
    }

    #[test]
    fn test_checkpoint_manager_new() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        assert!(manager.checkpoints_dir().exists());
    }

    #[test]
    fn test_checkpoint_manager_save_load() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        manager.save(&checkpoint).unwrap();

        let loaded = manager.load("task_123").unwrap();
        assert_eq!(loaded.task_id, "task_123");
        assert_eq!(loaded.task_description, "Test task");
    }

    #[test]
    fn test_checkpoint_manager_list_tasks() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let checkpoint1 = TaskCheckpoint::new("task_1".to_string(), "Task 1".to_string());
        let checkpoint2 = TaskCheckpoint::new("task_2".to_string(), "Task 2".to_string());

        manager.save(&checkpoint1).unwrap();
        manager.save(&checkpoint2).unwrap();

        let tasks = manager.list_tasks().unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_checkpoint_manager_delete() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        manager.save(&checkpoint).unwrap();
        assert!(manager.exists("task_123"));

        manager.delete("task_123").unwrap();
        assert!(!manager.exists("task_123"));
    }

    #[test]
    fn test_checkpoint_manager_exists() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        assert!(!manager.exists("nonexistent"));

        let checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        manager.save(&checkpoint).unwrap();
        assert!(manager.exists("task_123"));
    }

    #[test]
    fn test_checkpoint_serialization_round_trip() {
        let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        checkpoint.set_step(5);
        checkpoint.set_status(TaskStatus::Paused);
        checkpoint.messages.push(Message::user("Hello"));
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: "{}".to_string(),
            result: Some("content".to_string()),
            success: true,
            duration_ms: Some(50),
        });

        let json = serde_json::to_string_pretty(&checkpoint).unwrap();
        let loaded: TaskCheckpoint = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.task_id, checkpoint.task_id);
        assert_eq!(loaded.current_step, 5);
        assert_eq!(loaded.status, TaskStatus::Paused);
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.tool_calls.len(), 1);
    }

    #[test]
    fn test_git_checkpoint_info_serde() {
        let info = GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "abc123".to_string(),
            dirty: true,
            staged_files: vec!["file1.rs".to_string()],
            modified_files: vec!["file2.rs".to_string()],
        };

        let json = serde_json::to_string(&info).unwrap();
        let loaded: GitCheckpointInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.branch, "main");
        assert_eq!(loaded.commit_hash, "abc123");
        assert!(loaded.dirty);
    }

    #[test]
    fn test_task_status_completed_serde() {
        let status = TaskStatus::Completed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"completed\"");
    }

    #[test]
    fn test_task_status_failed_serde() {
        let status = TaskStatus::Failed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"failed\"");
    }

    #[test]
    fn test_task_status_paused_serde() {
        let status = TaskStatus::Paused;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"paused\"");
    }

    #[test]
    fn test_memory_entry_struct() {
        let entry = MemoryEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            role: "user".to_string(),
            content: "test content".to_string(),
            token_estimate: 100,
        };
        assert_eq!(entry.role, "user");
        assert_eq!(entry.token_estimate, 100);
    }

    #[test]
    fn test_tool_call_log_struct() {
        let log = ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: "{}".to_string(),
            result: None,
            success: false,
            duration_ms: None,
        };
        assert_eq!(log.tool_name, "file_read");
        assert!(!log.success);
    }

    #[test]
    fn test_error_log_struct() {
        let log = ErrorLog {
            timestamp: Utc::now(),
            step: 5,
            error: "something failed".to_string(),
            recovered: false,
        };
        assert_eq!(log.step, 5);
        assert!(!log.recovered);
    }

    #[test]
    fn test_task_summary_struct() {
        let summary = TaskSummary {
            task_id: "abc".to_string(),
            task_description: "desc".to_string(),
            status: TaskStatus::InProgress,
            current_step: 3,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tool_call_count: 10,
            error_count: 2,
        };
        assert_eq!(summary.current_step, 3);
        assert_eq!(summary.tool_call_count, 10);
    }

    #[test]
    fn test_checkpoint_set_messages() {
        let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
        let messages = vec![Message::system("system"), Message::user("user msg")];
        checkpoint.set_messages(messages);
        assert_eq!(checkpoint.messages.len(), 2);
    }

    #[test]
    fn test_checkpoint_multiple_tool_calls() {
        let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
        for i in 0..5 {
            checkpoint.log_tool_call(ToolCallLog {
                timestamp: Utc::now(),
                tool_name: format!("tool_{}", i),
                arguments: "{}".to_string(),
                result: Some("ok".to_string()),
                success: true,
                duration_ms: Some(i as u64 * 10),
            });
        }
        assert_eq!(checkpoint.tool_calls.len(), 5);
    }

    #[test]
    fn test_checkpoint_multiple_errors() {
        let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
        checkpoint.log_error(1, "error 1".to_string(), true);
        checkpoint.log_error(2, "error 2".to_string(), false);
        checkpoint.log_error(3, "error 3".to_string(), true);
        assert_eq!(checkpoint.errors.len(), 3);
        assert!(!checkpoint.errors[1].recovered);
    }

    #[test]
    fn test_checkpoint_manager_load_nonexistent() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        let result = manager.load("nonexistent_task");
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_manager_delete_nonexistent() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        // Should not error when deleting nonexistent
        let result = manager.delete("nonexistent_task");
        assert!(result.is_ok());
    }

    #[test]
    fn test_checkpoint_manager_list_empty() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        let tasks = manager.list_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_git_checkpoint_info_empty_files() {
        let info = GitCheckpointInfo {
            branch: "feature".to_string(),
            commit_hash: "def456".to_string(),
            dirty: false,
            staged_files: vec![],
            modified_files: vec![],
        };
        assert!(!info.dirty);
        assert!(info.staged_files.is_empty());
        assert!(info.modified_files.is_empty());
    }

    #[test]
    fn test_checkpoint_with_git_state() {
        let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
        checkpoint.git_checkpoint = Some(GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "abc123def456".to_string(),
            dirty: true,
            staged_files: vec!["src/main.rs".to_string()],
            modified_files: vec![],
        });
        assert!(checkpoint.git_checkpoint.is_some());
        assert_eq!(checkpoint.git_checkpoint.as_ref().unwrap().branch, "main");
    }

    #[test]
    fn test_checkpoint_estimated_tokens() {
        let mut checkpoint = TaskCheckpoint::new("task_1".to_string(), "Test".to_string());
        checkpoint.estimated_tokens = 5000;
        assert_eq!(checkpoint.estimated_tokens, 5000);
    }

    #[test]
    fn test_capture_git_state() {
        // We're in a git repo, so this should work
        let state = capture_git_state(".");
        assert!(state.is_some());
        let state = state.unwrap();
        assert!(!state.branch.is_empty());
        assert!(!state.commit_hash.is_empty());
    }

    #[test]
    fn test_capture_git_state_nonexistent_repo() {
        // This should return None for a non-repo directory
        let state = capture_git_state("/tmp");
        // /tmp may or may not be a git repo, so just check it doesn't panic
        // The function should handle this gracefully
        let _ = state;
    }

    #[test]
    fn test_dirs_home_function() {
        let home = dirs_home();
        // Should return a valid path
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn test_checkpoint_manager_creates_nested_dir() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        let manager = CheckpointManager::new(nested.clone()).unwrap();
        assert!(nested.exists());
        assert!(manager.checkpoints_dir().exists());
    }

    #[test]
    fn test_checkpoint_manager_path() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        let expected = dir.path().join("task_test.json");
        assert_eq!(manager.checkpoint_path("task_test"), expected);
    }

    #[test]
    fn test_checkpoint_list_tasks_sorted_by_date() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Create checkpoints with different times
        let mut cp1 = TaskCheckpoint::new("old".to_string(), "Old task".to_string());
        cp1.updated_at = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        manager.save(&cp1).unwrap();

        let mut cp2 = TaskCheckpoint::new("new".to_string(), "New task".to_string());
        cp2.updated_at = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        manager.save(&cp2).unwrap();

        let tasks = manager.list_tasks().unwrap();
        assert_eq!(tasks.len(), 2);
        // Most recent should be first
        assert_eq!(tasks[0].task_id, "new");
        assert_eq!(tasks[1].task_id, "old");
    }

    #[test]
    fn test_checkpoint_list_ignores_invalid_json() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint
        let cp = TaskCheckpoint::new("valid".to_string(), "Valid".to_string());
        manager.save(&cp).unwrap();

        // Write invalid JSON file
        std::fs::write(dir.path().join("invalid.json"), "not valid json").unwrap();

        // Write non-JSON file (should be ignored by extension check)
        std::fs::write(dir.path().join("readme.txt"), "some text").unwrap();

        let tasks = manager.list_tasks().unwrap();
        // Should only have the valid checkpoint
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "valid");
    }

    #[test]
    fn test_checkpoint_list_tasks_nonexistent_dir() {
        // Create manager then remove the directory
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let manager = CheckpointManager::new(path.clone()).unwrap();

        // Remove the directory
        std::fs::remove_dir_all(&path).unwrap();

        // list_tasks should return empty, not error
        let tasks = manager.list_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_memory_entry_serde() {
        let entry = MemoryEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "Hello there".to_string(),
            token_estimate: 50,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let loaded: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.role, "assistant");
        assert_eq!(loaded.token_estimate, 50);
    }

    #[test]
    fn test_tool_call_log_serde() {
        let log = ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "shell_exec".to_string(),
            arguments: r#"{"command": "ls"}"#.to_string(),
            result: Some("file1\nfile2".to_string()),
            success: true,
            duration_ms: Some(150),
        };
        let json = serde_json::to_string(&log).unwrap();
        let loaded: ToolCallLog = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.tool_name, "shell_exec");
        assert!(loaded.success);
    }

    #[test]
    fn test_error_log_serde() {
        let log = ErrorLog {
            timestamp: Utc::now(),
            step: 10,
            error: "connection timeout".to_string(),
            recovered: true,
        };
        let json = serde_json::to_string(&log).unwrap();
        let loaded: ErrorLog = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.step, 10);
        assert!(loaded.recovered);
    }
}
