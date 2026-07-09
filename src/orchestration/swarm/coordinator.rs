//! Swarm Coordinator
//!
//! The main swarm coordinator that manages agents, memory, decisions, and tasks.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use super::memory::SharedMemory;
use super::types::{
    Agent, AgentRole, AgentStatus, Decision, DecisionStatus, SwarmTask, TaskStatus,
};

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConflictStrategy {
    /// Highest priority role wins
    #[default]
    PriorityWins,
    /// Highest confidence wins
    ConfidenceWins,
    /// Majority vote wins
    MajorityWins,
    /// Request human intervention
    HumanIntervention,
    /// Accept all (merge if possible)
    AcceptAll,
}

/// Agent swarm coordinator
pub struct Swarm {
    /// Agents in the swarm
    agents: HashMap<String, Agent>,
    /// Shared memory.
    ///
    /// Uses `std::sync::RwLock` intentionally: all lock acquisitions are brief
    /// (HashMap read/write) and never held across `.await` points, so async
    /// executor starvation is not a concern. Callers use the
    /// `unwrap_or_else(|e| e.into_inner())` pattern to recover from poisoning.
    memory: Arc<RwLock<SharedMemory>>,
    /// Active decisions
    decisions: HashMap<String, Decision>,
    /// Conflict resolution strategy
    conflict_strategy: ConflictStrategy,
    /// Minimum consensus threshold (0.0 - 1.0)
    consensus_threshold: f32,
    /// Task queue (pending tasks waiting to be executed)
    task_queue: Vec<SwarmTask>,
    /// Active and completed tasks (tasks that have been popped from queue)
    active_tasks: HashMap<String, SwarmTask>,
    /// Timeout for pending decisions (seconds)
    decision_timeout_secs: u64,
    /// Optional shared resource pressure for task gating
    resource_pressure: Option<Arc<std::sync::RwLock<crate::resource::ResourcePressure>>>,
}

/// Swarm statistics
#[derive(Debug, Clone)]
pub struct SwarmStats {
    pub total_agents: usize,
    pub agents_by_role: HashMap<AgentRole, usize>,
    pub agents_by_status: HashMap<AgentStatus, usize>,
    pub pending_decisions: usize,
    pub queued_tasks: usize,
    pub average_trust: f32,
}

