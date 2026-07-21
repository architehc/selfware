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
    /// Model identifier for cost estimation
    model_id: RwLock<Option<String>>,
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
            model_id: RwLock::new(None),
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
            cost_basis: self.cost_basis_label(),
            duration: self.session_duration(),
            drift: self.drift_stats(),
        }
    }

    /// Set the model identifier for cost estimation
    pub fn set_model_id(&self, model_id: impl Into<String>) {
        if let Ok(mut id) = self.model_id.write() {
            *id = Some(model_id.into());
        }
    }

    /// Get the model identifier if set
    pub fn get_model_id(&self) -> Option<String> {
        self.model_id.read().ok().and_then(|id| id.clone())
    }

    /// Get pricing information based on model_id
    /// Returns model-specific pricing if known, otherwise returns default (sonnet) pricing
    fn get_model_pricing(&self) -> ModelPricing {
        if let Ok(model_id_guard) = self.model_id.read() {
            if let Some(ref model_id) = *model_id_guard {
                if let Some(pricing) = known_pricing_for_model(model_id) {
                    return pricing;
                }
                // Unknown model: fall through to the labelled default.
            }
        }
        // Default fallback to sonnet pricing (matches original hardcoded
        // values). Callers can see this is a fallback via `cost_basis_label`.
        ModelPricing::claude_sonnet()
    }

    /// Describe the rate card behind [`Self::estimate_cost`]: the preset whose
    /// rates are actually used when the configured model has known pricing, or
    /// an explicit note that the figure is a sonnet-based guess. Surfaced in
    /// the summary so a cost number is never presented as exact when the
    /// model's real rates are unknown.
    pub fn cost_basis_label(&self) -> String {
        if let Ok(model_id_guard) = self.model_id.read() {
            if let Some(ref model_id) = *model_id_guard {
                if let Some(pricing) = known_pricing_for_model(model_id) {
                    return format!("rates for {}", pricing.model_id);
                }
                return format!("unknown rates for '{model_id}' — claude-3-5-sonnet estimate");
            }
        }
        "no model set — claude-3-5-sonnet estimate".to_string()
    }

    /// Estimate cost based on model-specific pricing
    /// Uses the model_id to determine appropriate pricing, falling back to
    /// default values if model_id is not set or unknown.
    pub fn estimate_cost(&self) -> f64 {
        let prompt = self.total_prompt_tokens() as f64;
        let completion = self.total_completion_tokens() as f64;

        let pricing = self.get_model_pricing();

        // Convert from per-1K pricing to per-token
        let prompt_cost = (prompt / 1000.0) * pricing.input_cost_per_1k;
        let completion_cost = (completion / 1000.0) * pricing.output_cost_per_1k;

        prompt_cost + completion_cost
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
    /// Which rate card `estimated_cost` was computed with — e.g. "rates for
    /// z-ai/glm-5.2", or a fallback note when the model's rates are unknown.
    /// Keeps the estimate honest instead of silently applying one model's
    /// pricing to every model.
    pub cost_basis: String,
    pub duration: Option<std::time::Duration>,
    pub drift: DriftStats,
}

impl std::fmt::Display for TokenSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tokens: {} (prompt: {}, completion: {}) | API calls: {} | Est. cost: ${:.4} ({})",
            self.total_tokens,
            self.prompt_tokens,
            self.completion_tokens,
            self.api_calls,
            self.estimated_cost,
            self.cost_basis
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

/// Default token estimate per image when dimensions are unknown.
/// Based on a 1024×1024 high-detail image: 4 tiles × 170 + 85 = 765.
pub const DEFAULT_IMAGE_TOKEN_ESTIMATE: usize = 765;

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
        // Content (use text_all to capture all text blocks)
        total += estimate_tokens(&msg.content.text_all());
        // Image tokens
        total += msg.content.image_count() * DEFAULT_IMAGE_TOKEN_ESTIMATE;
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

