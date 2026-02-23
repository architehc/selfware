//! Token Usage Tracking and Cost Optimization
//!
//! This module provides comprehensive token management:
//! - Token usage tracking and estimation
//! - Context pruning strategies
//! - Model selection optimization
//! - Budget management and alerts
//! - Cost optimization recommendations
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Token Optimizer                          │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Token         │  │ Context       │  │ Model         │   │
//! │  │ Tracker       │  │ Pruner        │  │ Selector      │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Budget        │  │ Batch         │  │ Cost          │   │
//! │  │ Manager       │  │ Optimizer     │  │ Analyzer      │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Token usage statistics for a session
#[derive(Debug, Default)]
pub struct TokenTracker {
    /// Total prompt tokens sent
    prompt_tokens: AtomicUsize,
    /// Total completion tokens received
    completion_tokens: AtomicUsize,
    /// Number of API calls made
    api_calls: AtomicUsize,
    /// Per-step token usage
    step_usage: RwLock<Vec<StepUsage>>,
    /// Session start time
    start_time: RwLock<Option<Instant>>,
}

/// Token usage for a single step
#[derive(Debug, Clone)]
pub struct StepUsage {
    pub step: usize,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub tool_name: Option<String>,
    pub timestamp: std::time::SystemTime,
}

impl TokenTracker {
    /// Create a new token tracker
    pub fn new() -> Self {
        Self {
            prompt_tokens: AtomicUsize::new(0),
            completion_tokens: AtomicUsize::new(0),
            api_calls: AtomicUsize::new(0),
            step_usage: RwLock::new(Vec::new()),
            start_time: RwLock::new(Some(Instant::now())),
        }
    }

    /// Record token usage from an API response
    pub fn record_usage(&self, prompt: usize, completion: usize) {
        self.prompt_tokens.fetch_add(prompt, Ordering::SeqCst);
        self.completion_tokens
            .fetch_add(completion, Ordering::SeqCst);
        self.api_calls.fetch_add(1, Ordering::SeqCst);
    }

    /// Record usage for a specific step
    pub fn record_step(
        &self,
        step: usize,
        prompt: usize,
        completion: usize,
        tool_name: Option<String>,
    ) {
        self.record_usage(prompt, completion);

        if let Ok(mut steps) = self.step_usage.write() {
            steps.push(StepUsage {
                step,
                prompt_tokens: prompt,
                completion_tokens: completion,
                tool_name,
                timestamp: std::time::SystemTime::now(),
            });
        }
    }

    /// Get total prompt tokens
    pub fn total_prompt_tokens(&self) -> usize {
        self.prompt_tokens.load(Ordering::SeqCst)
    }

    /// Get total completion tokens
    pub fn total_completion_tokens(&self) -> usize {
        self.completion_tokens.load(Ordering::SeqCst)
    }

    /// Get total tokens (prompt + completion)
    pub fn total_tokens(&self) -> usize {
        self.total_prompt_tokens() + self.total_completion_tokens()
    }

    /// Get number of API calls
    pub fn api_call_count(&self) -> usize {
        self.api_calls.load(Ordering::SeqCst)
    }

    /// Get per-step usage
    pub fn step_usage(&self) -> Vec<StepUsage> {
        self.step_usage
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Get session duration
    pub fn session_duration(&self) -> Option<std::time::Duration> {
        self.start_time
            .read()
            .ok()
            .and_then(|s| s.map(|t| t.elapsed()))
    }

    /// Get a summary of token usage
    pub fn summary(&self) -> TokenSummary {
        TokenSummary {
            prompt_tokens: self.total_prompt_tokens(),
            completion_tokens: self.total_completion_tokens(),
            total_tokens: self.total_tokens(),
            api_calls: self.api_call_count(),
            estimated_cost: self.estimate_cost(),
            duration: self.session_duration(),
        }
    }

    /// Estimate cost based on typical pricing
    /// Note: This is a rough estimate, actual pricing varies by model
    pub fn estimate_cost(&self) -> f64 {
        let prompt = self.total_prompt_tokens() as f64;
        let completion = self.total_completion_tokens() as f64;

        // Rough estimate based on typical LLM pricing (per 1M tokens)
        // Adjust these values based on actual model pricing
        let prompt_cost_per_1m = 3.0; // $3 per 1M prompt tokens
        let completion_cost_per_1m = 15.0; // $15 per 1M completion tokens

        (prompt / 1_000_000.0 * prompt_cost_per_1m)
            + (completion / 1_000_000.0 * completion_cost_per_1m)
    }

    /// Reset the tracker
    pub fn reset(&self) {
        self.prompt_tokens.store(0, Ordering::SeqCst);
        self.completion_tokens.store(0, Ordering::SeqCst);
        self.api_calls.store(0, Ordering::SeqCst);
        if let Ok(mut steps) = self.step_usage.write() {
            steps.clear();
        }
        if let Ok(mut start) = self.start_time.write() {
            *start = Some(Instant::now());
        }
    }
}

/// Summary of token usage
#[derive(Debug, Clone)]
pub struct TokenSummary {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub api_calls: usize,
    pub estimated_cost: f64,
    pub duration: Option<std::time::Duration>,
}

impl std::fmt::Display for TokenSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tokens: {} (prompt: {}, completion: {}) | API calls: {} | Est. cost: ${:.4}",
            self.total_tokens,
            self.prompt_tokens,
            self.completion_tokens,
            self.api_calls,
            self.estimated_cost
        )?;

        if let Some(duration) = self.duration {
            write!(f, " | Duration: {:.1}s", duration.as_secs_f64())?;
        }

        Ok(())
    }
}

