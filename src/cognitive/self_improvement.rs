//! Self-Improvement Engine
//!
//! Meta-programming capabilities for agent self-improvement:
//! - Prompt optimization from effectiveness feedback
//! - Tool selection learning from outcomes
//! - Error pattern avoidance through learning
//! - Usage pattern analysis

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

// Decision: keep std::sync::RwLock. All public methods are synchronous.
// If async methods are added later, consider parking_lot::RwLock first (no poisoning)
// and only move to tokio::sync::RwLock if guards must be held across await points.

/// Global limit for record vectors to prevent unbounded memory growth in long-running sessions.
const MAX_ENTRIES: usize = 10_000;

/// Outcome of a task or action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Outcome {
    /// Task completed successfully
    Success,
    /// Task partially completed
    Partial,
    /// Task failed
    Failure,
    /// Task was abandoned
    Abandoned,
}

impl Outcome {
    /// Score for learning (0-1)
    pub fn score(&self) -> f32 {
        match self {
            Self::Success => 1.0,
            Self::Partial => 0.5,
            Self::Failure => 0.0,
            Self::Abandoned => 0.0,
        }
    }

    /// Is this a positive outcome?
    pub fn is_positive(&self) -> bool {
        matches!(self, Self::Success | Self::Partial)
    }
}

/// Record of a prompt's effectiveness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRecord {
    /// Original prompt text
    pub prompt: String,
    /// Task type it was used for
    pub task_type: String,
    /// Outcome
    pub outcome: Outcome,
    /// Response quality score (0-1)
    pub quality_score: f32,
    /// Token count used
    pub tokens_used: usize,
    /// Response time (ms)
    pub response_time_ms: u64,
    /// Timestamp
    pub timestamp: u64,
}

impl PromptRecord {
    pub fn new(prompt: String, task_type: String, outcome: Outcome) -> Self {
        Self {
            prompt,
            task_type,
            outcome,
            quality_score: outcome.score(),
            tokens_used: 0,
            response_time_ms: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_quality(mut self, score: f32) -> Self {
        self.quality_score = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_tokens(mut self, tokens: usize) -> Self {
        self.tokens_used = tokens;
        self
    }

    pub fn with_response_time(mut self, time_ms: u64) -> Self {
        self.response_time_ms = time_ms;
        self
    }
}

/// Prompt pattern for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPattern {
    /// Pattern identifier
    pub id: String,
    /// Template with placeholders
    pub template: String,
    /// Task types this pattern works well for
    pub effective_for: Vec<String>,
    /// Average quality score
    pub avg_quality: f32,
    /// Usage count
    pub usage_count: usize,
    /// Success rate
    pub success_rate: f32,
}

impl PromptPattern {
    pub fn new(id: &str, template: &str) -> Self {
        Self {
            id: id.to_string(),
            template: template.to_string(),
            effective_for: Vec::new(),
            avg_quality: 0.0,
            usage_count: 0,
            success_rate: 0.0,
        }
    }

    /// Update pattern stats with new outcome
    pub fn update(&mut self, outcome: Outcome, quality: f32) {
        let old_total = self.usage_count as f32 * self.avg_quality;
        self.usage_count += 1;
        self.avg_quality = (old_total + quality) / self.usage_count as f32;

        let previous_count = self.usage_count.saturating_sub(1);
        let old_success = if self.success_rate.is_finite() {
            (self.success_rate.clamp(0.0, 1.0) * previous_count as f32).round() as usize
        } else {
            0
        };
        let new_success = if outcome.is_positive() {
            old_success + 1
        } else {
            old_success
        };
        self.success_rate = new_success as f32 / self.usage_count as f32;
    }
}

/// Optimizer for prompts
pub struct PromptOptimizer {
    /// Recorded prompt outcomes
    records: Vec<PromptRecord>,
    /// Known effective patterns
    patterns: HashMap<String, PromptPattern>,
    /// Task type statistics
    task_stats: HashMap<String, TaskPromptStats>,
    /// Maximum records to keep
    max_records: usize,
}

/// Stats for prompts by task type
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskPromptStats {
    pub total_attempts: usize,
    pub successful: usize,
    pub avg_quality: f32,
    pub avg_tokens: f32,
    pub best_patterns: Vec<String>,
}

/// A scored prompt variant in an A/B tournament.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVariantScore {
    pub variant_id: String,
    pub strategy: String,
    pub prompt: String,
    pub predicted_quality: f32,
}

/// Result of an automated prompt evolution tournament.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTournamentResult {
    pub task_type: String,
    pub winner_prompt: String,
    pub winner_strategy: String,
    pub winner_score: f32,
    pub variants: Vec<PromptVariantScore>,
}

