//! Multi-Agent Swarm System
//!
//! Specialist agents with role-specific prompts, consensus voting,
//! conflict resolution, and shared working memory.
//!
//! Features:
//! - Specialist agent roles (architect, coder, tester, reviewer)
//! - Role-specific system prompts
//! - Consensus voting for decisions
//! - Conflict resolution strategies
//! - Shared working memory
//! - Agent coordination

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Agent role in the swarm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AgentRole {
    /// System architect - designs high-level structure
    Architect,
    /// Code writer - implements features
    Coder,
    /// Test writer - creates tests
    Tester,
    /// Code reviewer - reviews changes
    Reviewer,
    /// Documentation writer
    Documenter,
    /// DevOps specialist
    DevOps,
    /// Security specialist
    Security,
    /// Performance optimizer
    Performance,
    /// General purpose
    #[default]
    General,
}

impl AgentRole {
    /// Get role name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Architect => "Architect",
            Self::Coder => "Coder",
            Self::Tester => "Tester",
            Self::Reviewer => "Reviewer",
            Self::Documenter => "Documenter",
            Self::DevOps => "DevOps",
            Self::Security => "Security",
            Self::Performance => "Performance",
            Self::General => "General",
        }
    }

    /// Get system prompt for role
    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Architect => {
                "You are a system architect. Focus on high-level design, modularity, \
                 scalability, and maintainability. Consider trade-offs and long-term implications. \
                 Suggest patterns and structures that promote clean architecture."
            }
            Self::Coder => {
                "You are an expert programmer. Write clean, efficient, and idiomatic code. \
                 Follow best practices and coding standards. Focus on correctness, readability, \
                 and performance. Handle edge cases and error conditions properly."
            }
            Self::Tester => {
                "You are a testing specialist. Design comprehensive test cases covering \
                 edge cases, error conditions, and happy paths. Focus on test coverage, \
                 test quality, and maintainable test code. Consider unit, integration, \
                 and end-to-end testing strategies."
            }
            Self::Reviewer => {
                "You are a code reviewer. Evaluate code quality, correctness, security, \
                 and performance. Look for bugs, potential issues, and improvement opportunities. \
                 Provide constructive feedback with specific suggestions."
            }
            Self::Documenter => {
                "You are a documentation specialist. Write clear, comprehensive documentation. \
                 Focus on explaining the 'why' as well as the 'how'. Create examples and \
                 maintain consistency in style and format."
            }
            Self::DevOps => {
                "You are a DevOps specialist. Focus on deployment, CI/CD, infrastructure, \
                 and operational concerns. Consider reliability, monitoring, and automation."
            }
            Self::Security => {
                "You are a security specialist. Identify vulnerabilities, review for security \
                 issues, and suggest secure implementations. Consider OWASP guidelines and \
                 security best practices."
            }
            Self::Performance => {
                "You are a performance specialist. Analyze and optimize for speed, memory \
                 usage, and efficiency. Profile code, identify bottlenecks, and suggest \
                 optimizations."
            }
            Self::General => {
                "You are a general-purpose assistant. Help with various coding tasks \
                 while maintaining high quality and best practices."
            }
        }
    }

    /// Get priority for this role in consensus
    pub fn priority(&self) -> u8 {
        match self {
            Self::Security => 10,   // Security concerns are highest priority
            Self::Architect => 8,   // Architecture decisions are important
            Self::Reviewer => 7,    // Reviews should be respected
            Self::Tester => 6,      // Testing insights matter
            Self::Performance => 5, // Performance is important
            Self::Coder => 4,       // Coders know implementation details
            Self::DevOps => 4,      // DevOps understands operations
            Self::Documenter => 3,  // Documentation is supportive
            Self::General => 2,     // General is lowest priority
        }
    }
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AgentStatus {
    /// Ready to accept tasks
    #[default]
    Idle,
    /// Currently working
    Working,
    /// Waiting for input
    Waiting,
    /// Completed current task
    Completed,
    /// Error occurred
    Error,
    /// Agent is paused
    Paused,
}

/// A specialist agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique identifier
    pub id: String,
    /// Agent name
    pub name: String,
    /// Role
    pub role: AgentRole,
    /// Status
    pub status: AgentStatus,
    /// Custom system prompt (overrides role default)
    pub custom_prompt: Option<String>,
    /// Expertise tags
    pub expertise: Vec<String>,
    /// Trust score (0.0 - 1.0)
    pub trust_score: f32,
    /// Tasks completed
    pub tasks_completed: u32,
    /// Tasks failed
    pub tasks_failed: u32,
    /// Created timestamp
    pub created_at: u64,
    /// Last active timestamp
    pub last_active: u64,
}