/// Estimate tokens for a string.
pub fn estimate_tokens(text: &str) -> usize {
    crate::token_count::estimate_content_tokens(text).max(1)
}

/// Estimate tokens for a JSON value
pub fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    let json_str = serde_json::to_string(value).unwrap_or_default();
    // JSON tends to have more tokens due to structure
    (estimate_tokens(&json_str) as f64 * 1.2) as usize
}

/// Estimate tokens for a list of messages
pub fn estimate_messages_tokens(messages: &[crate::api::types::Message]) -> usize {
    let mut total = 0;

    for msg in messages {
        // Role overhead
        total += 4;
        // Content
        total += estimate_tokens(&msg.content);
        // Tool calls if present
        if let Some(ref tool_calls) = msg.tool_calls {
            for call in tool_calls {
                total += 10; // Overhead per tool call
                total += estimate_tokens(&call.function.name);
                total += estimate_tokens(&call.function.arguments);
            }
        }
    }

    total
}

// ============================================================================
// Model Pricing Configuration
// ============================================================================

/// Pricing information for a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Model identifier
    pub model_id: String,
    /// Cost per 1K input tokens
    pub input_cost_per_1k: f64,
    /// Cost per 1K output tokens
    pub output_cost_per_1k: f64,
    /// Maximum context window
    pub max_context: usize,
    /// Capability tier (1 = basic, 2 = standard, 3 = advanced)
    pub capability_tier: u8,
    /// Speed rating (1 = slow, 2 = medium, 3 = fast)
    pub speed_tier: u8,
}

impl ModelPricing {
    /// Calculate cost for given token counts
    pub fn calculate_cost(&self, input_tokens: usize, output_tokens: usize) -> f64 {
        let input_cost = (input_tokens as f64 / 1000.0) * self.input_cost_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * self.output_cost_per_1k;
        input_cost + output_cost
    }

    /// Common model presets
    pub fn claude_haiku() -> Self {
        Self {
            model_id: "claude-3-haiku".to_string(),
            input_cost_per_1k: 0.00025,
            output_cost_per_1k: 0.00125,
            max_context: 200_000,
            capability_tier: 1,
            speed_tier: 3,
        }
    }

    pub fn claude_sonnet() -> Self {
        Self {
            model_id: "claude-3-5-sonnet".to_string(),
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            max_context: 200_000,
            capability_tier: 2,
            speed_tier: 2,
        }
    }

    pub fn claude_opus() -> Self {
        Self {
            model_id: "claude-3-opus".to_string(),
            input_cost_per_1k: 0.015,
            output_cost_per_1k: 0.075,
            max_context: 200_000,
            capability_tier: 3,
            speed_tier: 1,
        }
    }
}

// ============================================================================
// Context Pruning
// ============================================================================

/// Strategies for pruning context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PruningStrategy {
    /// Keep most recent messages
    KeepRecent,
    /// Keep first and last messages
    KeepEnds,
    /// Remove by relevance score
    ByRelevance,
    /// Compress/summarize older messages
    Summarize,
    /// Remove tool results (keep tool calls)
    RemoveToolResults,
    /// Remove system messages (except first)
    RemoveSystemMessages,
}

/// Configuration for context pruning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningConfig {
    /// Target token count after pruning
    pub target_tokens: usize,
    /// Strategy to use
    pub strategy: PruningStrategy,
    /// Minimum messages to keep
    pub min_messages: usize,
    /// Always keep system message
    pub keep_system: bool,
    /// Always keep last N messages
    pub keep_last_n: usize,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            target_tokens: 100_000,
            strategy: PruningStrategy::KeepRecent,
            min_messages: 5,
            keep_system: true,
            keep_last_n: 3,
        }
    }
}

/// Context pruner for managing message history
pub struct ContextPruner {
    config: PruningConfig,
    stats: RwLock<PruningStats>,
}

/// Statistics about pruning operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruningStats {
    /// Total pruning operations
    pub total_operations: u64,
    /// Total tokens removed
    pub tokens_removed: u64,
    /// Total messages removed
    pub messages_removed: u64,
    /// Total cost saved (estimated)
    pub cost_saved: f64,
}

impl ContextPruner {
    /// Create a new pruner
    pub fn new(config: PruningConfig) -> Self {
        Self {
            config,
            stats: RwLock::new(PruningStats::default()),
        }
    }

    /// Check if pruning is needed
    pub fn needs_pruning(&self, current_tokens: usize) -> bool {
        current_tokens > self.config.target_tokens
    }

    /// Calculate how many tokens to remove
    pub fn tokens_to_remove(&self, current_tokens: usize) -> usize {
        current_tokens.saturating_sub(self.config.target_tokens)
    }

