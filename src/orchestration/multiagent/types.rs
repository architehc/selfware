//! Multi-Agent Types
//!
//! Core types for the multi-agent system including agent instances,
//! results, events, and status definitions.

use crate::api::types::Message;
use crate::swarm::AgentRole;
use std::time::{Duration, Instant};

/// Maximum number of concurrent agent streams
pub const MAX_CONCURRENT_AGENTS: usize = 16;

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Working,
    Completed,
    Failed,
}

/// A single agent instance in the multi-agent system
#[derive(Debug, Clone)]
pub struct AgentInstance {
    pub id: usize,
    pub role: AgentRole,
    pub name: String,
    pub messages: Vec<Message>,
    pub status: AgentStatus,
    /// Timestamp of the last heartbeat, updated during task execution
    pub last_heartbeat: Instant,
}

/// Result from an agent execution
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub agent_id: usize,
    pub agent_name: String,
    pub role: AgentRole,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub duration: Duration,
    pub success: bool,
    pub error: Option<String>,
}

/// Event emitted during multi-agent execution
#[derive(Debug, Clone)]
pub enum MultiAgentEvent {
    AgentStarted {
        agent_id: usize,
        name: String,
        task: String,
    },
    AgentProgress {
        agent_id: usize,
        content: String,
    },
    AgentToolCall {
        agent_id: usize,
        tool: String,
    },
    AgentCompleted {
        agent_id: usize,
        result: AgentResult,
    },
    AgentFailed {
        agent_id: usize,
        error: String,
    },
    AllCompleted {
        results: Vec<AgentResult>,
        total_duration: Duration,
    },
}