impl Agent {
    /// Create new agent
    pub fn new(name: impl Into<String>, role: AgentRole) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            role,
            status: AgentStatus::Idle,
            custom_prompt: None,
            expertise: Vec::new(),
            trust_score: 0.5,
            tasks_completed: 0,
            tasks_failed: 0,
            created_at: now,
            last_active: now,
        }
    }

    /// Set custom prompt
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.custom_prompt = Some(prompt.into());
        self
    }

    /// Add expertise
    pub fn with_expertise(mut self, expertise: impl Into<String>) -> Self {
        self.expertise.push(expertise.into());
        self
    }

    /// Get effective system prompt
    pub fn system_prompt(&self) -> &str {
        self.custom_prompt
            .as_deref()
            .unwrap_or_else(|| self.role.system_prompt())
    }

    /// Record task completion
    pub fn complete_task(&mut self, success: bool) {
        if success {
            self.tasks_completed += 1;
            self.trust_score = (self.trust_score + 0.1).min(1.0);
        } else {
            self.tasks_failed += 1;
            self.trust_score = (self.trust_score - 0.15).max(0.0);
        }
        self.status = AgentStatus::Completed;
        self.last_active = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Get success rate
    pub fn success_rate(&self) -> f32 {
        let total = self.tasks_completed + self.tasks_failed;
        if total == 0 {
            1.0
        } else {
            self.tasks_completed as f32 / total as f32
        }
    }

    /// Start working
    pub fn start_working(&mut self) {
        self.status = AgentStatus::Working;
        self.last_active = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Set idle
    pub fn set_idle(&mut self) {
        self.status = AgentStatus::Idle;
    }

    /// Set error
    pub fn set_error(&mut self) {
        self.status = AgentStatus::Error;
    }
}

/// Vote on a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// Agent ID
    pub agent_id: String,
    /// Agent role
    pub role: AgentRole,
    /// Vote choice
    pub choice: String,
    /// Confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Reasoning
    pub reasoning: String,
    /// Timestamp
    pub timestamp: u64,
}

impl Vote {
    /// Create new vote
    pub fn new(
        agent_id: impl Into<String>,
        role: AgentRole,
        choice: impl Into<String>,
        confidence: f32,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            role,
            choice: choice.into(),
            confidence: confidence.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Calculate weighted vote value
    pub fn weighted_value(&self, trust_score: f32) -> f32 {
        let role_weight = self.role.priority() as f32 / 10.0;
        self.confidence * role_weight * trust_score
    }
}

/// Decision requiring consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Decision ID
    pub id: String,
    /// Question/topic
    pub question: String,
    /// Available options
    pub options: Vec<String>,
    /// Collected votes
    pub votes: Vec<Vote>,
    /// Status
    pub status: DecisionStatus,
    /// Outcome (winning choice)
    pub outcome: Option<String>,
    /// Created timestamp
    pub created_at: u64,
    /// Resolved timestamp
    pub resolved_at: Option<u64>,
}

/// Decision status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DecisionStatus {
    /// Collecting votes
    #[default]
    Pending,
    /// Consensus reached
    Resolved,
    /// Conflict detected
    Conflict,
    /// Timed out
    TimedOut,
}

impl Decision {
    /// Create new decision
    pub fn new(question: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            question: question.into(),
            options,
            votes: Vec::new(),
            status: DecisionStatus::Pending,
            outcome: None,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            resolved_at: None,
        }
    }

    /// Add vote
    pub fn add_vote(&mut self, vote: Vote) {
        self.votes.push(vote);
    }

    /// Get votes for an option
    pub fn votes_for(&self, option: &str) -> Vec<&Vote> {
        self.votes.iter().filter(|v| v.choice == option).collect()
    }

    /// Calculate weighted score for an option
    pub fn weighted_score(&self, option: &str, trust_scores: &HashMap<String, f32>) -> f32 {
        self.votes
            .iter()
            .filter(|v| v.choice == option)
            .map(|v| {
                let trust = trust_scores.get(&v.agent_id).copied().unwrap_or(0.5);
                v.weighted_value(trust)
            })
            .sum()
    }

    /// Resolve the decision
    pub fn resolve(&mut self, trust_scores: &HashMap<String, f32>) -> Option<String> {
        if self.options.is_empty() {
            return None;
        }

        let mut scores: Vec<(String, f32)> = self
            .options
            .iter()
            .map(|opt| (opt.clone(), self.weighted_score(opt, trust_scores)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Check for conflict (scores too close)
        if scores.len() >= 2 {
            let diff = scores[0].1 - scores[1].1;
            if diff < 0.1 && scores[0].1 > 0.0 {
                self.status = DecisionStatus::Conflict;
                return None;
            }
        }

        self.outcome = Some(scores[0].0.clone());
        self.status = DecisionStatus::Resolved;
        self.resolved_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );

        self.outcome.clone()
    }

    /// Check if decision is pending
    pub fn is_pending(&self) -> bool {
        self.status == DecisionStatus::Pending
    }
}

/// Shared working memory for the swarm
#[derive(Debug, Clone, Default)]
pub struct SharedMemory {
    /// Key-value store
    data: HashMap<String, MemoryEntry>,
    /// Access log
    access_log: Vec<MemoryAccess>,
}

/// Memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Key
    pub key: String,
    /// Value
    pub value: String,
    /// Created by agent
    pub created_by: String,
    /// Created timestamp
    pub created_at: u64,
    /// Last modified by
    pub modified_by: Option<String>,
    /// Last modified timestamp
    pub modified_at: Option<u64>,
    /// Access count
    pub access_count: u32,
    /// Tags
    pub tags: Vec<String>,
}

/// Memory access record
#[derive(Debug, Clone)]
pub struct MemoryAccess {
    pub key: String,
    pub agent_id: String,
    pub action: MemoryAction,
    pub timestamp: u64,
}

/// Memory action type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAction {
    Read,
    Write,
    Delete,
}

impl SharedMemory {
    /// Create new shared memory
    pub fn new() -> Self {
        Self::default()
    }

