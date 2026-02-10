//! Multi-Model Router
//!
//! Intelligent model selection by task:
//! - Task type detection
//! - Route to appropriate model
//! - Agent swarm for parallel work
//! - Pair programming mode

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model provider
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelProvider {
    OpenAI,
    Anthropic,
    Ollama,
    Gemini,
    Local,
    Custom(String),
}

impl ModelProvider {
    /// Get provider name
    pub fn name(&self) -> &str {
        match self {
            ModelProvider::OpenAI => "openai",
            ModelProvider::Anthropic => "anthropic",
            ModelProvider::Ollama => "ollama",
            ModelProvider::Gemini => "gemini",
            ModelProvider::Local => "local",
            ModelProvider::Custom(name) => name,
        }
    }

    /// Parse from string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => ModelProvider::OpenAI,
            "anthropic" => ModelProvider::Anthropic,
            "ollama" => ModelProvider::Ollama,
            "gemini" => ModelProvider::Gemini,
            "local" => ModelProvider::Local,
            other => ModelProvider::Custom(other.to_string()),
        }
    }
}

impl std::fmt::Display for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Model capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Code generation
    CodeGeneration,
    /// Code review and analysis
    CodeReview,
    /// Natural language understanding
    NaturalLanguage,
    /// Image understanding
    Vision,
    /// Long context handling
    LongContext,
    /// Function/tool calling
    ToolUse,
    /// Fast response
    LowLatency,
    /// Complex reasoning
    Reasoning,
    /// Creative writing
    Creative,
    /// Technical documentation
    Documentation,
    /// Debugging
    Debugging,
    /// Testing
    Testing,
}

impl Capability {
    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            Capability::CodeGeneration => "💻",
            Capability::CodeReview => "🔍",
            Capability::NaturalLanguage => "💬",
            Capability::Vision => "👁️",
            Capability::LongContext => "📚",
            Capability::ToolUse => "🔧",
            Capability::LowLatency => "⚡",
            Capability::Reasoning => "🧠",
            Capability::Creative => "✨",
            Capability::Documentation => "📝",
            Capability::Debugging => "🐛",
            Capability::Testing => "🧪",
        }
    }

    /// Name for display
    pub fn name(&self) -> &'static str {
        match self {
            Capability::CodeGeneration => "Code Generation",
            Capability::CodeReview => "Code Review",
            Capability::NaturalLanguage => "Natural Language",
            Capability::Vision => "Vision",
            Capability::LongContext => "Long Context",
            Capability::ToolUse => "Tool Use",
            Capability::LowLatency => "Low Latency",
            Capability::Reasoning => "Reasoning",
            Capability::Creative => "Creative",
            Capability::Documentation => "Documentation",
            Capability::Debugging => "Debugging",
            Capability::Testing => "Testing",
        }
    }
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Provider
    pub provider: ModelProvider,
    /// API endpoint
    pub endpoint: Option<String>,
    /// API key environment variable
    pub api_key_env: Option<String>,
    /// Maximum context length
    pub max_context: usize,
    /// Capabilities
    pub capabilities: Vec<Capability>,
    /// Cost per 1K input tokens
    pub cost_input: Option<f64>,
    /// Cost per 1K output tokens
    pub cost_output: Option<f64>,
    /// Is this model available?
    pub available: bool,
    /// Default temperature
    pub temperature: f32,
    /// Priority (higher = preferred)
    pub priority: i32,
}

impl ModelConfig {
    /// Create a new model config
    pub fn new(id: &str, name: &str, provider: ModelProvider) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            provider,
            endpoint: None,
            api_key_env: None,
            max_context: 4096,
            capabilities: Vec::new(),
            cost_input: None,
            cost_output: None,
            available: true,
            temperature: 0.7,
            priority: 0,
        }
    }

    /// Set endpoint
    pub fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = Some(endpoint.to_string());
        self
    }

    /// Set API key env var
    pub fn with_api_key_env(mut self, env: &str) -> Self {
        self.api_key_env = Some(env.to_string());
        self
    }

    /// Set max context
    pub fn with_context(mut self, max: usize) -> Self {
        self.max_context = max;
        self
    }

    /// Add capability
    pub fn with_capability(mut self, cap: Capability) -> Self {
        self.capabilities.push(cap);
        self
    }

    /// Add multiple capabilities
    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.capabilities.extend(caps);
        self
    }

    /// Set costs
    pub fn with_cost(mut self, input: f64, output: f64) -> Self {
        self.cost_input = Some(input);
        self.cost_output = Some(output);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if model has capability
    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    /// Check if model has all capabilities
    pub fn has_all_capabilities(&self, caps: &[Capability]) -> bool {
        caps.iter().all(|c| self.has_capability(c))
    }

    /// Check if model has any of the capabilities
    pub fn has_any_capability(&self, caps: &[Capability]) -> bool {
        caps.iter().any(|c| self.has_capability(c))
    }

    /// Calculate estimated cost for tokens
    pub fn estimate_cost(&self, input_tokens: usize, output_tokens: usize) -> Option<f64> {
        match (self.cost_input, self.cost_output) {
            (Some(ci), Some(co)) => {
                let input_cost = (input_tokens as f64 / 1000.0) * ci;
                let output_cost = (output_tokens as f64 / 1000.0) * co;
                Some(input_cost + output_cost)
            }
            _ => None,
        }
    }

    /// Display string
    pub fn display(&self) -> String {
        format!(
            "{} ({}, {}K context)",
            self.name,
            self.provider,
            self.max_context / 1000
        )
    }
}

/// Task type for routing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// Writing new code
    CodeWrite,
    /// Reviewing existing code
    CodeReview,
    /// Debugging/fixing bugs
    Debug,
    /// Writing tests
    Test,
    /// Refactoring code
    Refactor,
    /// Writing documentation
    Document,
    /// Answering questions
    Question,
    /// Explaining code
    Explain,
    /// Complex multi-step task
    Complex,
    /// Quick one-off task
    Quick,
    /// Creative/brainstorming
    Creative,
    /// Unknown
    Unknown,
}

impl TaskType {
    /// Required capabilities for this task type
    pub fn required_capabilities(&self) -> Vec<Capability> {
        match self {
            TaskType::CodeWrite => vec![Capability::CodeGeneration],
            TaskType::CodeReview => vec![Capability::CodeReview, Capability::Reasoning],
            TaskType::Debug => vec![Capability::Debugging, Capability::CodeGeneration],
            TaskType::Test => vec![Capability::Testing, Capability::CodeGeneration],
            TaskType::Refactor => vec![Capability::CodeGeneration, Capability::CodeReview],
            TaskType::Document => vec![Capability::Documentation],
            TaskType::Question => vec![Capability::NaturalLanguage],
            TaskType::Explain => vec![Capability::NaturalLanguage, Capability::Reasoning],
            TaskType::Complex => vec![Capability::Reasoning, Capability::ToolUse],
            TaskType::Quick => vec![Capability::LowLatency],
            TaskType::Creative => vec![Capability::Creative],
            TaskType::Unknown => vec![],
        }
    }

    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            TaskType::CodeWrite => "✍️",
            TaskType::CodeReview => "🔍",
            TaskType::Debug => "🐛",
            TaskType::Test => "🧪",
            TaskType::Refactor => "♻️",
            TaskType::Document => "📝",
            TaskType::Question => "❓",
            TaskType::Explain => "💡",
            TaskType::Complex => "🧩",
            TaskType::Quick => "⚡",
            TaskType::Creative => "✨",
            TaskType::Unknown => "❔",
        }
    }

    /// Detect task type from prompt
    pub fn detect(prompt: &str) -> Self {
        let lower = prompt.to_lowercase();

        // Check for specific keywords
        if lower.contains("review") || lower.contains("check my code") {
            return TaskType::CodeReview;
        }
        if lower.contains("debug") || lower.contains("fix") || lower.contains("bug") {
            return TaskType::Debug;
        }
        if lower.contains("test") || lower.contains("spec") || lower.contains("unit test") {
            return TaskType::Test;
        }
        if lower.contains("refactor") || lower.contains("clean up") || lower.contains("improve") {
            return TaskType::Refactor;
        }
        if lower.contains("document") || lower.contains("readme") || lower.contains("docstring") {
            return TaskType::Document;
        }
        if lower.contains("explain") || lower.contains("what does") || lower.contains("how does") {
            return TaskType::Explain;
        }
        if lower.starts_with("what") || lower.starts_with("why") || lower.starts_with("how") {
            return TaskType::Question;
        }
        if lower.contains("write") || lower.contains("create") || lower.contains("implement") {
            return TaskType::CodeWrite;
        }
        if lower.contains("brainstorm") || lower.contains("ideas") || lower.contains("creative") {
            return TaskType::Creative;
        }

        // Check length for quick vs complex
        if prompt.len() < 50 {
            return TaskType::Quick;
        }
        if prompt.len() > 500 {
            return TaskType::Complex;
        }

        TaskType::Unknown
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            TaskType::CodeWrite => "Code Write",
            TaskType::CodeReview => "Code Review",
            TaskType::Debug => "Debug",
            TaskType::Test => "Test",
            TaskType::Refactor => "Refactor",
            TaskType::Document => "Document",
            TaskType::Question => "Question",
            TaskType::Explain => "Explain",
            TaskType::Complex => "Complex",
            TaskType::Quick => "Quick",
            TaskType::Creative => "Creative",
            TaskType::Unknown => "Unknown",
        };
        write!(f, "{}", name)
    }
}

