//! Agent worker implementation

use super::{AgentConfig, AgentHandle, AgentStatus, Task, TaskResult};
use crate::error::SelfwareError;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Agent worker that executes tasks
pub struct AgentWorker {
    id: String,
    config: AgentConfig,
    status: Arc<RwLock<AgentStatus>>,
    task_rx: mpsc::Receiver<Task>,
    result_tx: mpsc::Sender<(String, Result<TaskResult, SelfwareError>)>,
}

impl AgentWorker {
    /// Create a new agent worker
    pub fn new(
        id: impl Into<String>,
        config: AgentConfig,
        task_rx: mpsc::Receiver<Task>,
        result_tx: mpsc::Sender<(String, Result<TaskResult, SelfwareError>)>,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            status: Arc::new(RwLock::new(AgentStatus::Initializing)),
            task_rx,
            result_tx,
        }
    }
    
    /// Get agent handle
    pub fn handle(&self) -> AgentHandle {
        AgentHandle {
            id: self.id.clone(),
            status: AgentStatus::Initializing,
        }
    }
    
    /// Run the agent worker
    pub async fn run(mut self) {
        info!(agent_id = %self.id, "Agent worker started");
        
        *self.status.write().await = AgentStatus::Idle;
        
        while let Some(task) = self.task_rx.recv().await {
            *self.status.write().await = AgentStatus::Working;
            
            info!(agent_id = %self.id, task_id = %task.id, "Executing task");
            
            let result = self.execute_task(&task).await;
            
            if let Err(e) = self.result_tx.send((task.id.to_string(), result)).await {
                error!(agent_id = %self.id, error = %e, "Failed to send result");
            }
            
            *self.status.write().await = AgentStatus::Idle;
        }
        
        info!(agent_id = %self.id, "Agent worker stopped");
    }
    
    /// Execute a single task
    async fn execute_task(&self, task: &Task) -> Result<TaskResult, SelfwareError> {
        debug!(task_id = %task.id, task_type = %task.task_type, "Executing task");
        
        // Task execution logic would go here
        // This would integrate with the LLM and other components
        
        Ok(TaskResult::success())
    }
    
    /// Get current status
    pub async fn status(&self) -> AgentStatus {
        *self.status.read().await
    }
    
    /// Pause the worker
    pub async fn pause(&self) {
        warn!(agent_id = %self.id, "Pausing worker");
        *self.status.write().await = AgentStatus::Paused;
    }
    
    /// Resume the worker
    pub async fn resume(&self) {
        info!(agent_id = %self.id, "Resuming worker");
        *self.status.write().await = AgentStatus::Idle;
    }
}
