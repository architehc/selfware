//! Swarm Agent Types
//!
//! Core types for agent roles, agents, votes, decisions, and tasks.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Visual design critic (requires vision model)
    VisualCritic,
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
            Self::VisualCritic => "VisualCritic",
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
            Self::VisualCritic => {
                "You are a visual design critic with expertise in UI/UX, composition, \
                 color theory, typography, and accessibility. You evaluate screenshots \
                 and provide structured JSON scores with improvement suggestions. \
                 Rate each dimension 0-100: composition, hierarchy, readability, \
                 consistency, accessibility. Include an overall weighted average and \
                 a list of concrete improvement suggestions."
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
            Self::Security => 10,    // Security concerns are highest priority
            Self::Architect => 8,    // Architecture decisions are important
            Self::Reviewer => 7,     // Reviews should be respected
            Self::Tester => 6,       // Testing insights matter
            Self::Performance => 5,  // Performance is important
            Self::Coder => 4,        // Coders know implementation details
            Self::DevOps => 4,       // DevOps understands operations
            Self::VisualCritic => 6, // Visual evaluation matters for design tasks
            Self::Documenter => 3,   // Documentation is supportive
            Self::General => 2,      // General is lowest priority
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
    /// Key into `Config.models` for model selection (None = use default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
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
            model_id: None,
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

    /// Select a model profile by key (must exist in `Config.models`)
    pub fn with_model(mut self, model_id: &str) -> Self {
        self.model_id = Some(model_id.to_string());
        self
    }

    /// Returns `true` if the agent's model profile supports vision.
    pub fn supports_vision(&self, config: &crate::config::Config) -> bool {
        self.model_id
            .as_deref()
            .and_then(|id| config.resolve_model(Some(id)))
            .map(|p| p.supports_vision())
            .unwrap_or(false)
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
            // Keep a non-zero floor so agents can recover after failure streaks.
            self.trust_score = (self.trust_score - 0.1).max(0.05);
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

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
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
    /// Priority where higher values represent higher priority.
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