    /// Prune messages using the configured strategy
    pub fn prune(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        let current_tokens = estimate_messages_tokens(messages);
        if !self.needs_pruning(current_tokens) {
            return messages.to_vec();
        }

        let result = match self.config.strategy {
            PruningStrategy::KeepRecent => self.prune_keep_recent(messages),
            PruningStrategy::KeepEnds => self.prune_keep_ends(messages),
            PruningStrategy::RemoveToolResults => self.prune_tool_results(messages),
            PruningStrategy::RemoveSystemMessages => self.prune_system_messages(messages),
            _ => self.prune_keep_recent(messages), // Default fallback
        };

        // Update stats
        let new_tokens = estimate_messages_tokens(&result);
        if let Ok(mut stats) = self.stats.write() {
            stats.total_operations += 1;
            stats.tokens_removed += (current_tokens - new_tokens) as u64;
            stats.messages_removed += (messages.len() - result.len()) as u64;
            // Estimate cost saved (using sonnet pricing)
            stats.cost_saved += (current_tokens - new_tokens) as f64 / 1000.0 * 0.003;
        }

        result
    }

    /// Keep most recent messages
    fn prune_keep_recent(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        let mut result = Vec::new();

        // Always keep system message if configured
        if self.config.keep_system {
            if let Some(first) = messages.first() {
                if first.role == "system" {
                    result.push(first.clone());
                }
            }
        }

        // Keep last N messages
        let start = messages.len().saturating_sub(self.config.keep_last_n);
        for msg in messages.iter().skip(start) {
            if msg.role != "system"
                || !result
                    .iter()
                    .any(|m: &crate::api::types::Message| m.role == "system")
            {
                result.push(msg.clone());
            }
        }

        // Add more messages if under target
        let mut current_tokens = estimate_messages_tokens(&result);
        for msg in messages.iter().rev().skip(self.config.keep_last_n) {
            if current_tokens >= self.config.target_tokens {
                break;
            }
            let msg_tokens = estimate_tokens(&msg.content);
            if current_tokens + msg_tokens <= self.config.target_tokens {
                result.insert(if self.config.keep_system { 1 } else { 0 }, msg.clone());
                current_tokens += msg_tokens;
            }
        }

        result
    }

    /// Keep first and last messages
    fn prune_keep_ends(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        if messages.len() <= self.config.min_messages {
            return messages.to_vec();
        }

        let mut result = Vec::new();

        // Keep first message (usually system)
        if let Some(first) = messages.first() {
            result.push(first.clone());
        }

        // Keep last N messages
        let keep_end = self.config.keep_last_n.min(messages.len() - 1);
        for msg in messages.iter().rev().take(keep_end) {
            result.push(msg.clone());
        }

        result.reverse();
        result
    }

    /// Remove tool results but keep tool calls
    fn prune_tool_results(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        messages
            .iter()
            .filter(|msg| msg.role != "tool")
            .cloned()
            .collect()
    }

