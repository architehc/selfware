//! Recovery management for crash recovery

use super::{Checkpoint, CheckpointId, CheckpointStorage, ShutdownType, SystemCheckpointState};
use crate::agent::AgentState;
use crate::error::{CheckpointError, SelfwareError};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Recovery manager for handling crash recovery
pub struct RecoveryManager {
    storage: Arc<CheckpointStorage>,
}

/// Recovered state from checkpoint
#[derive(Debug, Clone)]
pub struct RecoveredState {
    /// The checkpoint that was recovered from
    pub checkpoint_id: CheckpointId,
    /// When the checkpoint was created
    pub checkpoint_timestamp: chrono::DateTime<chrono::Utc>,
    /// The recovered agent state
    pub agent_state: AgentState,
    /// Whether recovery was from a graceful shutdown
    pub was_graceful: bool,
    /// Number of journal entries replayed
    pub journal_entries_replayed: u64,
}

impl RecoveredState {
    /// Convert recovered state to agent state
    pub fn into_agent_state(self) -> AgentState {
        self.agent_state
    }
}

impl RecoveryManager {
    /// Create a new recovery manager
    pub fn new(storage: Arc<CheckpointStorage>) -> Self {
        Self { storage }
    }
    
    /// Check if recovery is needed
    pub async fn needs_recovery(&self) -> Result<bool, SelfwareError> {
        // Check for ungraceful shutdown marker
        let checkpoints = self.storage.list_checkpoints().await?;
        
        if let Some(latest) = checkpoints.first() {
            // Load the latest checkpoint to check shutdown type
            match self.storage.load(&latest.id).await {
                Ok(checkpoint) => {
                    if let super::CheckpointState::System(sys_state) = &checkpoint.state {
                        return Ok(sys_state.shutdown_type != ShutdownType::Graceful);
                    }
                }
                Err(e) => {
                    warn!("Failed to load latest checkpoint: {:?}", e);
                    return Ok(true); // Assume recovery needed if we can't check
                }
            }
        }
        
        Ok(false)
    }
    
    /// Recover from checkpoint
    pub async fn recover(
        &self,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<RecoveredState>, SelfwareError> {
        let checkpoint = if let Some(id) = checkpoint_id {
            // Load specific checkpoint
            let checkpoint_id = CheckpointId::new(); // Parse from string
            self.storage.load(&checkpoint_id).await?
        } else {
            // Find the latest valid checkpoint
            self.find_latest_valid_checkpoint().await?
        };
        
        info!(
            checkpoint_id = %checkpoint.id,
            timestamp = %checkpoint.timestamp,
            "Starting recovery from checkpoint"
        );
        
        // Replay recovery journal
        let journal_entries_replayed = self.replay_journal(&checkpoint).await?;
        
        // Extract agent state from checkpoint
        let agent_state = self.extract_agent_state(&checkpoint).await?;
        
        let was_graceful = matches!(
            &checkpoint.state,
            super::CheckpointState::System(SystemCheckpointState { shutdown_type: ShutdownType::Graceful, .. })
        );
        
        info!(
            checkpoint_id = %checkpoint.id,
            journal_entries = journal_entries_replayed,
            was_graceful = was_graceful,
            "Recovery completed successfully"
        );
        
        Ok(Some(RecoveredState {
            checkpoint_id: checkpoint.id.clone(),
            checkpoint_timestamp: checkpoint.timestamp,
            agent_state,
            was_graceful,
            journal_entries_replayed,
        }))
    }
    
    /// Find the latest valid checkpoint
    async fn find_latest_valid_checkpoint(&self) -> Result<Checkpoint, SelfwareError> {
        let checkpoints = self.storage.list_checkpoints().await?;
        
        for metadata in checkpoints {
            match self.storage.load(&metadata.id).await {
                Ok(checkpoint) => {
                    // Verify checkpoint integrity
                    if self.verify_checkpoint(&checkpoint).await? {
                        return Ok(checkpoint);
                    } else {
                        warn!(checkpoint_id = %metadata.id, "Checkpoint verification failed, trying next");
                    }
                }
                Err(e) => {
                    warn!(checkpoint_id = %metadata.id, error = %e, "Failed to load checkpoint, trying next");
                }
            }
        }
        
        Err(CheckpointError::NotFound("No valid checkpoint found".to_string()).into())
    }
    
    /// Verify checkpoint integrity
    async fn verify_checkpoint(&self, checkpoint: &Checkpoint) -> Result<bool, SelfwareError> {
        // Basic verification - in production, add checksums, signatures, etc.
        
        // Check timestamp is reasonable
        let now = chrono::Utc::now();
        if checkpoint.timestamp > now {
            warn!("Checkpoint timestamp is in the future");
            return Ok(false);
        }
        
        // Check timestamp is not too old (optional - might want to recover from old checkpoints)
        let max_age = chrono::Duration::days(30);
        if now - checkpoint.timestamp > max_age {
            warn!("Checkpoint is very old (>30 days)");
            // Still allow recovery, but warn
        }
        
        Ok(true)
    }
    
    /// Replay recovery journal
    async fn replay_journal(&self, checkpoint: &Checkpoint) -> Result<u64, SelfwareError> {
        // In a real implementation, this would replay journal entries
        // For now, return 0
        Ok(0)
    }
    
    /// Extract agent state from checkpoint
    async fn extract_agent_state(&self, checkpoint: &Checkpoint) -> Result<AgentState, SelfwareError> {
        match &checkpoint.state {
            super::CheckpointState::Session(session_state) => {
                // Convert session state to agent state
                Ok(AgentState::from_session_state(session_state))
            }
            super::CheckpointState::System(_) => {
                // System checkpoint doesn't contain agent state
                // Load previous session checkpoint
                if let Some(parent_id) = &checkpoint.parent {
                    let parent = self.storage.load(parent_id).await?;
                    self.extract_agent_state(&parent).await
                } else {
                    // No parent, start fresh
                    Ok(AgentState::new())
                }
            }
            _ => {
                warn!("Unexpected checkpoint level, starting fresh");
                Ok(AgentState::new())
            }
        }
    }
    
    /// Attempt recovery from earlier checkpoint if latest fails
    pub async fn attempt_earlier_recovery(&self) -> Result<RecoveredState, SelfwareError> {
        let checkpoints = self.storage.list_checkpoints().await?;
        
        // Skip the first (latest) checkpoint and try others
        for metadata in checkpoints.iter().skip(1) {
            match self.storage.load(&metadata.id).await {
                Ok(checkpoint) => {
                    if let Ok(agent_state) = self.extract_agent_state(&checkpoint).await {
                        info!(checkpoint_id = %metadata.id, "Recovered from earlier checkpoint");
                        return Ok(RecoveredState {
                            checkpoint_id: checkpoint.id.clone(),
                            checkpoint_timestamp: checkpoint.timestamp,
                            agent_state,
                            was_graceful: false,
                            journal_entries_replayed: 0,
                        });
                    }
                }
                Err(e) => {
                    warn!(checkpoint_id = %metadata.id, error = %e, "Failed to load earlier checkpoint");
                }
            }
        }
        
        Err(CheckpointError::RecoveryFailed("Could not recover from any checkpoint".to_string()).into())
    }
}
