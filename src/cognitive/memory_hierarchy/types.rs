//! Memory Hierarchy Types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Total context tokens for 1M context
pub const TOTAL_CONTEXT_TOKENS: usize = 1_000_000;

/// Importance level for memories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Importance {
    Low,
    Normal,
    Medium,
    High,
    Critical,
}

impl Importance {
    pub fn as_f32(&self) -> f32 {
        match self {
            Importance::Low => 0.25,
            Importance::Normal => 0.4,
            Importance::Medium => 0.5,
            Importance::High => 0.75,
            Importance::Critical => 1.0,
        }
    }
}

/// Episode type for episodic memory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeType {
    Conversation,
    Action,
    Thought,
    Outcome,
    Reflection,
    Success,
    Learning,
    Error,
    ToolExecution,
}

impl EpisodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EpisodeType::Conversation => "conversation",
            EpisodeType::Action => "action",
            EpisodeType::Thought => "thought",
            EpisodeType::Outcome => "outcome",
            EpisodeType::Reflection => "reflection",
            EpisodeType::Success => "success",
            EpisodeType::Learning => "learning",
            EpisodeType::Error => "error",
            EpisodeType::ToolExecution => "tool_execution",
        }
    }
}

/// An episode in episodic memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub episode_type: EpisodeType,
    pub content: String,
    pub timestamp: u64,
    pub importance: Importance,
    pub metadata: HashMap<String, serde_json::Value>,
    pub token_count: usize,
    pub embedding_id: String,
    pub related_episodes: Vec<String>,
    pub insights: Vec<String>,
    pub is_summarized: bool,
    pub original_id: Option<String>,
}

impl Episode {
    pub fn new(
        id: impl Into<String>,
        episode_type: EpisodeType,
        content: impl Into<String>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: id.into(),
            episode_type,
            content: content.into(),
            timestamp: now,
            importance: Importance::Medium,
            metadata: HashMap::new(),
            token_count: 0,
            embedding_id: String::new(),
            related_episodes: Vec::new(),
            insights: Vec::new(),
            is_summarized: false,
            original_id: None,
        }
    }

    pub fn with_importance(mut self, importance: Importance) -> Self {
        self.importance = importance;
        self
    }
}

/// Task context for current task tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub description: String,
    pub goal: String,
    pub progress: Vec<String>,
    pub next_steps: Vec<String>,
    pub relevant_files: Vec<String>,
}

/// Code edit record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEdit {
    pub timestamp: u64,
    pub description: String,
    pub lines_changed: (usize, usize),
}

/// Code content variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeContent {
    Full(String),
    Summary {
        overview: String,
        key_functions: Vec<String>,
    },
}

/// Active code context entry (single file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCodeContext {
    pub path: String,
    pub content: CodeContent,
    pub last_accessed: u64,
    pub edit_history: Vec<CodeEdit>,
}

/// File context entry with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContextEntry {
    pub path: String,
    pub content: String,
    pub relevance_score: f32,
}

/// Collection of active code files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCodeCollection {
    pub files: Vec<ActiveCodeContext>,
}

/// Working context for current conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingContext {
    pub messages: Vec<crate::api::types::Message>,
    pub system_prompt: String,
    pub estimated_tokens: usize,
    pub current_task: Option<TaskContext>,
    pub active_code: Vec<ActiveCodeContext>,
    pub usage: MemoryUsage,
}

impl WorkingContext {
    pub fn new(system_prompt: impl Into<String>) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: system_prompt.into(),
            estimated_tokens: 0,
            current_task: None,
            active_code: Vec::new(),
            usage: MemoryUsage::default(),
        }
    }

    pub fn add_message(&mut self, message: crate::api::types::Message, estimated_tokens: usize) {
        self.messages.push(message);
        self.estimated_tokens += estimated_tokens;
    }
}

/// Code context for semantic memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContext {
    pub files: Vec<FileContext>,
    pub symbols: Vec<SymbolContext>,
    pub total_tokens: usize,
}

impl CodeContext {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        }
    }
}

impl Default for CodeContext {
    fn default() -> Self {
        Self::new()
    }
}

/// File context within code context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    pub path: String,
    pub content: String,
    pub language: String,
    pub estimated_tokens: usize,
    pub relevance_score: f32,
}

/// Symbol context for code elements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolContext {
    pub name: String,
    pub symbol_type: String,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
}

/// Token budget allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    pub working_memory: usize,
    pub episodic_memory: usize,
    pub semantic_memory: usize,
    pub response_reserve: usize,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::new(TOTAL_CONTEXT_TOKENS)
    }
}

