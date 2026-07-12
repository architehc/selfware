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
        let base = self.goal.len()
            + self.self_model.len()
            + self.architecture.len()
            + self.recent_modifications.len();
        let code_tokens: usize = self
            .relevant_code
            .files
            .iter()
            .map(|f| f.content.len())
            .sum();
        let suggestions_tokens: usize = self.suggestions.iter().map(|s| s.len()).sum();
        (base + code_tokens + suggestions_tokens) / 4
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Message;

    // ========================================================================
    // Importance
    // ========================================================================

    #[test]
    fn test_importance_as_f32_values() {
        assert_eq!(Importance::Low.as_f32(), 0.25);
        assert_eq!(Importance::Normal.as_f32(), 0.4);
        assert_eq!(Importance::Medium.as_f32(), 0.5);
        assert_eq!(Importance::High.as_f32(), 0.75);
        assert_eq!(Importance::Critical.as_f32(), 1.0);
    }

    #[test]
    fn test_importance_ordering() {
        // PartialOrd + Ord derived — verify monotonic ordering
        assert!(Importance::Low < Importance::Normal);
        assert!(Importance::Normal < Importance::Medium);
        assert!(Importance::Medium < Importance::High);
        assert!(Importance::High < Importance::Critical);
    }

    #[test]
    fn test_importance_equality_and_copy() {
        let a = Importance::High;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(Importance::Low, Importance::Critical);
    }

    #[test]
    fn test_importance_as_f32_range() {
        for val in [
            Importance::Low,
            Importance::Normal,
            Importance::Medium,
            Importance::High,
            Importance::Critical,
        ] {
            let f = val.as_f32();
            assert!(f > 0.0 && f <= 1.0, "{:?} => {} out of range", val, f);
        }
    }

    #[test]
    fn test_importance_serde_roundtrip() {
        for val in [
            Importance::Low,
            Importance::Normal,
            Importance::Medium,
            Importance::High,
            Importance::Critical,
        ] {
            let json = serde_json::to_string(&val).unwrap();
            let back: Importance = serde_json::from_str(&json).unwrap();
            assert_eq!(val, back);
        }
    }

    // ========================================================================
    // EpisodeType
    // ========================================================================

    #[test]
    fn test_episode_type_as_str() {
        assert_eq!(EpisodeType::Conversation.as_str(), "conversation");
        assert_eq!(EpisodeType::Action.as_str(), "action");
        assert_eq!(EpisodeType::Thought.as_str(), "thought");
        assert_eq!(EpisodeType::Outcome.as_str(), "outcome");
        assert_eq!(EpisodeType::Reflection.as_str(), "reflection");
        assert_eq!(EpisodeType::Success.as_str(), "success");
        assert_eq!(EpisodeType::Learning.as_str(), "learning");
        assert_eq!(EpisodeType::Error.as_str(), "error");
        assert_eq!(EpisodeType::ToolExecution.as_str(), "tool_execution");
    }

    #[test]
    fn test_episode_type_as_str_unique() {
        let strs: Vec<&str> = [
            EpisodeType::Conversation,
            EpisodeType::Action,
            EpisodeType::Thought,
            EpisodeType::Outcome,
            EpisodeType::Reflection,
            EpisodeType::Success,
            EpisodeType::Learning,
            EpisodeType::Error,
            EpisodeType::ToolExecution,
        ]
        .iter()
        .map(|e| e.as_str())
        .collect();

        let unique: std::collections::HashSet<&str> = strs.iter().copied().collect();
        assert_eq!(
            strs.len(),
            unique.len(),
            "EpisodeType strings must be unique"
        );
    }

    #[test]
    fn test_episode_type_serde_roundtrip() {
        let et = EpisodeType::Learning;
        let json = serde_json::to_string(&et).unwrap();
        let back: EpisodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(et, back);
    }

    // ========================================================================
    // Episode
    // ========================================================================

    #[test]
    fn test_episode_new_defaults() {
        let ep = Episode::new("ep-1", EpisodeType::Action, "did something");
        assert_eq!(ep.id, "ep-1");
        assert_eq!(ep.episode_type, EpisodeType::Action);
        assert_eq!(ep.content, "did something");
        assert_eq!(ep.importance, Importance::Medium);
        assert!(ep.metadata.is_empty());
        assert_eq!(ep.token_count, 0);
        assert!(ep.embedding_id.is_empty());
        assert!(ep.related_episodes.is_empty());
        assert!(ep.insights.is_empty());
        assert!(!ep.is_summarized);
        assert!(ep.original_id.is_none());
        // timestamp should be a plausible unix epoch value
        assert!(ep.timestamp > 1_600_000_000);
    }

    #[test]
    fn test_episode_with_importance() {
        let ep =
            Episode::new("ep-2", EpisodeType::Error, "oops").with_importance(Importance::Critical);
        assert_eq!(ep.importance, Importance::Critical);
    }

    #[test]
    fn test_episode_with_importance_chained() {
        // Verify builder chaining returns Self
        let ep = Episode::new("ep-3", EpisodeType::Thought, "hmm").with_importance(Importance::Low);
        assert_eq!(ep.id, "ep-3");
        assert_eq!(ep.importance, Importance::Low);
    }

    #[test]
    fn test_episode_serde_roundtrip() {
        let mut ep =
            Episode::new("ep-serde", EpisodeType::Success, "yay").with_importance(Importance::High);
        ep.token_count = 42;
        ep.related_episodes.push("ep-1".to_string());
        ep.insights.push("learned a lot".to_string());
        ep.is_summarized = true;
        ep.original_id = Some("ep-orig".to_string());
        ep.metadata
            .insert("key".to_string(), serde_json::json!("value"));

        let json = serde_json::to_string(&ep).unwrap();
        let back: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, ep.id);
        assert_eq!(back.episode_type, ep.episode_type);
        assert_eq!(back.content, ep.content);
        assert_eq!(back.importance, ep.importance);
        assert_eq!(back.token_count, 42);
        assert_eq!(back.related_episodes, vec!["ep-1".to_string()]);
        assert_eq!(back.insights, vec!["learned a lot".to_string()]);
        assert!(back.is_summarized);
        assert_eq!(back.original_id, Some("ep-orig".to_string()));
        assert_eq!(back.metadata.get("key"), Some(&serde_json::json!("value")));
    }

    // ========================================================================
    // WorkingContext
    // ========================================================================

    #[test]
    fn test_working_context_new() {
        let wc = WorkingContext::new("You are a helpful assistant.");
        assert_eq!(wc.system_prompt, "You are a helpful assistant.");
        assert!(wc.messages.is_empty());
        assert_eq!(wc.estimated_tokens, 0);
        assert!(wc.current_task.is_none());
        assert!(wc.active_code.is_empty());
        assert_eq!(wc.usage.working_tokens, 0);
    }

    #[test]
    fn test_working_context_add_message_accumulates_tokens() {
        let mut wc = WorkingContext::new("system prompt");
        assert_eq!(wc.estimated_tokens, 0);
        assert_eq!(wc.messages.len(), 0);

        wc.add_message(Message::user("hello"), 10);
        assert_eq!(wc.messages.len(), 1);
        assert_eq!(wc.estimated_tokens, 10);

        wc.add_message(Message::assistant("hi there"), 20);
        assert_eq!(wc.messages.len(), 2);
        assert_eq!(wc.estimated_tokens, 30);

        wc.add_message(Message::user("bye"), 5);
        assert_eq!(wc.messages.len(), 3);
        assert_eq!(wc.estimated_tokens, 35);
    }

    #[test]
    fn test_working_context_add_message_preserves_role() {
        let mut wc = WorkingContext::new("sys");
        wc.add_message(Message::user("user msg"), 5);
        wc.add_message(Message::assistant("assistant msg"), 5);
        assert_eq!(wc.messages[0].role, "user");
        assert_eq!(wc.messages[1].role, "assistant");
    }

    // ========================================================================
    // CodeContext
    // ========================================================================

    #[test]
    fn test_code_context_new_empty() {
        let cc = CodeContext::new();
        assert!(cc.files.is_empty());
        assert!(cc.symbols.is_empty());
        assert_eq!(cc.total_tokens, 0);
    }

    #[test]
    fn test_code_context_default_equals_new() {
        let cc_new = CodeContext::new();
        let cc_default = CodeContext::default();
        assert_eq!(cc_new.files.len(), cc_default.files.len());
        assert_eq!(cc_new.symbols.len(), cc_default.symbols.len());
        assert_eq!(cc_new.total_tokens, cc_default.total_tokens);
    }

    // ========================================================================
    // TokenBudget
    // ========================================================================

    #[test]
    fn test_token_budget_new_quarters_total() {
        let tb = TokenBudget::new(1_000_000);
        assert_eq!(tb.working_memory, 250_000);
        assert_eq!(tb.episodic_memory, 250_000);
        assert_eq!(tb.semantic_memory, 250_000);
        assert_eq!(tb.response_reserve, 250_000);
    }

    #[test]
    fn test_token_budget_new_small_total() {
        let tb = TokenBudget::new(100);
        assert_eq!(tb.working_memory, 25);
        assert_eq!(tb.episodic_memory, 25);
        assert_eq!(tb.semantic_memory, 25);
        assert_eq!(tb.response_reserve, 25);
    }

    #[test]
    fn test_token_budget_default_equals_new_total_context() {
        let tb_default = TokenBudget::default();
        let tb_conv = TokenBudget::for_conversation();
        assert_eq!(tb_default.working_memory, tb_conv.working_memory);
        assert_eq!(tb_default.episodic_memory, tb_conv.episodic_memory);
        assert_eq!(tb_default.semantic_memory, tb_conv.semantic_memory);
        assert_eq!(tb_default.response_reserve, tb_conv.response_reserve);
    }

    #[test]
    fn test_token_budget_for_conversation_quarters() {
        let tb = TokenBudget::for_conversation();
        assert_eq!(tb.working_memory, TOTAL_CONTEXT_TOKENS / 4);
        assert_eq!(tb.episodic_memory, TOTAL_CONTEXT_TOKENS / 4);
        assert_eq!(tb.semantic_memory, TOTAL_CONTEXT_TOKENS / 4);
        assert_eq!(tb.response_reserve, TOTAL_CONTEXT_TOKENS / 4);
    }

    #[test]
    fn test_token_budget_for_self_improvement_skews_semantic() {
        let tb = TokenBudget::for_self_improvement();
        assert_eq!(tb.working_memory, TOTAL_CONTEXT_TOKENS / 8);
        assert_eq!(tb.episodic_memory, TOTAL_CONTEXT_TOKENS / 8);
        assert_eq!(tb.semantic_memory, TOTAL_CONTEXT_TOKENS * 3 / 4);
        assert_eq!(tb.response_reserve, TOTAL_CONTEXT_TOKENS / 8);
        // Semantic should dominate
        assert!(tb.semantic_memory > tb.working_memory);
        assert!(tb.semantic_memory > tb.episodic_memory);
        assert!(tb.semantic_memory > tb.response_reserve);
    }

    #[test]
    fn test_total_context_tokens_constant() {
        assert_eq!(TOTAL_CONTEXT_TOKENS, 1_000_000);
    }

    // ========================================================================
    // MemoryUsage
    // ========================================================================

    #[test]
    fn test_memory_usage_default_all_zero() {
        let mu = MemoryUsage::default();
        assert_eq!(mu.working_tokens, 0);
        assert_eq!(mu.episodic_tokens, 0);
        assert_eq!(mu.semantic_tokens, 0);
        assert_eq!(mu.self_tokens, 0);
        assert_eq!(mu.total_used, 0);
    }

    // ========================================================================
    // MemoryMetrics
    // ========================================================================

    #[test]
    fn test_memory_metrics_default_all_zero() {
        let mm = MemoryMetrics::default();
        assert_eq!(mm.cache_hits, 0);
        assert_eq!(mm.cache_misses, 0);
        assert_eq!(mm.evictions, 0);
        assert_eq!(mm.compressions, 0);
        assert_eq!(mm.avg_retrieval_time_ms, 0.0);
        assert_eq!(mm.last_updated, 0);
    }

    // ========================================================================
    // SelfImprovementContext
    // ========================================================================

    #[test]
    fn test_self_improvement_context_estimate_tokens_empty() {
        let sic = SelfImprovementContext {
            goal: String::new(),
            self_model: String::new(),
            architecture: String::new(),
            recent_modifications: String::new(),
            relevant_code: CodeContext::new(),
            suggestions: vec![],
        };
        assert_eq!(sic.estimate_tokens(), 0);
    }

    #[test]
    fn test_self_improvement_context_estimate_tokens_basic() {
        // 40 chars total base + 0 code + 0 suggestions => 40/4 = 10
        let sic = SelfImprovementContext {
            goal: "improve speed".to_string(),                // 13
            self_model: "model v1".to_string(),               // 8
            architecture: "modular".to_string(),              // 7
            recent_modifications: "refactored x".to_string(), // 12 = 40 total
            relevant_code: CodeContext::new(),
            suggestions: vec![],
        };
        assert_eq!(sic.estimate_tokens(), 40 / 4);
    }

    #[test]
    fn test_self_improvement_context_estimate_tokens_with_code() {
        let mut cc = CodeContext::new();
        cc.files.push(FileContext {
            path: "a.rs".to_string(),
            content: "x".repeat(40), // 40 chars
            language: "rust".to_string(),
            estimated_tokens: 0,
            relevance_score: 0.5,
        });
        let sic = SelfImprovementContext {
            goal: "g".to_string(),         // 1
            self_model: "m".to_string(),   // 1
            architecture: "a".to_string(), // 1
            recent_modifications: String::new(),
            relevant_code: cc,
            suggestions: vec![],
        };
        // base = 3, code = 40, total = 43, 43/4 = 10 (integer division)
        assert_eq!(sic.estimate_tokens(), (3 + 40) / 4);
    }

    #[test]
    fn test_self_improvement_context_estimate_tokens_with_suggestions() {
        let sic = SelfImprovementContext {
            goal: String::new(),
            self_model: String::new(),
            architecture: String::new(),
            recent_modifications: String::new(),
            relevant_code: CodeContext::new(),
            suggestions: vec!["do A".to_string(), "do B".to_string()], // 4 + 4 = 8
        };
        assert_eq!(sic.estimate_tokens(), 8 / 4);
    }

    #[test]
    fn test_self_improvement_context_to_prompt_has_goal() {
        let sic = SelfImprovementContext {
            goal: "improve latency".to_string(),
            self_model: "model v2".to_string(),
            architecture: "event-driven".to_string(),
            recent_modifications: String::new(),
            relevant_code: CodeContext::new(),
            suggestions: vec![],
        };
        let prompt = sic.to_prompt();
        assert!(prompt.contains("Self-Improvement Context"));
        assert!(prompt.contains("improve latency"));
        assert!(prompt.contains("model v2"));
        assert!(prompt.contains("event-driven"));
    }

    #[test]
    fn test_self_improvement_context_to_prompt_includes_modifications() {
        let sic = SelfImprovementContext {
            goal: "g".to_string(),
            self_model: "m".to_string(),
            architecture: "a".to_string(),
            recent_modifications: "changed handler".to_string(),
            relevant_code: CodeContext::new(),
            suggestions: vec![],
        };
        let prompt = sic.to_prompt();
        assert!(prompt.contains("Recent Modifications: changed handler"));
    }

    #[test]
    fn test_self_improvement_context_to_prompt_omits_empty_modifications() {
        let sic = SelfImprovementContext {
            goal: "g".to_string(),
            self_model: "m".to_string(),
            architecture: "a".to_string(),
            recent_modifications: String::new(),
            relevant_code: CodeContext::new(),
            suggestions: vec![],
        };
        let prompt = sic.to_prompt();
        assert!(!prompt.contains("Recent Modifications:"));
    }

    #[test]
    fn test_self_improvement_context_to_prompt_includes_suggestions() {
        let sic = SelfImprovementContext {
            goal: "g".to_string(),
            self_model: "m".to_string(),
            architecture: "a".to_string(),
            recent_modifications: String::new(),
            relevant_code: CodeContext::new(),
            suggestions: vec!["optimize loop".to_string(), "cache results".to_string()],
        };
        let prompt = sic.to_prompt();
        assert!(prompt.contains("Suggestions to Consider:"));
        assert!(prompt.contains("- optimize loop"));
        assert!(prompt.contains("- cache results"));
    }

    #[test]
    fn test_self_improvement_context_to_prompt_no_suggestions_header_present() {
        let sic = SelfImprovementContext {
            goal: "g".to_string(),
            self_model: "m".to_string(),
            architecture: "a".to_string(),
            recent_modifications: String::new(),
            relevant_code: CodeContext::new(),
            suggestions: vec![],
        };
        let prompt = sic.to_prompt();
        // Header should still be present even if no suggestions
        assert!(prompt.contains("Suggestions to Consider:"));
        // But no list items
        assert!(!prompt.contains("\n- "));
    }

    // ========================================================================
    // ChangeType
    // ========================================================================

    #[test]
    fn test_change_type_variants_not_equal() {
        assert_ne!(ChangeType::Addition, ChangeType::Deletion);
        assert_ne!(ChangeType::Modification, ChangeType::Refactor);
        assert_ne!(ChangeType::Addition, ChangeType::Modification);
    }

    #[test]
    fn test_change_type_serde_roundtrip() {
        for ct in [
            ChangeType::Addition,
            ChangeType::Deletion,
            ChangeType::Modification,
            ChangeType::Refactor,
        ] {
            let json = serde_json::to_string(&ct).unwrap();
            let back: ChangeType = serde_json::from_str(&json).unwrap();
            assert_eq!(ct, back);
        }
    }

    // ========================================================================
    // MemoryTier
    // ========================================================================

    #[test]
    fn test_memory_tier_ordering() {
        assert!(MemoryTier::Working < MemoryTier::ShortTerm);
        assert!(MemoryTier::ShortTerm < MemoryTier::LongTerm);
        assert!(MemoryTier::LongTerm < MemoryTier::Archive);
    }

    #[test]
    fn test_memory_tier_hash_and_eq() {
        let mut set = std::collections::HashSet::new();
        set.insert(MemoryTier::Working);
        assert!(set.contains(&MemoryTier::Working));
        assert!(!set.contains(&MemoryTier::LongTerm));
    }

    #[test]
    fn test_memory_tier_serde_roundtrip() {
        let tier = MemoryTier::LongTerm;
        let json = serde_json::to_string(&tier).unwrap();
        let back: MemoryTier = serde_json::from_str(&json).unwrap();
        assert_eq!(tier, back);
    }

    // ========================================================================
    // MemoryEntry
    // ========================================================================

    #[test]
    fn test_memory_entry_new_defaults() {
        let entry = MemoryEntry::new(42, "test content", MemoryTier::Working);
        assert_eq!(entry.id, 42);
        assert_eq!(entry.content, "test content");
        assert_eq!(entry.tier, MemoryTier::Working);
        assert!(entry.created_at > 0);
        assert_eq!(entry.accessed_at, entry.created_at);
        assert_eq!(entry.access_count, 0);
        assert_eq!(entry.importance, 0.5);
        assert!(entry.tags.is_empty());
        assert!(entry.metadata.is_empty());
    }

    #[test]
    fn test_memory_entry_accessed_increments() {
        let mut entry = MemoryEntry::new(1, "content", MemoryTier::ShortTerm);
        let initial_accessed_at = entry.accessed_at;
        std::thread::sleep(std::time::Duration::from_millis(5));

        entry.accessed();
        assert_eq!(entry.access_count, 1);
        assert!(entry.accessed_at >= initial_accessed_at);

        entry.accessed();
        assert_eq!(entry.access_count, 2);
    }

    #[test]
    fn test_memory_entry_with_importance_clamps_high() {
        let entry = MemoryEntry::new(1, "c", MemoryTier::Working).with_importance(5.0);
        assert_eq!(entry.importance, 1.0);
    }

    #[test]
    fn test_memory_entry_with_importance_clamps_low() {
        let entry = MemoryEntry::new(1, "c", MemoryTier::Working).with_importance(-3.0);
        assert_eq!(entry.importance, 0.0);
    }

    #[test]
    fn test_memory_entry_with_importance_in_range() {
        let entry = MemoryEntry::new(1, "c", MemoryTier::Working).with_importance(0.7);
        assert_eq!(entry.importance, 0.7);
    }

    #[test]
    fn test_memory_entry_with_tags() {
        let entry = MemoryEntry::new(1, "c", MemoryTier::Working)
            .with_tags(vec!["rust".to_string(), "test".to_string()]);
        assert_eq!(entry.tags, vec!["rust".to_string(), "test".to_string()]);
    }

    #[test]
    fn test_memory_entry_with_metadata() {
        let entry = MemoryEntry::new(1, "c", MemoryTier::Working)
            .with_metadata("source", serde_json::json!("unit-test"));
        assert_eq!(
            entry.metadata.get("source"),
            Some(&serde_json::json!("unit-test"))
        );
    }

    #[test]
    fn test_memory_entry_builder_chain() {
        let entry = MemoryEntry::new(10, "content", MemoryTier::LongTerm)
            .with_importance(0.9)
            .with_tags(vec!["important".to_string()])
            .with_metadata("key", serde_json::json!(42));
        assert_eq!(entry.id, 10);
        assert_eq!(entry.tier, MemoryTier::LongTerm);
        assert_eq!(entry.importance, 0.9);
        assert_eq!(entry.tags, vec!["important".to_string()]);
        assert_eq!(entry.metadata.get("key"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn test_memory_entry_serde_roundtrip() {
        let entry = MemoryEntry::new(99, "serialized", MemoryTier::Archive)
            .with_importance(0.8)
            .with_tags(vec!["a".to_string(), "b".to_string()])
            .with_metadata("m", serde_json::json!({"nested": true}));
        let json = serde_json::to_string(&entry).unwrap();
        let back: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 99);
        assert_eq!(back.content, "serialized");
        assert_eq!(back.tier, MemoryTier::Archive);
        assert_eq!(back.importance, 0.8);
        assert_eq!(back.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            back.metadata.get("m"),
            Some(&serde_json::json!({"nested": true}))
        );
    }

    // ========================================================================
    // MemoryConfig
    // ========================================================================

    #[test]
    fn test_memory_config_default() {
        let cfg = MemoryConfig::default();
        assert_eq!(cfg.working_capacity, 10);
        assert_eq!(cfg.short_term_capacity, 100);
        assert_eq!(cfg.long_term_capacity, 1000);
        assert_eq!(cfg.promotion_threshold, 3);
        assert_eq!(cfg.demotion_threshold, 3600);
        assert_eq!(cfg.importance_threshold, 0.7);
    }

    // ========================================================================
    // MemoryStats
    // ========================================================================

    #[test]
    fn test_memory_stats_default_all_zero() {
        let stats = MemoryStats::default();
        assert_eq!(stats.working_count, 0);
        assert_eq!(stats.short_term_count, 0);
        assert_eq!(stats.long_term_count, 0);
        assert_eq!(stats.total_inserts, 0);
        assert_eq!(stats.total_queries, 0);
        assert_eq!(stats.total_promotions, 0);
        assert_eq!(stats.total_demotions, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert!(stats.budget.is_none());
        assert!(stats.usage.is_none());
        assert!(stats.metrics.is_none());
        assert_eq!(stats.working_entries, 0);
        assert_eq!(stats.episodic_entries, 0);
        assert_eq!(stats.semantic_files, 0);
    }

    // ========================================================================
    // MemoryQuery
    // ========================================================================

    #[test]
    fn test_memory_query_new_defaults() {
        let q = MemoryQuery::new("test pattern");
        assert_eq!(q.pattern, "test pattern");
        assert!(q.tier.is_none());
        assert!(q.tags.is_empty());
        assert!(q.min_importance.is_none());
        assert!(q.since.is_none());
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn test_memory_query_builder_chain() {
        let q = MemoryQuery::new("search")
            .with_tier(MemoryTier::LongTerm)
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
            .with_min_importance(0.6)
            .with_limit(50)
            .since(1234567890);

        assert_eq!(q.pattern, "search");
        assert_eq!(q.tier, Some(MemoryTier::LongTerm));
        assert_eq!(q.tags, vec!["tag1".to_string(), "tag2".to_string()]);
        assert_eq!(q.min_importance, Some(0.6));
        assert_eq!(q.limit, Some(50));
        assert_eq!(q.since, Some(1234567890));
    }

    #[test]
    fn test_memory_query_with_tier_sets_correctly() {
        let q = MemoryQuery::new("p").with_tier(MemoryTier::Archive);
        assert_eq!(q.tier, Some(MemoryTier::Archive));
    }

    #[test]
    fn test_memory_query_with_min_importance_sets_value() {
        let q = MemoryQuery::new("p").with_min_importance(0.3);
        assert_eq!(q.min_importance, Some(0.3));
    }

    #[test]
    fn test_memory_query_with_limit_overrides_default() {
        let q = MemoryQuery::new("p").with_limit(100);
        assert_eq!(q.limit, Some(100));
    }

    #[test]
    fn test_memory_query_since_sets_timestamp() {
        let q = MemoryQuery::new("p").since(99);
        assert_eq!(q.since, Some(99));
    }

    // ========================================================================
    // MemoryIndex (async tests)
    // ========================================================================

    #[test]
    fn test_memory_index_new_starts_counter_at_one() {
        let idx = MemoryIndex::new();
        assert_eq!(idx.next_id(), 1);
        assert_eq!(idx.next_id(), 2);
        assert_eq!(idx.next_id(), 3);
    }

    #[test]
    fn test_memory_index_default_equals_new() {
        let idx = MemoryIndex::default();
        assert_eq!(idx.next_id(), 1);
    }

    #[tokio::test]
    async fn test_memory_index_index_entry_and_get_by_tier() {
        let idx = MemoryIndex::new();
        let entry = MemoryEntry::new(100, "content", MemoryTier::Working)
            .with_tags(vec!["alpha".to_string()]);
        idx.index_entry(&entry).await;

        let ids = idx.get_by_tier(MemoryTier::Working).await;
        assert_eq!(ids, vec![100]);

        // Other tier should be empty
        let ids_lt = idx.get_by_tier(MemoryTier::LongTerm).await;
        assert!(ids_lt.is_empty());
    }

    #[tokio::test]
    async fn test_memory_index_index_entry_and_get_by_tag() {
        let idx = MemoryIndex::new();
        let entry = MemoryEntry::new(200, "content", MemoryTier::ShortTerm)
            .with_tags(vec!["beta".to_string(), "gamma".to_string()]);
        idx.index_entry(&entry).await;

        let ids_beta = idx.get_by_tag("beta").await;
        assert_eq!(ids_beta, vec![200]);

        let ids_gamma = idx.get_by_tag("gamma").await;
        assert_eq!(ids_gamma, vec![200]);

        let ids_missing = idx.get_by_tag("nonexistent").await;
        assert!(ids_missing.is_empty());
    }

    #[tokio::test]
    async fn test_memory_index_multiple_entries_same_tier() {
        let idx = MemoryIndex::new();
        let e1 = MemoryEntry::new(1, "a", MemoryTier::Working);
        let e2 = MemoryEntry::new(2, "b", MemoryTier::Working);
        let e3 = MemoryEntry::new(3, "c", MemoryTier::LongTerm);
        idx.index_entry(&e1).await;
        idx.index_entry(&e2).await;
        idx.index_entry(&e3).await;

        let working_ids = idx.get_by_tier(MemoryTier::Working).await;
        assert_eq!(working_ids.len(), 2);
        assert!(working_ids.contains(&1));
        assert!(working_ids.contains(&2));

        let long_ids = idx.get_by_tier(MemoryTier::LongTerm).await;
        assert_eq!(long_ids, vec![3]);
    }

    #[tokio::test]
    async fn test_memory_index_multiple_entries_same_tag() {
        let idx = MemoryIndex::new();
        let e1 =
            MemoryEntry::new(1, "a", MemoryTier::Working).with_tags(vec!["shared".to_string()]);
        let e2 =
            MemoryEntry::new(2, "b", MemoryTier::LongTerm).with_tags(vec!["shared".to_string()]);
        idx.index_entry(&e1).await;
        idx.index_entry(&e2).await;

        let ids = idx.get_by_tag("shared").await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[tokio::test]
    async fn test_memory_index_remove_entry() {
        let idx = MemoryIndex::new();
        let entry = MemoryEntry::new(50, "content", MemoryTier::Working)
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()]);
        idx.index_entry(&entry).await;

        // Verify it's indexed
        assert_eq!(idx.get_by_tier(MemoryTier::Working).await, vec![50]);
        assert_eq!(idx.get_by_tag("tag1").await, vec![50]);

        idx.remove_entry(&entry).await;

        // After removal, tier and tag lookups should be empty
        assert!(idx.get_by_tier(MemoryTier::Working).await.is_empty());
        assert!(idx.get_by_tag("tag1").await.is_empty());
        assert!(idx.get_by_tag("tag2").await.is_empty());
    }

    #[tokio::test]
    async fn test_memory_index_remove_entry_preserves_others() {
        let idx = MemoryIndex::new();
        let e1 =
            MemoryEntry::new(1, "a", MemoryTier::Working).with_tags(vec!["shared".to_string()]);
        let e2 =
            MemoryEntry::new(2, "b", MemoryTier::Working).with_tags(vec!["shared".to_string()]);
        idx.index_entry(&e1).await;
        idx.index_entry(&e2).await;

        idx.remove_entry(&e1).await;

        let working = idx.get_by_tier(MemoryTier::Working).await;
        assert_eq!(working, vec![2]);

        let tagged = idx.get_by_tag("shared").await;
        assert_eq!(tagged, vec![2]);
    }

    #[tokio::test]
    async fn test_memory_index_get_by_tag_missing_returns_empty() {
        let idx = MemoryIndex::new();
        assert!(idx.get_by_tag("nope").await.is_empty());
    }

    #[tokio::test]
    async fn test_memory_index_get_by_tier_missing_returns_empty() {
        let idx = MemoryIndex::new();
        assert!(idx.get_by_tier(MemoryTier::Archive).await.is_empty());
    }

    #[tokio::test]
    async fn test_memory_index_remove_nonexistent_entry_is_noop() {
        let idx = MemoryIndex::new();
        let real = MemoryEntry::new(1, "a", MemoryTier::Working).with_tags(vec!["t".to_string()]);
        idx.index_entry(&real).await;

        // Remove a non-indexed entry — should not affect existing data
        let phantom = MemoryEntry::new(999, "ghost", MemoryTier::Working);
        idx.remove_entry(&phantom).await;

        assert_eq!(idx.get_by_tier(MemoryTier::Working).await, vec![1]);
    }

    #[tokio::test]
    async fn test_memory_index_entry_with_no_tags() {
        let idx = MemoryIndex::new();
        let entry = MemoryEntry::new(1, "no tags", MemoryTier::Working);
        idx.index_entry(&entry).await;

        // Should be indexed by tier but not by any tag
        assert_eq!(idx.get_by_tier(MemoryTier::Working).await, vec![1]);
        assert!(idx.get_by_tag("anything").await.is_empty());
    }

    // ========================================================================
    // TierTransition
    // ========================================================================

    #[test]
    fn test_tier_transition_variants_distinct() {
        use std::mem::discriminant;
        assert_ne!(
            discriminant(&TierTransition::Promote),
            discriminant(&TierTransition::Demote)
        );
        assert_ne!(
            discriminant(&TierTransition::Demote),
            discriminant(&TierTransition::Keep)
        );
        assert_ne!(
            discriminant(&TierTransition::Promote),
            discriminant(&TierTransition::Keep)
        );
    }

    // ========================================================================
    // ConsolidationResult
    // ========================================================================

    #[test]
    fn test_consolidation_result_construction() {
        let summaries = vec![MemoryEntry::new(1, "summary", MemoryTier::LongTerm)];
        let result = ConsolidationResult {
            entries_merged: 5,
            entries_removed: 3,
            new_summaries: summaries,
        };
        assert_eq!(result.entries_merged, 5);
        assert_eq!(result.entries_removed, 3);
        assert_eq!(result.new_summaries.len(), 1);
        assert_eq!(result.new_summaries[0].id, 1);
    }

    // ========================================================================
    // CodeContent
    // ========================================================================

    #[test]
    fn test_code_content_full_serde_roundtrip() {
        let cc = CodeContent::Full("let x = 42;".to_string());
        let json = serde_json::to_string(&cc).unwrap();
        let back: CodeContent = serde_json::from_str(&json).unwrap();
        match back {
            CodeContent::Full(s) => assert_eq!(s, "let x = 42;"),
            _ => panic!("Expected CodeContent::Full"),
        }
    }

    #[test]
    fn test_code_content_summary_serde_roundtrip() {
        let cc = CodeContent::Summary {
            overview: "module overview".to_string(),
            key_functions: vec!["fn1".to_string(), "fn2".to_string()],
        };
        let json = serde_json::to_string(&cc).unwrap();
        let back: CodeContent = serde_json::from_str(&json).unwrap();
        match back {
            CodeContent::Summary {
                overview,
                key_functions,
            } => {
                assert_eq!(overview, "module overview");
                assert_eq!(key_functions, vec!["fn1".to_string(), "fn2".to_string()]);
            }
            _ => panic!("Expected CodeContent::Summary"),
        }
    }

    // ========================================================================
    // ActiveCodeContext
    // ========================================================================

    #[test]
    fn test_active_code_context_serde_roundtrip() {
        let acc = ActiveCodeContext {
            path: "src/main.rs".to_string(),
            content: CodeContent::Full("fn main() {}".to_string()),
            last_accessed: 12345,
            edit_history: vec![CodeEdit {
                timestamp: 100,
                description: "initial".to_string(),
                lines_changed: (1, 5),
            }],
        };
        let json = serde_json::to_string(&acc).unwrap();
        let back: ActiveCodeContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "src/main.rs");
        assert_eq!(back.last_accessed, 12345);
        assert_eq!(back.edit_history.len(), 1);
        assert_eq!(back.edit_history[0].lines_changed, (1, 5));
    }

    // ========================================================================
    // FileContextEntry
    // ========================================================================

    #[test]
    fn test_file_context_entry_serde_roundtrip() {
        let fce = FileContextEntry {
            path: "lib.rs".to_string(),
            content: "content".to_string(),
            relevance_score: 0.85,
        };
        let json = serde_json::to_string(&fce).unwrap();
        let back: FileContextEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "lib.rs");
        assert_eq!(back.content, "content");
        assert!((back.relevance_score - 0.85).abs() < f32::EPSILON);
    }

    // ========================================================================
    // TaskContext
    // ========================================================================

    #[test]
    fn test_task_context_serde_roundtrip() {
        let tc = TaskContext {
            description: "fix bug".to_string(),
            goal: "all tests pass".to_string(),
            progress: vec!["found bug".to_string()],
            next_steps: vec!["write fix".to_string(), "run tests".to_string()],
            relevant_files: vec!["src/lib.rs".to_string()],
        };
        let json = serde_json::to_string(&tc).unwrap();
        let back: TaskContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.description, "fix bug");
        assert_eq!(back.goal, "all tests pass");
        assert_eq!(back.progress, vec!["found bug".to_string()]);
        assert_eq!(back.next_steps.len(), 2);
        assert_eq!(back.relevant_files, vec!["src/lib.rs".to_string()]);
    }

    // ========================================================================
    // SelfModel
    // ========================================================================

    #[test]
    fn test_self_model_serde_roundtrip() {
        let sm = SelfModel {
            version: "1.0.0".to_string(),
            capabilities: vec!["code".to_string(), "test".to_string()],
            limitations: vec!["no browser".to_string()],
            recent_changes: vec!["refactored".to_string()],
            modules: vec!["agent".to_string()],
        };
        let json = serde_json::to_string(&sm).unwrap();
        let back: SelfModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, "1.0.0");
        assert_eq!(
            back.capabilities,
            vec!["code".to_string(), "test".to_string()]
        );
        assert_eq!(back.limitations, vec!["no browser".to_string()]);
        assert_eq!(back.recent_changes, vec!["refactored".to_string()]);
        assert_eq!(back.modules, vec!["agent".to_string()]);
    }

    // ========================================================================
    // CodeModification
    // ========================================================================

    #[test]
    fn test_code_modification_serde_roundtrip() {
        let cm = CodeModification {
            id: "mod-1".to_string(),
            timestamp: 999,
            file_path: "src/lib.rs".to_string(),
            change_type: ChangeType::Refactor,
            description: "renamed function".to_string(),
            success: true,
        };
        let json = serde_json::to_string(&cm).unwrap();
        let back: CodeModification = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "mod-1");
        assert_eq!(back.timestamp, 999);
        assert_eq!(back.file_path, "src/lib.rs");
        assert_eq!(back.change_type, ChangeType::Refactor);
        assert_eq!(back.description, "renamed function");
        assert!(back.success);
    }

    // ========================================================================
    // FileContext
    // ========================================================================

    #[test]
    fn test_file_context_serde_roundtrip() {
        let fc = FileContext {
            path: "main.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: "rust".to_string(),
            estimated_tokens: 10,
            relevance_score: 0.5,
        };
        let json = serde_json::to_string(&fc).unwrap();
        let back: FileContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "main.rs");
        assert_eq!(back.content, "fn main() {}");
        assert_eq!(back.language, "rust");
        assert_eq!(back.estimated_tokens, 10);
    }

    // ========================================================================
    // SymbolContext
    // ========================================================================

    #[test]
    fn test_symbol_context_serde_roundtrip() {
        let sc = SymbolContext {
            name: "main".to_string(),
            symbol_type: "function".to_string(),
            file_path: "main.rs".to_string(),
            line_start: 1,
            line_end: 3,
        };
        let json = serde_json::to_string(&sc).unwrap();
        let back: SymbolContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "main");
        assert_eq!(back.symbol_type, "function");
        assert_eq!(back.file_path, "main.rs");
        assert_eq!(back.line_start, 1);
        assert_eq!(back.line_end, 3);
    }

    // ========================================================================
    // ActiveCodeCollection
    // ========================================================================

    #[test]
    fn test_active_code_collection_serde_roundtrip() {
        let acc = ActiveCodeCollection {
            files: vec![ActiveCodeContext {
                path: "a.rs".to_string(),
                content: CodeContent::Full("x".to_string()),
                last_accessed: 1,
                edit_history: vec![],
            }],
        };
        let json = serde_json::to_string(&acc).unwrap();
        let back: ActiveCodeCollection = serde_json::from_str(&json).unwrap();
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.files[0].path, "a.rs");
    }
}