impl PromptOptimizer {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            patterns: HashMap::new(),
            task_stats: HashMap::new(),
            max_records: MAX_ENTRIES,
        }
    }

    /// Record a prompt outcome
    pub fn record(&mut self, record: PromptRecord) {
        // Update task stats
        let stats = self.task_stats.entry(record.task_type.clone()).or_default();
        let old_total_quality = stats.avg_quality * stats.total_attempts as f32;
        let old_total_tokens = stats.avg_tokens * stats.total_attempts as f32;
        stats.total_attempts += 1;
        if record.outcome.is_positive() {
            stats.successful += 1;
        }
        stats.avg_quality =
            (old_total_quality + record.quality_score) / stats.total_attempts as f32;
        stats.avg_tokens =
            (old_total_tokens + record.tokens_used as f32) / stats.total_attempts as f32;

        // Trim task_stats if it grows too large
        if self.task_stats.len() > MAX_ENTRIES / 10 {
            let mut entries: Vec<_> = self
                .task_stats
                .iter()
                .map(|(k, v)| (k.clone(), v.total_attempts))
                .collect();
            entries.sort_by_key(|(_, count)| *count);
            for (key, _) in entries.iter().take(entries.len() / 2) {
                self.task_stats.remove(key);
            }
        }

        // Store record
        self.records.push(record);

        // Trim if needed — remove oldest entries
        if self.records.len() > self.max_records {
            self.records.drain(0..self.max_records / 2);
        }
    }

    /// Register a prompt pattern
    pub fn register_pattern(&mut self, pattern: PromptPattern) {
        self.patterns.insert(pattern.id.clone(), pattern);
    }

    /// Update a pattern with new outcome
    pub fn update_pattern(&mut self, pattern_id: &str, outcome: Outcome, quality: f32) {
        if let Some(pattern) = self.patterns.get_mut(pattern_id) {
            pattern.update(outcome, quality);
        }
    }

    /// Get best patterns for a task type
    pub fn best_patterns_for(&self, task_type: &str) -> Vec<&PromptPattern> {
        let mut patterns: Vec<_> = self
            .patterns
            .values()
            .filter(|p| {
                p.effective_for.contains(&task_type.to_string()) || p.effective_for.is_empty()
            })
            .filter(|p| p.usage_count >= 5) // Minimum usage for reliability
            .collect();

        patterns.sort_by(|a, b| {
            b.avg_quality
                .partial_cmp(&a.avg_quality)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        patterns.into_iter().take(5).collect()
    }

    /// Get task stats
    pub fn get_task_stats(&self, task_type: &str) -> Option<&TaskPromptStats> {
        self.task_stats.get(task_type)
    }

    /// Suggest prompt improvements based on patterns
    pub fn suggest_improvements(&self, prompt: &str, task_type: &str) -> Vec<PromptSuggestion> {
        let mut suggestions = Vec::new();

        // Check for common issues
        if prompt.len() < 20 {
            suggestions.push(PromptSuggestion {
                suggestion_type: SuggestionType::AddContext,
                description: "Prompt may be too short. Consider adding more context.".to_string(),
                example: None,
            });
        }

        if !prompt.contains("please") && !prompt.contains("should") && !prompt.contains("must") {
            suggestions.push(PromptSuggestion {
                suggestion_type: SuggestionType::ClarifyIntent,
                description: "Consider using clearer directive words.".to_string(),
                example: Some("Please implement...".to_string()),
            });
        }

        // Suggest best patterns for this task type
        let best_patterns = self.best_patterns_for(task_type);
        for pattern in best_patterns.iter().take(2) {
            suggestions.push(PromptSuggestion {
                suggestion_type: SuggestionType::UsePattern,
                description: format!(
                    "Pattern '{}' has {:.0}% success rate for this task type",
                    pattern.id,
                    pattern.success_rate * 100.0
                ),
                example: Some(pattern.template.clone()),
            });
        }

        suggestions
    }

    /// Run an A/B tournament with mutated variants and return the winner.
    pub fn evolve_prompt(
        &mut self,
        task_type: &str,
        baseline_prompt: &str,
    ) -> PromptTournamentResult {
        let variants = self.generate_prompt_variants(task_type, baseline_prompt);
        let mut scored: Vec<PromptVariantScore> = variants
            .into_iter()
            .map(|(id, strategy, prompt)| PromptVariantScore {
                variant_id: id,
                strategy,
                predicted_quality: self.estimate_prompt_quality(task_type, &prompt),
                prompt,
            })
            .collect();

        scored.sort_by(|a, b| {
            b.predicted_quality
                .partial_cmp(&a.predicted_quality)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let winner = scored.first().cloned().unwrap_or(PromptVariantScore {
            variant_id: "baseline".to_string(),
            strategy: "baseline".to_string(),
            prompt: baseline_prompt.to_string(),
            predicted_quality: self.estimate_prompt_quality(task_type, baseline_prompt),
        });

        // Persist the winner as a learned pattern so future tournaments have memory.
        let pattern_id = format!("evo-{}-{}", task_type, winner.variant_id);
        let mut pattern = PromptPattern::new(&pattern_id, &winner.prompt);
        pattern.effective_for.push(task_type.to_string());
        pattern.usage_count = 1;
        pattern.avg_quality = winner.predicted_quality;
        pattern.success_rate = winner.predicted_quality;
        self.register_pattern(pattern);

        PromptTournamentResult {
            task_type: task_type.to_string(),
            winner_prompt: winner.prompt,
            winner_strategy: winner.strategy,
            winner_score: winner.predicted_quality,
            variants: scored,
        }
    }

    fn generate_prompt_variants(
        &self,
        task_type: &str,
        baseline_prompt: &str,
    ) -> Vec<(String, String, String)> {
        let mut variants = vec![
            (
                "baseline".to_string(),
                "baseline".to_string(),
                baseline_prompt.to_string(),
            ),
            (
                "verify_first".to_string(),
                "add_verification_clause".to_string(),
                format!(
                    "{}\n\nBefore finalizing, explicitly verify assumptions and list validation checks.",
                    baseline_prompt
                ),
            ),
            (
                "structured_output".to_string(),
                "force_structure".to_string(),
                format!(
                    "{}\n\nRespond in sections: Plan, Changes, Verification, Risks.",
                    baseline_prompt
                ),
            ),
            (
                "concise_steps".to_string(),
                "concise_step_plan".to_string(),
                format!(
                    "{}\n\nKeep each step concise and actionable; avoid redundant narration.",
                    baseline_prompt
                ),
            ),
        ];

        for (idx, pattern) in self.best_patterns_for(task_type).iter().take(2).enumerate() {
            let rendered = Self::render_pattern_template(&pattern.template);
            variants.push((
                format!("pattern_{}", idx + 1),
                format!("pattern_{}", pattern.id),
                rendered,
            ));
        }

        // Deduplicate by normalized text while keeping order.
        let mut seen = std::collections::HashSet::new();
        variants
            .into_iter()
            .filter(|(_, _, prompt)| seen.insert(Self::normalize_prompt(prompt)))
            .collect()
    }

    fn render_pattern_template(template: &str) -> String {
        template
            .replace("{action}", "complete the requested task")
            .replace("{target}", "codebase")
    }

    fn estimate_prompt_quality(&self, task_type: &str, prompt: &str) -> f32 {
        let norm = Self::normalize_prompt(prompt);
        let matching: Vec<&PromptRecord> = self
            .records
            .iter()
            .filter(|r| r.task_type == task_type && Self::normalize_prompt(&r.prompt) == norm)
            .collect();

        let historical = if matching.is_empty() {
            self.task_stats
                .get(task_type)
                .map(|s| s.avg_quality)
                .unwrap_or(0.5)
        } else {
            matching.iter().map(|r| r.quality_score).sum::<f32>() / matching.len() as f32
        };

        // Structural priors for unseen prompts.
        let mut score = historical;
        let lower = prompt.to_lowercase();
        if lower.contains("verify") || lower.contains("validation") {
            score += 0.05;
        }
        if lower.contains("step") || lower.contains("plan") {
            score += 0.03;
        }
        if (200..=1800).contains(&prompt.len()) {
            score += 0.02;
        }

        score.clamp(0.0, 1.0)
    }

    fn normalize_prompt(prompt: &str) -> String {
        prompt
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// Get overall statistics
    pub fn get_stats(&self) -> PromptOptimizerStats {
        let total_records = self.records.len();
        let successful = self
            .records
            .iter()
            .filter(|r| r.outcome.is_positive())
            .count();
        let avg_quality = if total_records > 0 {
            self.records.iter().map(|r| r.quality_score).sum::<f32>() / total_records as f32
        } else {
            0.0
        };

        PromptOptimizerStats {
            total_records,
            successful_records: successful,
            pattern_count: self.patterns.len(),
            task_types_tracked: self.task_stats.len(),
            avg_quality,
        }
    }
}

impl PromptOptimizer {
    fn to_snapshot(&self) -> PromptOptimizerSnapshot {
        PromptOptimizerSnapshot {
            records: self.records.clone(),
            patterns: self.patterns.clone(),
            task_stats: self.task_stats.clone(),
        }
    }

    fn from_snapshot(snapshot: PromptOptimizerSnapshot) -> Self {
        Self {
            records: snapshot.records,
            patterns: snapshot.patterns,
            task_stats: snapshot.task_stats,
            max_records: MAX_ENTRIES,
        }
    }
}

impl Default for PromptOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Prompt improvement suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSuggestion {
    pub suggestion_type: SuggestionType,
    pub description: String,
    pub example: Option<String>,
}

/// Type of prompt suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionType {
    AddContext,
    ClarifyIntent,
    UsePattern,
    SimplifyPrompt,
    AddExamples,
}

/// Serializable snapshot of PromptOptimizer state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptOptimizerSnapshot {
    records: Vec<PromptRecord>,
    patterns: HashMap<String, PromptPattern>,
    task_stats: HashMap<String, TaskPromptStats>,
}

/// Stats for prompt optimizer
#[derive(Debug, Clone)]
pub struct PromptOptimizerStats {
    pub total_records: usize,
    pub successful_records: usize,
    pub pattern_count: usize,
    pub task_types_tracked: usize,
    pub avg_quality: f32,
}

/// Record of a tool usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageRecord {
    /// Tool name
    pub tool: String,
    /// Task context
    pub task_context: String,
    /// Outcome
    pub outcome: Outcome,
    /// Execution time (ms)
    pub execution_time_ms: u64,
    /// Error message (if any)
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl ToolUsageRecord {
    pub fn new(tool: String, task_context: String, outcome: Outcome) -> Self {
        Self {
            tool,
            task_context,
            outcome,
            execution_time_ms: 0,
            error: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_execution_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = time_ms;
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }
}

/// Stats for a tool
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolStats {
    pub usage_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub avg_execution_time_ms: f64,
    pub effective_contexts: Vec<String>,
    pub common_errors: HashMap<String, usize>,
}

impl ToolStats {
    pub fn success_rate(&self) -> f32 {
        if self.usage_count == 0 {
            0.0
        } else {
            self.success_count as f32 / self.usage_count as f32
        }
    }
}

/// Learner for tool selection
pub struct ToolSelectionLearner {
    /// Tool usage records
    records: Vec<ToolUsageRecord>,
    /// Stats per tool
    tool_stats: HashMap<String, ToolStats>,
    /// Context -> tool effectiveness mapping
    context_tools: HashMap<String, Vec<(String, f32)>>,
    /// Maximum records
    max_records: usize,
}

impl ToolSelectionLearner {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            tool_stats: HashMap::new(),
            context_tools: HashMap::new(),
            max_records: MAX_ENTRIES,
        }
    }

    /// Record a tool usage
    pub fn record(&mut self, record: ToolUsageRecord) {
        // Update tool stats
        let stats = self.tool_stats.entry(record.tool.clone()).or_default();
        let old_total_time = stats.avg_execution_time_ms * stats.usage_count as f64;
        stats.usage_count += 1;
        stats.avg_execution_time_ms =
            (old_total_time + record.execution_time_ms as f64) / stats.usage_count as f64;

        if record.outcome.is_positive() {
            stats.success_count += 1;
            // Track effective context
            if !stats.effective_contexts.contains(&record.task_context) {
                stats.effective_contexts.push(record.task_context.clone());
                if stats.effective_contexts.len() > 20 {
                    stats.effective_contexts.remove(0);
                }
            }
        } else {
            stats.failure_count += 1;
            // Track error patterns
            if let Some(error) = &record.error {
                let error_key = Self::normalize_error(error);
                *stats.common_errors.entry(error_key).or_insert(0) += 1;
            }
        }

        // Update context -> tool mapping
        let context_key = Self::normalize_context(&record.task_context);
        let tool_scores = self.context_tools.entry(context_key).or_default();
        if let Some((_, score)) = tool_scores.iter_mut().find(|(t, _)| t == &record.tool) {
            // Update existing score with exponential moving average
            *score = 0.8 * *score + 0.2 * record.outcome.score();
        } else {
            tool_scores.push((record.tool.clone(), record.outcome.score()));
        }

        // Trim context_tools if it grows too large
        if self.context_tools.len() > MAX_ENTRIES / 10 {
            let mut entries: Vec<_> = self.context_tools.keys().cloned().collect();
            entries.truncate(entries.len() / 2);
            for key in entries {
                self.context_tools.remove(&key);
            }
        }

        // Store record
        self.records.push(record);

        // Trim if needed — remove oldest entries
        if self.records.len() > self.max_records {
            self.records.drain(0..self.max_records / 2);
        }
    }

    /// Normalize error message for pattern matching
    fn normalize_error(error: &str) -> String {
        // Extract first line and remove specific values
        error
            .lines()
            .next()
            .unwrap_or(error)
            .chars()
            .take(100)
            .collect::<String>()
            .replace(char::is_numeric, "#")
    }

    /// Normalize context for matching
    fn normalize_context(context: &str) -> String {
        context
            .to_lowercase()
            .split_whitespace()
            .take(5)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get best tools for a context
    pub fn best_tools_for(&self, context: &str) -> Vec<(String, f32)> {
        let context_key = Self::normalize_context(context);

        if let Some(tools) = self.context_tools.get(&context_key) {
            let mut tools = tools.clone();
            tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            return tools.into_iter().take(5).collect();
        }

        // Fallback: return tools by overall success rate
        let mut tool_rates: Vec<_> = self
            .tool_stats
            .iter()
            .map(|(tool, stats)| (tool.clone(), stats.success_rate()))
            .collect();
        tool_rates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        tool_rates.into_iter().take(5).collect()
    }

    /// Get stats for a tool
    pub fn get_tool_stats(&self, tool: &str) -> Option<&ToolStats> {
        self.tool_stats.get(tool)
    }

    /// Get common errors for a tool
    pub fn common_errors_for(&self, tool: &str) -> Vec<(String, usize)> {
        if let Some(stats) = self.tool_stats.get(tool) {
            let mut errors: Vec<_> = stats
                .common_errors
                .iter()
                .map(|(e, c)| (e.clone(), *c))
                .collect();
            errors.sort_by_key(|x| std::cmp::Reverse(x.1));
            errors.into_iter().take(5).collect()
        } else {
            Vec::new()
        }
    }

    /// Get overall statistics
    pub fn get_stats(&self) -> ToolLearnerStats {
        let total_records = self.records.len();
        let successful = self
            .records
            .iter()
            .filter(|r| r.outcome.is_positive())
            .count();
        let unique_tools = self.tool_stats.len();

        ToolLearnerStats {
            total_records,
            successful_records: successful,
            unique_tools,
            contexts_tracked: self.context_tools.len(),
        }
    }
}

/// Serializable snapshot of ToolSelectionLearner state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolLearnerSnapshot {
    records: Vec<ToolUsageRecord>,
    tool_stats: HashMap<String, ToolStats>,
    context_tools: HashMap<String, Vec<(String, f32)>>,
}

impl ToolSelectionLearner {
    fn to_snapshot(&self) -> ToolLearnerSnapshot {
        ToolLearnerSnapshot {
            records: self.records.clone(),
            tool_stats: self.tool_stats.clone(),
            context_tools: self.context_tools.clone(),
        }
    }

    fn from_snapshot(snapshot: ToolLearnerSnapshot) -> Self {
        Self {
            records: snapshot.records,
            tool_stats: snapshot.tool_stats,
            context_tools: snapshot.context_tools,
            max_records: MAX_ENTRIES,
        }
    }
}

impl Default for ToolSelectionLearner {
    fn default() -> Self {
        Self::new()
    }
}

/// Stats for tool selection learner
#[derive(Debug, Clone)]
pub struct ToolLearnerStats {
    pub total_records: usize,
    pub successful_records: usize,
    pub unique_tools: usize,
    pub contexts_tracked: usize,
}

/// Record of an error occurrence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    /// Error message
    pub message: String,
    /// Error type/category
    pub error_type: String,
    /// Context when error occurred
    pub context: String,
    /// Action that caused error
    pub action: String,
    /// Was error recovered from?
    pub recovered: bool,
    /// Recovery action taken (if any)
    pub recovery_action: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl ErrorRecord {
    pub fn new(message: String, error_type: String, context: String, action: String) -> Self {
        Self {
            message,
            error_type,
            context,
            action,
            recovered: false,
            recovery_action: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_recovery(mut self, action: String) -> Self {
        self.recovered = true;
        self.recovery_action = Some(action);
        self
    }
}

/// Pattern of errors to avoid
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    /// Pattern identifier
    pub id: String,
    /// Error type
    pub error_type: String,
    /// Common contexts where this error occurs
    pub contexts: Vec<String>,
    /// Actions that trigger this error
    pub triggering_actions: Vec<String>,
    /// Known recovery strategies
    pub recovery_strategies: Vec<String>,
    /// Occurrence count
    pub count: usize,
    /// Prevention suggestions
    pub prevention: Vec<String>,
}

impl ErrorPattern {
    pub fn new(id: &str, error_type: &str) -> Self {
        Self {
            id: id.to_string(),
            error_type: error_type.to_string(),
            contexts: Vec::new(),
            triggering_actions: Vec::new(),
            recovery_strategies: Vec::new(),
            count: 0,
            prevention: Vec::new(),
        }
    }

    /// Update pattern with new error occurrence
    pub fn update(&mut self, record: &ErrorRecord) {
        self.count += 1;

        // Track context
        if !self.contexts.contains(&record.context) {
            self.contexts.push(record.context.clone());
            if self.contexts.len() > 10 {
                self.contexts.remove(0);
            }
        }

        // Track triggering action
        if !self.triggering_actions.contains(&record.action) {
            self.triggering_actions.push(record.action.clone());
            if self.triggering_actions.len() > 10 {
                self.triggering_actions.remove(0);
            }
        }

        // Track recovery strategy
        if let Some(recovery) = &record.recovery_action {
            if !self.recovery_strategies.contains(recovery) {
                self.recovery_strategies.push(recovery.clone());
            }
        }
    }

    /// Add prevention suggestion
    pub fn add_prevention(&mut self, suggestion: String) {
        if !self.prevention.contains(&suggestion) {
            self.prevention.push(suggestion);
        }
    }
}

/// Learner for error pattern avoidance
pub struct ErrorPatternLearner {
    /// Error records
    records: Vec<ErrorRecord>,
    /// Detected patterns
    patterns: HashMap<String, ErrorPattern>,
    /// Error type counts
    type_counts: HashMap<String, usize>,
    /// Maximum records
    max_records: usize,
}

impl ErrorPatternLearner {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            patterns: HashMap::new(),
            type_counts: HashMap::new(),
            max_records: MAX_ENTRIES,
        }
    }

    /// Record an error
    pub fn record(&mut self, record: ErrorRecord) {
        // Update type counts
        *self
            .type_counts
            .entry(record.error_type.clone())
            .or_insert(0) += 1;

        // Find or create pattern
        let pattern_id = Self::compute_pattern_id(&record);
        let pattern = self
            .patterns
            .entry(pattern_id.clone())
            .or_insert_with(|| ErrorPattern::new(&pattern_id, &record.error_type));
        pattern.update(&record);

        // Trim patterns if they grow too large
        if self.patterns.len() > MAX_ENTRIES / 10 {
            let mut entries: Vec<_> = self
                .patterns
                .iter()
                .map(|(k, v)| (k.clone(), v.count))
                .collect();
            entries.sort_by_key(|(_, count)| *count);
            for (key, _) in entries.iter().take(entries.len() / 2) {
                self.patterns.remove(key);
            }
        }

        // Store record
        self.records.push(record);

        // Trim if needed — remove oldest entries
        if self.records.len() > self.max_records {
            self.records.drain(0..self.max_records / 2);
        }
    }

    /// Compute pattern ID from error record
    fn compute_pattern_id(record: &ErrorRecord) -> String {
        // Create a hash-like ID from error type and message prefix
        let msg_prefix: String = record
            .message
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .take(30)
            .collect();
        format!("{}:{}", record.error_type, msg_prefix.trim())
    }

    /// Get pattern by ID
    pub fn get_pattern(&self, pattern_id: &str) -> Option<&ErrorPattern> {
        self.patterns.get(pattern_id)
    }

    /// Get most common error patterns
    pub fn most_common_patterns(&self, limit: usize) -> Vec<&ErrorPattern> {
        let mut patterns: Vec<_> = self.patterns.values().collect();
        patterns.sort_by_key(|x| std::cmp::Reverse(x.count));
        patterns.into_iter().take(limit).collect()
    }

    /// Check if action might trigger known error
    pub fn might_trigger_error(&self, action: &str, context: &str) -> Vec<ErrorWarning> {
        let mut warnings = Vec::new();

        for pattern in self.patterns.values() {
            let action_match = pattern
                .triggering_actions
                .iter()
                .any(|a| action.contains(a) || a.contains(action));
            let context_match = pattern
                .contexts
                .iter()
                .any(|c| context.contains(c) || c.contains(context));

            if action_match || context_match {
                warnings.push(ErrorWarning {
                    pattern_id: pattern.id.clone(),
                    error_type: pattern.error_type.clone(),
                    likelihood: if action_match && context_match {
                        0.8
                    } else {
                        0.4
                    },
                    prevention: pattern.prevention.clone(),
                    recovery: pattern.recovery_strategies.clone(),
                });
            }
        }

        warnings.sort_by(|a, b| {
            b.likelihood
                .partial_cmp(&a.likelihood)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        warnings
    }

    /// Get statistics
    pub fn get_stats(&self) -> ErrorLearnerStats {
        let total_errors = self.records.len();
        let recovered = self.records.iter().filter(|r| r.recovered).count();
        let pattern_count = self.patterns.len();

        ErrorLearnerStats {
            total_errors,
            recovered_count: recovered,
            pattern_count,
            top_error_types: self
                .type_counts
                .iter()
                .map(|(t, c)| (t.clone(), *c))
                .collect(),
        }
    }
}

/// Serializable snapshot of ErrorPatternLearner state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorLearnerSnapshot {
    records: Vec<ErrorRecord>,
    patterns: HashMap<String, ErrorPattern>,
    type_counts: HashMap<String, usize>,
}

impl ErrorPatternLearner {
    fn to_snapshot(&self) -> ErrorLearnerSnapshot {
        ErrorLearnerSnapshot {
            records: self.records.clone(),
            patterns: self.patterns.clone(),
            type_counts: self.type_counts.clone(),
        }
    }

    fn from_snapshot(snapshot: ErrorLearnerSnapshot) -> Self {
        Self {
            records: snapshot.records,
            patterns: snapshot.patterns,
            type_counts: snapshot.type_counts,
            max_records: MAX_ENTRIES,
        }
    }
}

impl Default for ErrorPatternLearner {
    fn default() -> Self {
        Self::new()
    }
}

/// Warning about potential error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorWarning {
    pub pattern_id: String,
    pub error_type: String,
    pub likelihood: f32,
    pub prevention: Vec<String>,
    pub recovery: Vec<String>,
}