/// Routing decision
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// Selected model
    pub model: ModelConfig,
    /// Task type detected
    pub task_type: TaskType,
    /// Confidence score (0-1)
    pub confidence: f32,
    /// Alternative models
    pub alternatives: Vec<ModelConfig>,
    /// Reason for selection
    pub reason: String,
}

impl RouteDecision {
    /// Create a new decision
    pub fn new(model: ModelConfig, task_type: TaskType, reason: String) -> Self {
        Self {
            model,
            task_type,
            confidence: 1.0,
            alternatives: Vec::new(),
            reason,
        }
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    /// Add alternatives
    pub fn with_alternatives(mut self, alts: Vec<ModelConfig>) -> Self {
        self.alternatives = alts;
        self
    }
}

/// Model router
#[derive(Debug, Default)]
pub struct ModelRouter {
    /// Available models
    models: Vec<ModelConfig>,
    /// Default model
    default_model: Option<String>,
    /// Task type overrides
    overrides: HashMap<TaskType, String>,
    /// Usage statistics
    stats: RoutingStats,
}

/// Routing statistics
#[derive(Debug, Clone, Default)]
pub struct RoutingStats {
    /// Total routes
    pub total: usize,
    /// Routes by model
    pub by_model: HashMap<String, usize>,
    /// Routes by task type
    pub by_task_type: HashMap<TaskType, usize>,
    /// Total estimated cost
    pub total_cost: f64,
}

impl ModelRouter {
    /// Create a new router
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a model
    pub fn add_model(&mut self, model: ModelConfig) {
        self.models.push(model);
    }

    /// Set default model
    pub fn set_default(&mut self, model_id: &str) {
        self.default_model = Some(model_id.to_string());
    }

    /// Set override for task type
    pub fn set_override(&mut self, task_type: TaskType, model_id: &str) {
        self.overrides.insert(task_type, model_id.to_string());
    }

    /// Get model by ID
    pub fn get_model(&self, id: &str) -> Option<&ModelConfig> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Get all models
    pub fn models(&self) -> &[ModelConfig] {
        &self.models
    }

    /// Get available models
    pub fn available_models(&self) -> Vec<&ModelConfig> {
        self.models.iter().filter(|m| m.available).collect()
    }

    /// Route a task to the best model
    pub fn route(&mut self, prompt: &str) -> Option<RouteDecision> {
        // Detect task type
        let task_type = TaskType::detect(prompt);

        // Check for override
        if let Some(model_id) = self.overrides.get(&task_type) {
            if let Some(model) = self.get_model(model_id).cloned() {
                self.record_route(&model.id, &task_type);
                return Some(RouteDecision::new(
                    model,
                    task_type.clone(),
                    format!("Override for {} task type", task_type),
                ));
            }
        }

        // Find models with required capabilities
        let required = task_type.required_capabilities();
        let mut candidates: Vec<_> = self
            .models
            .iter()
            .filter(|m| m.available)
            .filter(|m| m.has_all_capabilities(&required))
            .cloned()
            .collect();

        // Sort by priority (descending)
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority));

        if let Some(best) = candidates.first() {
            let alts = candidates.iter().skip(1).cloned().collect();
            self.record_route(&best.id, &task_type);
            return Some(
                RouteDecision::new(
                    best.clone(),
                    task_type,
                    format!("Best match for required capabilities: {:?}", required),
                )
                .with_alternatives(alts),
            );
        }

        // Fallback to default
        if let Some(default_id) = &self.default_model {
            if let Some(model) = self.get_model(default_id).cloned() {
                self.record_route(&model.id, &task_type);
                return Some(
                    RouteDecision::new(model, task_type, "Fallback to default model".to_string())
                        .with_confidence(0.5),
                );
            }
        }

        // Return first available
        if let Some(model) = self.models.iter().find(|m| m.available).cloned() {
            self.record_route(&model.id, &task_type);
            return Some(
                RouteDecision::new(model, task_type, "Only available model".to_string())
                    .with_confidence(0.3),
            );
        }

        None
    }

    /// Record a route for statistics
    fn record_route(&mut self, model_id: &str, task_type: &TaskType) {
        self.stats.total += 1;
        *self.stats.by_model.entry(model_id.to_string()).or_insert(0) += 1;
        *self
            .stats
            .by_task_type
            .entry(task_type.clone())
            .or_insert(0) += 1;
    }

    /// Get routing statistics
    pub fn stats(&self) -> &RoutingStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = RoutingStats::default();
    }
}

/// Agent role in a swarm
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    /// Lead agent coordinating work
    Lead,
    /// Worker agent executing tasks
    Worker,
    /// Reviewer checking work
    Reviewer,
    /// Specialist for specific tasks
    Specialist(String),
}

impl AgentRole {
    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            AgentRole::Lead => "👑",
            AgentRole::Worker => "👷",
            AgentRole::Reviewer => "🔍",
            AgentRole::Specialist(_) => "🎯",
        }
    }
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Lead => write!(f, "Lead"),
            AgentRole::Worker => write!(f, "Worker"),
            AgentRole::Reviewer => write!(f, "Reviewer"),
            AgentRole::Specialist(s) => write!(f, "Specialist({})", s),
        }
    }
}

/// Agent in a swarm
#[derive(Debug, Clone)]
pub struct SwarmAgent {
    /// Agent ID
    pub id: String,
    /// Agent role
    pub role: AgentRole,
    /// Assigned model
    pub model: ModelConfig,
    /// Current status
    pub status: AgentStatus,
    /// Assigned task
    pub task: Option<String>,
    /// Result of work
    pub result: Option<String>,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Working,
    Done,
    Failed,
    Waiting,
}

impl AgentStatus {
    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            AgentStatus::Idle => "💤",
            AgentStatus::Working => "⚙️",
            AgentStatus::Done => "✅",
            AgentStatus::Failed => "❌",
            AgentStatus::Waiting => "⏳",
        }
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            AgentStatus::Idle => "Idle",
            AgentStatus::Working => "Working",
            AgentStatus::Done => "Done",
            AgentStatus::Failed => "Failed",
            AgentStatus::Waiting => "Waiting",
        };
        write!(f, "{}", name)
    }
}

impl SwarmAgent {
    /// Create a new agent
    pub fn new(id: &str, role: AgentRole, model: ModelConfig) -> Self {
        Self {
            id: id.to_string(),
            role,
            model,
            status: AgentStatus::Idle,
            task: None,
            result: None,
        }
    }

    /// Assign a task
    pub fn assign(&mut self, task: String) {
        self.task = Some(task);
        self.status = AgentStatus::Working;
    }

    /// Mark as done
    pub fn complete(&mut self, result: String) {
        self.result = Some(result);
        self.status = AgentStatus::Done;
    }

    /// Mark as failed
    pub fn fail(&mut self) {
        self.status = AgentStatus::Failed;
    }

    /// Display string
    pub fn display(&self) -> String {
        format!(
            "{} {} [{}] - {}",
            self.role.icon(),
            self.id,
            self.model.name,
            self.status
        )
    }
}

/// Swarm for parallel work
#[derive(Debug, Default)]
pub struct AgentSwarm {
    /// Agents in the swarm
    agents: Vec<SwarmAgent>,
    /// Overall task
    task: Option<String>,
    /// Status
    status: SwarmStatus,
}

/// Swarm status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwarmStatus {
    #[default]
    Idle,
    Planning,
    Executing,
    Reviewing,
    Complete,
    Failed,
}

impl SwarmStatus {
    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            SwarmStatus::Idle => "💤",
            SwarmStatus::Planning => "📋",
            SwarmStatus::Executing => "⚙️",
            SwarmStatus::Reviewing => "🔍",
            SwarmStatus::Complete => "✅",
            SwarmStatus::Failed => "❌",
        }
    }
}

impl AgentSwarm {
    /// Create a new swarm
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an agent
    pub fn add_agent(&mut self, agent: SwarmAgent) {
        self.agents.push(agent);
    }

    /// Get agents
    pub fn agents(&self) -> &[SwarmAgent] {
        &self.agents
    }

    /// Get mutable agents
    pub fn agents_mut(&mut self) -> &mut [SwarmAgent] {
        &mut self.agents
    }

