//! Multi-Agent Swarm System
//!
//! Specialist agents with role-specific prompts, consensus voting,
//! conflict resolution, and shared working memory.
//!
//! Features:
//! - Specialist agent roles (architect, coder, tester, reviewer)
//! - Role-specific system prompts
//! - Consensus voting for decisions (library API — not currently wired into
//!   any production execution path)
//! - Conflict resolution strategies (same: library API only)
//! - Shared working memory
//! - Agent coordination

mod coordinator;
mod factory;
mod memory;
mod types;

#[cfg(test)]
mod tests;

// Re-export all public types
pub use coordinator::{ConflictStrategy, Swarm, SwarmStats};
pub use factory::create_dev_swarm;
pub use memory::{MemoryAccess, MemoryAction, MemoryEntry, SharedMemory};
pub use types::{
    Agent, AgentRole, AgentStatus, Decision, DecisionStatus, SwarmTask, TaskStatus, Vote,
};
