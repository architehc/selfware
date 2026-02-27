//! Selfware - Autonomous AI Agent Runtime
//! 
//! This crate provides infrastructure for running AI agents autonomously
//! for extended periods (3-7+ days) with recursive self-improvement capabilities.

#![warn(missing_docs)]
#![allow(clippy::type_complexity)]

pub mod agent;
pub mod checkpoint;
pub mod config;
pub mod error;
pub mod intervention;
pub mod llm;
pub mod observability;
pub mod resource;
pub mod self_healing;
pub mod supervision;

use std::sync::Arc;
use tokio::sync::RwLock;

/// Global session identifier
pub static SESSION_ID: once_cell::sync::OnceCell<String> = once_cell::sync::OnceCell::new();

/// Initialize the global session ID
pub fn init_session_id() -> String {
    let id = uuid::Uuid::new_v4().to_string();
    SESSION_ID.set(id.clone()).ok();
    id
}

/// Get the current session ID
pub fn get_session_id() -> &'static str {
    SESSION_ID.get().map(|s| s.as_str()).unwrap_or("unknown")
}

/// Core system state shared across components
#[derive(Debug)]
pub struct SystemState {
    /// Current session information
    pub session: SessionInfo,
    /// Active agent workers
    pub active_agents: Arc<RwLock<Vec<agent::AgentHandle>>>,
    /// Resource usage metrics
    pub resource_usage: Arc<RwLock<resource::ResourceUsage>>,
    /// Current checkpoint status
    pub checkpoint_status: Arc<RwLock<checkpoint::CheckpointStatus>>,
}

/// Session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Unique session identifier
    pub id: String,
    /// When the session started
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Current runtime duration
    pub runtime: std::time::Duration,
    /// Total tasks processed
    pub total_tasks: u64,
    /// Successfully completed tasks
    pub completed_tasks: u64,
    /// Failed tasks
    pub failed_tasks: u64,
    /// Current high-level goal
    pub current_goal: String,
}

impl SessionInfo {
    /// Create a new session
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            id: init_session_id(),
            started_at: chrono::Utc::now(),
            runtime: std::time::Duration::ZERO,
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            current_goal: goal.into(),
        }
    }
    
    /// Update runtime
    pub fn update_runtime(&mut self) {
        self.runtime = self.started_at.elapsed();
    }
    
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            1.0
        } else {
            self.completed_tasks as f64 / self.total_tasks as f64
        }
    }
}

/// Priority levels for tasks and operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum Priority {
    /// Critical system operations (recovery, checkpoints)
    Critical = 0,
    /// High priority user-facing operations
    High = 1,
    /// Normal agent work
    Normal = 2,
    /// Low priority background tasks
    Low = 3,
    /// Background self-improvement
    Background = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Unique identifier type
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Id(String);

impl Id {
    /// Generate a new unique ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    
    /// Get the ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Result type for Selfware operations
pub type Result<T> = std::result::Result<T, error::SelfwareError>;