    /// Set task
    pub fn set_task(&mut self, task: String) {
        self.task = Some(task);
        self.status = SwarmStatus::Planning;
    }

    /// Start execution
    pub fn start(&mut self) {
        self.status = SwarmStatus::Executing;
    }

    /// Check if all done
    pub fn is_complete(&self) -> bool {
        self.agents
            .iter()
            .all(|a| matches!(a.status, AgentStatus::Done | AgentStatus::Failed))
    }

    /// Get agent by ID
    pub fn get_agent(&self, id: &str) -> Option<&SwarmAgent> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// Get mutable agent by ID
    pub fn get_agent_mut(&mut self, id: &str) -> Option<&mut SwarmAgent> {
        self.agents.iter_mut().find(|a| a.id == id)
    }

    /// Get idle agents
    pub fn idle_agents(&self) -> Vec<&SwarmAgent> {
        self.agents
            .iter()
            .filter(|a| a.status == AgentStatus::Idle)
            .collect()
    }

    /// Get working agents
    pub fn working_agents(&self) -> Vec<&SwarmAgent> {
        self.agents
            .iter()
            .filter(|a| a.status == AgentStatus::Working)
            .collect()
    }

    /// Count agents by status
    pub fn status_counts(&self) -> HashMap<AgentStatus, usize> {
        let mut counts = HashMap::new();
        for agent in &self.agents {
            *counts.entry(agent.status).or_insert(0) += 1;
        }
        counts
    }

    /// Complete the swarm
    pub fn complete(&mut self) {
        let has_failure = self.agents.iter().any(|a| a.status == AgentStatus::Failed);
        self.status = if has_failure {
            SwarmStatus::Failed
        } else {
            SwarmStatus::Complete
        };
    }

    /// Get status
    pub fn status(&self) -> SwarmStatus {
        self.status
    }

    /// Agent count
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}

/// Pair programming session
#[derive(Debug)]
pub struct PairSession {
    /// Session ID
    pub id: String,
    /// Human participant
    pub human: String,
    /// AI partner model
    pub ai_model: ModelConfig,
    /// Current role (who is driving)
    pub driver: Driver,
    /// Turn count
    pub turns: usize,
    /// Session start
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Is active
    pub active: bool,
}

/// Who is driving the session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Human,
    AI,
}

impl Driver {
    /// Switch driver
    pub fn switch(&self) -> Self {
        match self {
            Driver::Human => Driver::AI,
            Driver::AI => Driver::Human,
        }
    }

    /// Icon
    pub fn icon(&self) -> &'static str {
        match self {
            Driver::Human => "👤",
            Driver::AI => "🤖",
        }
    }
}

impl std::fmt::Display for Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Driver::Human => write!(f, "Human"),
            Driver::AI => write!(f, "AI"),
        }
    }
}

impl PairSession {
    /// Create a new session
    pub fn new(human: &str, ai_model: ModelConfig) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            human: human.to_string(),
            ai_model,
            driver: Driver::Human, // Human starts as driver
            turns: 0,
            started_at: chrono::Utc::now(),
            active: true,
        }
    }

    /// Switch driver
    pub fn switch_driver(&mut self) {
        self.driver = self.driver.switch();
        self.turns += 1;
    }

    /// End session
    pub fn end(&mut self) {
        self.active = false;
    }

    /// Session duration
    pub fn duration(&self) -> chrono::Duration {
        chrono::Utc::now() - self.started_at
    }

    /// Display status
    pub fn display(&self) -> String {
        format!(
            "Pair session: {} + {} | Driver: {} | Turns: {}",
            self.human, self.ai_model.name, self.driver, self.turns
        )
    }
}

// ============================================================================
// Efficient Model Selection (Task #76)
// ============================================================================

/// Task complexity level for model sizing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// Simple tasks: typo fixes, small edits
    Trivial,
    /// Straightforward tasks: single function, clear requirements
    Simple,
    /// Moderate tasks: multi-function, some context needed
    Moderate,
    /// Complex tasks: multi-file, architectural decisions
    Complex,
    /// Expert tasks: novel algorithms, complex debugging
    Expert,
}

impl TaskComplexity {
    /// Estimate complexity from task description
    pub fn estimate(task: &str) -> Self {
        let words = task.split_whitespace().count();
        let lines_mentioned = task.matches("line").count() + task.matches("file").count();
        let complexity_words = [
            "refactor",
            "architect",
            "design",
            "optimize",
            "debug",
            "complex",
        ]
        .iter()
        .filter(|w| task.to_lowercase().contains(*w))
        .count();

        if words < 10 && lines_mentioned == 0 {
            Self::Trivial
        } else if words < 30 && complexity_words == 0 {
            Self::Simple
        } else if words < 100 && complexity_words <= 1 {
            Self::Moderate
        } else if complexity_words <= 2 {
            Self::Complex
        } else {
            Self::Expert
        }
    }

    /// Minimum model tier for this complexity
    pub fn min_model_tier(&self) -> ModelTier {
        match self {
            Self::Trivial => ModelTier::Nano,
            Self::Simple => ModelTier::Small,
            Self::Moderate => ModelTier::Medium,
            Self::Complex => ModelTier::Large,
            Self::Expert => ModelTier::XLarge,
        }
    }

    /// Token multiplier for estimating context needs
    pub fn token_multiplier(&self) -> f32 {
        match self {
            Self::Trivial => 0.1,
            Self::Simple => 0.3,
            Self::Moderate => 0.5,
            Self::Complex => 0.8,
            Self::Expert => 1.0,
        }
    }
}

/// Model size tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ModelTier {
    /// Tiny models: 1B params or less
    Nano,
    /// Small models: 1-7B params
    Small,
    /// Medium models: 7-13B params
    Medium,
    /// Large models: 13-70B params
    Large,
    /// Extra large: 70B+ or frontier models
    XLarge,
}

impl ModelTier {
    /// Typical response latency (ms)
    pub fn typical_latency_ms(&self) -> u64 {
        match self {
            Self::Nano => 100,
            Self::Small => 300,
            Self::Medium => 800,
            Self::Large => 2000,
            Self::XLarge => 5000,
        }
    }

    /// Relative cost multiplier
    pub fn cost_multiplier(&self) -> f64 {
        match self {
            Self::Nano => 0.01,
            Self::Small => 0.1,
            Self::Medium => 0.3,
            Self::Large => 1.0,
            Self::XLarge => 5.0,
        }
    }
}

/// Extended model config with sizing info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizedModel {
    /// Base model config
    pub config: ModelConfig,
    /// Model tier
    pub tier: ModelTier,
    /// Is this a local model?
    pub is_local: bool,
    /// Average tokens per second
    pub tokens_per_sec: f32,
    /// Whether model is currently available
    pub health_status: HealthStatus,
    /// Last health check time
    pub last_health_check: Option<u64>,
}

/// Health status of a model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Model is healthy and responsive
    Healthy,
    /// Model is degraded (slow or partial failures)
    Degraded,
    /// Model is unhealthy (not responding)
    Unhealthy,
    /// Health status unknown
    Unknown,
}

impl SizedModel {
    pub fn new(config: ModelConfig, tier: ModelTier, is_local: bool) -> Self {
        Self {
            config,
            tier,
            is_local,
            tokens_per_sec: match tier {
                ModelTier::Nano => 100.0,
                ModelTier::Small => 50.0,
                ModelTier::Medium => 30.0,
                ModelTier::Large => 15.0,
                ModelTier::XLarge => 10.0,
            },
            health_status: HealthStatus::Unknown,
            last_health_check: None,
        }
    }

    /// Estimate time to complete tokens
    pub fn estimate_time_ms(&self, tokens: usize) -> u64 {
        let seconds = tokens as f32 / self.tokens_per_sec;
        (seconds * 1000.0) as u64
    }

    /// Check if model meets minimum tier requirement
    pub fn meets_tier(&self, min_tier: ModelTier) -> bool {
        self.tier >= min_tier
    }
}

/// Criteria for model selection
#[derive(Debug, Clone, Default)]
pub struct SelectionCriteria {
    /// Required capabilities
    pub required_capabilities: Vec<Capability>,
    /// Minimum model tier
    pub min_tier: Option<ModelTier>,
    /// Maximum cost per 1K tokens
    pub max_cost: Option<f64>,
    /// Maximum latency (ms)
    pub max_latency_ms: Option<u64>,
    /// Prefer local models
    pub prefer_local: bool,
    /// Require local (no cloud)
    pub require_local: bool,
    /// Minimum context length
    pub min_context: Option<usize>,
    /// Whether to allow degraded models
    pub allow_degraded: bool,
}

