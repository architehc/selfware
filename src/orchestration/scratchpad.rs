//! Scratchpad for Coordinator Mode
//!
//! Provides durable, shared state storage for coordinator-worker communication.
//! Persisted to disk at `.selfware/scratchpad/{task_id}/` for durability across
//! agent restarts.

#![allow(dead_code)] // Work-in-progress: Coordinator mode not yet fully integrated

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// A single entry in the scratchpad
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScratchpadEntry {
    /// Unique key for this entry
    pub key: String,
    /// Value stored (JSON string or plain text)
    pub value: String,
    /// Agent ID that created/updated this entry
    pub author: String,
    /// ISO 8601 timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional metadata (e.g., entry type, priority)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl ScratchpadEntry {
    /// Create a new scratchpad entry
    pub fn new(
        key: impl Into<String>,
        value: impl Into<String>,
        author: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            author: author.into(),
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    /// Create with metadata
    pub fn with_metadata(
        key: impl Into<String>,
        value: impl Into<String>,
        author: impl Into<String>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            author: author.into(),
            timestamp: Utc::now(),
            metadata: Some(metadata),
        }
    }

    /// Add metadata to an existing entry (builder pattern)
    pub fn set_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get age of entry in seconds
    pub fn age_secs(&self) -> i64 {
        Utc::now()
            .signed_duration_since(self.timestamp)
            .num_seconds()
    }
}

/// Worker status for tracking active workers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerStatus {
    /// Worker is initializing
    Initializing,
    /// Worker is actively working
    Working,
    /// Worker has completed its task
    Completed,
    /// Worker encountered an error
    Failed,
    /// Worker was terminated
    Terminated,
}

impl std::fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerStatus::Initializing => write!(f, "initializing"),
            WorkerStatus::Working => write!(f, "working"),
            WorkerStatus::Completed => write!(f, "completed"),
            WorkerStatus::Failed => write!(f, "failed"),
            WorkerStatus::Terminated => write!(f, "terminated"),
        }
    }
}

/// Worker information stored in scratchpad
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    /// Worker ID
    pub id: String,
    /// Worker role/description
    pub role: String,
    /// Current status
    pub status: WorkerStatus,
    /// Task assignment
    pub task: String,
    /// Parent worker (if spawned by another worker)
    pub parent_id: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
    /// Completion timestamp (if finished)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

impl WorkerInfo {
    /// Create new worker info
    pub fn new(id: impl Into<String>, role: impl Into<String>, task: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            role: role.into(),
            status: WorkerStatus::Initializing,
            task: task.into(),
            parent_id: None,
            created_at: now,
            last_activity: now,
            completed_at: None,
        }
    }

    /// Set parent worker (for hierarchical spawning)
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Update status
    pub fn set_status(&mut self, status: WorkerStatus) {
        self.status = status;
        self.last_activity = Utc::now();
        if status == WorkerStatus::Completed || status == WorkerStatus::Failed {
            self.completed_at = Some(Utc::now());
        }
    }

    /// Check if worker has finished (completed or failed)
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            WorkerStatus::Completed | WorkerStatus::Failed | WorkerStatus::Terminated
        )
    }

    /// Get duration in seconds since creation
    pub fn duration_secs(&self) -> i64 {
        if let Some(completed) = self.completed_at {
            completed
                .signed_duration_since(self.created_at)
                .num_seconds()
        } else {
            Utc::now()
                .signed_duration_since(self.created_at)
                .num_seconds()
        }
    }
}

/// Shared state scratchpad for coordinator-worker communication
///
/// The scratchpad provides a durable key-value store that persists to disk,
/// allowing workers to share findings and coordinator to synthesize results.
/// All operations are thread-safe.
#[derive(Debug, Clone)]
pub struct Scratchpad {
    task_id: String,
    base_path: PathBuf,
    entries: Arc<RwLock<HashMap<String, ScratchpadEntry>>>,
    workers: Arc<RwLock<HashMap<String, WorkerInfo>>>,
}

