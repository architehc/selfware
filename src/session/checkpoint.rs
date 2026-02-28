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
        let path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("selfware")
            .join("checkpoint_hmac_key");

        if let Ok(key) = std::fs::read(&path) {
            if key.len() == 32 {
                return key;
            }
        }

        let mut key = vec![0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut key);

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
            {
                let _ = std::io::Write::write_all(&mut file, &key);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = std::fs::write(&path, &key);
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitCheckpointInfo {
    pub branch: String,
    pub commit_hash: String,
    pub dirty: bool,
    pub staged_files: Vec<String>,
    pub modified_files: Vec<String>,
}

/// Represents the delta/diff between two checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDelta {
    pub task_id: String,
    pub base_version: u32,
    pub target_version: u32,

    // Updates
    pub updated_at: DateTime<Utc>,
    pub status: Option<TaskStatus>,
    pub current_step: Option<usize>,
    pub current_iteration: Option<usize>,

    // Context additions (we only append messages in the context window)
    pub new_messages: Vec<Message>,
    pub new_memory_entries: Vec<MemoryEntry>,
    pub new_tool_calls: Vec<ToolCallLog>,
    pub new_errors: Vec<ErrorLog>,

    pub updated_tokens: Option<usize>,
    pub git_checkpoint: Option<GitCheckpointInfo>,
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

impl TaskCheckpoint {
    fn touch(&mut self) {
        self.version = self.version.saturating_add(1);
        self.updated_at = Utc::now();
    }

    /// Computes a differential payload to reduce disk IO during saves
    pub fn compute_delta(&self, base: &TaskCheckpoint) -> Option<CheckpointDelta> {
        if self.task_id != base.task_id || self.version <= base.version {
            return None;
        }

        let status = (self.status != base.status).then_some(self.status.clone());
        let current_step = (self.current_step != base.current_step).then_some(self.current_step);
        let current_iteration =
            (self.current_iteration != base.current_iteration).then_some(self.current_iteration);
        let updated_tokens =
            (self.estimated_tokens != base.estimated_tokens).then_some(self.estimated_tokens);
        if self.git_checkpoint != base.git_checkpoint && self.git_checkpoint.is_none() {
            // Delta format cannot encode "explicitly clear git checkpoint".
            // Force a full checkpoint write for this transition.
            return None;
        }
        let git_checkpoint = (self.git_checkpoint != base.git_checkpoint)
            .then(|| self.git_checkpoint.clone())
            .flatten();

        // Only capture appended elements. If vectors shrank or changed in place, prefer full save.
        let new_messages = if self.messages.len() >= base.messages.len() {
            self.messages[base.messages.len()..].to_vec()
        } else {
            return None;
        };
        let new_memory_entries = if self.memory_entries.len() >= base.memory_entries.len() {
            self.memory_entries[base.memory_entries.len()..].to_vec()
        } else {
            return None;
        };
        let new_tool_calls = if self.tool_calls.len() >= base.tool_calls.len() {
            self.tool_calls[base.tool_calls.len()..].to_vec()
        } else {
            return None;
        };
        let new_errors = if self.errors.len() >= base.errors.len() {
            self.errors[base.errors.len()..].to_vec()
        } else {
            return None;
        };

        let has_changes = status.is_some()
            || current_step.is_some()
            || current_iteration.is_some()
            || !new_messages.is_empty()
            || !new_memory_entries.is_empty()
            || !new_tool_calls.is_empty()
            || !new_errors.is_empty()
            || updated_tokens.is_some()
            || git_checkpoint.is_some();

        if !has_changes {
            return None;
        }

        Some(CheckpointDelta {
            task_id: self.task_id.clone(),
            base_version: base.version,
            target_version: self.version,
            updated_at: self.updated_at,
            status,
            current_step,
            current_iteration,
            new_messages,
            new_memory_entries,
            new_tool_calls,
            new_errors,
            updated_tokens,
            git_checkpoint,
        })
    }

    /// Applies a delta to an existing checkpoint to hydrate the full state
    pub fn apply_delta(&mut self, delta: &CheckpointDelta) -> Result<()> {
        if self.task_id != delta.task_id {
            return Err(anyhow::anyhow!("Delta task ID mismatch"));
        }
        if self.version != delta.base_version {
            return Err(anyhow::anyhow!(
                "Delta base version mismatch: expected {}, got {}",
                self.version,
                delta.base_version
            ));
        }

        self.version = delta.target_version;
        self.updated_at = delta.updated_at;

        if let Some(ref status) = delta.status {
            self.status = status.clone();
        }
        if let Some(step) = delta.current_step {
            self.current_step = step;
        }
        if let Some(iter) = delta.current_iteration {
            self.current_iteration = iter;
        }
        self.messages.extend(delta.new_messages.clone());
        self.memory_entries.extend(delta.new_memory_entries.clone());
        self.tool_calls.extend(delta.new_tool_calls.clone());
        self.errors.extend(delta.new_errors.clone());

        if let Some(tokens) = delta.updated_tokens {
            self.estimated_tokens = tokens;
        }
        if let Some(ref git) = delta.git_checkpoint {
            self.git_checkpoint = Some(git.clone());
        }

        Ok(())
    }
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
        self.touch();
    }

    /// Add an error log entry
    pub fn log_error(&mut self, step: usize, error: String, recovered: bool) {
        self.errors.push(ErrorLog {
            timestamp: Utc::now(),
            step,
            error,
            recovered,
        });
        self.touch();
    }

    /// Update the step
    pub fn set_step(&mut self, step: usize) {
        self.current_step = step;
        self.touch();
    }

    /// Update the loop iteration count
    pub fn set_iteration(&mut self, iteration: usize) {
        self.current_iteration = iteration;
        self.touch();
    }

    /// Update the status
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.touch();
    }

    /// Update messages
    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.touch();
    }

    /// Update token estimate and bump checkpoint version.
    pub fn set_estimated_tokens(&mut self, estimated_tokens: usize) {
        self.estimated_tokens = estimated_tokens;
        self.touch();
    }
}