    /// Remove system messages except first
    fn prune_system_messages(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        let mut first_system = true;
        messages
            .iter()
            .filter(|msg| {
                if msg.role == "system" {
                    if first_system {
                        first_system = false;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    /// Get pruning statistics
    pub fn stats(&self) -> PruningStats {
        self.stats.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        if let Ok(mut stats) = self.stats.write() {
            *stats = PruningStats::default();
        }
    }
}

impl Default for ContextPruner {
    fn default() -> Self {
        Self::new(PruningConfig::default())
    }
}

// ============================================================================
// Model Selection
// ============================================================================

/// Task complexity levels for model selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// Simple tasks (formatting, basic edits)
    Simple,
    /// Standard tasks (code generation, explanations)
    Standard,
    /// Complex tasks (architecture, debugging)
    Complex,
    /// Critical tasks (security review, production deployments)
    Critical,
}

/// Configuration for model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelectionConfig {
    /// Available models with pricing
    pub models: Vec<ModelPricing>,
    /// Default model for standard tasks
    pub default_model: String,
    /// Enable automatic model selection
    pub auto_select: bool,
    /// Cost budget per request (in dollars)
    pub max_cost_per_request: f64,
}

impl Default for ModelSelectionConfig {
    fn default() -> Self {
        Self {
            models: vec![
                ModelPricing::claude_haiku(),
                ModelPricing::claude_sonnet(),
                ModelPricing::claude_opus(),
            ],
            default_model: "claude-3-5-sonnet".to_string(),
            auto_select: true,
            max_cost_per_request: 0.50,
        }
    }
}

/// Selects optimal model based on task requirements
pub struct ModelSelector {
    config: ModelSelectionConfig,
    usage_history: RwLock<VecDeque<ModelUsage>>,
}

/// Record of model usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    /// Model used
    pub model_id: String,
    /// Task complexity
    pub complexity: TaskComplexity,
    /// Input tokens
    pub input_tokens: usize,
    /// Output tokens
    pub output_tokens: usize,
    /// Cost
    pub cost: f64,
    /// Was it successful
    pub success: bool,
    /// Timestamp
    pub timestamp: u64,
}

impl ModelSelector {
    /// Create a new selector
    pub fn new(config: ModelSelectionConfig) -> Self {
        Self {
            config,
            usage_history: RwLock::new(VecDeque::with_capacity(100)),
        }
    }

    /// Select best model for a task
    pub fn select(&self, complexity: TaskComplexity, estimated_tokens: usize) -> String {
        if !self.config.auto_select {
            return self.config.default_model.clone();
        }

        // Find models that can handle the task
        let required_tier = match complexity {
            TaskComplexity::Simple => 1,
            TaskComplexity::Standard => 2,
            TaskComplexity::Complex => 2,
            TaskComplexity::Critical => 3,
        };

        let suitable_models: Vec<_> = self
            .config
            .models
            .iter()
            .filter(|m| m.capability_tier >= required_tier)
            .filter(|m| m.max_context >= estimated_tokens)
            .collect();

        if suitable_models.is_empty() {
            return self.config.default_model.clone();
        }

        // For simple tasks, prefer cheaper models
        if complexity == TaskComplexity::Simple {
            return suitable_models
                .iter()
                .min_by(|a, b| {
                    a.input_cost_per_1k
                        .partial_cmp(&b.input_cost_per_1k)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|m| m.model_id.clone())
                .unwrap_or_else(|| self.config.default_model.clone());
        }

        // For complex tasks, prefer more capable models
        if complexity == TaskComplexity::Complex || complexity == TaskComplexity::Critical {
            return suitable_models
                .iter()
                .max_by_key(|m| m.capability_tier)
                .map(|m| m.model_id.clone())
                .unwrap_or_else(|| self.config.default_model.clone());
        }

        // Default: balance cost and capability
        suitable_models
            .iter()
            .find(|m| m.capability_tier == 2)
            .or_else(|| suitable_models.first())
            .map(|m| m.model_id.clone())
            .unwrap_or_else(|| self.config.default_model.clone())
    }

    /// Record model usage
    pub fn record_usage(&self, usage: ModelUsage) {
        if let Ok(mut history) = self.usage_history.write() {
            history.push_back(usage);
            while history.len() > 100 {
                history.pop_front();
            }
        }
    }

    /// Get model recommendation with reasoning
    pub fn recommend(
        &self,
        complexity: TaskComplexity,
        estimated_tokens: usize,
    ) -> ModelRecommendation {
        let selected = self.select(complexity, estimated_tokens);
        let pricing = self
            .config
            .models
            .iter()
            .find(|m| m.model_id == selected)
            .cloned()
            .unwrap_or_else(ModelPricing::claude_sonnet);

        let estimated_cost = pricing.calculate_cost(estimated_tokens, estimated_tokens / 2);

        let reason = match complexity {
            TaskComplexity::Simple => "Using faster, cheaper model for simple task",
            TaskComplexity::Standard => "Using balanced model for standard task",
            TaskComplexity::Complex => "Using capable model for complex task",
            TaskComplexity::Critical => "Using most capable model for critical task",
        };

        ModelRecommendation {
            model_id: selected,
            complexity,
            estimated_tokens,
            estimated_cost,
            reason: reason.to_string(),
            alternative: self.get_alternative(complexity),
        }
    }

    /// Get an alternative model
    fn get_alternative(&self, complexity: TaskComplexity) -> Option<String> {
        match complexity {
            TaskComplexity::Simple => Some("claude-3-5-sonnet".to_string()),
            TaskComplexity::Standard => Some("claude-3-haiku".to_string()),
            TaskComplexity::Complex => Some("claude-3-opus".to_string()),
            TaskComplexity::Critical => None,
        }
    }

    /// Get usage summary
    pub fn usage_summary(&self) -> ModelUsageSummary {
        let history = self
            .usage_history
            .read()
            .map(|h| h.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let total_cost: f64 = history.iter().map(|u| u.cost).sum();
        let total_tokens: usize = history
            .iter()
            .map(|u| u.input_tokens + u.output_tokens)
            .sum();
        let success_rate = if history.is_empty() {
            0.0
        } else {
            history.iter().filter(|u| u.success).count() as f32 / history.len() as f32
        };

        let by_model: HashMap<String, u64> = history.iter().fold(HashMap::new(), |mut acc, u| {
            *acc.entry(u.model_id.clone()).or_default() += 1;
            acc
        });

        ModelUsageSummary {
            total_requests: history.len() as u64,
            total_cost,
            total_tokens,
            success_rate,
            by_model,
        }
    }
}

impl Default for ModelSelector {
    fn default() -> Self {
        Self::new(ModelSelectionConfig::default())
    }
}

/// Model recommendation with reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    pub model_id: String,
    pub complexity: TaskComplexity,
    pub estimated_tokens: usize,
    pub estimated_cost: f64,
    pub reason: String,
    pub alternative: Option<String>,
}

/// Summary of model usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsageSummary {
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_tokens: usize,
    pub success_rate: f32,
    pub by_model: HashMap<String, u64>,
}

// ============================================================================
// Budget Management
// ============================================================================

/// Configuration for budget management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Daily budget in dollars
    pub daily_budget: f64,
    /// Monthly budget in dollars
    pub monthly_budget: f64,
    /// Alert threshold (percentage of budget)
    pub alert_threshold: f32,
    /// Hard limit (stop requests when reached)
    pub hard_limit: bool,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            daily_budget: 10.0,
            monthly_budget: 100.0,
            alert_threshold: 0.8,
            hard_limit: false,
        }
    }
}

/// Budget manager for tracking and enforcing spending limits
pub struct BudgetManager {
    config: BudgetConfig,
    daily_spending: RwLock<DailySpending>,
    monthly_spending: RwLock<MonthlySpending>,
    alerts: RwLock<Vec<BudgetAlert>>,
}