impl Scratchpad {
    /// Create or load a scratchpad for the given task
    pub fn for_task(task_id: impl Into<String>) -> Result<Self> {
        let task_id = task_id.into();
        let base_path = Self::scratchpad_path(&task_id);

        // Ensure directory exists
        std::fs::create_dir_all(&base_path)
            .with_context(|| format!("Failed to create scratchpad directory: {:?}", base_path))?;

        let scratchpad = Self {
            task_id: task_id.clone(),
            base_path,
            entries: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load existing entries if any
        scratchpad.load()?;

        Ok(scratchpad)
    }

    /// Get the default scratchpad base directory
    pub fn base_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".selfware")
            .join("scratchpad")
    }

    /// Get scratchpad path for a task
    fn scratchpad_path(task_id: &str) -> PathBuf {
        Self::base_dir().join(task_id)
    }

    /// Get entries file path
    fn entries_path(&self) -> PathBuf {
        self.base_path.join("entries.json")
    }

    /// Get workers file path
    fn workers_path(&self) -> PathBuf {
        self.base_path.join("workers.json")
    }

    /// Load all entries from disk
    fn load(&self) -> Result<()> {
        // Load entries
        let entries_path = self.entries_path();
        if entries_path.exists() {
            let content = std::fs::read_to_string(&entries_path)?;
            let loaded: HashMap<String, ScratchpadEntry> = serde_json::from_str(&content)?;
            let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
            *entries = loaded;
        }

        // Load workers
        let workers_path = self.workers_path();
        if workers_path.exists() {
            let content = std::fs::read_to_string(&workers_path)?;
            let loaded: HashMap<String, WorkerInfo> = serde_json::from_str(&content)?;
            let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
            *workers = loaded;
        }

        Ok(())
    }

    /// Persist all entries to disk
    pub fn persist(&self) -> Result<()> {
        // Persist entries
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let entries_json = serde_json::to_string_pretty(&*entries)?;
        std::fs::write(self.entries_path(), entries_json)?;

        // Persist workers
        let workers = self.workers.read().unwrap_or_else(|e| e.into_inner());
        let workers_json = serde_json::to_string_pretty(&*workers)?;
        std::fs::write(self.workers_path(), workers_json)?;

        Ok(())
    }

    /// Write an entry to the scratchpad
    pub fn write(&self, entry: ScratchpadEntry) -> Result<()> {
        {
            let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
            entries.insert(entry.key.clone(), entry);
        }
        self.persist()?;
        Ok(())
    }

    /// Read an entry by key
    pub fn read(&self, key: &str) -> Option<ScratchpadEntry> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        entries.get(key).cloned()
    }

    /// Read entry value as a specific type
    pub fn read_typed<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>> {
        match self.read(key) {
            Some(entry) => {
                let value: T = serde_json::from_str(&entry.value)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Delete an entry
    pub fn delete(&self, key: &str) -> Result<bool> {
        let existed = {
            let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
            entries.remove(key).is_some()
        };
        if existed {
            self.persist()?;
        }
        Ok(existed)
    }

    /// List all entry keys
    pub fn list_keys(&self) -> Vec<String> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        entries.keys().cloned().collect()
    }

    /// List entries by prefix
    pub fn list_by_prefix(&self, prefix: &str) -> Vec<ScratchpadEntry> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        entries
            .values()
            .filter(|e| e.key.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// List entries by author
    pub fn list_by_author(&self, author: &str) -> Vec<ScratchpadEntry> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        entries
            .values()
            .filter(|e| e.author == author)
            .cloned()
            .collect()
    }

    /// Get all entries
    pub fn all_entries(&self) -> Vec<ScratchpadEntry> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        entries.values().cloned().collect()
    }

    /// Register a worker
    pub fn register_worker(&self, worker: WorkerInfo) -> Result<()> {
        {
            let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
            workers.insert(worker.id.clone(), worker);
        }
        self.persist()?;
        Ok(())
    }

    /// Get worker info
    pub fn get_worker(&self, worker_id: &str) -> Option<WorkerInfo> {
        let workers = self.workers.read().unwrap_or_else(|e| e.into_inner());
        workers.get(worker_id).cloned()
    }

    /// Update worker status
    pub fn update_worker_status(&self, worker_id: &str, status: WorkerStatus) -> Result<()> {
        {
            let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
            if let Some(worker) = workers.get_mut(worker_id) {
                worker.set_status(status);
            } else {
                return Err(anyhow!("Worker not found: {}", worker_id));
            }
        }
        self.persist()?;
        Ok(())
    }

    /// List all workers
    pub fn list_workers(&self) -> Vec<WorkerInfo> {
        let workers = self.workers.read().unwrap_or_else(|e| e.into_inner());
        workers.values().cloned().collect()
    }

    /// List active workers (not finished)
    pub fn active_workers(&self) -> Vec<WorkerInfo> {
        self.list_workers()
            .into_iter()
            .filter(|w| !w.is_finished())
            .collect()
    }

    /// List workers by status
    pub fn workers_by_status(&self, status: WorkerStatus) -> Vec<WorkerInfo> {
        self.list_workers()
            .into_iter()
            .filter(|w| w.status == status)
            .collect()
    }

    /// Remove a worker
    pub fn remove_worker(&self, worker_id: &str) -> Result<bool> {
        let existed = {
            let mut workers = self.workers.write().unwrap_or_else(|e| e.into_inner());
            workers.remove(worker_id).is_some()
        };
        if existed {
            self.persist()?;
        }
        Ok(existed)
    }

    /// Wait for a worker to complete (blocking)
    /// Returns the final worker info
    pub fn await_worker(&self, worker_id: &str, timeout_ms: u64) -> Result<Option<WorkerInfo>> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if let Some(worker) = self.get_worker(worker_id) {
                if worker.is_finished() {
                    return Ok(Some(worker));
                }
            } else {
                return Err(anyhow!("Worker not found: {}", worker_id));
            }
            // Reload from disk to see updates from other processes
            self.load()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Timeout
        Ok(self.get_worker(worker_id))
    }

    /// Get the task ID
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Get the base path
    pub fn path(&self) -> &Path {
        &self.base_path
    }

    /// Clean up scratchpad files
    pub fn cleanup(&self) -> Result<()> {
        std::fs::remove_dir_all(&self.base_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_scratchpad() -> (Scratchpad, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        // Override base dir for testing
        let task_id = format!("test-task-{}", uuid::Uuid::new_v4());
        let base_path = temp_dir.path().join("scratchpad").join(&task_id);
        std::fs::create_dir_all(&base_path).unwrap();

        let scratchpad = Scratchpad {
            task_id,
            base_path,
            entries: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
        };

        (scratchpad, temp_dir)
    }

    #[test]
    fn test_scratchpad_write_read() {
        let (scratchpad, _temp) = test_scratchpad();

        let entry = ScratchpadEntry::new("test-key", "test-value", "coordinator");
        scratchpad.write(entry.clone()).unwrap();

        let read = scratchpad.read("test-key").unwrap();
        assert_eq!(read.key, "test-key");
        assert_eq!(read.value, "test-value");
        assert_eq!(read.author, "coordinator");
    }

    #[test]
    fn test_scratchpad_typed_read() {
        let (scratchpad, _temp) = test_scratchpad();

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct TestData {
            name: String,
            count: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            count: 42,
        };

        let entry =
            ScratchpadEntry::new("typed-key", serde_json::to_string(&data).unwrap(), "worker");
        scratchpad.write(entry).unwrap();

        let read: Option<TestData> = scratchpad.read_typed("typed-key").unwrap();
        assert_eq!(read, Some(data));
    }

    #[test]
    fn test_scratchpad_list_by_prefix() {
        let (scratchpad, _temp) = test_scratchpad();

        scratchpad
            .write(ScratchpadEntry::new("finding:1", "value1", "worker1"))
            .unwrap();
        scratchpad
            .write(ScratchpadEntry::new("finding:2", "value2", "worker2"))
            .unwrap();
        scratchpad
            .write(ScratchpadEntry::new("other", "value3", "worker3"))
            .unwrap();

        let findings = scratchpad.list_by_prefix("finding:");
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn test_worker_registration() {
        let (scratchpad, _temp) = test_scratchpad();

        let worker = WorkerInfo::new("worker-1", "researcher", "Find all bugs in src/parser.rs");
        scratchpad.register_worker(worker.clone()).unwrap();

        let retrieved = scratchpad.get_worker("worker-1").unwrap();
        assert_eq!(retrieved.id, "worker-1");
        assert_eq!(retrieved.role, "researcher");
        assert_eq!(retrieved.status, WorkerStatus::Initializing);

        scratchpad
            .update_worker_status("worker-1", WorkerStatus::Working)
            .unwrap();
        let updated = scratchpad.get_worker("worker-1").unwrap();
        assert_eq!(updated.status, WorkerStatus::Working);
    }

    #[test]
    fn test_active_workers() {
        let (scratchpad, _temp) = test_scratchpad();

        scratchpad
            .register_worker(WorkerInfo::new("w1", "role", "task"))
            .unwrap();
        scratchpad
            .register_worker(WorkerInfo::new("w2", "role", "task"))
            .unwrap();
        scratchpad
            .register_worker(WorkerInfo::new("w3", "role", "task"))
            .unwrap();

        scratchpad
            .update_worker_status("w1", WorkerStatus::Completed)
            .unwrap();
        scratchpad
            .update_worker_status("w2", WorkerStatus::Working)
            .unwrap();
        // w3 stays Initializing

        let active = scratchpad.active_workers();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_persistence() {
        let (scratchpad, _temp) = test_scratchpad();
        let base_path = scratchpad.base_path.clone();
        let task_id = scratchpad.task_id.clone();

        scratchpad
            .write(ScratchpadEntry::new("key1", "value1", "author1"))
            .unwrap();
        scratchpad
            .register_worker(WorkerInfo::new("w1", "role", "task"))
            .unwrap();

        // Create new scratchpad instance pointing to same path
        let scratchpad2 = Scratchpad {
            task_id,
            base_path,
            entries: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
        };
        scratchpad2.load().unwrap();

        let read = scratchpad2.read("key1").unwrap();
        assert_eq!(read.value, "value1");

        let worker = scratchpad2.get_worker("w1").unwrap();
        assert_eq!(worker.role, "role");
    }
}