/// Manager for saving and loading task checkpoints
pub struct CheckpointManager {
    checkpoints_dir: PathBuf,
}

/// Maximum number of incremental deltas before forcing a compacted full write.
const MAX_DELTA_ENTRIES_BEFORE_COMPACT: usize = 24;
/// Maximum delta log size before forcing compaction.
const MAX_DELTA_FILE_BYTES: u64 = 512 * 1024;

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

    /// Get the path for a checkpoint delta log
    fn checkpoint_delta_path(&self, task_id: &str) -> PathBuf {
        self.checkpoints_dir
            .join(format!("{}.delta.jsonl", task_id))
    }

    /// Save a checkpoint to disk (with secrets redacted and integrity hash).
    ///
    /// Security: The checkpoint data is run through `redact::redact_json()`
    /// before writing, which scrubs API keys, passwords, bearer tokens, and
    /// other sensitive patterns from all serialized string values.  The
    /// `TaskCheckpoint` struct intentionally does not include config-level
    /// secrets such as `api_key` -- those live only in `Config`.
    ///
    /// Integrity: An HMAC-SHA-256 digest is computed over the JSON payload and
    /// stored in a wrapper envelope so that `load()` can verify the file has
    /// not been corrupted or tampered with.
    pub fn save(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let full_path = self.checkpoint_path(&checkpoint.task_id);

        // Prefer a compact delta write when possible to reduce SSD wear.
        if full_path.exists() {
            if let Ok(mut base) = self.try_load_from_path(&full_path) {
                if let Err(e) = self.apply_deltas(&checkpoint.task_id, &mut base) {
                    tracing::warn!(
                        "Failed to hydrate checkpoint with deltas before save ({}). Falling back to full save.",
                        e
                    );
                    self.save_full_checkpoint(checkpoint)?;
                    self.clear_delta_log(&checkpoint.task_id)?;
                    return Ok(());
                }

                if let Some(delta) = checkpoint.compute_delta(&base) {
                    if self.delta_is_efficient(checkpoint, &delta)? {
                        match self.append_delta(&checkpoint.task_id, &delta) {
                            Ok(()) => {
                                if self.should_compact_deltas(&checkpoint.task_id)? {
                                    self.save_full_checkpoint(checkpoint)?;
                                    self.clear_delta_log(&checkpoint.task_id)?;
                                }
                                return Ok(());
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to append checkpoint delta: {}. Falling back to full save.",
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fallback to full checkpoint write when no efficient delta exists.
        self.save_full_checkpoint(checkpoint)?;
        self.clear_delta_log(&checkpoint.task_id)?;
        Ok(())
    }

    fn delta_is_efficient(
        &self,
        checkpoint: &TaskCheckpoint,
        delta: &CheckpointDelta,
    ) -> Result<bool> {
        let full_size = serde_json::to_vec(checkpoint)
            .context("Failed to estimate full checkpoint size")?
            .len();
        let delta_size = serde_json::to_vec(delta)
            .context("Failed to estimate checkpoint delta size")?
            .len();

        // Require a meaningful reduction, not just a few bytes.
        Ok(delta_size + 128 < full_size)
    }

    fn append_delta(&self, task_id: &str, delta: &CheckpointDelta) -> Result<()> {
        let path = self.checkpoint_delta_path(task_id);
        let mut json_value =
            serde_json::to_value(delta).context("Failed to serialize checkpoint delta")?;
        redact::redact_json(&mut json_value);
        let envelope = CheckpointEnvelope::wrap(json_value)
            .context("Failed to create checkpoint delta envelope")?;
        let line = serde_json::to_string(&envelope)
            .context("Failed to serialize checkpoint delta envelope")?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open checkpoint delta log {:?}", path))?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("Failed to write checkpoint delta log {:?}", path))?;
        file.write_all(b"\n")
            .with_context(|| format!("Failed to write checkpoint delta newline {:?}", path))?;
        file.sync_all()
            .with_context(|| format!("Failed to fsync checkpoint delta log {:?}", path))?;
        Ok(())
    }

    fn should_compact_deltas(&self, task_id: &str) -> Result<bool> {
        let path = self.checkpoint_delta_path(task_id);
        if !path.exists() {
            return Ok(false);
        }

        let metadata = fs::metadata(&path)
            .with_context(|| format!("Failed to stat checkpoint delta log {:?}", path))?;
        if metadata.len() > MAX_DELTA_FILE_BYTES {
            return Ok(true);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint delta log {:?}", path))?;
        let line_count = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        Ok(line_count >= MAX_DELTA_ENTRIES_BEFORE_COMPACT)
    }

    fn clear_delta_log(&self, task_id: &str) -> Result<()> {
        let delta_path = self.checkpoint_delta_path(task_id);
        if delta_path.exists() {
            fs::remove_file(&delta_path).with_context(|| {
                format!("Failed to delete checkpoint delta log {:?}", delta_path)
            })?;
        }
        Ok(())
    }

    fn save_full_checkpoint(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let path = self.checkpoint_path(&checkpoint.task_id);

        // Serialize to JSON value first so we can redact secrets
        let mut json_value =
            serde_json::to_value(checkpoint).context("Failed to serialize checkpoint")?;

        // Redact any secrets in the checkpoint data
        redact::redact_json(&mut json_value);

        // Wrap in an integrity envelope
        let envelope =
            CheckpointEnvelope::wrap(json_value).context("Failed to create checkpoint envelope")?;

        let json =
            serde_json::to_string_pretty(&envelope).context("Failed to format checkpoint JSON")?;

        // Atomic write: write to a temp file in the same directory, then rename.
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
        if path.exists() {
            let backup_path = path.with_extension("json.bak");
            if let Err(e) = fs::rename(&path, &backup_path) {
                tracing::warn!("Failed to create checkpoint backup: {}", e);
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
    /// Supports both the new envelope format (with HMAC integrity digest) and the
    /// legacy bare-checkpoint format for backward compatibility.
    ///
    /// If the primary file is corrupted (invalid JSON, truncated, failed
    /// integrity check), this automatically attempts recovery via
    /// [`recover_from_corruption`](Self::recover_from_corruption).
    pub fn load(&self, task_id: &str) -> Result<TaskCheckpoint> {
        let path = self.checkpoint_path(task_id);

        match self.try_load_from_path(&path).and_then(|mut checkpoint| {
            self.apply_deltas(task_id, &mut checkpoint)?;
            Ok(checkpoint)
        }) {
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

    fn apply_deltas(&self, task_id: &str, checkpoint: &mut TaskCheckpoint) -> Result<()> {
        let path = self.checkpoint_delta_path(task_id);
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint delta log {:?}", path))?;
        for (line_no, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let delta = if let Ok(envelope) = serde_json::from_str::<CheckpointEnvelope>(line) {
                envelope.verify().with_context(|| {
                    format!(
                        "Checkpoint delta integrity check failed for {:?} line {}",
                        path,
                        line_no + 1
                    )
                })?;
                serde_json::from_value::<CheckpointDelta>(envelope.payload).with_context(|| {
                    format!(
                        "Failed to deserialize checkpoint delta from {:?} line {}",
                        path,
                        line_no + 1
                    )
                })?
            } else {
                serde_json::from_str::<CheckpointDelta>(line).with_context(|| {
                    format!(
                        "Failed to deserialize legacy checkpoint delta from {:?} line {}",
                        path,
                        line_no + 1
                    )
                })?
            };

            checkpoint.apply_delta(&delta).with_context(|| {
                format!(
                    "Failed to apply checkpoint delta from {:?} line {}",
                    path,
                    line_no + 1
                )
            })?;
        }

        Ok(())
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
    /// 1. Try loading from the `.json.bak` backup (created by [`Self::save`]).
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
                if let Ok(mut checkpoint) = self.try_load_from_path(&path) {
                    if let Some(task_id) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Err(e) = self.apply_deltas(task_id, &mut checkpoint) {
                            tracing::warn!(
                                "Skipping checkpoint {:?} due to invalid deltas: {}",
                                path,
                                e
                            );
                            continue;
                        }
                    }
                    summaries.push(checkpoint.to_summary());
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
        let backup_path = path.with_extension("json.bak");
        if backup_path.exists() {
            fs::remove_file(&backup_path).with_context(|| {
                format!("Failed to delete checkpoint backup: {:?}", backup_path)
            })?;
        }
        let delta_path = self.checkpoint_delta_path(task_id);
        if delta_path.exists() {
            fs::remove_file(&delta_path).with_context(|| {
                format!("Failed to delete checkpoint delta log: {:?}", delta_path)
            })?;
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
    fn test_checkpoint_delta_round_trip() {
        let mut base = TaskCheckpoint::new("task_delta".to_string(), "Delta test".to_string());
        base.set_messages(vec![Message::user("hello")]);
        base.set_step(1);

        let mut next = base.clone();
        next.set_iteration(2);
        next.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(10),
        });

        let delta = next.compute_delta(&base).unwrap();
        let mut hydrated = base.clone();
        hydrated.apply_delta(&delta).unwrap();

        assert_eq!(hydrated.current_iteration, next.current_iteration);
        assert_eq!(hydrated.tool_calls.len(), next.tool_calls.len());
        assert_eq!(hydrated.version, next.version);
    }

    #[test]
    fn test_checkpoint_manager_replays_delta_log() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let mut checkpoint =
            TaskCheckpoint::new("task_delta_mgr".to_string(), "Delta manager".to_string());
        let mut large_messages = Vec::new();
        for i in 0..30 {
            large_messages.push(Message::user(format!("message-{} {}", i, "x".repeat(120))));
        }
        checkpoint.set_messages(large_messages);
        manager.save(&checkpoint).unwrap();

        checkpoint.set_step(2);
        checkpoint.set_iteration(3);
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "shell_exec".to_string(),
            arguments: "{\"command\":\"true\"}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(1),
        });
        manager.save(&checkpoint).unwrap();

        let delta_path = manager.checkpoint_delta_path("task_delta_mgr");
        assert!(delta_path.exists(), "expected delta log to exist");

        let loaded = manager.load("task_delta_mgr").unwrap();
        assert_eq!(loaded.current_step, 2);
        assert_eq!(loaded.current_iteration, 3);
        assert_eq!(loaded.tool_calls.len(), 1);
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