impl TokenBudget {
    pub fn new(total: usize) -> Self {
        Self {
            working_memory: total / 4,
            episodic_memory: total / 4,
            semantic_memory: total / 4,
            response_reserve: total / 4,
        }
    }

    pub fn for_conversation() -> Self {
        Self::new(TOTAL_CONTEXT_TOKENS)
    }

    pub fn for_self_improvement() -> Self {
        Self {
            working_memory: TOTAL_CONTEXT_TOKENS / 8,
            episodic_memory: TOTAL_CONTEXT_TOKENS / 8,
            semantic_memory: TOTAL_CONTEXT_TOKENS * 3 / 4,
            response_reserve: TOTAL_CONTEXT_TOKENS / 8,
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub working_tokens: usize,
    pub episodic_tokens: usize,
    pub semantic_tokens: usize,
    pub self_tokens: usize,
    pub total_used: usize,
}

/// Memory metrics for tracking performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub compressions: u64,
    pub avg_retrieval_time_ms: f64,
    pub last_updated: u64,
}

impl Default for MemoryMetrics {
    fn default() -> Self {
        Self {
            cache_hits: 0,
            cache_misses: 0,
            evictions: 0,
            compressions: 0,
            avg_retrieval_time_ms: 0.0,
            last_updated: 0,
        }
    }
}

/// Self-improvement context for cognitive system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfImprovementContext {
    pub goal: String,
    pub self_model: String,
    pub architecture: String,
    pub recent_modifications: String,
    pub relevant_code: CodeContext,
    pub suggestions: Vec<String>,
}

impl SelfImprovementContext {
    pub fn estimate_tokens(&self) -> usize {
        // AGENTS.md rule 4: token accounting goes through
        // `token_count::estimate_content_tokens`, never a chars/4 heuristic.
        let base = crate::token_count::estimate_content_tokens(&self.goal)
            + crate::token_count::estimate_content_tokens(&self.self_model)
            + crate::token_count::estimate_content_tokens(&self.architecture)
            + crate::token_count::estimate_content_tokens(&self.recent_modifications);
        let code_tokens: usize = self
            .relevant_code
            .files
            .iter()
            .map(|f| crate::token_count::estimate_content_tokens(&f.content))
            .sum();
        let suggestions_tokens: usize = self
            .suggestions
            .iter()
            .map(|s| crate::token_count::estimate_content_tokens(s))
            .sum();
        base + code_tokens + suggestions_tokens
    }

    pub fn to_prompt(&self) -> String {
        let mut prompt = String::new();
        prompt.push_str("## Self-Improvement Context\n");
        prompt.push_str(&format!("Goal: {}\n", self.goal));
        prompt.push_str(&format!("Self-Model: {}\n", self.self_model));
        prompt.push_str(&format!("Architecture: {}\n", self.architecture));
        if !self.recent_modifications.is_empty() {
            prompt.push_str(&format!(
                "Recent Modifications: {}\n",
                self.recent_modifications
            ));
        }
        prompt.push_str("Suggestions to Consider:\n");
        if !self.suggestions.is_empty() {
            for s in &self.suggestions {
                prompt.push_str(&format!("- {}\n", s));
            }
        }
        prompt
    }
}

/// Self-model representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModel {
    pub version: String,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub recent_changes: Vec<String>,
    pub modules: Vec<String>,
}

/// Code modification tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeModification {
    pub id: String,
    pub timestamp: u64,
    pub file_path: String,
    pub change_type: ChangeType,
    pub description: String,
    pub success: bool,
}

/// Type of code change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Addition,
    Deletion,
    Modification,
    Refactor,
}

/// Memory tier in the hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum MemoryTier {
    /// Immediate working memory
    Working,
    /// Short-term contextual memory
    ShortTerm,
    /// Long-term persistent memory
    LongTerm,
    /// Archive/backup memory
    Archive,
}

/// A memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: u64,
    pub content: String,
    pub tier: MemoryTier,
    pub created_at: u64,
    pub accessed_at: u64,
    pub access_count: u64,
    pub importance: f32,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl MemoryEntry {
    pub fn new(id: u64, content: impl Into<String>, tier: MemoryTier) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            id,
            content: content.into(),
            tier,
            created_at: now,
            accessed_at: now,
            access_count: 0,
            importance: 0.5,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn accessed(&mut self) {
        self.access_count += 1;
        self.accessed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
    }

    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

/// Configuration for memory hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub working_capacity: usize,
    pub short_term_capacity: usize,
    pub long_term_capacity: usize,
    pub promotion_threshold: u64,
    pub demotion_threshold: u64,
    pub importance_threshold: f32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            working_capacity: 10,
            short_term_capacity: 100,
            long_term_capacity: 1000,
            promotion_threshold: 3,
            demotion_threshold: 3600,
            importance_threshold: 0.7,
        }
    }
}