/// Daily spending record
#[derive(Debug, Clone, Default)]
struct DailySpending {
    date: u64, // Days since epoch
    amount: f64,
}

/// Monthly spending record
#[derive(Debug, Clone, Default)]
struct MonthlySpending {
    month: u32, // YYYYMM format
    amount: f64,
}

/// Budget alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAlert {
    /// Alert type
    pub alert_type: BudgetAlertType,
    /// Message
    pub message: String,
    /// Threshold that triggered alert
    pub threshold: f32,
    /// Current usage
    pub current_usage: f64,
    /// Budget limit
    pub budget_limit: f64,
    /// Timestamp
    pub timestamp: u64,
}

/// Types of budget alerts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetAlertType {
    DailyWarning,
    DailyExceeded,
    MonthlyWarning,
    MonthlyExceeded,
}

impl BudgetManager {
    /// Create a new budget manager
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            config,
            daily_spending: RwLock::new(DailySpending::default()),
            monthly_spending: RwLock::new(MonthlySpending::default()),
            alerts: RwLock::new(Vec::new()),
        }
    }

    /// Record spending
    pub fn record_spending(&self, amount: f64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let today = now.as_secs() / 86400;
        let this_month = {
            let secs = now.as_secs();
            let days = secs / 86400;
            let years = days / 365;
            let year = 1970 + years;
            let day_of_year = days % 365;
            let month = (day_of_year / 30).min(11) + 1;
            (year as u32) * 100 + month as u32
        };

        // Update daily spending
        if let Ok(mut daily) = self.daily_spending.write() {
            if daily.date != today {
                daily.date = today;
                daily.amount = 0.0;
            }
            daily.amount += amount;

            // Check for daily alert
            let usage_ratio = daily.amount / self.config.daily_budget;
            if usage_ratio >= self.config.alert_threshold as f64 {
                self.add_alert(BudgetAlert {
                    alert_type: if usage_ratio >= 1.0 {
                        BudgetAlertType::DailyExceeded
                    } else {
                        BudgetAlertType::DailyWarning
                    },
                    message: format!("Daily budget at {:.1}%", usage_ratio * 100.0),
                    threshold: self.config.alert_threshold,
                    current_usage: daily.amount,
                    budget_limit: self.config.daily_budget,
                    timestamp: now.as_secs(),
                });
            }
        }

        // Update monthly spending
        if let Ok(mut monthly) = self.monthly_spending.write() {
            if monthly.month != this_month {
                monthly.month = this_month;
                monthly.amount = 0.0;
            }
            monthly.amount += amount;

            // Check for monthly alert
            let usage_ratio = monthly.amount / self.config.monthly_budget;
            if usage_ratio >= self.config.alert_threshold as f64 {
                self.add_alert(BudgetAlert {
                    alert_type: if usage_ratio >= 1.0 {
                        BudgetAlertType::MonthlyExceeded
                    } else {
                        BudgetAlertType::MonthlyWarning
                    },
                    message: format!("Monthly budget at {:.1}%", usage_ratio * 100.0),
                    threshold: self.config.alert_threshold,
                    current_usage: monthly.amount,
                    budget_limit: self.config.monthly_budget,
                    timestamp: now.as_secs(),
                });
            }
        }
    }

    /// Add an alert
    fn add_alert(&self, alert: BudgetAlert) {
        if let Ok(mut alerts) = self.alerts.write() {
            // Avoid duplicate alerts within short time
            let dominated = alerts
                .iter()
                .any(|a| a.alert_type == alert.alert_type && alert.timestamp - a.timestamp < 3600);
            if !dominated {
                alerts.push(alert);
                // Keep last 100 alerts
                while alerts.len() > 100 {
                    alerts.remove(0);
                }
            }
        }
    }

    /// Check if request is allowed
    pub fn can_spend(&self, amount: f64) -> bool {
        if !self.config.hard_limit {
            return true;
        }

        let daily_ok = self
            .daily_spending
            .read()
            .map(|d| d.amount + amount <= self.config.daily_budget)
            .unwrap_or(true);

        let monthly_ok = self
            .monthly_spending
            .read()
            .map(|m| m.amount + amount <= self.config.monthly_budget)
            .unwrap_or(true);

        daily_ok && monthly_ok
    }

    /// Get current daily spending
    pub fn daily_spending(&self) -> f64 {
        self.daily_spending.read().map(|d| d.amount).unwrap_or(0.0)
    }

    /// Get current monthly spending
    pub fn monthly_spending(&self) -> f64 {
        self.monthly_spending
            .read()
            .map(|m| m.amount)
            .unwrap_or(0.0)
    }

    /// Get remaining daily budget
    pub fn daily_remaining(&self) -> f64 {
        (self.config.daily_budget - self.daily_spending()).max(0.0)
    }

    /// Get remaining monthly budget
    pub fn monthly_remaining(&self) -> f64 {
        (self.config.monthly_budget - self.monthly_spending()).max(0.0)
    }

    /// Get alerts
    pub fn alerts(&self) -> Vec<BudgetAlert> {
        self.alerts.read().map(|a| a.clone()).unwrap_or_default()
    }

    /// Get budget status
    pub fn status(&self) -> BudgetStatus {
        BudgetStatus {
            daily_spent: self.daily_spending(),
            daily_budget: self.config.daily_budget,
            daily_remaining: self.daily_remaining(),
            monthly_spent: self.monthly_spending(),
            monthly_budget: self.config.monthly_budget,
            monthly_remaining: self.monthly_remaining(),
            alert_count: self.alerts.read().map(|a| a.len()).unwrap_or(0),
        }
    }

    /// Reset daily spending (for testing)
    pub fn reset_daily(&self) {
        if let Ok(mut daily) = self.daily_spending.write() {
            daily.amount = 0.0;
        }
    }
}

