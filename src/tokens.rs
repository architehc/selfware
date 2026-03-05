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
    /// Drift tracking: cumulative (estimated - actual) for prompt tokens
    drift: RwLock<DriftStats>,
}

/// Tracks cumulative drift between estimated and actual token counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriftStats {
    /// Number of comparison samples recorded
    pub samples: u64,
    /// Sum of (estimated - actual) across all samples.  Positive means
    /// the estimator is over-counting; negative means under-counting.
    pub cumulative_drift: i64,
    /// Sum of |estimated - actual| for mean absolute error.
    pub cumulative_abs_drift: u64,
    /// Largest single over-estimate seen
    pub max_over: i64,
    /// Largest single under-estimate seen (stored as negative)
    pub max_under: i64,
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
            drift: RwLock::new(DriftStats::default()),
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
            drift: self.drift_stats(),
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

    /// Record the difference between an estimated token count and the actual
    /// count reported by the API.  Call this after each API response that
    /// includes a `usage` block so drift can be tracked over the session.
    pub fn record_drift(&self, estimated: usize, actual: usize) {
        if let Ok(mut drift) = self.drift.write() {
            let diff = estimated as i64 - actual as i64;
            drift.samples += 1;
            drift.cumulative_drift += diff;
            drift.cumulative_abs_drift += diff.unsigned_abs();
            if diff > drift.max_over {
                drift.max_over = diff;
            }
            if diff < drift.max_under {
                drift.max_under = diff;
            }
            // Log a warning when drift exceeds 15% on a single sample
            if actual > 0 {
                let pct = (diff.unsigned_abs() as f64 / actual as f64) * 100.0;
                if pct > 15.0 {
                    tracing::warn!(
                        estimated,
                        actual,
                        drift_pct = format!("{:.1}%", pct),
                        "Token estimation drift exceeds 15%"
                    );
                }
            }
        }
    }

    /// Return a snapshot of the current drift statistics.
    pub fn drift_stats(&self) -> DriftStats {
        self.drift.read().map(|d| d.clone()).unwrap_or_default()
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
        if let Ok(mut drift) = self.drift.write() {
            *drift = DriftStats::default();
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
    pub drift: DriftStats,
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

        if self.drift.samples > 0 {
            let mae = self.drift.cumulative_abs_drift as f64 / self.drift.samples as f64;
            write!(
                f,
                " | Drift: avg={:+.0}, MAE={:.0} ({} samples)",
                self.drift.cumulative_drift as f64 / self.drift.samples as f64,
                mae,
                self.drift.samples
            )?;
        }

        Ok(())
    }
}

/// Estimate tokens for a string.
pub fn estimate_tokens(text: &str) -> usize {
    crate::token_count::estimate_content_tokens(text).max(1)
}