/// Stats for error pattern learner
#[derive(Debug, Clone)]
pub struct ErrorLearnerStats {
    pub total_errors: usize,
    pub recovered_count: usize,
    pub pattern_count: usize,
    pub top_error_types: Vec<(String, usize)>,
}

/// Usage session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSession {
    /// Session ID
    pub id: String,
    /// Start timestamp
    pub start_time: u64,
    /// End timestamp (if ended)
    pub end_time: Option<u64>,
    /// Tasks attempted
    pub tasks_attempted: usize,
    /// Tasks completed
    pub tasks_completed: usize,
    /// Tools used
    pub tools_used: Vec<String>,
    /// Errors encountered
    pub errors: usize,
    /// User satisfaction (if rated)
    pub satisfaction: Option<f32>,
}

impl UsageSession {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            start_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            end_time: None,
            tasks_attempted: 0,
            tasks_completed: 0,
            tools_used: Vec::new(),
            errors: 0,
            satisfaction: None,
        }
    }

    /// End the session
    pub fn end(&mut self) {
        self.end_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    /// Duration in seconds
    pub fn duration_secs(&self) -> u64 {
        let end = self.end_time.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });
        end.saturating_sub(self.start_time)
    }

    /// Completion rate
    pub fn completion_rate(&self) -> f32 {
        if self.tasks_attempted == 0 {
            0.0
        } else {
            self.tasks_completed as f32 / self.tasks_attempted as f32
        }
    }
}

