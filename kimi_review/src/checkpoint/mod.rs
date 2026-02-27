//! Checkpoint management for crash recovery and state persistence

use crate::agent::AgentState;
use crate::config::CheckpointConfig;
use crate::error::{CheckpointError, SelfwareError};
use crate::Id;
use chrono;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod compression;
mod incremental;
mod recovery;
mod storage;

pub use compression::CompressionEngine;
pub use incremental::IncrementalCheckpointManager;
pub use recovery::{RecoveredState, RecoveryManager};
pub use storage::CheckpointStorage;

/// Unique checkpoint identifier
pub type CheckpointId = Id;

/// Checkpoint manager for creating and restoring checkpoints
pub struct CheckpointManager {
    config: CheckpointConfig,
    storage: Arc<CheckpointStorage>,
    incremental: IncrementalCheckpointManager,
    recovery: RecoveryManager,
    last_checkpoint: RwLock<Option<Instant>>,
    pending_changes: std::sync::atomic::AtomicU64,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub async fn new(config: &CheckpointConfig) -> Result<Self, SelfwareError> {
        let storage = Arc::new(CheckpointStorage::new(config).await?);
        let incremental = IncrementalCheckpointManager::new(storage.clone());
        let recovery = RecoveryManager::new(storage.clone());
        
        Ok(Self {
            config: config.clone(),
            storage,
            incremental,
            recovery,
            last_checkpoint: RwLock::new(None),
            pending_changes: std::sync::atomic::AtomicU64::new(0),
        })
    }
    
    /// Check if recovery is needed (ungraceful shutdown)
    pub async fn needs_recovery(&self) -> Result<bool, SelfwareError> {
        self.recovery.needs_recovery().await
    }
    
    /// Recover from the last checkpoint
    pub async fn recover(
        &self,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<RecoveredState>, SelfwareError> {
        self.recovery.recover(checkpoint_id).await
    }
    
    /// Create a session-level checkpoint
    pub async fn checkpoint_session(&self, state: &AgentState) -> Result<CheckpointId, SelfwareError> {
        if !self.config.enabled {
            return Ok(CheckpointId::new());
        }
        
        let checkpoint_id = CheckpointId::new();
        let timestamp = chrono::Utc::now();
        
        info!(checkpoint_id = %checkpoint_id, "Creating session checkpoint");
        
        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            timestamp,
            level: CheckpointLevel::Session,
            state: CheckpointState::Session(state.to_session_state()),
            parent: self.get_parent_checkpoint().await,
            diff_from_parent: None,
        };
        
        // Store checkpoint
        self.storage.store(&checkpoint).await?;
        
        // Update last checkpoint time
        *self.last_checkpoint.write().await = Some(Instant::now());
        self.pending_changes.store(0, std::sync::atomic::Ordering::SeqCst);
        
        info!(checkpoint_id = %checkpoint_id, "Session checkpoint created");
        
        Ok(checkpoint_id)
    }
    
    /// Create a task-level checkpoint
    pub async fn checkpoint_task(
        &self,
        task_id: &str,
        task_state: &TaskCheckpointState,
    ) -> Result<CheckpointId, SelfwareError> {
        if !self.config.enabled || !self.config.levels.task.enabled {
            return Ok(CheckpointId::new());
        }
        
        let checkpoint_id = CheckpointId::new();
        
        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            timestamp: chrono::Utc::now(),
            level: CheckpointLevel::Task,
            state: CheckpointState::Task(task_state.clone()),
            parent: None,
            diff_from_parent: None,
        };
        
        self.storage.store(&checkpoint).await?;
        
        debug!(checkpoint_id = %checkpoint_id, task_id = task_id, "Task checkpoint created");
        