/// Estimate vision token cost for an image based on dimensions and detail level.
///
/// Uses the OpenAI-compatible tiling formula:
/// - `"low"` detail: fixed 85 tokens regardless of size.
/// - `"high"` detail: image is scaled so the longest side ≤ 2048, then the
///   shortest side ≤ 768. The result is tiled into 512×512 tiles, each
///   costing 170 tokens, plus a base cost of 85.
/// - `"auto"` / anything else: uses high-detail for images > 512×512, low
///   otherwise.
pub fn estimate_image_tokens(width: u32, height: u32, detail: &str) -> usize {
    const LOW_COST: usize = 85;
    const TILE_COST: usize = 170;
    const BASE_COST: usize = 85;
    const TILE_SIZE: u32 = 512;

    let effective_detail = match detail {
        "low" => "low",
        "high" => "high",
        _ => {
            // "auto": high for large images, low for small
            if width > TILE_SIZE && height > TILE_SIZE {
                "high"
            } else {
                "low"
            }
        }
    };

    if effective_detail == "low" {
        return LOW_COST;
    }

    // High detail: scale longest side ≤ 2048
    let (mut w, mut h) = (width as f64, height as f64);
    let max_side = w.max(h);
    if max_side > 2048.0 {
        let scale = 2048.0 / max_side;
        w *= scale;
        h *= scale;
    }

    // Scale shortest side ≤ 768
    let min_side = w.min(h);
    if min_side > 768.0 {
        let scale = 768.0 / min_side;
        w *= scale;
        h *= scale;
    }

    // Count 512×512 tiles
    let tiles_w = (w / TILE_SIZE as f64).ceil() as usize;
    let tiles_h = (h / TILE_SIZE as f64).ceil() as usize;
    let num_tiles = tiles_w * tiles_h;

    num_tiles * TILE_COST + BASE_COST
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
        total += estimate_tokens(msg.content.text());
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
            let msg_tokens = estimate_tokens(msg.content.text());
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

    // ---- Drift tracking tests ----

    #[test]
    fn test_drift_stats_default() {
        let tracker = TokenTracker::new();
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 0);
        assert_eq!(drift.cumulative_drift, 0);
    }

    #[test]
    fn test_drift_over_estimate() {
        let tracker = TokenTracker::new();
        // Estimated 120, actual 100 → over-estimate by 20
        tracker.record_drift(120, 100);
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 1);
        assert_eq!(drift.cumulative_drift, 20);
        assert_eq!(drift.cumulative_abs_drift, 20);
        assert_eq!(drift.max_over, 20);
        assert_eq!(drift.max_under, 0);
    }

    #[test]
    fn test_drift_under_estimate() {
        let tracker = TokenTracker::new();
        // Estimated 80, actual 100 → under-estimate by 20
        tracker.record_drift(80, 100);
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 1);
        assert_eq!(drift.cumulative_drift, -20);
        assert_eq!(drift.cumulative_abs_drift, 20);
        assert_eq!(drift.max_over, 0);
        assert_eq!(drift.max_under, -20);
    }

    #[test]
    fn test_drift_accumulation() {
        let tracker = TokenTracker::new();
        tracker.record_drift(110, 100); // +10
        tracker.record_drift(90, 100); // -10
        tracker.record_drift(130, 100); // +30
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 3);
        assert_eq!(drift.cumulative_drift, 30); // 10 + (-10) + 30
        assert_eq!(drift.cumulative_abs_drift, 50); // 10 + 10 + 30
        assert_eq!(drift.max_over, 30);
        assert_eq!(drift.max_under, -10);
    }

    #[test]
    fn test_drift_reset() {
        let tracker = TokenTracker::new();
        tracker.record_drift(150, 100);
        tracker.reset();
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 0);
        assert_eq!(drift.cumulative_drift, 0);
    }

    #[test]
    fn test_drift_in_summary() {
        let tracker = TokenTracker::new();
        tracker.record_usage(1000, 500);
        tracker.record_drift(1100, 1000);
        let summary = tracker.summary();
        assert_eq!(summary.drift.samples, 1);
        let display = format!("{}", summary);
        assert!(display.contains("Drift"));
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

    // ── estimate_image_tokens tests ──

    #[test]
    fn test_image_tokens_low_detail() {
        // Low detail always returns 85 regardless of size
        assert_eq!(estimate_image_tokens(4096, 4096, "low"), 85);
        assert_eq!(estimate_image_tokens(1, 1, "low"), 85);
        assert_eq!(estimate_image_tokens(1920, 1080, "low"), 85);
    }

    #[test]
    fn test_image_tokens_high_detail_small() {
        // 512×512: fits in 1 tile → 170 + 85 = 255
        assert_eq!(estimate_image_tokens(512, 512, "high"), 170 + 85);
    }

    #[test]
    fn test_image_tokens_high_detail_1024x1024() {
        // 1024×1024: shortest side > 768, scaled to 768×768
        // tiles: ceil(768/512) × ceil(768/512) = 2 × 2 = 4
        // cost: 4 × 170 + 85 = 765
        assert_eq!(estimate_image_tokens(1024, 1024, "high"), 765);
    }

    #[test]
    fn test_image_tokens_high_detail_1920x1080() {
        // 1920×1080: shortest=1080 > 768, scale by 768/1080 ≈ 0.7111
        // → 1365.3 × 768
        // tiles: ceil(1365.3/512) × ceil(768/512) = 3 × 2 = 6
        // cost: 6 × 170 + 85 = 1105
        assert_eq!(estimate_image_tokens(1920, 1080, "high"), 1105);
    }

    #[test]
    fn test_image_tokens_auto_small_uses_low() {
        // 256×256: both sides ≤ 512, auto chooses low
        assert_eq!(estimate_image_tokens(256, 256, "auto"), 85);
    }

    #[test]
    fn test_image_tokens_auto_large_uses_high() {
        // 1024×1024: both sides > 512, auto chooses high
        assert_eq!(estimate_image_tokens(1024, 1024, "auto"), 765);
    }

    // ── Additional coverage tests ──

    #[test]
    fn test_estimate_tokens_empty_string() {
        // Empty string should return at least 1 (due to .max(1))
        let estimate = estimate_tokens("");
        assert_eq!(estimate, 1);
    }

    #[test]
    fn test_estimate_tokens_whitespace_only() {
        let estimate = estimate_tokens("   \n\t  ");
        assert!(estimate >= 1);
    }

    #[test]
    fn test_estimate_tokens_very_long_text() {
        let text = "word ".repeat(10_000);
        let estimate = estimate_tokens(&text);
        assert!(estimate > 1000);
    }

    #[test]
    fn test_estimate_json_tokens_empty_object() {
        let json = serde_json::json!({});
        let estimate = estimate_json_tokens(&json);
        assert!(estimate >= 1);
    }

    #[test]
    fn test_estimate_json_tokens_null() {
        let json = serde_json::json!(null);
        let estimate = estimate_json_tokens(&json);
        assert!(estimate >= 1);
    }

    #[test]
    fn test_estimate_json_tokens_array() {
        let json = serde_json::json!([1, 2, 3, 4, 5]);
        let estimate = estimate_json_tokens(&json);
        assert!(estimate >= 1);
    }

    #[test]
    fn test_estimate_json_tokens_deeply_nested() {
        let json = serde_json::json!({
            "a": {"b": {"c": {"d": {"e": "deep"}}}}
        });
        let estimate = estimate_json_tokens(&json);
        assert!(estimate > 3);
    }

    #[test]
    fn test_image_tokens_high_detail_very_large() {
        // 8000×6000: longest side 8000 > 2048, scale by 2048/8000 = 0.256
        // → 2048 × 1536; shortest side 1536 > 768, scale by 768/1536 = 0.5
        // → 1024 × 768
        // tiles: ceil(1024/512) × ceil(768/512) = 2 × 2 = 4
        // cost: 4 × 170 + 85 = 765
        assert_eq!(estimate_image_tokens(8000, 6000, "high"), 765);
    }

    #[test]
    fn test_image_tokens_high_detail_no_scaling_needed() {
        // 400×300: no scaling needed (both < 2048, shortest 300 < 768)
        // tiles: ceil(400/512) × ceil(300/512) = 1 × 1 = 1
        // cost: 1 × 170 + 85 = 255
        assert_eq!(estimate_image_tokens(400, 300, "high"), 255);
    }

    #[test]
    fn test_image_tokens_high_detail_only_longest_side_scaling() {
        // 3000×500: longest=3000 > 2048, scale by 2048/3000 ≈ 0.6827
        // → 2048 × 341.3; shortest=341.3 < 768, no second scaling
        // tiles: ceil(2048/512) × ceil(341.3/512) = 4 × 1 = 4
        // cost: 4 × 170 + 85 = 765
        assert_eq!(estimate_image_tokens(3000, 500, "high"), 765);
    }

    #[test]
    fn test_image_tokens_auto_one_side_large_one_small() {
        // 600×200: width > 512 but height <= 512, auto checks BOTH > 512
        // Since height is not > 512, auto chooses low
        assert_eq!(estimate_image_tokens(600, 200, "auto"), 85);
    }

    #[test]
    fn test_image_tokens_auto_both_at_boundary() {
        // 512×512: both sides are exactly 512, not > 512, so auto chooses low
        assert_eq!(estimate_image_tokens(512, 512, "auto"), 85);
    }

    #[test]
    fn test_image_tokens_unknown_detail_treated_as_auto() {
        // Unknown detail string treated like "auto"
        assert_eq!(estimate_image_tokens(256, 256, "medium"), 85);
        assert_eq!(estimate_image_tokens(1024, 1024, "something"), 765);
    }

    #[test]
    fn test_image_tokens_high_detail_tall_narrow() {
        // 200×4000: longest=4000 > 2048, scale by 2048/4000 = 0.512
        // → 102.4 × 2048; shortest=102.4 < 768, no second scaling
        // tiles: ceil(102.4/512) × ceil(2048/512) = 1 × 4 = 4
        // cost: 4 × 170 + 85 = 765
        assert_eq!(estimate_image_tokens(200, 4000, "high"), 765);
    }

    #[test]
    fn test_summary_display_without_duration_and_without_drift() {
        // Construct a TokenSummary manually with no duration and no drift
        let summary = TokenSummary {
            prompt_tokens: 500,
            completion_tokens: 200,
            total_tokens: 700,
            api_calls: 3,
            estimated_cost: 0.0045,
            duration: None,
            drift: DriftStats::default(),
        };
        let display = format!("{}", summary);
        assert!(display.contains("700"));
        assert!(display.contains("500"));
        assert!(display.contains("200"));
        assert!(display.contains("3"));
        // Should not contain Duration or Drift sections
        assert!(!display.contains("Duration"));
        assert!(!display.contains("Drift"));
    }

    #[test]
    fn test_summary_display_with_duration() {
        let summary = TokenSummary {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
            api_calls: 2,
            estimated_cost: 0.01,
            duration: Some(std::time::Duration::from_secs_f64(5.3)),
            drift: DriftStats::default(),
        };
        let display = format!("{}", summary);
        assert!(display.contains("Duration: 5.3s"));
        // No drift samples, so no drift section
        assert!(!display.contains("Drift"));
    }

    #[test]
    fn test_summary_display_with_drift() {
        let summary = TokenSummary {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
            api_calls: 2,
            estimated_cost: 0.01,
            duration: None,
            drift: DriftStats {
                samples: 4,
                cumulative_drift: 40,
                cumulative_abs_drift: 60,
                max_over: 30,
                max_under: -10,
            },
        };
        let display = format!("{}", summary);
        // avg drift = 40/4 = 10, MAE = 60/4 = 15
        assert!(display.contains("Drift"));
        assert!(display.contains("4 samples"));
    }

    #[test]
    fn test_summary_display_with_both_duration_and_drift() {
        let summary = TokenSummary {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
            api_calls: 2,
            estimated_cost: 0.01,
            duration: Some(std::time::Duration::from_secs(10)),
            drift: DriftStats {
                samples: 2,
                cumulative_drift: -20,
                cumulative_abs_drift: 20,
                max_over: 0,
                max_under: -10,
            },
        };
        let display = format!("{}", summary);
        assert!(display.contains("Duration"));
        assert!(display.contains("Drift"));
    }

    #[test]
    fn test_tracker_record_step_no_tool_name() {
        let tracker = TokenTracker::new();
        tracker.record_step(0, 50, 25, None);
        let steps = tracker.step_usage();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].tool_name, None);
        assert_eq!(steps[0].prompt_tokens, 50);
        assert_eq!(steps[0].completion_tokens, 25);
    }

    #[test]
    fn test_tracker_estimate_cost_zero_tokens() {
        let tracker = TokenTracker::new();
        let cost = tracker.estimate_cost();
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_drift_exact_match() {
        let tracker = TokenTracker::new();
        tracker.record_drift(100, 100);
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 1);
        assert_eq!(drift.cumulative_drift, 0);
        assert_eq!(drift.cumulative_abs_drift, 0);
        assert_eq!(drift.max_over, 0);
        assert_eq!(drift.max_under, 0);
    }

    #[test]
    fn test_drift_zero_actual() {
        // When actual is 0, drift percentage check is skipped (no divide-by-zero)
        let tracker = TokenTracker::new();
        tracker.record_drift(50, 0);
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 1);
        assert_eq!(drift.cumulative_drift, 50);
        assert_eq!(drift.max_over, 50);
    }

    #[test]
    fn test_drift_large_deviation_triggers_log() {
        // >15% deviation with actual > 0: exercises the tracing::warn branch
        let tracker = TokenTracker::new();
        // estimated=200, actual=100 -> 100% deviation
        tracker.record_drift(200, 100);
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 1);
        assert_eq!(drift.cumulative_drift, 100);
    }

    #[test]
    fn test_drift_small_deviation_no_warn() {
        // <15% deviation: does not trigger the warn branch
        let tracker = TokenTracker::new();
        // estimated=105, actual=100 -> 5% deviation
        tracker.record_drift(105, 100);
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 1);
        assert_eq!(drift.cumulative_drift, 5);
    }

    #[test]
    fn test_reset_clears_step_usage() {
        let tracker = TokenTracker::new();
        tracker.record_step(1, 100, 50, Some("tool".to_string()));
        tracker.record_step(2, 200, 100, None);
        assert_eq!(tracker.step_usage().len(), 2);
        tracker.reset();
        assert_eq!(tracker.step_usage().len(), 0);
        assert_eq!(tracker.total_tokens(), 0);
    }

    #[test]
    fn test_reset_clears_drift() {
        let tracker = TokenTracker::new();
        tracker.record_drift(200, 100);
        tracker.record_drift(50, 100);
        assert_eq!(tracker.drift_stats().samples, 2);
        tracker.reset();
        let drift = tracker.drift_stats();
        assert_eq!(drift.samples, 0);
        assert_eq!(drift.cumulative_drift, 0);
        assert_eq!(drift.cumulative_abs_drift, 0);
        assert_eq!(drift.max_over, 0);
        assert_eq!(drift.max_under, 0);
    }

    #[test]
    fn test_model_pricing_calculate_cost_zero_tokens() {
        let pricing = ModelPricing::claude_sonnet();
        let cost = pricing.calculate_cost(0, 0);
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_model_pricing_calculate_cost_large_tokens() {
        let pricing = ModelPricing::claude_haiku();
        // 1M input at 0.00025/1K + 1M output at 0.00125/1K
        // = 1000 * 0.00025 + 1000 * 0.00125 = 0.25 + 1.25 = 1.50
        let cost = pricing.calculate_cost(1_000_000, 1_000_000);
        assert!((cost - 1.50).abs() < 0.001);
    }

    #[test]
    fn test_model_pricing_opus_cost() {
        let pricing = ModelPricing::claude_opus();
        // 10K input at 0.015/1K + 5K output at 0.075/1K
        // = 10 * 0.015 + 5 * 0.075 = 0.15 + 0.375 = 0.525
        let cost = pricing.calculate_cost(10_000, 5_000);
        assert!((cost - 0.525).abs() < 0.0001);
    }
}