/// Analyzer for usage patterns
pub struct UsageAnalyzer {
    /// Session records
    sessions: Vec<UsageSession>,
    /// Current session
    current_session: Option<UsageSession>,
    /// Daily stats
    daily_stats: HashMap<String, DailyStats>,
    /// Tool usage frequency
    tool_frequency: HashMap<String, usize>,
}

/// Daily usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyStats {
    pub sessions: usize,
    pub tasks_attempted: usize,
    pub tasks_completed: usize,
    pub errors: usize,
    pub avg_satisfaction: f32,
    pub total_duration_secs: u64,
}

impl UsageAnalyzer {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            current_session: None,
            daily_stats: HashMap::new(),
            tool_frequency: HashMap::new(),
        }
    }

    /// Start a new session
    pub fn start_session(&mut self, session_id: &str) {
        if let Some(mut old_session) = self.current_session.take() {
            old_session.end();
            self.record_session(old_session);
        }
        self.current_session = Some(UsageSession::new(session_id));
    }

    /// Record task attempt
    pub fn record_task_attempt(&mut self, completed: bool) {
        if let Some(ref mut session) = self.current_session {
            session.tasks_attempted += 1;
            if completed {
                session.tasks_completed += 1;
            }
        }
    }

    /// Record tool usage
    pub fn record_tool_usage(&mut self, tool: &str) {
        *self.tool_frequency.entry(tool.to_string()).or_insert(0) += 1;
        if let Some(ref mut session) = self.current_session {
            if !session.tools_used.contains(&tool.to_string()) {
                session.tools_used.push(tool.to_string());
            }
        }
    }

    /// Record error
    pub fn record_error(&mut self) {
        if let Some(ref mut session) = self.current_session {
            session.errors += 1;
        }
    }

    /// End current session
    pub fn end_session(&mut self, satisfaction: Option<f32>) {
        if let Some(mut session) = self.current_session.take() {
            session.satisfaction = satisfaction;
            session.end();
            self.record_session(session);
        }
    }

    /// Record a completed session
    fn record_session(&mut self, session: UsageSession) {
        // Update daily stats
        let date = Self::timestamp_to_date(session.start_time);
        let daily = self.daily_stats.entry(date).or_default();
        daily.sessions += 1;
        daily.tasks_attempted += session.tasks_attempted;
        daily.tasks_completed += session.tasks_completed;
        daily.errors += session.errors;
        daily.total_duration_secs += session.duration_secs();
        if let Some(sat) = session.satisfaction {
            let old_total = daily.avg_satisfaction * (daily.sessions - 1) as f32;
            daily.avg_satisfaction = (old_total + sat) / daily.sessions as f32;
        }

        // Store session
        self.sessions.push(session);

        // Trim old sessions (keep last 1000)
        if self.sessions.len() > 1000 {
            self.sessions.drain(0..500);
        }
    }

    /// Convert timestamp to date string
    fn timestamp_to_date(timestamp: u64) -> String {
        let days = timestamp / 86400;
        format!("day_{}", days)
    }

    /// Get most used tools
    pub fn most_used_tools(&self, limit: usize) -> Vec<(String, usize)> {
        let mut tools: Vec<_> = self
            .tool_frequency
            .iter()
            .map(|(t, c)| (t.clone(), *c))
            .collect();
        tools.sort_by_key(|x| std::cmp::Reverse(x.1));
        tools.into_iter().take(limit).collect()
    }

    /// Get statistics
    pub fn get_stats(&self) -> UsageStats {
        let total_sessions = self.sessions.len();
        let total_tasks: usize = self.sessions.iter().map(|s| s.tasks_attempted).sum();
        let completed_tasks: usize = self.sessions.iter().map(|s| s.tasks_completed).sum();
        let total_errors: usize = self.sessions.iter().map(|s| s.errors).sum();

        let avg_satisfaction = {
            let rated: Vec<_> = self
                .sessions
                .iter()
                .filter_map(|s| s.satisfaction)
                .collect();
            if rated.is_empty() {
                0.0
            } else {
                rated.iter().sum::<f32>() / rated.len() as f32
            }
        };

        UsageStats {
            total_sessions,
            total_tasks,
            completed_tasks,
            total_errors,
            avg_satisfaction,
            unique_tools: self.tool_frequency.len(),
        }
    }
}