impl SelectionCriteria {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.required_capabilities = caps;
        self
    }

    pub fn with_min_tier(mut self, tier: ModelTier) -> Self {
        self.min_tier = Some(tier);
        self
    }

    pub fn with_max_cost(mut self, cost: f64) -> Self {
        self.max_cost = Some(cost);
        self
    }

    pub fn with_max_latency(mut self, latency_ms: u64) -> Self {
        self.max_latency_ms = Some(latency_ms);
        self
    }

    pub fn local_only(mut self) -> Self {
        self.require_local = true;
        self
    }

    pub fn prefer_local(mut self) -> Self {
        self.prefer_local = true;
        self
    }

    pub fn with_min_context(mut self, context: usize) -> Self {
        self.min_context = Some(context);
        self
    }
}

/// Decision for local vs cloud execution
#[derive(Debug, Clone)]
pub struct LocalCloudDecision {
    /// Recommended location
    pub location: ExecutionLocation,
    /// Confidence in decision
    pub confidence: f32,
    /// Estimated latency
    pub estimated_latency_ms: u64,
    /// Estimated cost
    pub estimated_cost: f64,
    /// Reason for decision
    pub reason: String,
}

/// Where to execute the model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLocation {
    /// Execute on local hardware
    Local,
    /// Execute in cloud
    Cloud,
    /// Execute part locally, part in cloud
    Hybrid,
}

/// Analyzer for local vs cloud decisions
pub struct LocalCloudDecider {
    /// Available local models
    local_models: Vec<SizedModel>,
    /// Available cloud models
    cloud_models: Vec<SizedModel>,
    /// Cost sensitivity (0-1, higher = more cost sensitive)
    cost_sensitivity: f32,
    /// Latency sensitivity (0-1, higher = more latency sensitive)
    latency_sensitivity: f32,
    /// Privacy requirements (require local for sensitive data)
    require_privacy: bool,
}

impl LocalCloudDecider {
    pub fn new() -> Self {
        Self {
            local_models: Vec::new(),
            cloud_models: Vec::new(),
            cost_sensitivity: 0.5,
            latency_sensitivity: 0.5,
            require_privacy: false,
        }
    }

    pub fn add_local_model(&mut self, model: SizedModel) {
        self.local_models.push(model);
    }

    pub fn add_cloud_model(&mut self, model: SizedModel) {
        self.cloud_models.push(model);
    }

    pub fn with_cost_sensitivity(mut self, sensitivity: f32) -> Self {
        self.cost_sensitivity = sensitivity.clamp(0.0, 1.0);
        self
    }

    pub fn with_latency_sensitivity(mut self, sensitivity: f32) -> Self {
        self.latency_sensitivity = sensitivity.clamp(0.0, 1.0);
        self
    }

    pub fn with_privacy_required(mut self) -> Self {
        self.require_privacy = true;
        self
    }

    /// Decide between local and cloud for a task
    pub fn decide(&self, task: &str, criteria: &SelectionCriteria) -> LocalCloudDecision {
        // Privacy requirement forces local
        if self.require_privacy || criteria.require_local {
            if self.local_models.is_empty() {
                return LocalCloudDecision {
                    location: ExecutionLocation::Local,
                    confidence: 0.0,
                    estimated_latency_ms: 0,
                    estimated_cost: 0.0,
                    reason: "Local required but no local models available".to_string(),
                };
            }
            return LocalCloudDecision {
                location: ExecutionLocation::Local,
                confidence: 1.0,
                estimated_latency_ms: self.local_models[0].tier.typical_latency_ms(),
                estimated_cost: 0.0,
                reason: "Privacy/local requirement".to_string(),
            };
        }

        // Estimate task complexity
        let complexity = TaskComplexity::estimate(task);
        let min_tier = complexity.min_model_tier();

        // Find capable local models
        let local_capable: Vec<_> = self
            .local_models
            .iter()
            .filter(|m| m.meets_tier(min_tier))
            .filter(|m| m.health_status != HealthStatus::Unhealthy)
            .collect();

        // Find capable cloud models
        let cloud_capable: Vec<_> = self
            .cloud_models
            .iter()
            .filter(|m| m.meets_tier(min_tier))
            .filter(|m| m.health_status != HealthStatus::Unhealthy)
            .collect();

        // Score each option
        let local_score = if local_capable.is_empty() {
            0.0
        } else {
            let latency_score = 1.0 - (self.latency_sensitivity * 0.3); // Local is usually faster
            let cost_score = 1.0; // Local is free
            let capability_score = if local_capable.iter().any(|m| m.tier >= min_tier) {
                1.0
            } else {
                0.5
            };
            (latency_score + cost_score * self.cost_sensitivity + capability_score) / 3.0
        };

        let cloud_score = if cloud_capable.is_empty() {
            0.0
        } else {
            let latency_score = 1.0 - (self.latency_sensitivity * 0.5); // Cloud has more latency
            let cost_score = 1.0 - self.cost_sensitivity; // Cloud costs money
            let capability_score = 1.0; // Cloud usually has best models
            (latency_score + cost_score + capability_score) / 3.0
        };

        let (location, confidence, latency, cost, reason) =
            if local_score > cloud_score && !local_capable.is_empty() {
                let model = local_capable[0];
                (
                    ExecutionLocation::Local,
                    local_score,
                    model.tier.typical_latency_ms(),
                    0.0,
                    format!(
                        "Local preferred (score: {:.2} vs {:.2})",
                        local_score, cloud_score
                    ),
                )
            } else if !cloud_capable.is_empty() {
                let model = cloud_capable[0];
                (
                    ExecutionLocation::Cloud,
                    cloud_score,
                    model.tier.typical_latency_ms() + 100, // Add network latency
                    model.config.cost_input.unwrap_or(0.01),
                    format!(
                        "Cloud preferred (score: {:.2} vs {:.2})",
                        cloud_score, local_score
                    ),
                )
            } else if !local_capable.is_empty() {
                let model = local_capable[0];
                (
                    ExecutionLocation::Local,
                    0.5,
                    model.tier.typical_latency_ms(),
                    0.0,
                    "Fallback to local (no cloud available)".to_string(),
                )
            } else {
                (
                    ExecutionLocation::Cloud,
                    0.0,
                    5000,
                    0.0,
                    "No suitable models available".to_string(),
                )
            };

        LocalCloudDecision {
            location,
            confidence,
            estimated_latency_ms: latency,
            estimated_cost: cost,
            reason,
        }
    }
}

impl Default for LocalCloudDecider {
    fn default() -> Self {
        Self::new()
    }
}

/// A batch of similar tasks for efficient processing
#[derive(Debug, Clone)]
pub struct TaskBatch {
    /// Batch ID
    pub id: String,
    /// Tasks in this batch
    pub tasks: Vec<BatchTask>,
    /// Common task type
    pub task_type: TaskType,
    /// Selected model for batch
    pub model: Option<SizedModel>,
    /// Batch status
    pub status: BatchStatus,
    /// Created timestamp
    pub created_at: u64,
}

/// A task within a batch
#[derive(Debug, Clone)]
pub struct BatchTask {
    /// Task ID
    pub id: String,
    /// Task content
    pub content: String,
    /// Priority (higher = more important)
    pub priority: u32,
    /// Result (if completed)
    pub result: Option<String>,
    /// Status
    pub status: BatchTaskStatus,
}

/// Status of a batch task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchTaskStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

/// Status of a batch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Collecting,
    Ready,
    Processing,
    Completed,
    Failed,
}

impl TaskBatch {
    pub fn new(task_type: TaskType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tasks: Vec::new(),
            task_type,
            model: None,
            status: BatchStatus::Collecting,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn add_task(&mut self, content: String, priority: u32) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.tasks.push(BatchTask {
            id: id.clone(),
            content,
            priority,
            result: None,
            status: BatchTaskStatus::Pending,
        });
        id
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn pending_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status == BatchTaskStatus::Pending)
            .count()
    }

    pub fn completed_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status == BatchTaskStatus::Completed)
            .count()
    }

    /// Get tasks sorted by priority
    pub fn prioritized_tasks(&self) -> Vec<&BatchTask> {
        let mut tasks: Vec<_> = self.tasks.iter().collect();
        tasks.sort_by(|a, b| b.priority.cmp(&a.priority));
        tasks
    }

    /// Mark batch as ready for processing
    pub fn finalize(&mut self) {
        self.status = BatchStatus::Ready;
    }

    /// Set model for batch
    pub fn set_model(&mut self, model: SizedModel) {
        self.model = Some(model);
    }

    /// Estimate total tokens for batch
    pub fn estimate_tokens(&self) -> usize {
        self.tasks
            .iter()
            .map(|t| t.content.split_whitespace().count() * 2)
            .sum()
    }
}

/// Manager for batch processing
pub struct BatchProcessor {
    /// Active batches by task type
    batches: HashMap<TaskType, TaskBatch>,
    /// Completed batches (for history)
    completed: Vec<TaskBatch>,
    /// Minimum batch size before processing
    min_batch_size: usize,
    /// Maximum wait time before processing (ms)
    max_wait_ms: u64,
    /// Stats
    stats: BatchStats,
}