/// Estimate the token count of tool definitions sent via native function calling.
/// vLLM/OpenAI count these as input tokens, so they must be included in budget calculations.
pub fn estimate_tool_definitions_tokens(tools: &[crate::api::types::ToolDefinition]) -> usize {
    let mut total = 0;
    for tool in tools {
        // Each tool definition has overhead + name + description + parameter schema
        total += 10; // structural overhead (type, function wrapper)
        total += estimate_tokens(&tool.function.name);
        total += estimate_tokens(&tool.function.description);
        // Parameter schema JSON — serialize and estimate
        let schema_str = serde_json::to_string(&tool.function.parameters).unwrap_or_default();
        total += estimate_tokens(&schema_str);
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

    /// GLM-5.2 via OpenRouter (`z-ai/glm-5.2`) — the shipped default model.
    /// Rates from the z.ai list price carried by OpenRouter: $1.40 / 1M input
    /// tokens, $4.40 / 1M output tokens.
    pub fn glm_5_2() -> Self {
        Self {
            model_id: "z-ai/glm-5.2".to_string(),
            input_cost_per_1k: 0.0014,
            output_cost_per_1k: 0.0044,
            max_context: 1_000_000,
            capability_tier: 2,
            speed_tier: 2,
        }
    }
}

/// Look up the known rate card for a model id (case-insensitive substring
/// match against the small built-in map). Returns `None` when the model's
/// real rates are unknown — callers must not silently bill it at another
/// model's rates without saying so.
fn known_pricing_for_model(model_id: &str) -> Option<ModelPricing> {
    let lower = model_id.to_lowercase();
    if lower.contains("haiku") {
        Some(ModelPricing::claude_haiku())
    } else if lower.contains("opus") {
        Some(ModelPricing::claude_opus())
    } else if lower.contains("sonnet") {
        Some(ModelPricing::claude_sonnet())
    } else if lower.contains("glm-5.2") {
        Some(ModelPricing::glm_5_2())
    } else {
        None
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

        Self::ensure_single_system_first(result)
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
        Self::ensure_single_system_first(result)
    }

    /// Ensure the message vector has exactly one system message and it is
    /// in the first position.  Any extra system messages are removed, and
    /// the single system message (if any) is moved to the front.
    fn ensure_single_system_first(
        mut messages: Vec<crate::api::types::Message>,
    ) -> Vec<crate::api::types::Message> {
        if let Some(idx) = messages.iter().position(|m| m.role == "system") {
            let system_msg = messages.remove(idx);
            messages.retain(|m| m.role != "system");
            messages.insert(0, system_msg);
        }
        messages
    }

    /// Remove tool results but keep tool calls
    fn prune_tool_results(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        let result: Vec<crate::api::types::Message> = messages
            .iter()
            .filter(|msg| msg.role != "tool")
            .cloned()
            .collect();
        Self::ensure_single_system_first(result)
    }

    /// Remove system messages except first
    fn prune_system_messages(
        &self,
        messages: &[crate::api::types::Message],
    ) -> Vec<crate::api::types::Message> {
        let mut first_system = true;
        let result: Vec<crate::api::types::Message> = messages
            .iter()
            .filter(|msg| {
                if msg.role == "system" {
                    std::mem::take(&mut first_system)
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        Self::ensure_single_system_first(result)
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
#[path = "../tests/unit/tokens/tokens_test.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_model_pricing_test.rs"]
mod model_pricing_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_context_pruner_test.rs"]
mod context_pruner_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_model_selector_test.rs"]
mod model_selector_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_budget_manager_test.rs"]
mod budget_manager_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_cost_optimizer_test.rs"]
mod cost_optimizer_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_extended_model_selector_test.rs"]
mod extended_model_selector_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_extended_context_pruner_test.rs"]
mod extended_context_pruner_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_extended_budget_manager_test.rs"]
mod extended_budget_manager_tests;

#[cfg(test)]
#[path = "../tests/unit/tokens/tokens_extended_cost_optimizer_test.rs"]
mod extended_cost_optimizer_tests;