/// Serializable snapshot of UsageAnalyzer state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageAnalyzerSnapshot {
    sessions: Vec<UsageSession>,
    current_session: Option<UsageSession>,
    daily_stats: HashMap<String, DailyStats>,
    tool_frequency: HashMap<String, usize>,
}

impl UsageAnalyzer {
    fn to_snapshot(&self) -> UsageAnalyzerSnapshot {
        UsageAnalyzerSnapshot {
            sessions: self.sessions.clone(),
            current_session: self.current_session.clone(),
            daily_stats: self.daily_stats.clone(),
            tool_frequency: self.tool_frequency.clone(),
        }
    }

    fn from_snapshot(snapshot: UsageAnalyzerSnapshot) -> Self {
        Self {
            sessions: snapshot.sessions,
            current_session: snapshot.current_session,
            daily_stats: snapshot.daily_stats,
            tool_frequency: snapshot.tool_frequency,
        }
    }
}

impl Default for UsageAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Usage statistics
#[derive(Debug, Clone)]
pub struct UsageStats {
    pub total_sessions: usize,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub total_errors: usize,
    pub avg_satisfaction: f32,
    pub unique_tools: usize,
}

/// Main self-improvement engine
pub struct SelfImprovementEngine {
    /// Prompt optimizer
    prompt_optimizer: RwLock<PromptOptimizer>,
    /// Tool selection learner
    tool_learner: RwLock<ToolSelectionLearner>,
    /// Error pattern learner
    error_learner: RwLock<ErrorPatternLearner>,
    /// Usage analyzer
    usage_analyzer: RwLock<UsageAnalyzer>,
    /// Enable learning
    learning_enabled: bool,
}