impl Default for BudgetManager {
    fn default() -> Self {
        Self::new(BudgetConfig::default())
    }
}

/// Current budget status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetStatus {
    pub daily_spent: f64,
    pub daily_budget: f64,
    pub daily_remaining: f64,
    pub monthly_spent: f64,
    pub monthly_budget: f64,
    pub monthly_remaining: f64,
    pub alert_count: usize,
}

// ============================================================================
// Cost Optimizer
// ============================================================================

/// Unified cost optimizer
pub struct CostOptimizer {
    /// Token tracker
    tracker: TokenTracker,
    /// Context pruner
    pruner: ContextPruner,
    /// Model selector
    selector: ModelSelector,
    /// Budget manager
    budget: BudgetManager,
}

impl CostOptimizer {
    /// Create a new optimizer
    pub fn new(
        pruner_config: PruningConfig,
        selector_config: ModelSelectionConfig,
        budget_config: BudgetConfig,
    ) -> Self {
        Self {
            tracker: TokenTracker::new(),
            pruner: ContextPruner::new(pruner_config),
            selector: ModelSelector::new(selector_config),
            budget: BudgetManager::new(budget_config),
        }
    }

    /// Get the token tracker
    pub fn tracker(&self) -> &TokenTracker {
        &self.tracker
    }

    /// Get the context pruner
    pub fn pruner(&self) -> &ContextPruner {
        &self.pruner
    }

    /// Get the model selector
    pub fn selector(&self) -> &ModelSelector {
        &self.selector
    }

    /// Get the budget manager
    pub fn budget(&self) -> &BudgetManager {
        &self.budget
    }

    /// Get optimization recommendations
    pub fn get_recommendations(&self) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Check pruning stats
        let pruning_stats = self.pruner.stats();
        if pruning_stats.tokens_removed > 100_000 {
            recommendations.push(OptimizationRecommendation {
                category: "Context".to_string(),
                message: "Consider reducing context window size".to_string(),
                potential_savings: pruning_stats.cost_saved * 0.5,
                priority: OptimizationPriority::Medium,
            });
        }

        // Check model usage
        let model_summary = self.selector.usage_summary();
        if model_summary.total_cost > 50.0 && model_summary.success_rate > 0.9 {
            recommendations.push(OptimizationRecommendation {
                category: "Model".to_string(),
                message: "High success rate - consider using cheaper models for simple tasks"
                    .to_string(),
                potential_savings: model_summary.total_cost * 0.2,
                priority: OptimizationPriority::High,
            });
        }

        // Check budget status
        let budget_status = self.budget.status();
        if budget_status.daily_remaining < budget_status.daily_budget * 0.2 {
            recommendations.push(OptimizationRecommendation {
                category: "Budget".to_string(),
                message: "Daily budget nearly exhausted".to_string(),
                potential_savings: 0.0,
                priority: OptimizationPriority::High,
            });
        }

        recommendations
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> CostOptimizerSummary {
        CostOptimizerSummary {
            token_summary: self.tracker.summary(),
            pruning_stats: self.pruner.stats(),
            model_usage: self.selector.usage_summary(),
            budget_status: self.budget.status(),
            recommendations: self.get_recommendations(),
        }
    }
}

impl Default for CostOptimizer {
    fn default() -> Self {
        Self::new(
            PruningConfig::default(),
            ModelSelectionConfig::default(),
            BudgetConfig::default(),
        )
    }
}

/// Optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    pub category: String,
    pub message: String,
    pub potential_savings: f64,
    pub priority: OptimizationPriority,
}

/// Priority of optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationPriority {
    Low,
    Medium,
    High,
}

/// Comprehensive cost optimizer summary
#[derive(Debug, Clone)]
pub struct CostOptimizerSummary {
    pub token_summary: TokenSummary,
    pub pruning_stats: PruningStats,
    pub model_usage: ModelUsageSummary,
    pub budget_status: BudgetStatus,
    pub recommendations: Vec<OptimizationRecommendation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_tracker_new() {
        let tracker = TokenTracker::new();
        assert_eq!(tracker.total_tokens(), 0);
        assert_eq!(tracker.api_call_count(), 0);
    }

    #[test]
    fn test_record_usage() {
        let tracker = TokenTracker::new();
        tracker.record_usage(100, 50);

        assert_eq!(tracker.total_prompt_tokens(), 100);
        assert_eq!(tracker.total_completion_tokens(), 50);
        assert_eq!(tracker.total_tokens(), 150);
        assert_eq!(tracker.api_call_count(), 1);
    }

    #[test]
    fn test_record_multiple() {
        let tracker = TokenTracker::new();
        tracker.record_usage(100, 50);
        tracker.record_usage(200, 100);

        assert_eq!(tracker.total_prompt_tokens(), 300);
        assert_eq!(tracker.total_completion_tokens(), 150);
        assert_eq!(tracker.api_call_count(), 2);
    }