/// Statistics for batch processing
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    /// Total batches processed
    pub batches_processed: usize,
    /// Total tasks processed
    pub tasks_processed: usize,
    /// Average batch size
    pub avg_batch_size: f32,
    /// Tokens saved by batching
    pub tokens_saved: usize,
}

impl BatchProcessor {
    pub fn new() -> Self {
        Self {
            batches: HashMap::new(),
            completed: Vec::new(),
            min_batch_size: 3,
            max_wait_ms: 5000,
            stats: BatchStats::default(),
        }
    }

    pub fn with_min_batch_size(mut self, size: usize) -> Self {
        self.min_batch_size = size;
        self
    }

    pub fn with_max_wait_ms(mut self, ms: u64) -> Self {
        self.max_wait_ms = ms;
        self
    }

    /// Add a task to appropriate batch
    pub fn add_task(&mut self, task: &str, priority: u32) -> (String, String) {
        let task_type = TaskType::detect(task);
        let batch = self
            .batches
            .entry(task_type.clone())
            .or_insert_with(|| TaskBatch::new(task_type));
        let task_id = batch.add_task(task.to_string(), priority);
        (batch.id.clone(), task_id)
    }

    /// Get batch by ID
    pub fn get_batch(&self, batch_id: &str) -> Option<&TaskBatch> {
        self.batches.values().find(|b| b.id == batch_id)
    }

    /// Get ready batches
    pub fn ready_batches(&self) -> Vec<&TaskBatch> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.batches
            .values()
            .filter(|b| {
                b.task_count() >= self.min_batch_size
                    || (now - b.created_at) * 1000 >= self.max_wait_ms
            })
            .collect()
    }

    /// Finalize a batch for processing
    pub fn finalize_batch(&mut self, task_type: &TaskType) -> Option<TaskBatch> {
        if let Some(mut batch) = self.batches.remove(task_type) {
            batch.finalize();
            Some(batch)
        } else {
            None
        }
    }

    /// Record batch completion
    pub fn complete_batch(&mut self, batch: TaskBatch) {
        let size = batch.task_count();
        self.stats.batches_processed += 1;
        self.stats.tasks_processed += size;
        self.stats.avg_batch_size =
            self.stats.tasks_processed as f32 / self.stats.batches_processed as f32;
        // Estimate ~20% token savings from batching (shared context)
        self.stats.tokens_saved += batch.estimate_tokens() / 5;
        self.completed.push(batch);

        // Keep only last 100 completed batches
        if self.completed.len() > 100 {
            self.completed.remove(0);
        }
    }

    /// Get statistics
    pub fn stats(&self) -> &BatchStats {
        &self.stats
    }

    /// Get active batch count
    pub fn active_batch_count(&self) -> usize {
        self.batches.len()
    }

    /// Get total pending tasks
    pub fn pending_task_count(&self) -> usize {
        self.batches.values().map(|b| b.pending_count()).sum()
    }
}

impl Default for BatchProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Efficient model selector combining sizing, local/cloud, and batching
pub struct EfficientModelSelector {
    /// Sized models
    models: Vec<SizedModel>,
    /// Local/cloud decider
    decider: LocalCloudDecider,
    /// Batch processor
    batch_processor: BatchProcessor,
    /// Selection history for learning
    selection_history: Vec<SelectionRecord>,
    /// Enable cost tracking
    _track_costs: bool,
    /// Total cost tracked
    total_cost: f64,
}

/// Record of a model selection
#[derive(Debug, Clone)]
pub struct SelectionRecord {
    /// Task description
    pub task: String,
    /// Selected model
    pub model_id: String,
    /// Was local?
    pub was_local: bool,
    /// Complexity
    pub complexity: TaskComplexity,
    /// Actual latency (if measured)
    pub actual_latency_ms: Option<u64>,
    /// Actual cost
    pub actual_cost: Option<f64>,
    /// Timestamp
    pub timestamp: u64,
}