#[cfg(test)]
mod extended_model_selector_tests {
    use super::*;

    #[test]
    fn test_selector_auto_select_disabled() {
        let config = ModelSelectionConfig {
            auto_select: false,
            ..Default::default()
        };
        let selector = ModelSelector::new(config);
        // With auto_select disabled, always returns default model
        let model = selector.select(TaskComplexity::Simple, 1000);
        assert_eq!(model, "claude-3-5-sonnet");
        let model = selector.select(TaskComplexity::Critical, 1000);
        assert_eq!(model, "claude-3-5-sonnet");
    }

    #[test]
    fn test_selector_select_standard() {
        let selector = ModelSelector::default();
        let model = selector.select(TaskComplexity::Standard, 1000);
        // Standard falls through to "balance cost and capability" branch,
        // finds tier 2 model
        assert_eq!(model, "claude-3-5-sonnet");
    }

    #[test]
    fn test_selector_select_complex() {
        let selector = ModelSelector::default();
        let model = selector.select(TaskComplexity::Complex, 1000);
        // Complex prefers most capable (highest tier)
        assert_eq!(model, "claude-3-opus");
    }

    #[test]
    fn test_selector_select_no_suitable_models() {
        let config = ModelSelectionConfig {
            models: vec![ModelPricing {
                model_id: "tiny-model".to_string(),
                input_cost_per_1k: 0.0001,
                output_cost_per_1k: 0.0005,
                max_context: 100,
                capability_tier: 1,
                speed_tier: 3,
            }],
            default_model: "fallback".to_string(),
            auto_select: true,
            max_cost_per_request: 0.50,
        };
        let selector = ModelSelector::new(config);
        // Token count exceeds max_context, no suitable models
        let model = selector.select(TaskComplexity::Standard, 50_000);
        assert_eq!(model, "fallback");
    }