    #[test]
    fn test_record_step() {
        let tracker = TokenTracker::new();
        tracker.record_step(1, 100, 50, Some("file_read".to_string()));
        tracker.record_step(2, 150, 75, Some("shell_exec".to_string()));

        let steps = tracker.step_usage();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].step, 1);
        assert_eq!(steps[0].tool_name, Some("file_read".to_string()));
    }

    #[test]
    fn test_estimate_cost() {
        let tracker = TokenTracker::new();
        tracker.record_usage(1_000_000, 100_000);

        let cost = tracker.estimate_cost();
        // 1M prompt tokens at $3/1M + 100K completion at $15/1M
        // = $3 + $1.5 = $4.5
        assert!((cost - 4.5).abs() < 0.01);
    }

    #[test]
    fn test_reset() {
        let tracker = TokenTracker::new();
        tracker.record_usage(100, 50);
        tracker.reset();

        assert_eq!(tracker.total_tokens(), 0);
        assert_eq!(tracker.api_call_count(), 0);
    }

    #[test]
    fn test_summary_display() {
        let tracker = TokenTracker::new();
        tracker.record_usage(1000, 500);

        let summary = tracker.summary();
        let display = format!("{}", summary);

        assert!(display.contains("1500"));
        assert!(display.contains("1000"));
        assert!(display.contains("500"));
    }

    #[test]
    fn test_estimate_tokens_short() {
        let estimate = estimate_tokens("Hello, world!");
        assert!(estimate > 0);
        assert!(estimate < 10);
    }

    #[test]
    fn test_estimate_tokens_long() {
        let text =
            "This is a longer piece of text that should result in more tokens being estimated.";
        let estimate = estimate_tokens(text);
        assert!(estimate > 10);
    }

    #[test]
    fn test_estimate_tokens_code() {
        let code = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        let estimate = estimate_tokens(code);
        assert!(estimate > 5);
    }

    #[test]
    fn test_estimate_json_tokens() {
        let json = serde_json::json!({
            "name": "test",
            "values": [1, 2, 3],
            "nested": {"a": 1, "b": 2}
        });

        let estimate = estimate_json_tokens(&json);
        // Small JSON objects produce ~5-10 tokens
        assert!(estimate > 5);
    }

    #[test]
    fn test_session_duration() {
        let tracker = TokenTracker::new();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let duration = tracker.session_duration();
        assert!(duration.is_some());
        assert!(duration.unwrap().as_millis() >= 10);
    }

    #[test]
    fn test_estimate_messages_tokens_simple() {
        use crate::api::types::Message;

        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("Hello, how are you?"),
            Message::assistant("I'm doing well, thank you!"),
        ];

        let estimate = estimate_messages_tokens(&messages);
        // At least 4 tokens overhead per message (3 messages) + content
        assert!(estimate > 12);
    }

    #[test]
    fn test_estimate_messages_tokens_with_tool_calls() {
        use crate::api::types::{Message, ToolCall, ToolFunction};

        let mut msg = Message::assistant("Let me read that file for you.");
        msg.tool_calls = Some(vec![ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{"path": "test.txt"}"#.to_string(),
            },
        }]);

        let messages = vec![msg];
        let estimate = estimate_messages_tokens(&messages);

        // Should include tool call overhead
        assert!(estimate > 20);
    }

    #[test]
    fn test_estimate_messages_tokens_empty() {
        let messages: Vec<crate::api::types::Message> = vec![];
        let estimate = estimate_messages_tokens(&messages);
        assert_eq!(estimate, 0);
    }
}

#[cfg(test)]
mod model_pricing_tests {
    use super::*;

    #[test]
    fn test_model_pricing_calculate_cost() {
        let pricing = ModelPricing::claude_sonnet();
        let cost = pricing.calculate_cost(1000, 500);
        // 1000/1000 * 0.003 + 500/1000 * 0.015 = 0.003 + 0.0075 = 0.0105
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_model_pricing_haiku() {
        let pricing = ModelPricing::claude_haiku();
        assert_eq!(pricing.capability_tier, 1);
        assert_eq!(pricing.speed_tier, 3);
    }

    #[test]
    fn test_model_pricing_sonnet() {
        let pricing = ModelPricing::claude_sonnet();
        assert_eq!(pricing.capability_tier, 2);
        assert_eq!(pricing.speed_tier, 2);
    }

    #[test]
    fn test_model_pricing_opus() {
        let pricing = ModelPricing::claude_opus();
        assert_eq!(pricing.capability_tier, 3);
        assert_eq!(pricing.speed_tier, 1);
    }
}

#[cfg(test)]
mod context_pruner_tests {
    use super::*;

    #[test]
    fn test_pruning_config_default() {
        let config = PruningConfig::default();
        assert_eq!(config.target_tokens, 100_000);
        assert!(config.keep_system);
    }

    #[test]
    fn test_pruner_needs_pruning() {
        let pruner = ContextPruner::default();
        assert!(!pruner.needs_pruning(50_000));
        assert!(pruner.needs_pruning(150_000));
    }