        Ok(checkpoint_id)
    }
    
    /// Create a graceful shutdown checkpoint
    pub async fn create_graceful_shutdown_checkpoint(&self) -> Result<CheckpointId, SelfwareError> {
        info!("Creating graceful shutdown checkpoint");
        
        let checkpoint_id = CheckpointId::new();
        let timestamp = chrono::Utc::now();
        
        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            timestamp,
            level: CheckpointLevel::System,
            state: CheckpointState::System(SystemCheckpointState {
                shutdown_type: ShutdownType::Graceful,
                session_id: crate::get_session_id().to_string(),
            }),
            parent: self.get_parent_checkpoint().await,
            diff_from_parent: None,
        };
        
        self.storage.store(&checkpoint).await?;
        
        info!(checkpoint_id = %checkpoint_id, "Graceful shutdown checkpoint created");
        
        Ok(checkpoint_id)
    }
    
    /// Flush all pending writes to storage
    pub async fn flush(&self) -> Result<(), SelfwareError> {
        self.storage.flush().await
    }
    
    /// Checkpoint scheduler loop
    pub async fn scheduler_loop(&self) {
        if !self.config.enabled {
            return;
        }
        
        let interval = Duration::from_secs(self.config.interval_seconds);
        let mut ticker = tokio::time::interval(interval);
        
        loop {
            ticker.tick().await;
            
            // Check if we should checkpoint based on pending changes
            let pending = self.pending_changes.load(std::sync::atomic::Ordering::Relaxed);
            
            // This is a simplified check - in reality, we'd check the actual state
            if pending > 0 {
                debug!(pending_changes = pending, "Checkpoint scheduler: changes pending");
            }
        }
    }
    
    /// Record a change that should trigger checkpointing
    pub fn record_change(&self) {
        self.pending_changes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    /// Get the parent checkpoint for incremental checkpointing
    async fn get_parent_checkpoint(&self) -> Option<CheckpointId> {
        // In a real implementation, this would track the checkpoint chain
        None
    }
    
    /// List available checkpoints
    pub async fn list_checkpoints(&self) -> Result<Vec<CheckpointMetadata>, SelfwareError> {
        self.storage.list_checkpoints().await
    }
    
    /// Delete old checkpoints based on retention policy
    pub async fn cleanup_old_checkpoints(&self) -> Result<u64, SelfwareError> {
        self.storage.cleanup_old(self.config.retention_days).await
    }
}

/// Checkpoint structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: CheckpointLevel,
    pub state: CheckpointState,
    pub parent: Option<CheckpointId>,
    pub diff_from_parent: Option<Vec<u8>>,
}

/// Checkpoint level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointLevel {
    Micro,
    Task,
    Session,
    System,
}

/// Checkpoint state variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointState {
    Micro(MicroCheckpointState),
    Task(TaskCheckpointState),
    Session(SessionCheckpointState),
    System(SystemCheckpointState),
}

/// Micro checkpoint state (token-level)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroCheckpointState {
    pub token_position: usize,
    pub partial_output: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Task checkpoint state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpointState {
    pub task_id: String,
    pub task_type: String,
    pub status: TaskStatus,
    pub input: serde_json::Value,
    pub partial_result: Option<serde_json::Value>,
    pub attempts: u32,
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// Session checkpoint state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCheckpointState {
    pub session_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub iteration_count: u64,
    pub completed_tasks: Vec<CompletedTaskInfo>,
    pub pending_tasks: Vec<PendingTaskInfo>,
    pub context_summary: String,
}

/// Completed task info for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedTaskInfo {
    pub task_id: String,
    pub task_type: String,
    pub completed_at: chrono::DateTime<chrono::Utc>,
    pub success: bool,
}

/// Pending task info for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTaskInfo {
    pub task_id: String,
    pub task_type: String,
    pub priority: crate::Priority,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// System checkpoint state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCheckpointState {
    pub shutdown_type: ShutdownType,
    pub session_id: String,
}

/// Shutdown type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutdownType {
    Graceful,
    Crash,
    Unknown,
}

/// Checkpoint metadata (without full state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub id: CheckpointId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: CheckpointLevel,
    pub size_bytes: u64,
    pub compressed: bool,
}

/// Checkpoint status for health checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointStatus {
    pub last_checkpoint: Option<chrono::DateTime<chrono::Utc>>,
    pub total_checkpoints: u64,
    pub storage_used_bytes: u64,
    pub healthy: bool,
}