    #[test]
    fn test_selector_select_no_tier_match() {
        let config = ModelSelectionConfig {
            models: vec![ModelPricing {
                model_id: "basic-model".to_string(),
                input_cost_per_1k: 0.0001,
                output_cost_per_1k: 0.0005,
                max_context: 200_000,
                capability_tier: 1,
                speed_tier: 3,
            }],
            default_model: "fallback".to_string(),
            auto_select: true,
            max_cost_per_request: 0.50,
        };
        let selector = ModelSelector::new(config);
        // Critical needs tier 3, only have tier 1
        let model = selector.select(TaskComplexity::Critical, 1000);
        assert_eq!(model, "fallback");
    }

    #[test]
    fn test_selector_simple_picks_cheapest() {
        let config = ModelSelectionConfig {
            models: vec![
                ModelPricing {
                    model_id: "expensive".to_string(),
                    input_cost_per_1k: 0.01,
                    output_cost_per_1k: 0.05,
                    max_context: 200_000,
                    capability_tier: 1,
                    speed_tier: 2,
                },
                ModelPricing {
                    model_id: "cheap".to_string(),
                    input_cost_per_1k: 0.001,
                    output_cost_per_1k: 0.005,
                    max_context: 200_000,
                    capability_tier: 1,
                    speed_tier: 3,
                },
            ],
            default_model: "expensive".to_string(),
            auto_select: true,
            max_cost_per_request: 1.0,
        };
        let selector = ModelSelector::new(config);
        let model = selector.select(TaskComplexity::Simple, 1000);
        assert_eq!(model, "cheap");
    }