    /// Write a value
    pub fn write(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        agent_id: impl Into<String>,
    ) {
        let key = key.into();
        let agent_id = agent_id.into();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(entry) = self.data.get_mut(&key) {
            entry.value = value.into();
            entry.modified_by = Some(agent_id.clone());
            entry.modified_at = Some(now);
        } else {
            self.data.insert(
                key.clone(),
                MemoryEntry {
                    key: key.clone(),
                    value: value.into(),
                    created_by: agent_id.clone(),
                    created_at: now,
                    modified_by: None,
                    modified_at: None,
                    access_count: 0,
                    tags: Vec::new(),
                },
            );
        }

        self.access_log.push(MemoryAccess {
            key,
            agent_id,
            action: MemoryAction::Write,
            timestamp: now,
        });
    }

    /// Read a value
    pub fn read(&mut self, key: &str, agent_id: impl Into<String>) -> Option<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(entry) = self.data.get_mut(key) {
            entry.access_count += 1;

            self.access_log.push(MemoryAccess {
                key: key.to_string(),
                agent_id: agent_id.into(),
                action: MemoryAction::Read,
                timestamp: now,
            });

            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Read without tracking
    pub fn peek(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|e| e.value.as_str())
    }

    /// Delete a value
    pub fn delete(&mut self, key: &str, agent_id: impl Into<String>) -> Option<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.access_log.push(MemoryAccess {
            key: key.to_string(),
            agent_id: agent_id.into(),
            action: MemoryAction::Delete,
            timestamp: now,
        });

        self.data.remove(key).map(|e| e.value)
    }

    /// List all keys
    pub fn keys(&self) -> Vec<&str> {
        self.data.keys().map(|k| k.as_str()).collect()
    }

    /// Get all entries
    pub fn entries(&self) -> Vec<&MemoryEntry> {
        self.data.values().collect()
    }

    /// Tag an entry
    pub fn tag(&mut self, key: &str, tag: impl Into<String>) {
        if let Some(entry) = self.data.get_mut(key) {
            entry.tags.push(tag.into());
        }
    }

    /// Find by tag
    pub fn find_by_tag(&self, tag: &str) -> Vec<&MemoryEntry> {
        self.data
            .values()
            .filter(|e| e.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Get access log
    pub fn access_log(&self) -> &[MemoryAccess] {
        &self.access_log
    }

    /// Clear memory
    pub fn clear(&mut self) {
        self.data.clear();
        self.access_log.clear();
    }
}

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
    /// Shared memory
    memory: Arc<RwLock<SharedMemory>>,
    /// Active decisions
    decisions: HashMap<String, Decision>,
    /// Conflict resolution strategy
    conflict_strategy: ConflictStrategy,
    /// Minimum consensus threshold (0.0 - 1.0)
    consensus_threshold: f32,
    /// Task queue
    task_queue: Vec<SwarmTask>,
}