impl SelfImprovementEngine {
    pub fn new() -> Self {
        Self {
            prompt_optimizer: RwLock::new(PromptOptimizer::new()),
            tool_learner: RwLock::new(ToolSelectionLearner::new()),
            error_learner: RwLock::new(ErrorPatternLearner::new()),
            usage_analyzer: RwLock::new(UsageAnalyzer::new()),
            learning_enabled: true,
        }
    }

    /// Enable/disable learning
    pub fn set_learning_enabled(&mut self, enabled: bool) {
        self.learning_enabled = enabled;
    }

    /// Record a prompt outcome
    pub fn record_prompt(&self, prompt: &str, task_type: &str, outcome: Outcome, quality: f32) {
        if !self.learning_enabled {
            return;
        }
        if let Ok(mut optimizer) = self.prompt_optimizer.write() {
            optimizer.record(
                PromptRecord::new(prompt.to_string(), task_type.to_string(), outcome)
                    .with_quality(quality),
            );
        }
    }

    /// Record a tool usage
    pub fn record_tool(
        &self,
        tool: &str,
        context: &str,
        outcome: Outcome,
        time_ms: u64,
        error: Option<String>,
    ) {
        if !self.learning_enabled {
            return;
        }
        if let Ok(mut learner) = self.tool_learner.write() {
            let mut record = ToolUsageRecord::new(tool.to_string(), context.to_string(), outcome)
                .with_execution_time(time_ms);
            if let Some(err) = error {
                record = record.with_error(err);
            }
            learner.record(record);
        }

        // Also record in usage analyzer
        if let Ok(mut analyzer) = self.usage_analyzer.write() {
            analyzer.record_tool_usage(tool);
        }
    }