    #[test]
    fn test_selector_standard_no_tier2_falls_to_first() {
        // If no tier-2 model exists, Standard balance branch takes first suitable
        let config = ModelSelectionConfig {
            models: vec![ModelPricing {
                model_id: "tier3-only".to_string(),
                input_cost_per_1k: 0.01,
                output_cost_per_1k: 0.05,
                max_context: 200_000,
                capability_tier: 3,
                speed_tier: 1,
            }],
            default_model: "fallback".to_string(),
            auto_select: true,
            max_cost_per_request: 1.0,
        };
        let selector = ModelSelector::new(config);
        let model = selector.select(TaskComplexity::Standard, 1000);
        assert_eq!(model, "tier3-only");
    }

    #[test]
    fn test_selector_record_usage_overflow() {
        let selector = ModelSelector::default();
        // Record > 100 entries to exercise the pop_front trimming
        for i in 0..110 {
            selector.record_usage(ModelUsage {
                model_id: format!("model-{}", i),
                complexity: TaskComplexity::Standard,
                input_tokens: 100,
                output_tokens: 50,
                cost: 0.001,
                success: true,
                timestamp: i as u64,
            });
        }
        let summary = selector.usage_summary();
        assert_eq!(summary.total_requests, 100);
    }

    #[test]
    fn test_selector_usage_summary_empty() {
        let selector = ModelSelector::default();
        let summary = selector.usage_summary();
        assert_eq!(summary.total_requests, 0);
        assert_eq!(summary.total_cost, 0.0);
        assert_eq!(summary.total_tokens, 0);
        assert_eq!(summary.success_rate, 0.0);
        assert!(summary.by_model.is_empty());
    }

    #[test]
    fn test_selector_usage_summary_success_rate() {
        let selector = ModelSelector::default();
        for i in 0..10 {
            selector.record_usage(ModelUsage {
                model_id: "claude-3-5-sonnet".to_string(),
                complexity: TaskComplexity::Standard,
                input_tokens: 100,
                output_tokens: 50,
                cost: 0.01,
                success: i < 8, // 8 successes, 2 failures
                timestamp: i as u64,
            });
        }
        let summary = selector.usage_summary();
        assert_eq!(summary.total_requests, 10);
        assert!((summary.success_rate - 0.8).abs() < 0.001);
        assert_eq!(*summary.by_model.get("claude-3-5-sonnet").unwrap(), 10);
    }

    #[test]
    fn test_selector_usage_summary_multiple_models() {
        let selector = ModelSelector::default();
        selector.record_usage(ModelUsage {
            model_id: "model-a".to_string(),
            complexity: TaskComplexity::Simple,
            input_tokens: 100,
            output_tokens: 50,
            cost: 0.01,
            success: true,
            timestamp: 1,
        });
        selector.record_usage(ModelUsage {
            model_id: "model-b".to_string(),
            complexity: TaskComplexity::Complex,
            input_tokens: 200,
            output_tokens: 100,
            cost: 0.05,
            success: true,
            timestamp: 2,
        });
        selector.record_usage(ModelUsage {
            model_id: "model-a".to_string(),
            complexity: TaskComplexity::Standard,
            input_tokens: 150,
            output_tokens: 75,
            cost: 0.02,
            success: false,
            timestamp: 3,
        });

        let summary = selector.usage_summary();
        assert_eq!(summary.total_requests, 3);
        assert!((summary.total_cost - 0.08).abs() < 0.001);
        assert_eq!(summary.total_tokens, 100 + 50 + 200 + 100 + 150 + 75);
        assert_eq!(*summary.by_model.get("model-a").unwrap(), 2);
        assert_eq!(*summary.by_model.get("model-b").unwrap(), 1);
    }

    #[test]
    fn test_recommend_simple() {
        let selector = ModelSelector::default();
        let rec = selector.recommend(TaskComplexity::Simple, 1000);
        assert_eq!(rec.model_id, "claude-3-haiku");
        assert_eq!(rec.reason, "Using faster, cheaper model for simple task");
        assert_eq!(rec.alternative, Some("claude-3-5-sonnet".to_string()));
        assert!(rec.estimated_cost > 0.0);
    }