impl EfficientModelSelector {
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            decider: LocalCloudDecider::new(),
            batch_processor: BatchProcessor::new(),
            selection_history: Vec::new(),
            _track_costs: true,
            total_cost: 0.0,
        }
    }

    /// Add a model
    pub fn add_model(&mut self, model: SizedModel) {
        if model.is_local {
            self.decider.add_local_model(model.clone());
        } else {
            self.decider.add_cloud_model(model.clone());
        }
        self.models.push(model);
    }

    /// Select best model for task
    pub fn select(
        &mut self,
        task: &str,
        criteria: Option<SelectionCriteria>,
    ) -> Option<SizedModel> {
        let criteria = criteria.unwrap_or_default();
        let complexity = TaskComplexity::estimate(task);
        let min_tier = criteria
            .min_tier
            .unwrap_or_else(|| complexity.min_model_tier());

        // Get local/cloud decision
        let decision = self.decider.decide(task, &criteria);

        // Filter models by criteria
        let candidates: Vec<_> = self
            .models
            .iter()
            .filter(|m| m.meets_tier(min_tier))
            .filter(|m| {
                if criteria.require_local || decision.location == ExecutionLocation::Local {
                    m.is_local
                } else if decision.location == ExecutionLocation::Cloud {
                    !m.is_local
                } else {
                    true
                }
            })
            .filter(|m| {
                criteria
                    .required_capabilities
                    .iter()
                    .all(|c| m.config.has_capability(c))
            })
            .filter(|m| {
                if let Some(max_cost) = criteria.max_cost {
                    m.config.cost_input.unwrap_or(0.0) <= max_cost
                } else {
                    true
                }
            })
            .filter(|m| {
                if !criteria.allow_degraded && m.health_status == HealthStatus::Degraded {
                    false
                } else {
                    m.health_status != HealthStatus::Unhealthy
                }
            })
            .cloned()
            .collect();

        // Sort by priority and return best
        let mut candidates = candidates;
        candidates.sort_by(|a, b| {
            // Prefer local if criteria prefers it
            if criteria.prefer_local && a.is_local != b.is_local {
                return if a.is_local {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            // Then by tier (prefer smaller that meets requirements)
            if a.tier != b.tier {
                return a.tier.cmp(&b.tier);
            }
            // Then by priority
            b.config.priority.cmp(&a.config.priority)
        });

        let selected = candidates.first().cloned();

        // Record selection
        if let Some(ref model) = selected {
            self.selection_history.push(SelectionRecord {
                task: task.to_string(),
                model_id: model.config.id.clone(),
                was_local: model.is_local,
                complexity,
                actual_latency_ms: None,
                actual_cost: None,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });

            // Trim history
            if self.selection_history.len() > 1000 {
                self.selection_history.drain(0..500);
            }
        }

        selected
    }

    /// Record actual performance for learning
    pub fn record_performance(&mut self, task: &str, latency_ms: u64, cost: f64) {
        // Find matching recent selection
        if let Some(record) = self
            .selection_history
            .iter_mut()
            .rev()
            .find(|r| r.task == task && r.actual_latency_ms.is_none())
        {
            record.actual_latency_ms = Some(latency_ms);
            record.actual_cost = Some(cost);
            self.total_cost += cost;
        }
    }

    /// Get selection statistics
    pub fn get_stats(&self) -> SelectionStats {
        let total = self.selection_history.len();
        let local_count = self
            .selection_history
            .iter()
            .filter(|r| r.was_local)
            .count();
        let cloud_count = total - local_count;

        let avg_latency: f64 = self
            .selection_history
            .iter()
            .filter_map(|r| r.actual_latency_ms.map(|l| l as f64))
            .sum::<f64>()
            / self
                .selection_history
                .iter()
                .filter(|r| r.actual_latency_ms.is_some())
                .count()
                .max(1) as f64;

        let mut by_complexity: HashMap<TaskComplexity, usize> = HashMap::new();
        for record in &self.selection_history {
            *by_complexity.entry(record.complexity).or_insert(0) += 1;
        }

        SelectionStats {
            total_selections: total,
            local_selections: local_count,
            cloud_selections: cloud_count,
            avg_latency_ms: avg_latency,
            total_cost: self.total_cost,
            by_complexity,
        }
    }

    /// Get batch processor
    pub fn batch_processor(&mut self) -> &mut BatchProcessor {
        &mut self.batch_processor
    }

    /// Get models
    pub fn models(&self) -> &[SizedModel] {
        &self.models
    }
}

impl Default for EfficientModelSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for model selection
#[derive(Debug, Clone)]
pub struct SelectionStats {
    pub total_selections: usize,
    pub local_selections: usize,
    pub cloud_selections: usize,
    pub avg_latency_ms: f64,
    pub total_cost: f64,
    pub by_complexity: HashMap<TaskComplexity, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_provider_name() {
        assert_eq!(ModelProvider::OpenAI.name(), "openai");
        assert_eq!(ModelProvider::Anthropic.name(), "anthropic");
        assert_eq!(ModelProvider::Ollama.name(), "ollama");
        assert_eq!(ModelProvider::Local.name(), "local");
        assert_eq!(ModelProvider::Custom("test".to_string()).name(), "test");
    }

    #[test]
    fn test_model_provider_parse() {
        assert_eq!(ModelProvider::parse("openai"), ModelProvider::OpenAI);
        assert_eq!(ModelProvider::parse("ANTHROPIC"), ModelProvider::Anthropic);
        assert_eq!(
            ModelProvider::parse("unknown"),
            ModelProvider::Custom("unknown".to_string())
        );
    }

    #[test]
    fn test_model_provider_display() {
        assert_eq!(format!("{}", ModelProvider::OpenAI), "openai");
    }

    #[test]
    fn test_capability_icon_name() {
        assert_eq!(Capability::CodeGeneration.icon(), "💻");
        assert_eq!(Capability::CodeGeneration.name(), "Code Generation");
        assert_eq!(Capability::Debugging.icon(), "🐛");
    }

    #[test]
    fn test_model_config_creation() {
        let model = ModelConfig::new("gpt-4", "GPT-4", ModelProvider::OpenAI);
        assert_eq!(model.id, "gpt-4");
        assert_eq!(model.name, "GPT-4");
        assert!(model.available);
    }

    #[test]
    fn test_model_config_builder() {
        let model = ModelConfig::new("claude", "Claude", ModelProvider::Anthropic)
            .with_endpoint("https://api.anthropic.com")
            .with_api_key_env("ANTHROPIC_API_KEY")
            .with_context(100000)
            .with_capability(Capability::CodeGeneration)
            .with_capability(Capability::Reasoning)
            .with_cost(0.01, 0.03)
            .with_temperature(0.5)
            .with_priority(10);

        assert_eq!(model.max_context, 100000);
        assert!(model.has_capability(&Capability::CodeGeneration));
        assert!(model.has_capability(&Capability::Reasoning));
        assert_eq!(model.priority, 10);
    }

    #[test]
    fn test_model_config_capabilities() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local)
            .with_capabilities(vec![Capability::CodeGeneration, Capability::Debugging]);

        assert!(model.has_capability(&Capability::CodeGeneration));
        assert!(!model.has_capability(&Capability::Vision));
        assert!(model.has_all_capabilities(&[Capability::CodeGeneration, Capability::Debugging]));
        assert!(!model.has_all_capabilities(&[Capability::CodeGeneration, Capability::Vision]));
        assert!(model.has_any_capability(&[Capability::Vision, Capability::Debugging]));
    }

    #[test]
    fn test_model_config_estimate_cost() {
        let model = ModelConfig::new("test", "Test", ModelProvider::OpenAI).with_cost(0.01, 0.03);

        let cost = model.estimate_cost(1000, 1000);
        assert_eq!(cost, Some(0.04));

        let model_no_cost = ModelConfig::new("test", "Test", ModelProvider::Local);
        assert!(model_no_cost.estimate_cost(1000, 1000).is_none());
    }

    #[test]
    fn test_model_config_display() {
        let model = ModelConfig::new("gpt-4", "GPT-4", ModelProvider::OpenAI).with_context(128000);
        let display = model.display();
        assert!(display.contains("GPT-4"));
        assert!(display.contains("128K"));
    }

    #[test]
    fn test_task_type_detect_code_write() {
        assert_eq!(
            TaskType::detect("write a function to sort"),
            TaskType::CodeWrite
        );
        assert_eq!(TaskType::detect("create a new class"), TaskType::CodeWrite);
        assert_eq!(
            TaskType::detect("implement the feature"),
            TaskType::CodeWrite
        );
    }

    #[test]
    fn test_task_type_detect_review() {
        assert_eq!(TaskType::detect("review this code"), TaskType::CodeReview);
        assert_eq!(TaskType::detect("check my code"), TaskType::CodeReview);
    }

    #[test]
    fn test_task_type_detect_debug() {
        assert_eq!(TaskType::detect("debug this function"), TaskType::Debug);
        assert_eq!(TaskType::detect("fix the bug"), TaskType::Debug);
    }

    #[test]
    fn test_task_type_detect_test() {
        assert_eq!(TaskType::detect("write tests for"), TaskType::Test);
        assert_eq!(TaskType::detect("add unit test"), TaskType::Test);
    }

    #[test]
    fn test_task_type_detect_question() {
        assert_eq!(TaskType::detect("what is this?"), TaskType::Question);
        assert_eq!(TaskType::detect("why does this work?"), TaskType::Question);
    }

    #[test]
    fn test_task_type_detect_explain() {
        assert_eq!(TaskType::detect("explain this code"), TaskType::Explain);
        assert_eq!(
            TaskType::detect("what does this function do"),
            TaskType::Explain
        );
    }

    #[test]
    fn test_task_type_required_capabilities() {
        let caps = TaskType::CodeWrite.required_capabilities();
        assert!(caps.contains(&Capability::CodeGeneration));

        let caps = TaskType::Debug.required_capabilities();
        assert!(caps.contains(&Capability::Debugging));
    }

    #[test]
    fn test_task_type_icon() {
        assert_eq!(TaskType::CodeWrite.icon(), "✍️");
        assert_eq!(TaskType::Debug.icon(), "🐛");
    }

    #[test]
    fn test_task_type_display() {
        assert_eq!(format!("{}", TaskType::CodeWrite), "Code Write");
        assert_eq!(format!("{}", TaskType::Debug), "Debug");
    }

    #[test]
    fn test_route_decision_creation() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let decision = RouteDecision::new(model, TaskType::CodeWrite, "test".to_string());
        assert_eq!(decision.confidence, 1.0);
        assert!(decision.alternatives.is_empty());
    }

    #[test]
    fn test_route_decision_builder() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let alt = ModelConfig::new("alt", "Alt", ModelProvider::Local);
        let decision = RouteDecision::new(model, TaskType::CodeWrite, "test".to_string())
            .with_confidence(0.8)
            .with_alternatives(vec![alt]);

        assert_eq!(decision.confidence, 0.8);
        assert_eq!(decision.alternatives.len(), 1);
    }

    #[test]
    fn test_model_router_new() {
        let router = ModelRouter::new();
        assert!(router.models().is_empty());
    }

    #[test]
    fn test_model_router_add_model() {
        let mut router = ModelRouter::new();
        router.add_model(ModelConfig::new("test", "Test", ModelProvider::Local));
        assert_eq!(router.models().len(), 1);
    }

    #[test]
    fn test_model_router_get_model() {
        let mut router = ModelRouter::new();
        router.add_model(ModelConfig::new("test", "Test", ModelProvider::Local));
        assert!(router.get_model("test").is_some());
        assert!(router.get_model("nonexistent").is_none());
    }

    #[test]
    fn test_model_router_set_default() {
        let mut router = ModelRouter::new();
        router.add_model(ModelConfig::new("test", "Test", ModelProvider::Local));
        router.set_default("test");
        assert_eq!(router.default_model, Some("test".to_string()));
    }

    #[test]
    fn test_model_router_route() {
        let mut router = ModelRouter::new();
        router.add_model(
            ModelConfig::new("coder", "Coder", ModelProvider::Local)
                .with_capability(Capability::CodeGeneration)
                .with_priority(10),
        );

        let decision = router.route("write a function");
        assert!(decision.is_some());
        let decision = decision.unwrap();
        assert_eq!(decision.model.id, "coder");
    }

    #[test]
    fn test_model_router_route_with_override() {
        let mut router = ModelRouter::new();
        router.add_model(ModelConfig::new("default", "Default", ModelProvider::Local));
        router.add_model(ModelConfig::new(
            "reviewer",
            "Reviewer",
            ModelProvider::Local,
        ));
        router.set_override(TaskType::CodeReview, "reviewer");

        let decision = router.route("review this code");
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().model.id, "reviewer");
    }

    #[test]
    fn test_model_router_stats() {
        let mut router = ModelRouter::new();
        router.add_model(
            ModelConfig::new("test", "Test", ModelProvider::Local)
                .with_capability(Capability::CodeGeneration),
        );
        router.route("write code");
        router.route("create function");

        let stats = router.stats();
        assert_eq!(stats.total, 2);
    }

    #[test]
    fn test_model_router_reset_stats() {
        let mut router = ModelRouter::new();
        router.add_model(ModelConfig::new("test", "Test", ModelProvider::Local));
        router.set_default("test");
        router.route("anything");
        router.reset_stats();
        assert_eq!(router.stats().total, 0);
    }

    #[test]
    fn test_agent_role_icon() {
        assert_eq!(AgentRole::Lead.icon(), "👑");
        assert_eq!(AgentRole::Worker.icon(), "👷");
        assert_eq!(AgentRole::Reviewer.icon(), "🔍");
        assert_eq!(AgentRole::Specialist("test".to_string()).icon(), "🎯");
    }

    #[test]
    fn test_agent_role_display() {
        assert_eq!(format!("{}", AgentRole::Lead), "Lead");
        assert_eq!(
            format!("{}", AgentRole::Specialist("code".to_string())),
            "Specialist(code)"
        );
    }

    #[test]
    fn test_agent_status_icon() {
        assert_eq!(AgentStatus::Idle.icon(), "💤");
        assert_eq!(AgentStatus::Working.icon(), "⚙️");
        assert_eq!(AgentStatus::Done.icon(), "✅");
        assert_eq!(AgentStatus::Failed.icon(), "❌");
    }

    #[test]
    fn test_swarm_agent_creation() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let agent = SwarmAgent::new("agent-1", AgentRole::Worker, model);
        assert_eq!(agent.id, "agent-1");
        assert_eq!(agent.status, AgentStatus::Idle);
    }

    #[test]
    fn test_swarm_agent_lifecycle() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let mut agent = SwarmAgent::new("agent-1", AgentRole::Worker, model);

        agent.assign("write code".to_string());
        assert_eq!(agent.status, AgentStatus::Working);
        assert!(agent.task.is_some());

        agent.complete("done".to_string());
        assert_eq!(agent.status, AgentStatus::Done);
        assert!(agent.result.is_some());
    }

    #[test]
    fn test_swarm_agent_fail() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let mut agent = SwarmAgent::new("agent-1", AgentRole::Worker, model);
        agent.assign("task".to_string());
        agent.fail();
        assert_eq!(agent.status, AgentStatus::Failed);
    }

    #[test]
    fn test_agent_swarm_new() {
        let swarm = AgentSwarm::new();
        assert!(swarm.is_empty());
        assert_eq!(swarm.status(), SwarmStatus::Idle);
    }

    #[test]
    fn test_agent_swarm_add_agent() {
        let mut swarm = AgentSwarm::new();
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        swarm.add_agent(SwarmAgent::new("agent-1", AgentRole::Worker, model));
        assert_eq!(swarm.len(), 1);
    }

    #[test]
    fn test_agent_swarm_get_agent() {
        let mut swarm = AgentSwarm::new();
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        swarm.add_agent(SwarmAgent::new("agent-1", AgentRole::Worker, model));

        assert!(swarm.get_agent("agent-1").is_some());
        assert!(swarm.get_agent("nonexistent").is_none());
    }

    #[test]
    fn test_agent_swarm_set_task() {
        let mut swarm = AgentSwarm::new();
        swarm.set_task("do work".to_string());
        assert_eq!(swarm.status(), SwarmStatus::Planning);
    }

    #[test]
    fn test_agent_swarm_start() {
        let mut swarm = AgentSwarm::new();
        swarm.start();
        assert_eq!(swarm.status(), SwarmStatus::Executing);
    }

    #[test]
    fn test_agent_swarm_is_complete() {
        let mut swarm = AgentSwarm::new();
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let mut agent = SwarmAgent::new("agent-1", AgentRole::Worker, model);
        agent.complete("done".to_string());
        swarm.add_agent(agent);

        assert!(swarm.is_complete());
    }

    #[test]
    fn test_agent_swarm_status_counts() {
        let mut swarm = AgentSwarm::new();
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);

        let mut agent1 = SwarmAgent::new("a1", AgentRole::Worker, model.clone());
        agent1.complete("done".to_string());
        swarm.add_agent(agent1);

        let agent2 = SwarmAgent::new("a2", AgentRole::Worker, model);
        swarm.add_agent(agent2);

        let counts = swarm.status_counts();
        assert_eq!(counts.get(&AgentStatus::Done), Some(&1));
        assert_eq!(counts.get(&AgentStatus::Idle), Some(&1));
    }

    #[test]
    fn test_agent_swarm_complete() {
        let mut swarm = AgentSwarm::new();
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);

        let mut agent = SwarmAgent::new("a1", AgentRole::Worker, model);
        agent.complete("done".to_string());
        swarm.add_agent(agent);

        swarm.complete();
        assert_eq!(swarm.status(), SwarmStatus::Complete);
    }

    #[test]
    fn test_agent_swarm_complete_with_failure() {
        let mut swarm = AgentSwarm::new();
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);

        let mut agent = SwarmAgent::new("a1", AgentRole::Worker, model);
        agent.fail();
        swarm.add_agent(agent);

        swarm.complete();
        assert_eq!(swarm.status(), SwarmStatus::Failed);
    }

    #[test]
    fn test_driver_switch() {
        assert_eq!(Driver::Human.switch(), Driver::AI);
        assert_eq!(Driver::AI.switch(), Driver::Human);
    }

    #[test]
    fn test_driver_icon() {
        assert_eq!(Driver::Human.icon(), "👤");
        assert_eq!(Driver::AI.icon(), "🤖");
    }

    #[test]
    fn test_pair_session_creation() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let session = PairSession::new("alice", model);
        assert_eq!(session.human, "alice");
        assert_eq!(session.driver, Driver::Human);
        assert!(session.active);
    }

    #[test]
    fn test_pair_session_switch_driver() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let mut session = PairSession::new("alice", model);
        session.switch_driver();
        assert_eq!(session.driver, Driver::AI);
        assert_eq!(session.turns, 1);
    }

    #[test]
    fn test_pair_session_end() {
        let model = ModelConfig::new("test", "Test", ModelProvider::Local);
        let mut session = PairSession::new("alice", model);
        session.end();
        assert!(!session.active);
    }

    #[test]
    fn test_pair_session_display() {
        let model = ModelConfig::new("claude", "Claude", ModelProvider::Anthropic);
        let session = PairSession::new("alice", model);
        let display = session.display();
        assert!(display.contains("alice"));
        assert!(display.contains("Claude"));
    }

    #[test]
    fn test_swarm_status_icon() {
        assert_eq!(SwarmStatus::Idle.icon(), "💤");
        assert_eq!(SwarmStatus::Executing.icon(), "⚙️");
        assert_eq!(SwarmStatus::Complete.icon(), "✅");
    }

    // ========================================================================
    // Efficient Model Selection Tests
    // ========================================================================

    #[test]
    fn test_task_complexity_estimate_trivial() {
        assert_eq!(
            TaskComplexity::estimate("fix typo"),
            TaskComplexity::Trivial
        );
        assert_eq!(
            TaskComplexity::estimate("add comma"),
            TaskComplexity::Trivial
        );
    }

    #[test]
    fn test_task_complexity_estimate_simple() {
        // Short simple tasks are Trivial, need more words to be Simple
        assert_eq!(
            TaskComplexity::estimate("write a function that takes two integers and returns their sum with proper error handling"),
            TaskComplexity::Simple
        );
    }

    #[test]
    fn test_task_complexity_estimate_complex() {
        // Need to use complexity words like "refactor", "complex", etc.
        let task = "refactor the entire authentication system to use JWT tokens, update all endpoints, ensure backwards compatibility with existing sessions, optimize the database queries, and implement proper error handling throughout the complex codebase";
        let complexity = TaskComplexity::estimate(task);
        // Should be at least Moderate due to complexity words
        assert!(complexity >= TaskComplexity::Moderate);
    }

    #[test]
    fn test_task_complexity_min_tier() {
        assert_eq!(TaskComplexity::Trivial.min_model_tier(), ModelTier::Nano);
        assert_eq!(TaskComplexity::Simple.min_model_tier(), ModelTier::Small);
        assert_eq!(TaskComplexity::Expert.min_model_tier(), ModelTier::XLarge);
    }

    #[test]
    fn test_model_tier_ordering() {
        assert!(ModelTier::Nano < ModelTier::Small);
        assert!(ModelTier::Small < ModelTier::Medium);
        assert!(ModelTier::Medium < ModelTier::Large);
        assert!(ModelTier::Large < ModelTier::XLarge);
    }

    #[test]
    fn test_model_tier_latency() {
        assert!(ModelTier::Nano.typical_latency_ms() < ModelTier::XLarge.typical_latency_ms());
    }

    #[test]
    fn test_model_tier_cost() {
        assert!(ModelTier::Nano.cost_multiplier() < ModelTier::XLarge.cost_multiplier());
    }

    #[test]
    fn test_sized_model_creation() {
        let config = ModelConfig::new("test", "Test", ModelProvider::Local);
        let sized = SizedModel::new(config, ModelTier::Medium, true);
        assert!(sized.is_local);
        assert_eq!(sized.tier, ModelTier::Medium);
    }

    #[test]
    fn test_sized_model_estimate_time() {
        let config = ModelConfig::new("test", "Test", ModelProvider::Local);
        let sized = SizedModel::new(config, ModelTier::Medium, true);
        let time = sized.estimate_time_ms(100);
        assert!(time > 0);
    }

    #[test]
    fn test_sized_model_meets_tier() {
        let config = ModelConfig::new("test", "Test", ModelProvider::Local);
        let sized = SizedModel::new(config, ModelTier::Medium, true);
        assert!(sized.meets_tier(ModelTier::Small));
        assert!(sized.meets_tier(ModelTier::Medium));
        assert!(!sized.meets_tier(ModelTier::Large));
    }

    #[test]
    fn test_selection_criteria_builder() {
        let criteria = SelectionCriteria::new()
            .with_capabilities(vec![Capability::CodeGeneration])
            .with_min_tier(ModelTier::Medium)
            .with_max_cost(0.01)
            .with_max_latency(1000)
            .local_only();

        assert!(criteria.require_local);
        assert_eq!(criteria.min_tier, Some(ModelTier::Medium));
        assert_eq!(criteria.max_cost, Some(0.01));
    }

    #[test]
    fn test_local_cloud_decider_new() {
        let decider = LocalCloudDecider::new();
        assert_eq!(decider.cost_sensitivity, 0.5);
        assert_eq!(decider.latency_sensitivity, 0.5);
    }

    #[test]
    fn test_local_cloud_decider_with_privacy() {
        let decider = LocalCloudDecider::new().with_privacy_required();
        assert!(decider.require_privacy);
    }

    #[test]
    fn test_local_cloud_decider_decide_local() {
        let mut decider = LocalCloudDecider::new();
        let config = ModelConfig::new("local", "Local", ModelProvider::Local)
            .with_capability(Capability::CodeGeneration);
        decider.add_local_model(SizedModel::new(config, ModelTier::Medium, true));

        let criteria = SelectionCriteria::new().local_only();
        let decision = decider.decide("write code", &criteria);

        assert_eq!(decision.location, ExecutionLocation::Local);
    }

    #[test]
    fn test_local_cloud_decider_decide_cloud() {
        let mut decider = LocalCloudDecider::new();
        let config = ModelConfig::new("cloud", "Cloud", ModelProvider::OpenAI)
            .with_capability(Capability::CodeGeneration);
        let mut sized = SizedModel::new(config, ModelTier::Large, false);
        sized.health_status = HealthStatus::Healthy;
        decider.add_cloud_model(sized);

        let decision = decider.decide("complex refactoring task", &SelectionCriteria::new());

        // Should prefer cloud for complex tasks when no local available
        assert_eq!(decision.location, ExecutionLocation::Cloud);
    }

    #[test]
    fn test_task_batch_new() {
        let batch = TaskBatch::new(TaskType::CodeWrite);
        assert_eq!(batch.task_count(), 0);
        assert_eq!(batch.status, BatchStatus::Collecting);
    }

    #[test]
    fn test_task_batch_add_task() {
        let mut batch = TaskBatch::new(TaskType::CodeWrite);
        let task_id = batch.add_task("write function".to_string(), 5);
        assert!(!task_id.is_empty());
        assert_eq!(batch.task_count(), 1);
    }

    #[test]
    fn test_task_batch_prioritized_tasks() {
        let mut batch = TaskBatch::new(TaskType::CodeWrite);
        batch.add_task("low priority".to_string(), 1);
        batch.add_task("high priority".to_string(), 10);
        batch.add_task("medium priority".to_string(), 5);

        let prioritized = batch.prioritized_tasks();
        assert_eq!(prioritized[0].priority, 10);
        assert_eq!(prioritized[2].priority, 1);
    }

    #[test]
    fn test_task_batch_finalize() {
        let mut batch = TaskBatch::new(TaskType::CodeWrite);
        batch.finalize();
        assert_eq!(batch.status, BatchStatus::Ready);
    }

    #[test]
    fn test_batch_processor_new() {
        let processor = BatchProcessor::new();
        assert_eq!(processor.active_batch_count(), 0);
    }

    #[test]
    fn test_batch_processor_add_task() {
        let mut processor = BatchProcessor::new();
        let (batch_id, task_id) = processor.add_task("write code", 5);
        assert!(!batch_id.is_empty());
        assert!(!task_id.is_empty());
        assert_eq!(processor.pending_task_count(), 1);
    }

    #[test]
    fn test_batch_processor_ready_batches() {
        let mut processor = BatchProcessor::new().with_min_batch_size(2);
        processor.add_task("task 1", 1);
        processor.add_task("task 2", 2);
        processor.add_task("task 3", 3);

        let ready = processor.ready_batches();
        assert!(!ready.is_empty());
    }

    #[test]
    fn test_batch_processor_complete_batch() {
        let mut processor = BatchProcessor::new();
        let mut batch = TaskBatch::new(TaskType::CodeWrite);
        batch.add_task("task".to_string(), 1);
        batch.finalize();

        processor.complete_batch(batch);
        assert_eq!(processor.stats().batches_processed, 1);
    }

    #[test]
    fn test_efficient_model_selector_new() {
        let selector = EfficientModelSelector::new();
        assert!(selector.models().is_empty());
    }

    #[test]
    fn test_efficient_model_selector_add_model() {
        let mut selector = EfficientModelSelector::new();
        let config = ModelConfig::new("test", "Test", ModelProvider::Local);
        selector.add_model(SizedModel::new(config, ModelTier::Medium, true));
        assert_eq!(selector.models().len(), 1);
    }

    #[test]
    fn test_efficient_model_selector_select() {
        let mut selector = EfficientModelSelector::new();
        let config = ModelConfig::new("coder", "Coder", ModelProvider::Local)
            .with_capability(Capability::CodeGeneration);
        let mut sized = SizedModel::new(config, ModelTier::Medium, true);
        sized.health_status = HealthStatus::Healthy;
        selector.add_model(sized);

        let selected = selector.select("write a function", None);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().config.id, "coder");
    }

    #[test]
    fn test_efficient_model_selector_select_with_criteria() {
        let mut selector = EfficientModelSelector::new();

        let local = ModelConfig::new("local", "Local", ModelProvider::Local)
            .with_capability(Capability::CodeGeneration);
        let mut local_sized = SizedModel::new(local, ModelTier::Small, true);
        local_sized.health_status = HealthStatus::Healthy;
        selector.add_model(local_sized);

        let cloud = ModelConfig::new("cloud", "Cloud", ModelProvider::OpenAI)
            .with_capability(Capability::CodeGeneration);
        let mut cloud_sized = SizedModel::new(cloud, ModelTier::Large, false);
        cloud_sized.health_status = HealthStatus::Healthy;
        selector.add_model(cloud_sized);

        let criteria = SelectionCriteria::new().local_only();
        let selected = selector.select("write code", Some(criteria));

        assert!(selected.is_some());
        assert!(selected.unwrap().is_local);
    }

    #[test]
    fn test_efficient_model_selector_record_performance() {
        let mut selector = EfficientModelSelector::new();
        let config = ModelConfig::new("test", "Test", ModelProvider::Local);
        let mut sized = SizedModel::new(config, ModelTier::Medium, true);
        sized.health_status = HealthStatus::Healthy;
        selector.add_model(sized);

        selector.select("test task", None);
        selector.record_performance("test task", 500, 0.01);

        let stats = selector.get_stats();
        assert_eq!(stats.total_selections, 1);
        assert!(stats.total_cost > 0.0);
    }

    #[test]
    fn test_efficient_model_selector_stats() {
        let mut selector = EfficientModelSelector::new();
        let config = ModelConfig::new("local", "Local", ModelProvider::Local);
        let mut sized = SizedModel::new(config, ModelTier::Medium, true);
        sized.health_status = HealthStatus::Healthy;
        selector.add_model(sized);

        selector.select("task 1", None);
        selector.select("task 2", None);

        let stats = selector.get_stats();
        assert_eq!(stats.total_selections, 2);
        assert_eq!(stats.local_selections, 2);
        assert_eq!(stats.cloud_selections, 0);
    }

    #[test]
    fn test_health_status() {
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
        assert_ne!(HealthStatus::Degraded, HealthStatus::Unknown);
    }

    #[test]
    fn test_execution_location() {
        assert_ne!(ExecutionLocation::Local, ExecutionLocation::Cloud);
        assert_ne!(ExecutionLocation::Cloud, ExecutionLocation::Hybrid);
    }

    #[test]
    fn test_batch_task_status() {
        assert_ne!(BatchTaskStatus::Pending, BatchTaskStatus::Completed);
        assert_ne!(BatchTaskStatus::Processing, BatchTaskStatus::Failed);
    }

    #[test]
    fn test_batch_status() {
        assert_ne!(BatchStatus::Collecting, BatchStatus::Ready);
        assert_ne!(BatchStatus::Processing, BatchStatus::Completed);
    }
}