    /// Record an error
    pub fn record_error(
        &self,
        message: &str,
        error_type: &str,
        context: &str,
        action: &str,
        recovery: Option<String>,
    ) {
        if !self.learning_enabled {
            return;
        }
        if let Ok(mut learner) = self.error_learner.write() {
            let mut record = ErrorRecord::new(
                message.to_string(),
                error_type.to_string(),
                context.to_string(),
                action.to_string(),
            );
            if let Some(rec) = recovery {
                record = record.with_recovery(rec);
            }
            learner.record(record);
        }

        // Also record in usage analyzer
        if let Ok(mut analyzer) = self.usage_analyzer.write() {
            analyzer.record_error();
        }
    }

    /// Get best tools for a context
    pub fn best_tools_for(&self, context: &str) -> Vec<(String, f32)> {
        if let Ok(learner) = self.tool_learner.read() {
            learner.best_tools_for(context)
        } else {
            Vec::new()
        }
    }

    /// Check for potential errors
    pub fn check_for_errors(&self, action: &str, context: &str) -> Vec<ErrorWarning> {
        if let Ok(learner) = self.error_learner.read() {
            learner.might_trigger_error(action, context)
        } else {
            Vec::new()
        }
    }

    /// Get prompt suggestions
    pub fn suggest_prompt_improvements(
        &self,
        prompt: &str,
        task_type: &str,
    ) -> Vec<PromptSuggestion> {
        if let Ok(optimizer) = self.prompt_optimizer.read() {
            optimizer.suggest_improvements(prompt, task_type)
        } else {
            Vec::new()
        }
    }