/// A task for the swarm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    /// Task ID
    pub id: String,
    /// Task description
    pub description: String,
    /// Required roles
    pub required_roles: Vec<AgentRole>,
    /// Priority
    pub priority: u8,
    /// Status
    pub status: TaskStatus,
    /// Assigned agents
    pub assigned_agents: Vec<String>,
    /// Results from agents
    pub results: HashMap<String, String>,
    /// Created timestamp
    pub created_at: u64,
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl SwarmTask {
    /// Create new task
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            required_roles: Vec::new(),
            priority: 5,
            status: TaskStatus::Pending,
            assigned_agents: Vec::new(),
            results: HashMap::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Add required role
    pub fn with_role(mut self, role: AgentRole) -> Self {
        self.required_roles.push(role);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
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

        let vote = Vote::new(agent_id, agent.role, choice, confidence, reasoning);
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

        let decision = self
            .decisions
            .get_mut(decision_id)
            .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?;

        Ok(decision.resolve(&trust_scores))
    }

    /// Handle conflict
    pub fn resolve_conflict(&mut self, decision_id: &str) -> Result<Option<String>> {
        let decision = self
            .decisions
            .get(decision_id)
            .ok_or_else(|| anyhow!("Decision not found: {}", decision_id))?;

        if decision.status != DecisionStatus::Conflict {
            return Ok(decision.outcome.clone());
        }

        match self.conflict_strategy {
            ConflictStrategy::PriorityWins => {
                // Find vote with highest priority role
                let best_vote = decision.votes.iter().max_by_key(|v| v.role.priority());

                Ok(best_vote.map(|v| v.choice.clone()))
            }
            ConflictStrategy::ConfidenceWins => {
                // Find vote with highest confidence
                let best_vote = decision
                    .votes
                    .iter()
                    .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

                Ok(best_vote.map(|v| v.choice.clone()))
            }
            ConflictStrategy::MajorityWins => {
                // Simple majority
                let mut counts: HashMap<&str, usize> = HashMap::new();
                for vote in &decision.votes {
                    *counts.entry(&vote.choice).or_insert(0) += 1;
                }
                Ok(counts
                    .into_iter()
                    .max_by_key(|(_, count)| *count)
                    .map(|(choice, _)| choice.to_string()))
            }
            ConflictStrategy::HumanIntervention => {
                // Return None to indicate human input needed
                Ok(None)
            }
            ConflictStrategy::AcceptAll => {
                // Return all unique choices joined
                let choices: HashSet<_> = decision.votes.iter().map(|v| &v.choice).collect();
                Ok(Some(
                    choices.into_iter().cloned().collect::<Vec<_>>().join(", "),
                ))
            }
        }
    }

    /// Queue a task
    pub fn queue_task(&mut self, task: SwarmTask) {
        self.task_queue.push(task);
        // Sort by ascending priority so pop() returns highest priority
        self.task_queue.sort_by(|a, b| a.priority.cmp(&b.priority));
    }

    /// Get next task (highest priority)
    pub fn next_task(&mut self) -> Option<SwarmTask> {
        self.task_queue.pop()
    }

    /// Assign task to agents
    pub fn assign_task(&mut self, task_id: &str) -> Vec<String> {
        let task = match self.task_queue.iter_mut().find(|t| t.id == task_id) {
            Some(t) => t,
            None => return Vec::new(),
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

        task.assigned_agents = assigned.clone();
        task.status = TaskStatus::InProgress;

        assigned
    }

    /// Complete task for an agent
    pub fn complete_task(&mut self, task_id: &str, agent_id: &str, result: impl Into<String>) {
        if let Some(task) = self.task_queue.iter_mut().find(|t| t.id == task_id) {
            task.results.insert(agent_id.to_string(), result.into());

            // Check if all agents have submitted results
            if task.results.len() >= task.assigned_agents.len() {
                task.status = TaskStatus::Completed;
            }
        }

        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.complete_task(true);
        }
    }

    /// Get swarm statistics
    pub fn stats(&self) -> SwarmStats {
        let by_role: HashMap<AgentRole, usize> =
            self.agents.values().fold(HashMap::new(), |mut acc, a| {
                *acc.entry(a.role).or_insert(0) += 1;
                acc
            });

        let by_status: HashMap<AgentStatus, usize> =
            self.agents.values().fold(HashMap::new(), |mut acc, a| {
                *acc.entry(a.status).or_insert(0) += 1;
                acc
            });

        let avg_trust = if self.agents.is_empty() {
            0.0
        } else {
            self.agents.values().map(|a| a.trust_score).sum::<f32>() / self.agents.len() as f32
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

/// Create a standard development swarm
pub fn create_dev_swarm() -> Swarm {
    let mut swarm = Swarm::new();

    swarm.add_agent(Agent::new("Archie", AgentRole::Architect));
    swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));
    swarm.add_agent(Agent::new("Rex", AgentRole::Reviewer));

    swarm
}

/// Create a security-focused swarm
pub fn create_security_swarm() -> Swarm {
    let mut swarm = Swarm::new();

    swarm.add_agent(Agent::new("Guardian", AgentRole::Security));
    swarm.add_agent(Agent::new("Rex", AgentRole::Reviewer));
    swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));

    swarm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_default() {
        assert_eq!(AgentRole::default(), AgentRole::General);
    }

    #[test]
    fn test_agent_role_name() {
        assert_eq!(AgentRole::Architect.name(), "Architect");
        assert_eq!(AgentRole::Coder.name(), "Coder");
    }

    #[test]
    fn test_agent_role_priority() {
        assert!(AgentRole::Security.priority() > AgentRole::Coder.priority());
        assert!(AgentRole::Architect.priority() > AgentRole::General.priority());
    }

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new("TestAgent", AgentRole::Coder)
            .with_expertise("Rust")
            .with_expertise("Python");

        assert_eq!(agent.name, "TestAgent");
        assert_eq!(agent.role, AgentRole::Coder);
        assert_eq!(agent.expertise.len(), 2);
    }

    #[test]
    fn test_agent_custom_prompt() {
        let agent = Agent::new("Test", AgentRole::General).with_prompt("Custom prompt here");

        assert_eq!(agent.system_prompt(), "Custom prompt here");
    }

    #[test]
    fn test_agent_task_completion() {
        let mut agent = Agent::new("Test", AgentRole::Coder);
        let initial_trust = agent.trust_score;

        agent.complete_task(true);
        assert!(agent.trust_score > initial_trust);
        assert_eq!(agent.tasks_completed, 1);

        agent.complete_task(false);
        assert_eq!(agent.tasks_failed, 1);
    }

    #[test]
    fn test_agent_success_rate() {
        let mut agent = Agent::new("Test", AgentRole::Coder);
        agent.tasks_completed = 8;
        agent.tasks_failed = 2;

        assert!((agent.success_rate() - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_vote_creation() {
        let vote = Vote::new(
            "agent1",
            AgentRole::Reviewer,
            "option_a",
            0.9,
            "Good choice",
        );

        assert_eq!(vote.choice, "option_a");
        assert_eq!(vote.confidence, 0.9);
    }

    #[test]
    fn test_vote_weighted_value() {
        let vote = Vote::new("agent1", AgentRole::Security, "opt", 1.0, "reason");
        let value = vote.weighted_value(1.0);

        // Security has priority 10, so weight = 1.0
        assert!((value - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_decision_creation() {
        let decision = Decision::new("Which approach?", vec!["A".into(), "B".into()]);

        assert!(decision.is_pending());
        assert_eq!(decision.options.len(), 2);
    }

    #[test]
    fn test_decision_add_vote() {
        let mut decision = Decision::new("Test?", vec!["Yes".into(), "No".into()]);
        decision.add_vote(Vote::new("a1", AgentRole::Coder, "Yes", 0.8, "reason"));

        assert_eq!(decision.votes.len(), 1);
        assert_eq!(decision.votes_for("Yes").len(), 1);
    }

    #[test]
    fn test_decision_resolve() {
        let mut decision = Decision::new("Test?", vec!["A".into(), "B".into()]);
        decision.add_vote(Vote::new("a1", AgentRole::Coder, "A", 0.9, "r1"));
        decision.add_vote(Vote::new("a2", AgentRole::Tester, "A", 0.8, "r2"));
        decision.add_vote(Vote::new("a3", AgentRole::Reviewer, "B", 0.5, "r3"));

        let trust_scores: HashMap<String, f32> = [
            ("a1".to_string(), 0.8),
            ("a2".to_string(), 0.7),
            ("a3".to_string(), 0.6),
        ]
        .into_iter()
        .collect();

        let outcome = decision.resolve(&trust_scores);
        assert!(outcome.is_some());
        assert_eq!(outcome.unwrap(), "A");
    }

    #[test]
    fn test_shared_memory_write_read() {
        let mut memory = SharedMemory::new();

        memory.write("key1", "value1", "agent1");
        let value = memory.read("key1", "agent2");

        assert_eq!(value, Some("value1".to_string()));
    }

    #[test]
    fn test_shared_memory_peek() {
        let mut memory = SharedMemory::new();
        memory.write("key1", "value1", "agent1");

        let value = memory.peek("key1");
        assert_eq!(value, Some("value1"));
    }

    #[test]
    fn test_shared_memory_delete() {
        let mut memory = SharedMemory::new();
        memory.write("key1", "value1", "agent1");

        let deleted = memory.delete("key1", "agent1");
        assert_eq!(deleted, Some("value1".to_string()));
        assert!(memory.peek("key1").is_none());
    }

    #[test]
    fn test_shared_memory_tags() {
        let mut memory = SharedMemory::new();
        memory.write("key1", "value1", "agent1");
        memory.tag("key1", "important");

        let tagged = memory.find_by_tag("important");
        assert_eq!(tagged.len(), 1);
    }

    #[test]
    fn test_shared_memory_access_log() {
        let mut memory = SharedMemory::new();
        memory.write("key1", "value1", "agent1");
        memory.read("key1", "agent2");

        assert_eq!(memory.access_log().len(), 2);
    }

    #[test]
    fn test_swarm_task_creation() {
        let task = SwarmTask::new("Implement feature")
            .with_role(AgentRole::Coder)
            .with_role(AgentRole::Tester)
            .with_priority(8);

        assert_eq!(task.required_roles.len(), 2);
        assert_eq!(task.priority, 8);
    }

    #[test]
    fn test_swarm_creation() {
        let swarm = Swarm::new();
        assert_eq!(swarm.list_agents().len(), 0);
    }

    #[test]
    fn test_swarm_add_agent() {
        let mut swarm = Swarm::new();
        let agent = Agent::new("Test", AgentRole::Coder);
        let id = swarm.add_agent(agent);

        assert!(swarm.get_agent(&id).is_some());
    }

    #[test]
    fn test_swarm_remove_agent() {
        let mut swarm = Swarm::new();
        let agent = Agent::new("Test", AgentRole::Coder);
        let id = swarm.add_agent(agent);

        let removed = swarm.remove_agent(&id);
        assert!(removed.is_some());
        assert!(swarm.get_agent(&id).is_none());
    }

    #[test]
    fn test_swarm_agents_by_role() {
        let mut swarm = Swarm::new();
        swarm.add_agent(Agent::new("C1", AgentRole::Coder));
        swarm.add_agent(Agent::new("C2", AgentRole::Coder));
        swarm.add_agent(Agent::new("T1", AgentRole::Tester));

        assert_eq!(swarm.agents_by_role(AgentRole::Coder).len(), 2);
        assert_eq!(swarm.agents_by_role(AgentRole::Tester).len(), 1);
    }

    #[test]
    fn test_swarm_idle_agents() {
        let mut swarm = Swarm::new();
        let id1 = swarm.add_agent(Agent::new("A1", AgentRole::Coder));
        swarm.add_agent(Agent::new("A2", AgentRole::Coder));

        swarm.get_agent_mut(&id1).unwrap().start_working();

        assert_eq!(swarm.idle_agents().len(), 1);
    }

    #[test]
    fn test_swarm_create_decision() {
        let mut swarm = Swarm::new();
        let decision_id = swarm.create_decision("Which?", vec!["A".into(), "B".into()]);

        assert!(!decision_id.is_empty());
    }

    #[test]
    fn test_swarm_vote() {
        let mut swarm = Swarm::new();
        let agent_id = swarm.add_agent(Agent::new("Test", AgentRole::Coder));
        let decision_id = swarm.create_decision("Which?", vec!["A".into(), "B".into()]);

        let result = swarm.vote(&decision_id, &agent_id, "A", 0.9, "Looks good");
        assert!(result.is_ok());
    }

    #[test]
    fn test_swarm_resolve_decision() {
        let mut swarm = Swarm::new();
        let a1 = swarm.add_agent(Agent::new("A1", AgentRole::Architect));
        let a2 = swarm.add_agent(Agent::new("A2", AgentRole::Coder));

        let decision_id = swarm.create_decision("Which?", vec!["X".into(), "Y".into()]);

        swarm.vote(&decision_id, &a1, "X", 0.9, "r1").unwrap();
        swarm.vote(&decision_id, &a2, "X", 0.8, "r2").unwrap();

        let outcome = swarm.resolve_decision(&decision_id).unwrap();
        assert_eq!(outcome, Some("X".to_string()));
    }

    #[test]
    fn test_swarm_queue_task() {
        let mut swarm = Swarm::new();

        swarm.queue_task(SwarmTask::new("Task 1").with_priority(5));
        swarm.queue_task(SwarmTask::new("Task 2").with_priority(8));

        // Higher priority should come first
        let task = swarm.next_task().unwrap();
        assert_eq!(task.priority, 8);
    }

    #[test]
    fn test_swarm_stats() {
        let mut swarm = Swarm::new();
        swarm.add_agent(Agent::new("A1", AgentRole::Coder));
        swarm.add_agent(Agent::new("A2", AgentRole::Tester));

        let stats = swarm.stats();
        assert_eq!(stats.total_agents, 2);
    }

    #[test]
    fn test_create_dev_swarm() {
        let swarm = create_dev_swarm();
        assert_eq!(swarm.list_agents().len(), 4);
    }

    #[test]
    fn test_create_security_swarm() {
        let swarm = create_security_swarm();
        assert!(!swarm.agents_by_role(AgentRole::Security).is_empty());
    }

    #[test]
    fn test_conflict_strategy_default() {
        assert_eq!(ConflictStrategy::default(), ConflictStrategy::PriorityWins);
    }

    #[test]
    fn test_agent_status_default() {
        assert_eq!(AgentStatus::default(), AgentStatus::Idle);
    }

    #[test]
    fn test_decision_status_default() {
        assert_eq!(DecisionStatus::default(), DecisionStatus::Pending);
    }

    #[test]
    fn test_task_status_default() {
        assert_eq!(TaskStatus::default(), TaskStatus::Pending);
    }

    #[test]
    fn test_shared_memory_keys() {
        let mut memory = SharedMemory::new();
        memory.write("k1", "v1", "a1");
        memory.write("k2", "v2", "a1");

        let keys = memory.keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_shared_memory_clear() {
        let mut memory = SharedMemory::new();
        memory.write("k1", "v1", "a1");
        memory.clear();

        assert!(memory.keys().is_empty());
    }

    #[test]
    fn test_swarm_with_settings() {
        let swarm = Swarm::new()
            .with_conflict_strategy(ConflictStrategy::MajorityWins)
            .with_consensus_threshold(0.7);

        assert_eq!(swarm.conflict_strategy, ConflictStrategy::MajorityWins);
        assert!((swarm.consensus_threshold - 0.7).abs() < 0.01);
    }

    // =========================================================================
    // AGENTIC SWARM INTEGRATION TESTS
    // =========================================================================

    /// Comprehensive test simulating a full agentic swarm workflow:
    /// - Multiple specialized agents collaborate on a feature
    /// - Agents use shared memory to coordinate
    /// - Consensus voting on architectural decisions
    /// - Conflict resolution when agents disagree
    /// - Task assignment and completion tracking
    #[test]
    fn test_agentic_swarm_feature_development() {
        // Create a development swarm with specialized agents
        let mut swarm = Swarm::new()
            .with_conflict_strategy(ConflictStrategy::PriorityWins)
            .with_consensus_threshold(0.6);

        // Add specialized agents
        let architect_id = swarm.add_agent(
            Agent::new("Alice", AgentRole::Architect)
                .with_expertise("System Design")
                .with_expertise("Scalability"),
        );
        let coder_id = swarm.add_agent(
            Agent::new("Bob", AgentRole::Coder)
                .with_expertise("Rust")
                .with_expertise("Performance"),
        );
        let _tester_id = swarm.add_agent(
            Agent::new("Carol", AgentRole::Tester)
                .with_expertise("Integration Testing")
                .with_expertise("TDD"),
        );
        let reviewer_id = swarm.add_agent(
            Agent::new("Dave", AgentRole::Reviewer)
                .with_expertise("Code Quality")
                .with_expertise("Security"),
        );
        let security_id = swarm.add_agent(
            Agent::new("Eve", AgentRole::Security)
                .with_expertise("OWASP")
                .with_expertise("Cryptography"),
        );

        assert_eq!(swarm.list_agents().len(), 5);

        // Phase 1: Architect proposes design in shared memory
        {
            let memory = swarm.memory();
            let mut mem = memory.write().unwrap();

            mem.write(
                "feature:auth:design",
                "Implement JWT-based authentication with refresh tokens",
                &architect_id,
            );
            mem.write(
                "feature:auth:components",
                "TokenService, AuthMiddleware, UserRepository",
                &architect_id,
            );
            mem.tag("feature:auth:design", "architecture");
            mem.tag("feature:auth:design", "auth");
        }

        // Phase 2: Create a decision for authentication approach
        let auth_decision_id = swarm.create_decision(
            "Which authentication approach should we use?",
            vec![
                "JWT with HttpOnly cookies".into(),
                "JWT in Authorization header".into(),
                "Session-based auth".into(),
            ],
        );

        // Agents vote based on their expertise
        swarm
            .vote(
                &auth_decision_id,
                &architect_id,
                "JWT with HttpOnly cookies",
                0.85,
                "More secure against XSS, better for web apps",
            )
            .unwrap();
        swarm
            .vote(
                &auth_decision_id,
                &security_id,
                "JWT with HttpOnly cookies",
                0.95,
                "HttpOnly cookies prevent XSS token theft",
            )
            .unwrap();
        swarm
            .vote(
                &auth_decision_id,
                &coder_id,
                "JWT in Authorization header",
                0.7,
                "Simpler to implement for API clients",
            )
            .unwrap();
        swarm
            .vote(
                &auth_decision_id,
                &reviewer_id,
                "JWT with HttpOnly cookies",
                0.8,
                "Industry best practice for web security",
            )
            .unwrap();

        // Resolve the decision
        let auth_outcome = swarm.resolve_decision(&auth_decision_id).unwrap();
        assert_eq!(auth_outcome, Some("JWT with HttpOnly cookies".to_string()));

        // Store decision in shared memory
        {
            let memory = swarm.memory();
            let mut mem = memory.write().unwrap();
            mem.write(
                "decision:auth:approach",
                auth_outcome.as_ref().unwrap(),
                "swarm",
            );
        }

        // Phase 3: Create implementation tasks
        let impl_task = SwarmTask::new("Implement JWT authentication")
            .with_role(AgentRole::Coder)
            .with_role(AgentRole::Tester)
            .with_priority(8);

        let review_task = SwarmTask::new("Review authentication implementation")
            .with_role(AgentRole::Reviewer)
            .with_role(AgentRole::Security)
            .with_priority(7);

        swarm.queue_task(impl_task);
        swarm.queue_task(review_task);

        // Get highest priority task
        let task = swarm.next_task().unwrap();
        assert_eq!(task.priority, 8);
        assert!(task.description.contains("Implement"));

        // Phase 4: Simulate agent work and completion
        {
            let coder = swarm.get_agent_mut(&coder_id).unwrap();
            coder.start_working();
            assert_eq!(coder.status, AgentStatus::Working);

            // Coder completes implementation
            coder.complete_task(true);
            assert_eq!(coder.tasks_completed, 1);
            assert!(coder.trust_score > 0.5); // Trust increased
        }

        // Phase 5: Store implementation results in shared memory
        {
            let memory = swarm.memory();
            let mut mem = memory.write().unwrap();

            mem.write(
                "impl:auth:token_service",
                "TokenService with sign/verify/refresh methods implemented",
                &coder_id,
            );
            mem.write(
                "impl:auth:middleware",
                "AuthMiddleware extracts and validates JWT from cookies",
                &coder_id,
            );
            mem.tag("impl:auth:token_service", "implementation");
            mem.tag("impl:auth:middleware", "implementation");
        }

        // Phase 6: Security agent reviews and flags concern
        let security_decision_id = swarm.create_decision(
            "Should we add rate limiting to auth endpoints?",
            vec![
                "Yes, implement rate limiting".into(),
                "No, not needed initially".into(),
            ],
        );

        swarm
            .vote(
                &security_decision_id,
                &security_id,
                "Yes, implement rate limiting",
                0.95,
                "Essential to prevent brute force attacks",
            )
            .unwrap();
        swarm
            .vote(
                &security_decision_id,
                &coder_id,
                "No, not needed initially",
                0.6,
                "Can add later, want to ship faster",
            )
            .unwrap();
        swarm
            .vote(
                &security_decision_id,
                &architect_id,
                "Yes, implement rate limiting",
                0.8,
                "Security should not be deferred",
            )
            .unwrap();

        let security_outcome = swarm.resolve_decision(&security_decision_id).unwrap();
        assert_eq!(
            security_outcome,
            Some("Yes, implement rate limiting".to_string())
        );

        // Phase 7: Verify shared memory state
        {
            let memory = swarm.memory();
            let mem = memory.read().unwrap();

            // Check all entries exist
            assert!(mem.peek("feature:auth:design").is_some());
            assert!(mem.peek("decision:auth:approach").is_some());
            assert!(mem.peek("impl:auth:token_service").is_some());

            // Check tagged entries
            let impl_entries = mem.find_by_tag("implementation");
            assert_eq!(impl_entries.len(), 2);

            // Verify access log recorded activity
            assert!(!mem.access_log().is_empty());
        }

        // Phase 8: Final stats verification
        let stats = swarm.stats();
        assert_eq!(stats.total_agents, 5);
        // Most agents should be idle or completed
        let idle_count = stats
            .agents_by_status
            .get(&AgentStatus::Idle)
            .copied()
            .unwrap_or(0);
        let completed_count = stats
            .agents_by_status
            .get(&AgentStatus::Completed)
            .copied()
            .unwrap_or(0);
        assert!(idle_count + completed_count >= 4);

        // Verify agent trust scores reflect performance
        let coder = swarm.get_agent(&coder_id).unwrap();
        assert!(coder.success_rate() > 0.0);
    }

    /// Test conflict resolution when agents strongly disagree
    #[test]
    fn test_agentic_swarm_conflict_resolution() {
        let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::PriorityWins);

        let security_id = swarm.add_agent(Agent::new("Security", AgentRole::Security));
        let coder_id = swarm.add_agent(Agent::new("Coder", AgentRole::Coder));
        let perf_id = swarm.add_agent(Agent::new("Perf", AgentRole::Performance));

        // Create a decision where agents will conflict
        let decision_id = swarm.create_decision(
            "How to handle sensitive data?",
            vec![
                "Encrypt everything".into(),
                "Encrypt only PII".into(),
                "No encryption, faster performance".into(),
            ],
        );

        // Each agent votes differently with high confidence
        swarm
            .vote(
                &decision_id,
                &security_id,
                "Encrypt everything",
                1.0,
                "Security is paramount",
            )
            .unwrap();
        swarm
            .vote(
                &decision_id,
                &perf_id,
                "No encryption, faster performance",
                0.9,
                "Encryption adds latency",
            )
            .unwrap();
        swarm
            .vote(
                &decision_id,
                &coder_id,
                "Encrypt only PII",
                0.85,
                "Balanced approach",
            )
            .unwrap();

        // Resolve - security should win due to priority
        let outcome = swarm.resolve_decision(&decision_id).unwrap();

        // Security has highest priority (10), so their vote should win
        assert_eq!(outcome, Some("Encrypt everything".to_string()));
    }

    /// Test majority voting conflict strategy
    #[test]
    fn test_agentic_swarm_majority_voting() {
        let mut swarm = Swarm::new().with_conflict_strategy(ConflictStrategy::MajorityWins);

        // Add 5 coders (same priority)
        let mut agent_ids = Vec::new();
        for i in 0..5 {
            let id = swarm.add_agent(Agent::new(format!("Coder{}", i), AgentRole::Coder));
            agent_ids.push(id);
        }

        let decision_id = swarm.create_decision(
            "Which framework?",
            vec!["Actix".into(), "Axum".into(), "Rocket".into()],
        );

        // 3 vote Axum, 2 vote Actix
        swarm
            .vote(&decision_id, &agent_ids[0], "Axum", 0.8, "Modern")
            .unwrap();
        swarm
            .vote(&decision_id, &agent_ids[1], "Axum", 0.7, "Good DX")
            .unwrap();
        swarm
            .vote(&decision_id, &agent_ids[2], "Axum", 0.9, "Tower ecosystem")
            .unwrap();
        swarm
            .vote(&decision_id, &agent_ids[3], "Actix", 0.85, "Battle tested")
            .unwrap();
        swarm
            .vote(&decision_id, &agent_ids[4], "Actix", 0.8, "Performance")
            .unwrap();

        // With similar weights, might trigger conflict
        let outcome = swarm.resolve_decision(&decision_id);

        // If conflict, use conflict resolution
        if let Ok(Some(choice)) = outcome {
            // Either Axum wins outright or conflict resolution picks one
            assert!(choice == "Axum" || choice == "Actix");
        } else {
            // Conflict detected - resolve with majority strategy
            let resolved = swarm.resolve_conflict(&decision_id).unwrap();
            assert_eq!(resolved, Some("Axum".to_string())); // 3 vs 2
        }
    }

    /// Test swarm coordination with shared memory
    #[test]
    fn test_agentic_swarm_memory_coordination() {
        let mut swarm = Swarm::new();

        let writer_id = swarm.add_agent(Agent::new("Writer", AgentRole::Coder));
        let reader_id = swarm.add_agent(Agent::new("Reader", AgentRole::Tester));

        // Writer stores state
        {
            let memory = swarm.memory();
            let mut mem = memory.write().unwrap();

            mem.write("state:phase", "testing", &writer_id);
            mem.write("state:tests_passed", "42", &writer_id);
            mem.write("state:tests_failed", "3", &writer_id);
        }

        // Reader accesses state
        {
            let memory = swarm.memory();
            let mut mem = memory.write().unwrap();

            let phase = mem.read("state:phase", &reader_id);
            assert_eq!(phase, Some("testing".to_string()));

            let passed = mem.read("state:tests_passed", &reader_id);
            assert_eq!(passed, Some("42".to_string()));
        }

        // Verify access log shows both agents
        {
            let memory = swarm.memory();
            let mem = memory.read().unwrap();
            let log = mem.access_log();

            let writer_actions: Vec<_> = log.iter().filter(|a| a.agent_id == writer_id).collect();
            let reader_actions: Vec<_> = log.iter().filter(|a| a.agent_id == reader_id).collect();

            assert!(!writer_actions.is_empty());
            assert!(!reader_actions.is_empty());
        }
    }

    /// Test dynamic agent trust adjustment
    #[test]
    fn test_agentic_swarm_trust_dynamics() {
        let mut swarm = Swarm::new();

        let agent_id = swarm.add_agent(Agent::new("Dynamic", AgentRole::Coder));

        // Initial trust
        let initial_trust = swarm.get_agent(&agent_id).unwrap().trust_score;
        assert!((initial_trust - 0.5).abs() < 0.01);

        // Success increases trust
        swarm.get_agent_mut(&agent_id).unwrap().complete_task(true);
        let after_success = swarm.get_agent(&agent_id).unwrap().trust_score;
        assert!(after_success > initial_trust);

        // Failure decreases trust
        swarm.get_agent_mut(&agent_id).unwrap().complete_task(false);
        let after_failure = swarm.get_agent(&agent_id).unwrap().trust_score;
        assert!(after_failure < after_success);

        // Multiple successes build trust
        for _ in 0..5 {
            swarm.get_agent_mut(&agent_id).unwrap().complete_task(true);
        }
        let high_trust = swarm.get_agent(&agent_id).unwrap().trust_score;
        assert!(high_trust > 0.7);

        // Verify success rate
        let agent = swarm.get_agent(&agent_id).unwrap();
        let rate = agent.success_rate();
        assert!(rate > 0.5); // 6 successes, 1 failure
    }
}