/// Statistics for memory operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStats {
    pub working_count: usize,
    pub short_term_count: usize,
    pub long_term_count: usize,
    pub total_inserts: u64,
    pub total_queries: u64,
    pub total_promotions: u64,
    pub total_demotions: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    // Extended fields for cognitive system
    pub budget: Option<TokenBudget>,
    pub usage: Option<MemoryUsage>,
    pub metrics: Option<MemoryMetrics>,
    pub working_entries: usize,
    pub episodic_entries: usize,
    pub semantic_files: usize,
}

/// Query for memory search
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    pub pattern: String,
    pub tier: Option<MemoryTier>,
    pub tags: Vec<String>,
    pub min_importance: Option<f32>,
    pub since: Option<u64>,
    pub limit: Option<usize>,
}

impl MemoryQuery {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            tier: None,
            tags: Vec::new(),
            min_importance: None,
            since: None,
            limit: Some(10),
        }
    }

    pub fn with_tier(mut self, tier: MemoryTier) -> Self {
        self.tier = Some(tier);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_min_importance(mut self, importance: f32) -> Self {
        self.min_importance = Some(importance);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn since(mut self, timestamp: u64) -> Self {
        self.since = Some(timestamp);
        self
    }
}

/// Memory index for fast lookups
pub struct MemoryIndex {
    by_tag: Arc<RwLock<HashMap<String, Vec<u64>>>>,
    by_tier: Arc<RwLock<HashMap<MemoryTier, Vec<u64>>>>,
    counter: AtomicU64,
}

impl MemoryIndex {
    pub fn new() -> Self {
        Self {
            by_tag: Arc::new(RwLock::new(HashMap::new())),
            by_tier: Arc::new(RwLock::new(HashMap::new())),
            counter: AtomicU64::new(1),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn index_entry(&self, entry: &MemoryEntry) {
        {
            let mut by_tier = self.by_tier.write().await;
            by_tier.entry(entry.tier).or_default().push(entry.id);
        }

        {
            let mut by_tag = self.by_tag.write().await;
            for tag in &entry.tags {
                by_tag.entry(tag.clone()).or_default().push(entry.id);
            }
        }
    }

    pub async fn get_by_tag(&self, tag: &str) -> Vec<u64> {
        let by_tag = self.by_tag.read().await;
        by_tag.get(tag).cloned().unwrap_or_default()
    }

    pub async fn get_by_tier(&self, tier: MemoryTier) -> Vec<u64> {
        let by_tier = self.by_tier.read().await;
        by_tier.get(&tier).cloned().unwrap_or_default()
    }

    pub async fn remove_entry(&self, entry: &MemoryEntry) {
        {
            let mut by_tier = self.by_tier.write().await;
            if let Some(ids) = by_tier.get_mut(&entry.tier) {
                ids.retain(|&id| id != entry.id);
            }
        }

        {
            let mut by_tag = self.by_tag.write().await;
            for tag in &entry.tags {
                if let Some(ids) = by_tag.get_mut(tag) {
                    ids.retain(|&id| id != entry.id);
                }
            }
        }
    }
}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Promotion/demotion decision
#[derive(Debug, Clone, Copy)]
pub enum TierTransition {
    Promote,
    Demote,
    Keep,
}

/// Result of consolidation
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    pub entries_merged: usize,
    pub entries_removed: usize,
    pub new_summaries: Vec<MemoryEntry>,
}

/// Whether a memory entry satisfies a query. Shared by the short- and long-term
/// stores, which previously carried a verbatim copy of this predicate each.
pub(crate) fn matches_query(entry: &MemoryEntry, query: &MemoryQuery) -> bool {
    if !entry
        .content
        .to_lowercase()
        .contains(&query.pattern.to_lowercase())
    {
        return false;
    }
    if let Some(tier) = query.tier {
        if entry.tier != tier {
            return false;
        }
    }
    if !query.tags.is_empty() && !query.tags.iter().all(|t| entry.tags.contains(t)) {
        return false;
    }
    if let Some(min_importance) = query.min_importance {
        if entry.importance < min_importance {
            return false;
        }
    }
    if let Some(since) = query.since {
        if entry.created_at < since {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "../../../tests/unit/cognitive/memory_hierarchy/types/types_test.rs"]
mod tests;
