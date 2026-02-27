//! Agent state management

use super::{CompletedTask, Task, TaskResult};
use crate::checkpoint::{CompletedTaskInfo, PendingTaskInfo, SessionCheckpointState};
use crate::error::SelfwareError;
use crate::Priority;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Agent state for checkpointing and recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub iteration_count: u64,
    pub consecutive_failures: u32,
    pub pending_tasks: VecDeque<Task>,
    pub completed_tasks: Vec<CompletedTask>,
    pub current_goal: String,
    pub context_history: Vec<ContextEntry>,
    pub metrics: AgentMetrics,
}

/// Context entry for conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub role: String,
    pub content: String,
    pub token_count: usize,
}

/// Agent metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub total_tasks: u64,
    pub successful_tasks: u64,
    pub failed_tasks: u64,
    pub total_tokens_used: u64,
    pub total_execution_time_ms: u64,
    pub checkpoints_created: u64,
}

impl AgentState {
    /// Create a new agent state
    pub fn new() -> Self {
        Self {
            iteration_count: 0,
            consecutive_failures: 0,
            pending_tasks: VecDeque::new(),
            completed_tasks: Vec::new(),
            current_goal: "Initialize and begin autonomous operation".to_string(),
            context_history: Vec::new(),
            metrics: AgentMetrics::default(),
        }
    }
    
    /// Get the next task to execute
    pub async fn next_task(&mut self) -> Option<Task> {
        // Sort by priority
        let mut tasks: Vec<_> = self.pending_tasks.drain(..).collect();
        tasks.sort_by_key(|t| t.priority);
        
        // Return highest priority task
        let next = tasks.first().cloned();
        
        // Put remaining tasks back
        self.pending_tasks = tasks.into_iter().skip(1).collect();
        
        next
    }
    
    /// Queue a task for execution
    pub fn queue_task(&mut self, task: Task) {
        self.pending_tasks.push_back(task);
    }
    
    /// Requeue a failed task for retry
    pub fn requeue_task(&mut self, task: Task) {
        // Put at front of queue for immediate retry
        self.pending_tasks.push_front(task);
        self.consecutive_failures += 1;
    }
    
    /// Record a completed task
    pub fn record_completion(&mut self, task: Task, result: TaskResult) {
        self.consecutive_failures = 0;
        self.metrics.total_tasks += 1;
        self.metrics.successful_tasks += 1;
        self.metrics.total_tokens_used += result.tokens_used;
        self.metrics.total_execution_time_ms += result.duration_ms;
        
        self.completed_tasks.push(CompletedTask {
            task,
            result,
            completed_at: chrono::Utc::now(),
        });
    }
    
    /// Record a failed task
    pub fn record_failure(&mut self, task: Task, error: SelfwareError) {
        self.consecutive_failures += 1;
        self.metrics.total_tasks += 1;
        self.metrics.failed_tasks += 1;
        
        self.completed_tasks.push(CompletedTask {
            task,
            result: TaskResult::failure(error.to_string()),
            completed_at: chrono::Utc::now(),
        });
    }
    
    /// Convert to session checkpoint state
    pub fn to_session_state(&self) -> SessionCheckpointState {
        SessionCheckpointState {
            session_id: crate::get_session_id().to_string(),
            started_at: chrono::Utc::now(), // Would be actual start time
            iteration_count: self.iteration_count,
            completed_tasks: self.completed_tasks
                .iter()
                .map(|ct| CompletedTaskInfo {
                    task_id: ct.task.id.to_string(),
                    task_type: ct.task.task_type.clone(),
                    completed_at: ct.completed_at,
                    success: ct.result.success,
                })
                .collect(),
            pending_tasks: self.pending_tasks
                .iter()
                .map(|t| PendingTaskInfo {
                    task_id: t.id.to_string(),
                    task_type: t.task_type.clone(),
                    priority: t.priority,
                    created_at: t.created_at,
                })
                .collect(),
            context_summary: self.summarize_context(),
        }
    }
    
    /// Create from session checkpoint state
    pub fn from_session_state(state: &SessionCheckpointState) -> Self {
        Self {
            iteration_count: state.iteration_count,
            consecutive_failures: 0,
            pending_tasks: state.pending_tasks
                .iter()
                .map(|pt| Task {
                    id: crate::Id::new(),
                    task_type: pt.task_type.clone(),
                    description: "Restored from checkpoint".to_string(),
                    priority: pt.priority,
                    created_at: pt.created_at,
                    deadline: None,
                    input: serde_json::Value::Null,
                    checkpoint_on_completion: true,
                    max_retries: 3,
                })
                .collect(),
            completed_tasks: state.completed_tasks
                .iter()
                .map(|ct| CompletedTask {
                    task: Task {
                        id: crate::Id::new(),
                        task_type: ct.task_type.clone(),
                        description: "Restored from checkpoint".to_string(),
                        priority: Priority::Normal,
                        created_at: ct.completed_at,
                        deadline: None,
                        input: serde_json::Value::Null,
                        checkpoint_on_completion: true,
                        max_retries: 3,
                    },
                    result: TaskResult {
                        success: ct.success,
                        output: serde_json::Value::Null,
                        tokens_used: 0,
                        duration_ms: 0,
                        checkpoint_id: None,
                    },
                    completed_at: ct.completed_at,
                })
                .collect(),
            current_goal: "Continue from checkpoint".to_string(),
            context_history: Vec::new(),
            metrics: AgentMetrics::default(),
        }
    }
    
    /// Compact state to reduce memory usage
    pub fn compact(mut self) -> Self {
        // Keep only last N completed tasks
        let max_completed = 1000;
        if self.completed_tasks.len() > max_completed {
            let to_remove = self.completed_tasks.len() - max_completed;
            self.completed_tasks.drain(..to_remove);
        }
        
        // Keep only last N context entries
        let max_context = 10000;
        if self.context_history.len() > max_context {
            let to_remove = self.context_history.len() - max_context;
            self.context_history.drain(..to_remove);
        }
        
        self
    }
    
    /// Summarize context for checkpointing
    fn summarize_context(&self) -> String {
        format!(
            "{} iterations, {} completed tasks, {} pending tasks",
            self.iteration_count,
            self.completed_tasks.len(),
            self.pending_tasks.len()
        )
    }
    
    /// Add context entry
    pub fn add_context(&mut self, role: impl Into<String>, content: impl Into<String>, tokens: usize) {
        self.context_history.push(ContextEntry {
            timestamp: chrono::Utc::now(),
            role: role.into(),
            content: content.into(),
            token_count: tokens,
        });
    }
    
    /// Get total context tokens
    pub fn context_tokens(&self) -> usize {
        self.context_history.iter().map(|e| e.token_count).sum()
    }
    
    /// Check if should generate self-improvement task
    pub fn should_self_improve(&self, interval: u32) -> bool {
        self.iteration_count % interval as u64 == 0 && self.iteration_count > 0
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}