    /// Evolve a prompt via an automated A/B tournament and return the winning variant.
    pub fn evolve_prompt(&self, prompt: &str, task_type: &str) -> Option<PromptTournamentResult> {
        if !self.learning_enabled {
            return None;
        }
        if let Ok(mut optimizer) = self.prompt_optimizer.write() {
            Some(optimizer.evolve_prompt(task_type, prompt))
        } else {
            None
        }
    }

    /// Start a usage session
    pub fn start_session(&self, session_id: &str) {
        if let Ok(mut analyzer) = self.usage_analyzer.write() {
            analyzer.start_session(session_id);
        }
    }

    /// Record task attempt
    pub fn record_task(&self, completed: bool) {
        if let Ok(mut analyzer) = self.usage_analyzer.write() {
            analyzer.record_task_attempt(completed);
        }
    }

    /// End session
    pub fn end_session(&self, satisfaction: Option<f32>) {
        if let Ok(mut analyzer) = self.usage_analyzer.write() {
            analyzer.end_session(satisfaction);
        }
    }

    /// Get comprehensive stats
    pub fn get_stats(&self) -> ImprovementStats {
        let prompt_stats = self.prompt_optimizer.read().ok().map(|o| o.get_stats());
        let tool_stats = self.tool_learner.read().ok().map(|l| l.get_stats());
        let error_stats = self.error_learner.read().ok().map(|l| l.get_stats());
        let usage_stats = self.usage_analyzer.read().ok().map(|a| a.get_stats());

        ImprovementStats {
            prompt_stats,
            tool_stats,
            error_stats,
            usage_stats,
            learning_enabled: self.learning_enabled,
        }
    }
}

/// Serializable snapshot of the entire engine state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EngineSnapshot {
    prompt_optimizer: PromptOptimizerSnapshot,
    tool_learner: ToolLearnerSnapshot,
    error_learner: ErrorLearnerSnapshot,
    usage_analyzer: UsageAnalyzerSnapshot,
    learning_enabled: bool,
}

impl SelfImprovementEngine {
    /// Save engine state to a JSON file
    pub fn save(&self, path: &Path) -> Result<()> {
        let snapshot = EngineSnapshot {
            prompt_optimizer: self
                .prompt_optimizer
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                .to_snapshot(),
            tool_learner: self
                .tool_learner
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                .to_snapshot(),
            error_learner: self
                .error_learner
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                .to_snapshot(),
            usage_analyzer: self
                .usage_analyzer
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                .to_snapshot(),
            learning_enabled: self.learning_enabled,
        };

        let content = serde_json::to_string_pretty(&snapshot)?;
        write_private_atomic(path, content.as_bytes())?;
        Ok(())
    }

    /// Load engine state from a JSON file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;

        // Migrate legacy files written before at-rest hardening: tighten to
        // owner-only on load so an existing world-readable learning file
        // (mode 664) becomes 0600. Best-effort; ignore errors (e.g. not owner).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            if let Some(parent) = path.parent() {
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }

        let snapshot: EngineSnapshot = serde_json::from_str(&content)?;

        Ok(Self {
            prompt_optimizer: RwLock::new(PromptOptimizer::from_snapshot(
                snapshot.prompt_optimizer,
            )),
            tool_learner: RwLock::new(ToolSelectionLearner::from_snapshot(snapshot.tool_learner)),
            error_learner: RwLock::new(ErrorPatternLearner::from_snapshot(snapshot.error_learner)),
            usage_analyzer: RwLock::new(UsageAnalyzer::from_snapshot(snapshot.usage_analyzer)),
            learning_enabled: snapshot.learning_enabled,
        })
    }
}

impl Default for SelfImprovementEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Write `bytes` to `path` atomically and with owner-only (0600) permissions.
///
/// The learning store holds raw task prompts, so it must not be readable by
/// other local users. We write to a temp file in the same directory, chmod
/// it to 0600 (Unix) before it is visible under the real name, then rename
/// over the target. The parent directory is created 0700 on Unix.
fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Best-effort: tighten the dir to owner-only. Ignore errors on
            // pre-existing dirs we may not own.
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    // Unique per write: pid alone collides when two agents in the same
    // process save concurrently.
    static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => path.with_extension(format!("{ext}.tmp.{}.{}", std::process::id(), seq)),
        None => path.with_extension(format!("tmp.{}.{}", std::process::id(), seq)),
    };

    std::fs::write(&tmp_path, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
    }

    // Atomic replace. On failure, clean up the temp file.
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e.into());
    }
    Ok(())
}

/// Comprehensive stats for self-improvement
#[derive(Debug, Clone)]
pub struct ImprovementStats {
    pub prompt_stats: Option<PromptOptimizerStats>,
    pub tool_stats: Option<ToolLearnerStats>,
    pub error_stats: Option<ErrorLearnerStats>,
    pub usage_stats: Option<UsageStats>,
    pub learning_enabled: bool,
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/self_improvement/self_improvement_test.rs"]
mod tests;
