//! Agent worker and state management

use crate::checkpoint::{PendingTaskInfo, SessionCheckpointState, TaskCheckpointState};
use crate::error::SelfwareError;
use crate::Id;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub mod state;
pub mod supervisor;
pub mod worker;

pub use state::AgentState;
pub use supervisor::AgentSupervisor;
pub use worker::AgentWorker;

/// Unique task identifier
pub type TaskId = Id;

/// Task structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub task_type: String,
    pub description: String,
    pub priority: crate::Priority,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub input: serde_json::Value,
    pub checkpoint_on_completion: bool,
    pub max_retries: u32,
}

impl Task {
    /// Create a new task
    pub fn new(
        task_type: impl Into<String>,
        description: impl Into<String>,
        priority: crate::Priority,
    ) -> Self {
        Self {
            id: TaskId::new(),
            task_type: task_type.into(),
            description: description.into(),
            priority,
            created_at: chrono::Utc::now(),
            deadline: None,
            input: serde_json::Value::Null,
            checkpoint_on_completion: true,
            max_retries: 3,
        }
    }
    
    /// Set task input
    pub fn with_input(mut self, input: impl Serialize) -> Result<Self, serde_json::Error> {
        self.input = serde_json::to_value(input)?;
        Ok(self)
    }
    
    /// Set deadline
    pub fn with_deadline(mut self, deadline: chrono::DateTime<chrono::Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }
    
    /// Convert to checkpoint state
    pub fn to_checkpoint_state(&self) -> TaskCheckpointState {
        TaskCheckpointState {
            task_id: self.id.to_string(),
            task_type: self.task_type.clone(),
            status: crate::checkpoint::TaskStatus::Pending,
            input: self.input.clone(),
            partial_result: None,
            attempts: 0,
        }
    }
}

/// Task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub tokens_used: u64,
    pub duration_ms: u64,
    pub checkpoint_id: Option<String>,
}

impl TaskResult {
    /// Create a successful result
    pub fn success() -> Self {
        Self {
            success: true,
            output: serde_json::Value::Null,
            tokens_used: 0,
            duration_ms: 0,
            checkpoint_id: None,
        }
    }
    
    /// Create a failed result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: serde_json::json!({"error": error.into()}),
            tokens_used: 0,
            duration_ms: 0,
            checkpoint_id: None,
        }
    }
    
    /// Set output
    pub fn with_output(mut self, output: impl Serialize) -> Result<Self, serde_json::Error> {
        self.output = serde_json::to_value(output)?;
        Ok(self)
    }
    
    /// Set token count
    pub fn with_tokens(mut self, tokens: u64) -> Self {
        self.tokens_used = tokens;
        self
    }
}

/// Completed task information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedTask {
    pub task: Task,
    pub result: TaskResult,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Agent handle for external control
#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub id: String,
    pub status: AgentStatus,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Initializing,
    Idle,
    Working,
    Paused,
    Error,
    ShuttingDown,
}

/// Agent worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub max_concurrent_tasks: usize,
    pub task_timeout_seconds: u64,
    pub enable_self_improvement: bool,
    pub improvement_interval_tasks: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 4,
            task_timeout_seconds: 14400, // 4 hours
            enable_self_improvement: true,
            improvement_interval_tasks: 100,
        }
    }
}
