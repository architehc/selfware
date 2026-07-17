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
    /// Timeout per agent request. `None` (default) inherits
    /// `agent.step_timeout_secs` from the loaded [`Config`](crate::config::Config).
    ///
    /// Note: this wraps the API client's own retry policy, whose worst-case
    /// backoff window is ~242s at the default 8 retries. A timeout shorter
    /// than that can discard a generation the provider has already billed;
    /// the inherited default (300s) stays clear of it.
    pub timeout_secs: Option<u64>,
    /// Temperature override for generation. `None` (default) inherits the
    /// top-level `temperature` from the loaded [`Config`](crate::config::Config).
    pub temperature: Option<f32>,
    /// Max-tokens override per response. `None` (default) inherits the
    /// top-level `max_tokens` from the loaded [`Config`](crate::config::Config).
    ///
    /// This value is also used as the pessimistic per-call estimate when
    /// enforcing `--max-budget-tokens` (see `run_task`).
    pub max_tokens: Option<usize>,
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
            // `None` = inherit the user's loaded Config (temperature,
            // max_tokens, agent.step_timeout_secs) instead of clobbering it.
            timeout_secs: None,
            temperature: None,
            max_tokens: None,
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