    #[test]
    fn test_recommend_complex() {
        let selector = ModelSelector::default();
        let rec = selector.recommend(TaskComplexity::Complex, 5000);
        assert_eq!(rec.reason, "Using capable model for complex task");
        assert_eq!(rec.alternative, Some("claude-3-opus".to_string()));
    }

    #[test]
    fn test_recommend_critical() {
        let selector = ModelSelector::default();
        let rec = selector.recommend(TaskComplexity::Critical, 5000);
        assert_eq!(rec.reason, "Using most capable model for critical task");
        assert_eq!(rec.alternative, None);
    }

    #[test]
    fn test_get_alternative_standard() {
        let selector = ModelSelector::default();
        let rec = selector.recommend(TaskComplexity::Standard, 1000);
        assert_eq!(rec.alternative, Some("claude-3-haiku".to_string()));
    }
}

#[cfg(test)]
mod extended_context_pruner_tests {
    use super::*;
    use crate::api::types::Message;

    #[test]
    fn test_pruner_default() {
        let pruner = ContextPruner::default();
        assert!(!pruner.needs_pruning(50_000));
        assert_eq!(pruner.stats().total_operations, 0);
    }

    #[test]
    fn test_pruner_reset_stats() {
        let pruner = ContextPruner::default();
        // Manually can't trigger prune without enough messages to exceed tokens,
        // but we can test reset_stats independently
        pruner.reset_stats();
        let stats = pruner.stats();
        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.tokens_removed, 0);
        assert_eq!(stats.messages_removed, 0);
        assert!((stats.cost_saved - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_prune_no_pruning_needed() {
        let config = PruningConfig {
            target_tokens: 1_000_000, // very high target
            strategy: PruningStrategy::KeepRecent,
            min_messages: 2,
            keep_system: true,
            keep_last_n: 2,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("System prompt"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];
        let result = pruner.prune(&messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_prune_keep_recent_strategy() {
        let config = PruningConfig {
            target_tokens: 1, // force pruning
            strategy: PruningStrategy::KeepRecent,
            min_messages: 1,
            keep_system: true,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("First question"),
            Message::assistant("First answer"),
            Message::user("Second question"),
            Message::assistant("Second answer"),
        ];
        let result = pruner.prune(&messages);
        // Should keep system message + last 1 message at minimum
        assert!(result.len() >= 2);
        // System message should be first
        assert_eq!(result[0].role, "system");
        // stats should be updated
        let stats = pruner.stats();
        assert_eq!(stats.total_operations, 1);
    }

    #[test]
    fn test_prune_keep_ends_strategy() {
        let config = PruningConfig {
            target_tokens: 1, // force pruning
            strategy: PruningStrategy::KeepEnds,
            min_messages: 2,
            keep_system: true,
            keep_last_n: 2,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("System prompt"),
            Message::user("First question"),
            Message::assistant("First answer"),
            Message::user("Middle question"),
            Message::assistant("Middle answer"),
            Message::user("Last question"),
            Message::assistant("Last answer"),
        ];
        let result = pruner.prune(&messages);
        // KeepEnds: pushes first, then last N in reverse, then reverses all.
        // Result = [Last question, Last answer, System prompt]
        assert_eq!(result.len(), 3);
        // After the reversal, the last 2 come first, then system at end
        assert_eq!(result[0].content.text(), "Last question");
        assert_eq!(result[1].content.text(), "Last answer");
        assert_eq!(result[2].role, "system");
        // Stats should be updated
        let stats = pruner.stats();
        assert_eq!(stats.total_operations, 1);
    }

    #[test]
    fn test_prune_keep_ends_few_messages() {
        let config = PruningConfig {
            target_tokens: 1, // force pruning
            strategy: PruningStrategy::KeepEnds,
            min_messages: 10, // more than messages.len()
            keep_system: true,
            keep_last_n: 2,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("System"),
            Message::user("Hello"),
            Message::assistant("Hi"),
        ];
        // len <= min_messages, so returns all
        let result = pruner.prune(&messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_prune_remove_tool_results_strategy() {
        let config = PruningConfig {
            target_tokens: 1, // force pruning
            strategy: PruningStrategy::RemoveToolResults,
            min_messages: 1,
            keep_system: true,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("System"),
            Message::user("Run a command"),
            Message::assistant("Sure"),
            Message::tool("command output", "call_1"),
            Message::assistant("Done"),
        ];
        let result = pruner.prune(&messages);
        // Tool messages should be removed
        assert!(result.iter().all(|m| m.role != "tool"));
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_prune_remove_system_messages_strategy() {
        let config = PruningConfig {
            target_tokens: 1, // force pruning
            strategy: PruningStrategy::RemoveSystemMessages,
            min_messages: 1,
            keep_system: true,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("First system"),
            Message::user("Hello"),
            Message::system("Second system"),
            Message::assistant("Hi"),
            Message::system("Third system"),
        ];
        let result = pruner.prune(&messages);
        // Only first system should remain
        let system_count = result.iter().filter(|m| m.role == "system").count();
        assert_eq!(system_count, 1);
        assert_eq!(result[0].role, "system");
    }

    #[test]
    fn test_prune_fallback_strategies() {
        // ByRelevance and Summarize fall through to KeepRecent
        for strategy in [PruningStrategy::ByRelevance, PruningStrategy::Summarize] {
            let config = PruningConfig {
                target_tokens: 1,
                strategy,
                min_messages: 1,
                keep_system: false,
                keep_last_n: 1,
            };
            let pruner = ContextPruner::new(config);
            let messages = vec![
                Message::user("Hello"),
                Message::assistant("Hi"),
                Message::user("Bye"),
            ];
            let result = pruner.prune(&messages);
            // Should not panic, and should return some messages
            assert!(!result.is_empty());
        }
    }

    #[test]
    fn test_prune_keep_recent_no_system() {
        let config = PruningConfig {
            target_tokens: 1,
            strategy: PruningStrategy::KeepRecent,
            min_messages: 1,
            keep_system: false,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::user("First"),
            Message::assistant("First reply"),
            Message::user("Second"),
            Message::assistant("Second reply"),
        ];
        let result = pruner.prune(&messages);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_prune_updates_stats_correctly() {
        let config = PruningConfig {
            target_tokens: 1,
            strategy: PruningStrategy::RemoveToolResults,
            min_messages: 1,
            keep_system: true,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::system("System"),
            Message::user("Do something"),
            Message::tool("result", "call_1"),
            Message::assistant("Done"),
        ];
        pruner.prune(&messages);
        let stats = pruner.stats();
        assert_eq!(stats.total_operations, 1);
        assert!(stats.messages_removed > 0);
    }

    #[test]
    fn test_prune_empty_messages() {
        let config = PruningConfig {
            target_tokens: 1,
            strategy: PruningStrategy::KeepRecent,
            min_messages: 1,
            keep_system: true,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages: Vec<Message> = vec![];
        let result = pruner.prune(&messages);
        // No pruning needed since 0 tokens < target
        assert!(result.is_empty());
    }
}

#[cfg(test)]
mod extended_budget_manager_tests {
    use super::*;

    #[test]
    fn test_budget_can_spend_no_hard_limit() {
        let config = BudgetConfig {
            hard_limit: false,
            daily_budget: 5.0,
            ..Default::default()
        };
        let manager = BudgetManager::new(config);
        manager.record_spending(100.0); // Way over budget
                                        // Without hard limit, can always spend
        assert!(manager.can_spend(1000.0));
    }

    #[test]
    fn test_budget_monthly_spending_and_remaining() {
        let manager = BudgetManager::default();
        manager.record_spending(20.0);
        assert!((manager.monthly_spending() - 20.0).abs() < 0.001);
        assert!((manager.monthly_remaining() - 80.0).abs() < 0.001);
    }

    #[test]
    fn test_budget_daily_exceeded_alert() {
        let config = BudgetConfig {
            daily_budget: 10.0,
            monthly_budget: 100.0,
            alert_threshold: 0.8,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        // Spend 11.0 > daily budget of 10.0 → should trigger DailyExceeded
        manager.record_spending(11.0);
        let alerts = manager.alerts();
        assert!(!alerts.is_empty());
        let daily_exceeded = alerts
            .iter()
            .any(|a| a.alert_type == BudgetAlertType::DailyExceeded);
        assert!(daily_exceeded);
    }

    #[test]
    fn test_budget_daily_warning_alert() {
        let config = BudgetConfig {
            daily_budget: 10.0,
            monthly_budget: 100.0,
            alert_threshold: 0.8,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        // Spend 8.5 → 85% of 10.0, above threshold but below budget
        manager.record_spending(8.5);
        let alerts = manager.alerts();
        let daily_warning = alerts
            .iter()
            .any(|a| a.alert_type == BudgetAlertType::DailyWarning);
        assert!(daily_warning);
    }

    #[test]
    fn test_budget_monthly_exceeded_alert() {
        let config = BudgetConfig {
            daily_budget: 1000.0, // High daily to avoid daily alerts
            monthly_budget: 50.0,
            alert_threshold: 0.8,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        manager.record_spending(55.0);
        let alerts = manager.alerts();
        let monthly_exceeded = alerts
            .iter()
            .any(|a| a.alert_type == BudgetAlertType::MonthlyExceeded);
        assert!(monthly_exceeded);
    }

    #[test]
    fn test_budget_monthly_warning_alert() {
        let config = BudgetConfig {
            daily_budget: 1000.0,
            monthly_budget: 100.0,
            alert_threshold: 0.8,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        // 85% of 100 = 85
        manager.record_spending(85.0);
        let alerts = manager.alerts();
        let monthly_warning = alerts
            .iter()
            .any(|a| a.alert_type == BudgetAlertType::MonthlyWarning);
        assert!(monthly_warning);
    }

    #[test]
    fn test_budget_no_alert_below_threshold() {
        let config = BudgetConfig {
            daily_budget: 100.0,
            monthly_budget: 1000.0,
            alert_threshold: 0.8,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        // Spend only 10% of daily/monthly
        manager.record_spending(10.0);
        let alerts = manager.alerts();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_budget_hard_limit_blocks_monthly() {
        let config = BudgetConfig {
            daily_budget: 1000.0,
            monthly_budget: 5.0,
            alert_threshold: 0.8,
            hard_limit: true,
        };
        let manager = BudgetManager::new(config);
        manager.record_spending(4.0);
        // 4.0 + 2.0 = 6.0 > monthly budget 5.0
        assert!(!manager.can_spend(2.0));
        assert!(manager.can_spend(0.5));
    }

    #[test]
    fn test_budget_status_full() {
        let config = BudgetConfig {
            daily_budget: 20.0,
            monthly_budget: 200.0,
            alert_threshold: 0.8,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        manager.record_spending(5.0);
        let status = manager.status();
        assert!((status.daily_spent - 5.0).abs() < 0.001);
        assert_eq!(status.daily_budget, 20.0);
        assert!((status.daily_remaining - 15.0).abs() < 0.001);
        assert!((status.monthly_spent - 5.0).abs() < 0.001);
        assert_eq!(status.monthly_budget, 200.0);
        assert!((status.monthly_remaining - 195.0).abs() < 0.001);
    }

    #[test]
    fn test_budget_daily_remaining_saturates_at_zero() {
        let manager = BudgetManager::default(); // daily_budget = 10.0
        manager.record_spending(15.0);
        assert_eq!(manager.daily_remaining(), 0.0);
    }

    #[test]
    fn test_budget_monthly_remaining_saturates_at_zero() {
        let manager = BudgetManager::default(); // monthly_budget = 100.0
        manager.record_spending(150.0);
        assert_eq!(manager.monthly_remaining(), 0.0);
    }

    #[test]
    fn test_budget_multiple_spending_records() {
        let manager = BudgetManager::default();
        manager.record_spending(1.0);
        manager.record_spending(2.0);
        manager.record_spending(3.0);
        assert!((manager.daily_spending() - 6.0).abs() < 0.001);
        assert!((manager.monthly_spending() - 6.0).abs() < 0.001);
    }

    #[test]
    fn test_budget_alert_message_format() {
        let config = BudgetConfig {
            daily_budget: 10.0,
            monthly_budget: 100.0,
            alert_threshold: 0.5,
            hard_limit: false,
        };
        let manager = BudgetManager::new(config);
        manager.record_spending(6.0); // 60% of daily
        let alerts = manager.alerts();
        assert!(!alerts.is_empty());
        let alert = &alerts[0];
        assert!(alert.message.contains("budget at"));
        assert_eq!(alert.threshold, 0.5);
        assert!((alert.current_usage - 6.0).abs() < 0.001);
    }
}

#[cfg(test)]
mod extended_cost_optimizer_tests {
    use super::*;

    #[test]
    fn test_cost_optimizer_new_custom_configs() {
        let optimizer = CostOptimizer::new(
            PruningConfig {
                target_tokens: 50_000,
                ..Default::default()
            },
            ModelSelectionConfig::default(),
            BudgetConfig {
                daily_budget: 5.0,
                ..Default::default()
            },
        );
        assert_eq!(optimizer.tracker().total_tokens(), 0);
        assert!(optimizer.pruner().needs_pruning(60_000));
        assert!(!optimizer.pruner().needs_pruning(40_000));
    }

    #[test]
    fn test_cost_optimizer_components_interact() {
        let optimizer = CostOptimizer::default();
        optimizer.tracker().record_usage(5000, 2000);
        optimizer.budget().record_spending(0.05);

        let summary = optimizer.summary();
        assert_eq!(summary.token_summary.prompt_tokens, 5000);
        assert_eq!(summary.token_summary.completion_tokens, 2000);
        assert!(summary.budget_status.daily_spent > 0.0);
    }

    #[test]
    fn test_cost_optimizer_recommendations_budget_nearly_exhausted() {
        let optimizer = CostOptimizer::new(
            PruningConfig::default(),
            ModelSelectionConfig::default(),
            BudgetConfig {
                daily_budget: 10.0,
                monthly_budget: 100.0,
                alert_threshold: 0.8,
                hard_limit: false,
            },
        );
        // Spend 9.0 out of 10.0, leaving only 10% remaining
        optimizer.budget().record_spending(9.0);

        let recommendations = optimizer.get_recommendations();
        let budget_rec = recommendations
            .iter()
            .any(|r| r.category == "Budget" && r.message.contains("nearly exhausted"));
        assert!(budget_rec);
    }

    #[test]
    fn test_cost_optimizer_no_recommendations_fresh() {
        let optimizer = CostOptimizer::default();
        let recommendations = optimizer.get_recommendations();
        // With fresh state, budget is full → no budget recommendation
        // No pruning or model usage → no other recommendations
        assert!(
            recommendations.is_empty(),
            "Expected no recommendations for fresh optimizer, got: {:?}",
            recommendations
        );
    }

    #[test]
    fn test_cost_optimizer_summary_fields() {
        let optimizer = CostOptimizer::default();
        optimizer.tracker().record_usage(1000, 500);
        optimizer.tracker().record_drift(1100, 1000);

        let summary = optimizer.summary();
        assert_eq!(summary.token_summary.total_tokens, 1500);
        assert_eq!(summary.token_summary.api_calls, 1);
        assert_eq!(summary.pruning_stats.total_operations, 0);
        assert_eq!(summary.model_usage.total_requests, 0);
        assert_eq!(summary.token_summary.drift.samples, 1);
    }
}