impl Swarm {
    /// Create new swarm
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            memory: Arc::new(RwLock::new(SharedMemory::new())),
            decisions: HashMap::new(),
            conflict_strategy: ConflictStrategy::default(),
            consensus_threshold: 0.6,
            task_queue: Vec::new(),
            active_tasks: HashMap::new(),
            decision_timeout_secs: 300,
            resource_pressure: None,
        }
    }

    /// Set conflict strategy
    pub fn with_conflict_strategy(mut self, strategy: ConflictStrategy) -> Self {
        self.conflict_strategy = strategy;
        self
    }

    /// Set consensus threshold
    pub fn with_consensus_threshold(mut self, threshold: f32) -> Self {
        self.consensus_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set decision timeout in seconds
    pub fn with_decision_timeout(mut self, secs: u64) -> Self {
        self.decision_timeout_secs = secs;
        self
    }

    /// Set shared resource pressure handle for task gating
    pub fn set_resource_pressure(
        &mut self,
        pressure: Arc<std::sync::RwLock<crate::resource::ResourcePressure>>,
    ) {
        self.resource_pressure = Some(pressure);
    }

    /// Sweep pending decisions that have exceeded the timeout, marking them
    /// as `TimedOut`. Returns the IDs of timed-out decisions.
    pub fn sweep_timed_out_decisions(&mut self) -> Vec<String> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let timeout = self.decision_timeout_secs;
        let mut timed_out = Vec::new();

        for (id, decision) in &mut self.decisions {
            if decision.status == DecisionStatus::Pending
                && now.saturating_sub(decision.created_at) >= timeout
            {
                decision.status = DecisionStatus::TimedOut;
                decision.resolved_at = Some(now);
                timed_out.push(id.clone());
            }
        }

        timed_out
    }

    /// Add agent to swarm
    pub fn add_agent(&mut self, agent: Agent) -> String {
        let id = agent.id.clone();
        self.agents.insert(id.clone(), agent);
        id
    }

    /// Remove agent
    pub fn remove_agent(&mut self, id: &str) -> Option<Agent> {
        self.agents.remove(id)
    }

    /// Get agent
    pub fn get_agent(&self, id: &str) -> Option<&Agent> {
        self.agents.get(id)
    }

    /// Get agent mutably
    pub fn get_agent_mut(&mut self, id: &str) -> Option<&mut Agent> {
        self.agents.get_mut(id)
    }

    /// List agents
    pub fn list_agents(&self) -> Vec<&Agent> {
        self.agents.values().collect()
    }

    /// List agents by role
    pub fn agents_by_role(&self, role: AgentRole) -> Vec<&Agent> {
        self.agents.values().filter(|a| a.role == role).collect()
    }

    /// List idle agents
    pub fn idle_agents(&self) -> Vec<&Agent> {
        self.agents
            .values()
            .filter(|a| a.status == AgentStatus::Idle)
            .collect()
    }

    /// Get shared memory
    pub fn memory(&self) -> Arc<RwLock<SharedMemory>> {
        Arc::clone(&self.memory)
    }

    /// Create a decision
    pub fn create_decision(&mut self, question: impl Into<String>, options: Vec<String>) -> String {
        let decision = Decision::new(question, options);
        let id = decision.id.clone();
        self.decisions.insert(id.clone(), decision);
        id
    }

    /// Add vote to decision
    pub fn vote(
        &mut self,
        decision_id: &str,
        agent_id: &str,
        choice: impl Into<String>,
        confidence: f32,
        reasoning: impl Into<String>,
    ) -> Result<()> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;

        let decision = self
            .decisions
            .get_mut(decision_id)
            .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?;

        if !decision.is_pending() {
            return Err(anyhow!("Decision already resolved"));
        }

        let vote = super::types::Vote::new(agent_id, agent.role, choice, confidence, reasoning);
        decision.add_vote(vote);

        Ok(())
    }

    /// Resolve a decision
    pub fn resolve_decision(&mut self, decision_id: &str) -> Result<Option<String>> {
        let trust_scores: HashMap<String, f32> = self
            .agents
            .iter()
            .map(|(id, a)| (id.clone(), a.trust_score))
            .collect();

        let consensus_threshold = self.consensus_threshold;

        let decision = self
            .decisions
            .get_mut(decision_id)
            .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?;

        Ok(decision.resolve_with_threshold(&trust_scores, consensus_threshold))
    }

    /// List all decisions
    pub fn list_decisions(&self) -> Vec<&Decision> {
        self.decisions.values().collect()
    }

    /// Get a specific decision
    pub fn get_decision(&self, id: &str) -> Option<&Decision> {
        self.decisions.get(id)
    }

    /// List all tasks in the queue
    pub fn list_tasks(&self) -> Vec<&SwarmTask> {
        self.task_queue.iter().collect()
    }

    /// Get a specific task (checks both queued and active tasks)
    pub fn get_task(&self, id: &str) -> Option<&SwarmTask> {
        self.active_tasks
            .get(id)
            .or_else(|| self.task_queue.iter().find(|t| t.id == *id))
    }

    /// Handle conflict
    pub fn resolve_conflict(&mut self, decision_id: &str) -> Result<Option<String>> {
        // First, check the status without holding a mutable borrow.
        let status = self
            .decisions
            .get(decision_id)
            .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?
            .status;

        if status != DecisionStatus::Conflict {
            return Ok(
                self.decisions
                    .get(decision_id)
                    .and_then(|d| d.outcome.clone()),
            );
        }

        // Compute the resolution based on conflict strategy.
        let resolution: Option<String> = {
            let decision = self
                .decisions
                .get(decision_id)
                .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?;

            match self.conflict_strategy {
                ConflictStrategy::PriorityWins => {
                    // Find vote with highest priority role
                    let best_vote = decision.votes.iter().max_by_key(|v| v.role.priority());
                    best_vote.map(|v| v.choice.clone())
                }
                ConflictStrategy::ConfidenceWins => {
                    // Find vote with highest confidence
                    let best_vote = decision.votes.iter().max_by(|a, b| {
                        a.confidence
                            .partial_cmp(&b.confidence)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    best_vote.map(|v| v.choice.clone())
                }
                ConflictStrategy::MajorityWins => {
                    // Simple majority
                    let mut counts: std::collections::HashMap<&str, usize> = HashMap::new();
                    for vote in &decision.votes {
                        *counts.entry(&vote.choice).or_insert(0) += 1;
                    }
                    counts
                        .into_iter()
                        .max_by_key(|(_, count)| *count)
                        .map(|(choice, _)| choice.to_string())
                }
                ConflictStrategy::HumanIntervention => {
                    // Return None to indicate human input needed
                    None
                }
                ConflictStrategy::AcceptAll => {
                    // Return all unique choices joined
                    let choices: std::collections::HashSet<_> =
                        decision.votes.iter().map(|v| &v.choice).collect();
                    Some(
                        choices.into_iter().cloned().collect::<Vec<_>>().join(", "),
                    )
                }
            }
        };

        // Update the decision's status and outcome to reflect the resolution.
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let decision = self
            .decisions
            .get_mut(decision_id)
            .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?;

        if resolution.is_some() {
            decision.status = DecisionStatus::Resolved;
            decision.outcome = resolution.clone();
        } else {
            // HumanIntervention or no votes: leave as Conflict but record that
            // resolution was attempted (no outcome to set).
            // Keep status as Conflict so callers know human input is still needed.
        }
        decision.resolved_at = Some(now);

        Ok(resolution)
    }

    /// Queue a task. Returns an error if resource pressure is `High` or `Critical`.
    pub fn queue_task(&mut self, task: SwarmTask) -> Result<()> {
        if let Some(ref pressure_lock) = self.resource_pressure {
            let pressure = pressure_lock.read().unwrap_or_else(|e| e.into_inner());
            if matches!(
                *pressure,
                crate::resource::ResourcePressure::High
                    | crate::resource::ResourcePressure::Critical
            ) {
                return Err(anyhow!(
                    "Cannot queue task: resource pressure is {:?}",
                    *pressure
                ));
            }
        }
        self.task_queue.push(task);
        // Keep ascending order so `pop()` returns the highest numeric priority.
        self.task_queue.sort_unstable_by_key(|task| task.priority);
        Ok(())
    }

    /// Get next task (highest priority)
    /// Moves the task from the queue to active_tasks
    pub fn next_task(&mut self) -> Option<String> {
        let task = self.task_queue.pop()?;
        let task_id = task.id.clone();
        // Move task to active_tasks so it can be assigned and completed
        self.active_tasks.insert(task_id.clone(), task);
        Some(task_id)
    }

    /// Assign task to agents
    /// If the task is still in the queue (not yet moved to active_tasks),
    /// it will be moved automatically before assignment.
    pub fn assign_task(&mut self, task_id: &str) -> Vec<String> {
        // Ensure the task is in active_tasks (move from queue if needed)
        if !self.active_tasks.contains_key(task_id) {
            if let Some(index) = self.task_queue.iter().position(|t| t.id == *task_id) {
                let task = self.task_queue.remove(index);
                self.active_tasks.insert(task_id.to_string(), task);
            } else {
                tracing::warn!("Task {} not found in active tasks or queue", task_id);
                return Vec::new();
            }
        }

        let task = match self.active_tasks.get_mut(task_id) {
            Some(t) => t,
            None => {
                tracing::error!("Task {} disappeared after queue check", task_id);
                return Vec::new();
            }
        };

        let mut assigned = Vec::new();

        for role in &task.required_roles.clone() {
            // Find best idle agent for this role
            let best = self
                .agents
                .values()
                .filter(|a| a.role == *role && a.status == AgentStatus::Idle)
                .max_by(|a, b| {
                    a.trust_score
                        .partial_cmp(&b.trust_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let Some(agent) = best {
                let agent_id = agent.id.clone();
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.start_working();
                    assigned.push(agent_id);
                }
            }
        }

        // Don't mark InProgress if no agents were assigned
        if assigned.is_empty() {
            tracing::warn!("No idle agents available for task {}", task_id);
            return Vec::new();
        }

        task.assigned_agents = assigned.clone();
        task.status = TaskStatus::InProgress;

        assigned
    }

    /// Complete task for an agent
    pub fn complete_task(&mut self, task_id: &str, agent_id: &str, result: impl Into<String>) {
        let task = match self.active_tasks.get_mut(task_id) {
            Some(t) => t,
            None => {
                tracing::warn!(
                    "Task {} not found in active tasks during completion",
                    task_id
                );
                return;
            }
        };

        task.results.insert(agent_id.to_string(), result.into());

        // Don't complete a task with no assigned agents
        if task.assigned_agents.is_empty() {
            tracing::warn!(
                "Task {} has no assigned agents, cannot complete",
                task_id
            );
            // Still update the agent status
            if let Some(agent) = self.agents.get_mut(agent_id) {
                agent.complete_task(true);
            }
            return;
        }

        // Check if all agents have submitted results — done atomically
        // within the same mutable borrow to avoid inconsistent state
        let all_done = task.results.len() >= task.assigned_agents.len();
        if all_done {
            task.status = TaskStatus::Completed;
        }

        // Update agent status only when task was found, keeping both
        // operations together so they succeed or fail as a unit
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.complete_task(true);
        }
    }

    /// Get swarm statistics
    pub fn stats(&self) -> SwarmStats {
        let mut by_role = HashMap::new();
        let mut by_status = HashMap::new();
        let mut total_trust = 0.0;

        for agent in self.agents.values() {
            *by_role.entry(agent.role).or_insert(0) += 1;
            *by_status.entry(agent.status).or_insert(0) += 1;
            total_trust += agent.trust_score;
        }

        let avg_trust = if self.agents.is_empty() {
            0.0
        } else {
            total_trust / self.agents.len() as f32
        };

        SwarmStats {
            total_agents: self.agents.len(),
            agents_by_role: by_role,
            agents_by_status: by_status,
            pending_decisions: self.decisions.values().filter(|d| d.is_pending()).count(),
            queued_tasks: self.task_queue.len(),
            average_trust: avg_trust,
        }
    }
}

impl Default for Swarm {
    fn default() -> Self {
        Self::new()
    }
}
