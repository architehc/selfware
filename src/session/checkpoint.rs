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

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::api::types::Message;
use crate::redact;

/// Envelope that wraps a checkpoint with an integrity checksum.
///
/// The `sha256` field holds the hex-encoded SHA-256 hash of `payload` (the
/// compact-JSON serialized checkpoint data).  On load, the hash is recomputed
/// and compared to detect corruption or tampering.
#[derive(Debug, Serialize, Deserialize)]
struct CheckpointEnvelope {
    /// SHA-256 hex digest of the `payload` string
    sha256: String,
    /// The checkpoint data serialized as a JSON value
    payload: serde_json::Value,
}

impl CheckpointEnvelope {
    fn get_hmac_key() -> Vec<u8> {
        // Fallback for getting a unique machine key to tie the checkpoint to this system.
        let mut key = b"selfware-checkpoint-hmac-key".to_vec();
        if let Ok(name) = whoami::username() {
            key.extend_from_slice(name.as_bytes());
        }
        key
    }

    /// Create a new envelope by computing the HMAC-SHA-256 hash of the payload.
    fn wrap(payload: serde_json::Value) -> Result<Self> {
        use hmac::{Hmac, Mac};
        let canonical =
            serde_json::to_string(&payload).context("Failed to serialize payload for hashing")?;

        let mut mac = Hmac::<Sha256>::new_from_slice(&Self::get_hmac_key())
            .expect("HMAC can take key of any size");
        mac.update(canonical.as_bytes());
        let hash = hex::encode(mac.finalize().into_bytes());
        Ok(Self {
            sha256: hash,
            payload,
        })
    }

    /// Verify the integrity of the envelope by recomputing the HMAC.
    fn verify(&self) -> Result<()> {
        use hmac::{Hmac, Mac};
        let canonical = serde_json::to_string(&self.payload)
            .context("Failed to serialize payload for verification")?;

        let mut mac = Hmac::<Sha256>::new_from_slice(&Self::get_hmac_key())
            .expect("HMAC can take key of any size");
        mac.update(canonical.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());

        if expected != self.sha256 {
            bail!(
                "Checkpoint integrity check failed: expected HMAC {}, got {}",
                expected,
                self.sha256
            );
        }
        Ok(())
    }
}

/// Current version of the checkpoint format
pub const CURRENT_CHECKPOINT_VERSION: u32 = 1;

