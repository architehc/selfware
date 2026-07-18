//! Multi-Agent Chat System
//!
//! Supports up to 16 concurrent agent streams for parallel task execution.
//!
//! Features:
//! - Concurrent agent execution with configurable parallelism
//! - Task distribution and coordination
//! - Shared context and results aggregation

mod chat;
mod config;
mod interactive;
mod types;

#[cfg(test)]
mod tests;

// Re-export all public types
pub use chat::MultiAgentChat;
pub use config::{MultiAgentConfig, MultiAgentFailurePolicy};
pub use interactive::run_multiagent_task;
pub use types::{AgentInstance, AgentResult, AgentStatus, MultiAgentEvent, MAX_CONCURRENT_AGENTS};
