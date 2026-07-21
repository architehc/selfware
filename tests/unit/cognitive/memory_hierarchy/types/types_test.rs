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
    let ep = Episode::new("ep-2", EpisodeType::Error, "oops").with_importance(Importance::Critical);
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
    assert_eq!(sic.estimate_tokens(), 40_usize.div_ceil(4));
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
    let entry =
        MemoryEntry::new(100, "content", MemoryTier::Working).with_tags(vec!["alpha".to_string()]);
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
    let e1 = MemoryEntry::new(1, "a", MemoryTier::Working).with_tags(vec!["shared".to_string()]);
    let e2 = MemoryEntry::new(2, "b", MemoryTier::LongTerm).with_tags(vec!["shared".to_string()]);
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
    let e1 = MemoryEntry::new(1, "a", MemoryTier::Working).with_tags(vec!["shared".to_string()]);
    let e2 = MemoryEntry::new(2, "b", MemoryTier::Working).with_tags(vec!["shared".to_string()]);
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
