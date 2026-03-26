//! Multi-Agent Configuration
//!
//! Configuration types and builders for the multi-agent system.

use crate::swarm::AgentRole;

use super::types::MAX_CONCURRENT_AGENTS;

/// Failure policy for multi-agent execution
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MultiAgentFailurePolicy {
    /// Continue even if some agents fail (default)
    BestEffort,
    /// Abort all remaining tasks if any agent fails
    #[default]
    FailFast,
}

/// Configuration for multi-agent chat
#[derive(Debug, Clone)]
pub struct MultiAgentConfig {
    /// Maximum concurrent streams (1-16)
    pub max_concurrency: usize,
    /// Agent roles to spawn
    pub roles: Vec<AgentRole>,
    /// Whether to use streaming responses
    pub streaming: bool,
    /// Timeout per agent request
    pub timeout_secs: u64,
    /// Temperature for generation
    pub temperature: f32,
    /// Max tokens per response
    pub max_tokens: usize,
    /// Failure policy
    pub failure_policy: MultiAgentFailurePolicy,
}

impl Default for MultiAgentConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            roles: vec![
                AgentRole::Architect,
                AgentRole::Coder,
                AgentRole::Tester,
                AgentRole::Reviewer,
            ],
            streaming: true,
            timeout_secs: 120,
            temperature: 1.0,
            max_tokens: 65536,
            failure_policy: MultiAgentFailurePolicy::BestEffort,
        }
    }
}

impl MultiAgentConfig {
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.clamp(1, MAX_CONCURRENT_AGENTS);
        self
    }

    pub fn with_roles(mut self, roles: Vec<AgentRole>) -> Self {
        self.roles = roles;
        self
    }
}