fn default_version() -> u32 {
    0 // Legacy checkpoints have version 0
}

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
    #[serde(default = "default_version")]
    pub version: u32,
    pub task_id: String,
    pub task_description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: TaskStatus,
    pub current_step: usize,
    #[serde(default)]
    pub current_iteration: usize,

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
            version: CURRENT_CHECKPOINT_VERSION,
            task_id,
            task_description,
            created_at: now,
            updated_at: now,
            status: TaskStatus::InProgress,
            current_step: 0,
            current_iteration: 0,
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

    /// Update the loop iteration count
    pub fn set_iteration(&mut self, iteration: usize) {
        self.current_iteration = iteration;
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

    /// Save a checkpoint to disk (with secrets redacted and integrity hash).
    ///
    /// Security: The checkpoint data is run through `redact::redact_json()`
    /// before writing, which scrubs API keys, passwords, bearer tokens, and
    /// other sensitive patterns from all serialized string values.  The
    /// `TaskCheckpoint` struct intentionally does not include config-level
    /// secrets such as `api_key` -- those live only in `Config`.
    ///
    /// Integrity: A SHA-256 checksum is computed over the JSON payload and
    /// stored in a wrapper envelope so that `load()` can verify the file has
    /// not been corrupted or tampered with.
    pub fn save(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let path = self.checkpoint_path(&checkpoint.task_id);

        // Serialize to JSON value first so we can redact secrets
        let mut json_value =
            serde_json::to_value(checkpoint).context("Failed to serialize checkpoint")?;

        // Redact any secrets in the checkpoint data
        redact::redact_json(&mut json_value);

        // Wrap in an integrity envelope with SHA-256 checksum
        let envelope =
            CheckpointEnvelope::wrap(json_value).context("Failed to create checkpoint envelope")?;

        let json =
            serde_json::to_string_pretty(&envelope).context("Failed to format checkpoint JSON")?;

        // Atomic write: write to a temp file in the same directory, then rename.
        // `rename` within the same filesystem is atomic on Unix/Windows.
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_path = path.with_extension(format!(
            "json.tmp.{}.{}.{}",
            checkpoint.task_id,
            std::process::id(),
            suffix
        ));
        {
            let mut tmp_file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)
                .with_context(|| format!("Failed to create checkpoint temp file {:?}", tmp_path))?;
            tmp_file
                .write_all(json.as_bytes())
                .with_context(|| format!("Failed to write checkpoint temp file {:?}", tmp_path))?;
            tmp_file
                .sync_all()
                .with_context(|| format!("Failed to fsync checkpoint temp file {:?}", tmp_path))?;
        }
        // Keep a backup of the previous checkpoint so it can be recovered
        // if the new one turns out to be corrupt or the rename is interrupted.
        if path.exists() {
            let backup_path = path.with_extension("json.bak");
            if let Err(e) = fs::rename(&path, &backup_path) {
                tracing::warn!("Failed to create checkpoint backup: {}", e);
                // Continue anyway — losing the backup is better than failing the save
            }
        }

        if let Err(err) = fs::rename(&tmp_path, &path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(err).with_context(|| {
                format!(
                    "Failed to atomically replace checkpoint {:?} from {:?}",
                    path, tmp_path
                )
            });
        }
        #[cfg(unix)]
        {
            if let Some(parent) = path.parent() {
                let dir = fs::OpenOptions::new()
                    .read(true)
                    .open(parent)
                    .with_context(|| {
                        format!("Failed to open checkpoint directory for fsync {:?}", parent)
                    })?;
                dir.sync_all().with_context(|| {
                    format!("Failed to fsync checkpoint directory {:?}", parent)
                })?;
            }
        }
        Ok(())
    }

    /// Load a checkpoint from disk, verifying integrity.
    ///
    /// Supports both the new envelope format (with SHA-256 checksum) and the
    /// legacy bare-checkpoint format for backward compatibility.
    ///
    /// If the primary file is corrupted (invalid JSON, truncated, failed
    /// integrity check), this automatically attempts recovery via
    /// [`recover_from_corruption`](Self::recover_from_corruption).
    pub fn load(&self, task_id: &str) -> Result<TaskCheckpoint> {
        let path = self.checkpoint_path(task_id);

        match self.try_load_from_path(&path) {
            Ok(checkpoint) => Ok(checkpoint),
            Err(primary_err) => {
                // The primary file is missing or corrupt -- attempt recovery.
                tracing::warn!(
                    "Primary checkpoint load failed for {:?}: {}. Attempting recovery.",
                    path,
                    primary_err
                );
                self.recover_from_corruption(task_id).with_context(|| {
                    format!(
                        "Recovery also failed for task '{}'. Original error: {}",
                        task_id, primary_err
                    )
                })
            }
        }
    }

    /// Attempt to load and verify a checkpoint from a specific path.
    fn try_load_from_path(&self, path: &std::path::Path) -> Result<TaskCheckpoint> {
        let json = fs::read_to_string(path)
            .with_context(|| format!("Failed to read checkpoint from {:?}", path))?;

        // Try to parse as an envelope first (new format with integrity check)
        if let Ok(envelope) = serde_json::from_str::<CheckpointEnvelope>(&json) {
            // Verify integrity before deserializing the payload
            envelope
                .verify()
                .with_context(|| format!("Checkpoint integrity check failed for {:?}", path))?;

            let checkpoint: TaskCheckpoint = serde_json::from_value(envelope.payload)
                .context("Failed to deserialize checkpoint from envelope payload")?;
            return Ok(checkpoint);
        }

        // Fall back to legacy format (bare checkpoint without envelope)
        let checkpoint: TaskCheckpoint =
            serde_json::from_str(&json).context("Failed to deserialize checkpoint")?;
        Ok(checkpoint)
    }

    /// Attempt to recover a corrupted checkpoint.
    ///
    /// Strategy:
    /// 1. Try loading from the `.json.bak` backup (created by [`save`]).
    /// 2. If the backup is also unusable, create a fresh checkpoint with the
    ///    task ID preserved so the caller can resume from a clean state.
    pub fn recover_from_corruption(&self, task_id: &str) -> Result<TaskCheckpoint> {
        let backup_path = self.checkpoint_path(task_id).with_extension("json.bak");

        // Attempt 1: try the backup file
        if backup_path.exists() {
            match self.try_load_from_path(&backup_path) {
                Ok(checkpoint) => {
                    tracing::info!(
                        "Recovered checkpoint for task '{}' from backup {:?}",
                        task_id,
                        backup_path
                    );
                    // Re-save the recovered checkpoint as the primary file so
                    // subsequent loads succeed without hitting recovery again.
                    if let Err(e) = self.save(&checkpoint) {
                        tracing::warn!(
                            "Failed to re-save recovered checkpoint for '{}': {}",
                            task_id,
                            e
                        );
                    }
                    return Ok(checkpoint);
                }
                Err(e) => {
                    tracing::warn!("Backup checkpoint {:?} is also corrupt: {}", backup_path, e);
                }
            }
        }

        // Attempt 2: create a fresh checkpoint so the caller can continue.
        tracing::warn!(
            "Creating fresh checkpoint for task '{}' after recovery failure",
            task_id
        );
        let fresh = TaskCheckpoint::new(task_id.to_string(), String::new());
        self.save(&fresh)
            .with_context(|| format!("Failed to save fresh checkpoint for '{}'", task_id))?;
        Ok(fresh)
    }

    /// Save a checkpoint with retry and exponential backoff.
    ///
    /// Attempts up to 3 saves with delays of 100 ms, 500 ms, and 2000 ms
    /// between failures.  Each failure is logged.  Returns the first success
    /// or the last error.
    pub fn save_with_retry(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        const DELAYS_MS: [u64; 3] = [100, 500, 2000];

        let mut last_err: Option<anyhow::Error> = None;

        for (attempt, delay_ms) in DELAYS_MS.iter().enumerate() {
            if attempt > 0 {
                if let Some(ref e) = last_err {
                    tracing::warn!(
                        "Checkpoint save attempt {}/3 failed for task '{}': {}. Retrying in {} ms.",
                        attempt,
                        checkpoint.task_id,
                        e,
                        delay_ms
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }

            match self.save(checkpoint) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Checkpoint save failed")))
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
                    // Try envelope format first, then legacy bare format
                    let checkpoint_opt = serde_json::from_str::<CheckpointEnvelope>(&json)
                        .ok()
                        .and_then(|env| {
                            env.verify().ok()?;
                            serde_json::from_value::<TaskCheckpoint>(env.payload).ok()
                        })
                        .or_else(|| serde_json::from_str::<TaskCheckpoint>(&json).ok());

                    if let Some(checkpoint) = checkpoint_opt {
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
        assert_eq!(checkpoint.current_iteration, 0);
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
    fn test_task_checkpoint_set_iteration() {
        let mut checkpoint = TaskCheckpoint::new("task_123".to_string(), "Test task".to_string());
        checkpoint.set_iteration(12);
        assert_eq!(checkpoint.current_iteration, 12);
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
        checkpoint.set_iteration(9);
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
        assert_eq!(loaded.current_iteration, 9);
        assert_eq!(loaded.status, TaskStatus::Paused);
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.tool_calls.len(), 1);
    }

    #[test]
    fn test_checkpoint_deserialize_without_iteration_defaults_zero() {
        let json = r#"{
            "task_id":"task_old",
            "task_description":"legacy",
            "created_at":"2026-01-01T00:00:00Z",
            "updated_at":"2026-01-01T00:00:00Z",
            "status":"in_progress",
            "current_step":2,
            "messages":[],
            "memory_entries":[],
            "estimated_tokens":0,
            "tool_calls":[],
            "errors":[],
            "git_checkpoint":null
        }"#;

        let loaded: TaskCheckpoint = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.current_step, 2);
        assert_eq!(loaded.current_iteration, 0);
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
    fn test_checkpoint_manager_load_nonexistent_recovers_fresh() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();
        // With recovery, loading a nonexistent task creates a fresh checkpoint
        let result = manager.load("nonexistent_task").unwrap();
        assert_eq!(result.task_id, "nonexistent_task");
        assert_eq!(result.task_description, "");
        assert_eq!(result.status, TaskStatus::InProgress);
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

    // ---- Checkpoint integrity tests ----

    #[test]
    fn test_checkpoint_envelope_round_trip() {
        let payload = serde_json::json!({"task_id": "test", "data": "hello"});
        let envelope = CheckpointEnvelope::wrap(payload.clone()).unwrap();
        assert!(!envelope.sha256.is_empty());
        assert_eq!(envelope.payload, payload);
        assert!(envelope.verify().is_ok());
    }

    #[test]
    fn test_checkpoint_envelope_detects_tampering() {
        let payload = serde_json::json!({"task_id": "test", "data": "hello"});
        let mut envelope = CheckpointEnvelope::wrap(payload).unwrap();
        // Tamper with the payload
        envelope.payload = serde_json::json!({"task_id": "test", "data": "TAMPERED"});
        assert!(envelope.verify().is_err());
    }

    #[test]
    fn test_checkpoint_envelope_detects_bad_hash() {
        let payload = serde_json::json!({"task_id": "test"});
        let mut envelope = CheckpointEnvelope::wrap(payload).unwrap();
        // Corrupt the hash
        envelope.sha256 =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        assert!(envelope.verify().is_err());
    }

    #[test]
    fn test_save_load_with_integrity() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let checkpoint =
            TaskCheckpoint::new("integrity_test".to_string(), "Integrity test".to_string());
        manager.save(&checkpoint).unwrap();

        // Load should succeed and verify integrity
        let loaded = manager.load("integrity_test").unwrap();
        assert_eq!(loaded.task_id, "integrity_test");
        assert_eq!(loaded.task_description, "Integrity test");
    }

    #[test]
    fn test_load_detects_corrupted_file_and_recovers() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint
        let checkpoint =
            TaskCheckpoint::new("corrupt_test".to_string(), "Corruption test".to_string());
        manager.save(&checkpoint).unwrap();

        // Corrupt the file by modifying the payload while keeping envelope structure
        let path = dir.path().join("corrupt_test.json");
        let content = std::fs::read_to_string(&path).unwrap();
        let mut envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
        envelope["payload"]["task_description"] = serde_json::Value::String("TAMPERED".to_string());
        std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();

        // Load should detect corruption and recover with a fresh checkpoint
        // (no backup exists, so recovery creates a new empty one)
        let result = manager.load("corrupt_test").unwrap();
        assert_eq!(result.task_id, "corrupt_test");
        // The description is empty because recovery created a fresh checkpoint
        assert_eq!(result.task_description, "");
    }

    #[test]
    fn test_try_load_from_path_detects_integrity_error() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint
        let checkpoint =
            TaskCheckpoint::new("direct_test".to_string(), "Direct load test".to_string());
        manager.save(&checkpoint).unwrap();

        // Corrupt the file
        let path = dir.path().join("direct_test.json");
        let content = std::fs::read_to_string(&path).unwrap();
        let mut envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
        envelope["payload"]["task_description"] = serde_json::Value::String("TAMPERED".to_string());
        std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();

        // try_load_from_path should fail with integrity error
        let result = manager.try_load_from_path(&path);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("integrity"),
            "Expected integrity error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_load_legacy_format_backward_compatible() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Write a legacy-format checkpoint (bare JSON without envelope)
        let checkpoint =
            TaskCheckpoint::new("legacy_test".to_string(), "Legacy format".to_string());
        let bare_json = serde_json::to_string_pretty(&checkpoint).unwrap();
        let path = dir.path().join("legacy_test.json");
        std::fs::write(&path, bare_json).unwrap();

        // Load should succeed via legacy fallback
        let loaded = manager.load("legacy_test").unwrap();
        assert_eq!(loaded.task_id, "legacy_test");
        assert_eq!(loaded.task_description, "Legacy format");
    }

    #[test]
    fn test_save_redacts_secrets_in_messages() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let mut checkpoint =
            TaskCheckpoint::new("redact_test".to_string(), "Secret redaction".to_string());
        checkpoint.messages.push(Message::user(
            "Use api_key=sk-secretkey12345678901234567890 to connect",
        ));
        manager.save(&checkpoint).unwrap();

        // Read the raw file and verify secrets are redacted
        let path = dir.path().join("redact_test.json");
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("sk-secretkey12345678901234567890"),
            "API key should have been redacted in checkpoint file"
        );
        assert!(raw.contains("[REDACTED]"));
    }

    #[test]
    fn test_list_tasks_handles_envelope_format() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save with new envelope format
        let cp1 = TaskCheckpoint::new("env_task".to_string(), "Envelope task".to_string());
        manager.save(&cp1).unwrap();

        // Also write a legacy bare-format file
        let cp2 = TaskCheckpoint::new("bare_task".to_string(), "Bare task".to_string());
        let bare_json = serde_json::to_string_pretty(&cp2).unwrap();
        std::fs::write(dir.path().join("bare_task.json"), bare_json).unwrap();

        let tasks = manager.list_tasks().unwrap();
        assert_eq!(tasks.len(), 2);
        let ids: Vec<&str> = tasks.iter().map(|t| t.task_id.as_str()).collect();
        assert!(ids.contains(&"env_task"));
        assert!(ids.contains(&"bare_task"));
    }

    // ---- Corruption recovery tests ----

    #[test]
    fn test_recover_from_corruption_uses_backup() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint (this also creates the primary file)
        let checkpoint =
            TaskCheckpoint::new("recover_bak".to_string(), "Backup recovery".to_string());
        manager.save(&checkpoint).unwrap();

        // Manually create a backup copy of the valid file
        let primary = dir.path().join("recover_bak.json");
        let backup = dir.path().join("recover_bak.json.bak");
        std::fs::copy(&primary, &backup).unwrap();

        // Now corrupt the primary file
        std::fs::write(&primary, "THIS IS NOT JSON").unwrap();

        // Load should recover from the backup
        let loaded = manager.load("recover_bak").unwrap();
        assert_eq!(loaded.task_id, "recover_bak");
        assert_eq!(loaded.task_description, "Backup recovery");
    }

    #[test]
    fn test_recover_from_corruption_creates_fresh_when_no_backup() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Write a corrupt primary file with no backup
        let primary = dir.path().join("no_bak.json");
        std::fs::write(&primary, "CORRUPT DATA").unwrap();

        // Load should create a fresh checkpoint
        let loaded = manager.load("no_bak").unwrap();
        assert_eq!(loaded.task_id, "no_bak");
        assert_eq!(loaded.task_description, "");
        assert_eq!(loaded.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_recover_from_corruption_creates_fresh_when_backup_also_corrupt() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Write corrupt primary and corrupt backup
        let primary = dir.path().join("both_bad.json");
        let backup = dir.path().join("both_bad.json.bak");
        std::fs::write(&primary, "CORRUPT").unwrap();
        std::fs::write(&backup, "ALSO CORRUPT").unwrap();

        // Load should create a fresh checkpoint
        let loaded = manager.load("both_bad").unwrap();
        assert_eq!(loaded.task_id, "both_bad");
        assert_eq!(loaded.task_description, "");
    }

    #[test]
    fn test_recover_from_corruption_resaves_recovered() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint
        let checkpoint = TaskCheckpoint::new(
            "resave_test".to_string(),
            "Resave after recovery".to_string(),
        );
        manager.save(&checkpoint).unwrap();

        // Create backup, then corrupt primary
        let primary = dir.path().join("resave_test.json");
        let backup = dir.path().join("resave_test.json.bak");
        std::fs::copy(&primary, &backup).unwrap();
        std::fs::write(&primary, "CORRUPT").unwrap();

        // First load triggers recovery
        let loaded = manager.load("resave_test").unwrap();
        assert_eq!(loaded.task_description, "Resave after recovery");

        // Remove backup; second load should succeed from re-saved primary
        std::fs::remove_file(&backup).unwrap();
        let loaded2 = manager.load("resave_test").unwrap();
        assert_eq!(loaded2.task_description, "Resave after recovery");
    }

    #[test]
    fn test_recover_detects_integrity_failure_and_falls_back() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint
        let checkpoint = TaskCheckpoint::new(
            "integrity_recover".to_string(),
            "Integrity recovery".to_string(),
        );
        manager.save(&checkpoint).unwrap();

        // Create a good backup
        let primary = dir.path().join("integrity_recover.json");
        let backup = dir.path().join("integrity_recover.json.bak");
        std::fs::copy(&primary, &backup).unwrap();

        // Tamper with primary envelope payload (valid JSON but bad hash)
        let content = std::fs::read_to_string(&primary).unwrap();
        let mut envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
        envelope["payload"]["task_description"] = serde_json::Value::String("TAMPERED".to_string());
        std::fs::write(&primary, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();

        // Load should detect integrity failure and recover from backup
        let loaded = manager.load("integrity_recover").unwrap();
        assert_eq!(loaded.task_description, "Integrity recovery");
    }

    // ---- Retry logic tests ----

    #[test]
    fn test_save_with_retry_succeeds_immediately() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let checkpoint = TaskCheckpoint::new("retry_ok".to_string(), "Retry success".to_string());
        manager.save_with_retry(&checkpoint).unwrap();

        let loaded = manager.load("retry_ok").unwrap();
        assert_eq!(loaded.task_id, "retry_ok");
    }

    #[test]
    fn test_save_with_retry_fails_on_readonly_dir() {
        // Create a directory and make it read-only so saves fail
        let dir = tempdir().unwrap();
        let readonly_dir = dir.path().join("readonly_checkpoints");
        std::fs::create_dir_all(&readonly_dir).unwrap();
        let manager = CheckpointManager::new(readonly_dir.clone()).unwrap();

        // Make directory read-only
        let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        {
            perms.set_readonly(true);
        }
        std::fs::set_permissions(&readonly_dir, perms.clone()).unwrap();

        let checkpoint = TaskCheckpoint::new(
            "retry_fail".to_string(),
            "Should fail all retries".to_string(),
        );
        let result = manager.save_with_retry(&checkpoint);
        assert!(result.is_err());

        // Restore permissions so tempdir cleanup works
        #[allow(clippy::permissions_set_readonly_false)]
        {
            perms.set_readonly(false);
        }
        std::fs::set_permissions(&readonly_dir, perms).unwrap();
    }
}