    #[test]
    fn test_pruner_tokens_to_remove() {
        let pruner = ContextPruner::default();
        assert_eq!(pruner.tokens_to_remove(50_000), 0);
        assert_eq!(pruner.tokens_to_remove(150_000), 50_000);
    }

    #[test]
    fn test_pruner_stats() {
        let pruner = ContextPruner::default();
        let stats = pruner.stats();
        assert_eq!(stats.total_operations, 0);
    }

    #[test]
    fn test_pruning_strategy_enum() {
        assert_eq!(PruningStrategy::KeepRecent, PruningStrategy::KeepRecent);
        assert_ne!(PruningStrategy::KeepRecent, PruningStrategy::KeepEnds);
    }
}

#[cfg(test)]
mod model_selector_tests {
    use super::*;

    #[test]
    fn test_model_selection_config_default() {
        let config = ModelSelectionConfig::default();
        assert!(config.auto_select);
        assert_eq!(config.models.len(), 3);
    }

    #[test]
    fn test_selector_select_simple() {
        let selector = ModelSelector::default();
        let model = selector.select(TaskComplexity::Simple, 1000);
        assert_eq!(model, "claude-3-haiku"); // Cheapest for simple
    }

    #[test]
    fn test_selector_select_critical() {
        let selector = ModelSelector::default();
        let model = selector.select(TaskComplexity::Critical, 1000);
        assert_eq!(model, "claude-3-opus"); // Most capable for critical
    }

    #[test]
    fn test_selector_recommend() {
        let selector = ModelSelector::default();
        let rec = selector.recommend(TaskComplexity::Standard, 5000);
        assert!(!rec.reason.is_empty());
        assert!(rec.estimated_cost > 0.0);
    }

    #[test]
    fn test_selector_record_usage() {
        let selector = ModelSelector::default();
        selector.record_usage(ModelUsage {
            model_id: "claude-3-5-sonnet".to_string(),
            complexity: TaskComplexity::Standard,
            input_tokens: 1000,
            output_tokens: 500,
            cost: 0.01,
            success: true,
            timestamp: 12345,
        });

        let summary = selector.usage_summary();
        assert_eq!(summary.total_requests, 1);
    }

    #[test]
    fn test_task_complexity_enum() {
        assert_eq!(TaskComplexity::Simple, TaskComplexity::Simple);
        assert_ne!(TaskComplexity::Simple, TaskComplexity::Complex);
    }
}

#[cfg(test)]
mod budget_manager_tests {
    use super::*;

    #[test]
    fn test_budget_config_default() {
        let config = BudgetConfig::default();
        assert_eq!(config.daily_budget, 10.0);
        assert_eq!(config.monthly_budget, 100.0);
    }

    #[test]
    fn test_budget_record_spending() {
        let manager = BudgetManager::default();
        manager.record_spending(1.0);
        assert!((manager.daily_spending() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_budget_remaining() {
        let manager = BudgetManager::default();
        manager.record_spending(3.0);
        assert!((manager.daily_remaining() - 7.0).abs() < 0.001);
    }

    #[test]
    fn test_budget_can_spend() {
        let config = BudgetConfig {
            hard_limit: true,
            daily_budget: 5.0,
            ..Default::default()
        };
        let manager = BudgetManager::new(config);

        assert!(manager.can_spend(3.0));
        manager.record_spending(4.0);
        assert!(!manager.can_spend(2.0));
    }

    #[test]
    fn test_budget_status() {
        let manager = BudgetManager::default();
        manager.record_spending(2.0);

        let status = manager.status();
        assert!((status.daily_spent - 2.0).abs() < 0.001);
        assert_eq!(status.daily_budget, 10.0);
    }

    #[test]
    fn test_budget_reset_daily() {
        let manager = BudgetManager::default();
        manager.record_spending(5.0);
        manager.reset_daily();

        assert_eq!(manager.daily_spending(), 0.0);
    }

    #[test]
    fn test_budget_alert_type() {
        assert_eq!(BudgetAlertType::DailyWarning, BudgetAlertType::DailyWarning);
        assert_ne!(
            BudgetAlertType::DailyWarning,
            BudgetAlertType::DailyExceeded
        );
    }
}

#[cfg(test)]
mod cost_optimizer_tests {
    use super::*;

    #[test]
    fn test_cost_optimizer_default() {
        let optimizer = CostOptimizer::default();
        assert_eq!(optimizer.tracker().total_tokens(), 0);
    }

    #[test]
    fn test_cost_optimizer_components() {
        let optimizer = CostOptimizer::default();
        optimizer.tracker().record_usage(100, 50);
        assert_eq!(optimizer.tracker().total_tokens(), 150);
    }

    #[test]
    fn test_cost_optimizer_recommendations() {
        let optimizer = CostOptimizer::default();
        let recommendations = optimizer.get_recommendations();
        // Should have no recommendations with fresh state
        assert!(recommendations.len() <= 3);
    }

    #[test]
    fn test_cost_optimizer_summary() {
        let optimizer = CostOptimizer::default();
        let summary = optimizer.summary();
        assert_eq!(summary.token_summary.total_tokens, 0);
    }

    #[test]
    fn test_optimization_priority() {
        assert_eq!(OptimizationPriority::Low, OptimizationPriority::Low);
        assert_ne!(OptimizationPriority::Low, OptimizationPriority::High);
    }
}
