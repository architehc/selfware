use super::*;

// ── EpisodicMemory ────────────────────────────────────────────

#[test]
fn test_episodic_memory_new() {
    let em = EpisodicMemory::new();
    // Just verify construction succeeds (Default impl delegates to new)
    let em2 = EpisodicMemory::default();
    drop(em);
    drop(em2);
}

#[tokio::test]
async fn test_episodic_memory_add_and_retrieve() {
    let em = EpisodicMemory::new();
    let ep = Episode::new("ep-1", EpisodeType::Action, "did something");
    em.add(ep).await;

    let results = em
        .retrieve_relevant("query", 10, Importance::Low)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "ep-1");
}

#[tokio::test]
async fn test_episodic_memory_filter_by_importance() {
    let em = EpisodicMemory::new();

    let low = Episode::new("ep-low", EpisodeType::Thought, "low importance")
        .with_importance(Importance::Low);
    let high = Episode::new("ep-high", EpisodeType::Success, "high importance")
        .with_importance(Importance::High);

    em.add(low).await;
    em.add(high).await;

    let results = em
        .retrieve_relevant("q", 10, Importance::High)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "ep-high");
}

#[tokio::test]
async fn test_episodic_memory_truncate_limit() {
    let em = EpisodicMemory::new();
    for i in 0..5 {
        em.add(Episode::new(format!("ep-{i}"), EpisodeType::Action, "x"))
            .await;
    }

    let results = em.retrieve_relevant("q", 2, Importance::Low).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_episodic_memory_clone() {
    let em = EpisodicMemory::new();
    let em2 = em.clone();
    drop(em2);
}

// ── SemanticMemory ───────��────────────────────────────────────

#[test]
fn test_semantic_memory_new() {
    let sm = SemanticMemory::new();
    let sm2 = SemanticMemory::default();
    drop(sm);
    drop(sm2);
}

#[test]
fn test_semantic_memory_clone() {
    let sm = SemanticMemory::new();
    let _sm2 = sm.clone();
}

#[tokio::test]
async fn test_semantic_memory_retrieve_code_context_empty() {
    let sm = SemanticMemory::new();
    let ctx = sm
        .retrieve_code_context("anything", 1000, true)
        .await
        .unwrap();
    assert!(ctx.files.is_empty());
    assert!(ctx.symbols.is_empty());
    assert_eq!(ctx.total_tokens, 0);
}

/// Helper: create a non-dot-prefixed subdirectory inside a tempdir.
/// The production `index_codebase` uses walkdir with `filter_entry`
/// that skips directory names starting with '.', and Rust's tempfile
/// crate creates dirs like `.tmpXXXXXX`, so we nest under a regular name.
fn test_dir(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    let dir = tmp.path().join("project");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn test_semantic_memory_index_codebase() {
    let tmp = tempfile::tempdir().unwrap();
    let base = test_dir(&tmp);
    std::fs::write(base.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(base.join("lib.py"), "def hello(): pass").unwrap();
    std::fs::write(base.join("notes.md"), "# Notes").unwrap();
    // A file with unsupported extension should be skipped
    std::fs::write(base.join("data.bin"), "binary").unwrap();

    let sm = SemanticMemory::new();
    sm.index_codebase(&base).await.unwrap();

    let ctx = sm.retrieve_code_context("q", 100_000, false).await.unwrap();
    // Should have indexed main.rs, lib.py, notes.md but not data.bin
    assert_eq!(ctx.files.len(), 3);
}

#[tokio::test]
async fn test_semantic_memory_index_skips_hidden_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let base = test_dir(&tmp);
    let hidden = base.join(".hidden");
    std::fs::create_dir_all(&hidden).unwrap();
    std::fs::write(hidden.join("secret.rs"), "fn secret() {}").unwrap();
    // Also put a normal file
    std::fs::write(base.join("visible.rs"), "fn visible() {}").unwrap();

    let sm = SemanticMemory::new();
    sm.index_codebase(&base).await.unwrap();

    let ctx = sm.retrieve_code_context("q", 100_000, false).await.unwrap();
    assert_eq!(ctx.files.len(), 1);
    assert_eq!(ctx.files[0].path, "visible.rs");
}

#[tokio::test]
async fn test_semantic_memory_language_detection() {
    let tmp = tempfile::tempdir().unwrap();
    let base = test_dir(&tmp);
    std::fs::write(base.join("a.rs"), "x").unwrap();
    std::fs::write(base.join("b.py"), "x").unwrap();
    std::fs::write(base.join("c.ts"), "x").unwrap();
    std::fs::write(base.join("d.js"), "x").unwrap();
    std::fs::write(base.join("e.toml"), "x").unwrap();
    std::fs::write(base.join("f.yaml"), "x").unwrap();
    std::fs::write(base.join("g.yml"), "x").unwrap();
    std::fs::write(base.join("h.json"), "x").unwrap();
    std::fs::write(base.join("i.md"), "x").unwrap();

    let sm = SemanticMemory::new();
    sm.index_codebase(&base).await.unwrap();

    let ctx = sm.retrieve_code_context("q", 100_000, false).await.unwrap();

    let langs: std::collections::HashSet<_> =
        ctx.files.iter().map(|f| f.language.as_str()).collect();
    assert!(langs.contains("rust"));
    assert!(langs.contains("python"));
    assert!(langs.contains("javascript")); // ts and js both map to javascript
    assert!(langs.contains("toml"));
    assert!(langs.contains("yaml")); // yaml and yml
    assert!(langs.contains("json"));
    assert!(langs.contains("markdown"));
}

#[tokio::test]
async fn test_semantic_memory_max_tokens_capped() {
    let tmp = tempfile::tempdir().unwrap();
    let base = test_dir(&tmp);
    std::fs::write(base.join("main.rs"), "fn main() {}").unwrap();

    let sm = SemanticMemory::new();
    sm.index_codebase(&base).await.unwrap();

    // Retrieve with a very small max_tokens
    let ctx = sm.retrieve_code_context("q", 1, false).await.unwrap();
    assert!(ctx.total_tokens <= 1);
}

// ── HierarchicalMemory ─────���──────────────────────────────────

#[tokio::test]
async fn test_hierarchical_memory_default() {
    let hm = HierarchicalMemory::default().await.unwrap();
    assert!(hm.is_within_budget());
}

#[tokio::test]
async fn test_hierarchical_memory_store_and_retrieve() {
    let hm = HierarchicalMemory::default().await.unwrap();

    let id = hm.store("hello world", MemoryTier::Working).await.unwrap();
    let entry = hm.retrieve(id).await;
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().content, "hello world");
}

#[tokio::test]
async fn test_hierarchical_memory_store_each_tier() {
    let hm = HierarchicalMemory::default().await.unwrap();

    let w = hm.store("w", MemoryTier::Working).await.unwrap();
    let s = hm.store("s", MemoryTier::ShortTerm).await.unwrap();
    let l = hm.store("l", MemoryTier::LongTerm).await.unwrap();
    let a = hm.store("a", MemoryTier::Archive).await.unwrap();

    assert!(hm.retrieve(w).await.is_some());
    assert!(hm.retrieve(s).await.is_some());
    assert!(hm.retrieve(l).await.is_some());
    assert!(hm.retrieve(a).await.is_some());
}

#[tokio::test]
async fn test_hierarchical_memory_retrieve_missing() {
    let hm = HierarchicalMemory::default().await.unwrap();
    assert!(hm.retrieve(999999).await.is_none());
}

#[tokio::test]
async fn test_hierarchical_memory_query_all_tiers() {
    let hm = HierarchicalMemory::default().await.unwrap();
    hm.store("alpha data", MemoryTier::Working).await.unwrap();
    hm.store("alpha info", MemoryTier::ShortTerm).await.unwrap();
    hm.store("alpha note", MemoryTier::LongTerm).await.unwrap();

    let q = MemoryQuery::new("alpha").with_limit(10);
    let results = hm.query(q).await;
    // All three entries should be found across tiers
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_hierarchical_memory_query_specific_tier() {
    let hm = HierarchicalMemory::default().await.unwrap();
    hm.store("foo", MemoryTier::Working).await.unwrap();
    hm.store("foo", MemoryTier::ShortTerm).await.unwrap();

    let q = MemoryQuery::new("foo").with_tier(MemoryTier::ShortTerm);
    let results = hm.query(q).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tier, MemoryTier::ShortTerm);
}

#[tokio::test]
async fn test_hierarchical_memory_query_with_limit() {
    let hm = HierarchicalMemory::default().await.unwrap();
    for i in 0..5 {
        hm.store(format!("item {i}"), MemoryTier::Working)
            .await
            .unwrap();
    }

    let q = MemoryQuery::new("item").with_limit(2);
    let results = hm.query(q).await;
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_hierarchical_memory_promote_from_short_term() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("promote me", MemoryTier::ShortTerm).await.unwrap();

    hm.promote(id).await.unwrap();

    // After promotion, entry should be retrievable via working memory
    let entry = hm.working().retrieve(id).await;
    assert!(entry.is_some());
}

#[tokio::test]
async fn test_hierarchical_memory_promote_from_long_term() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("promote me", MemoryTier::LongTerm).await.unwrap();

    hm.promote(id).await.unwrap();

    // After promotion from long-term, entry should be in short-term
    let entry = hm.retrieve(id).await;
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().tier, MemoryTier::ShortTerm);
}

#[tokio::test]
async fn test_hierarchical_memory_promote_already_working() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("already top", MemoryTier::Working).await.unwrap();

    // Should be a no-op, not an error
    hm.promote(id).await.unwrap();
}

#[tokio::test]
async fn test_hierarchical_memory_promote_not_found() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let result = hm.promote(999999).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_hierarchical_memory_demote_from_working() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("demote me", MemoryTier::Working).await.unwrap();

    hm.demote(id).await.unwrap();

    // After demotion, the entry should be retrievable from short-term
    let entry = hm.short_term().retrieve(id).await;
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().tier, MemoryTier::ShortTerm);
}

#[tokio::test]
async fn test_hierarchical_memory_demote_from_short_term() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("demote me", MemoryTier::ShortTerm).await.unwrap();

    hm.demote(id).await.unwrap();

    // After demotion, entry should be in long-term
    let entry = hm.long_term().retrieve(id).await;
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().tier, MemoryTier::LongTerm);
}

#[tokio::test]
async fn test_hierarchical_memory_demote_not_found() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let result = hm.demote(999999).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_hierarchical_memory_stats() {
    let hm = HierarchicalMemory::default().await.unwrap();
    hm.store("a", MemoryTier::Working).await.unwrap();
    hm.store("b", MemoryTier::ShortTerm).await.unwrap();

    let stats = hm.stats().await;
    assert_eq!(stats.working_count, 1);
    assert_eq!(stats.short_term_count, 1);
    assert_eq!(stats.total_inserts, 2);
}

#[tokio::test]
async fn test_hierarchical_memory_stats_tracks_queries() {
    let hm = HierarchicalMemory::default().await.unwrap();
    hm.store("x", MemoryTier::Working).await.unwrap();

    let q = MemoryQuery::new("x");
    hm.query(q).await;

    let stats = hm.stats().await;
    assert_eq!(stats.total_queries, 1);
}

#[tokio::test]
async fn test_hierarchical_memory_stats_tracks_promotions() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("p", MemoryTier::ShortTerm).await.unwrap();
    hm.promote(id).await.unwrap();

    let stats = hm.stats().await;
    assert_eq!(stats.total_promotions, 1);
}

#[tokio::test]
async fn test_hierarchical_memory_stats_tracks_demotions() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let id = hm.store("d", MemoryTier::Working).await.unwrap();
    hm.demote(id).await.unwrap();

    let stats = hm.stats().await;
    assert_eq!(stats.total_demotions, 1);
}

#[tokio::test]
async fn test_hierarchical_memory_get_stats() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let stats = hm.get_stats().await;
    assert_eq!(stats.total_inserts, 0);
}

#[tokio::test]
async fn test_hierarchical_memory_index() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let idx = hm.index();
    // next_id should return incrementing values
    let a = idx.next_id();
    let b = idx.next_id();
    assert!(b > a);
}

#[tokio::test]
async fn test_hierarchical_memory_accessors() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let _ = hm.working();
    let _ = hm.short_term();
    let _ = hm.long_term();
}

#[tokio::test]
async fn test_hierarchical_memory_record_episode() {
    let mut hm = HierarchicalMemory::default().await.unwrap();
    let ep = Episode::new("ep-1", EpisodeType::Learning, "learned something");
    hm.record_episode(ep).await.unwrap();

    let episodes = hm
        .episodic
        .retrieve_relevant("q", 10, Importance::Low)
        .await
        .unwrap();
    assert_eq!(episodes.len(), 1);
}

#[tokio::test]
async fn test_hierarchical_memory_consolidate() {
    let hm = HierarchicalMemory::default().await.unwrap();
    // Store a low-importance entry
    let id = hm.store("unimportant", MemoryTier::LongTerm).await.unwrap();
    // Set importance low by retrieving and checking
    hm.long_term.update(id, |e| e.importance = 0.1).await;

    let result = hm.consolidate().await;
    assert_eq!(result.entries_removed, 1);
}

#[tokio::test]
async fn test_hierarchical_memory_is_within_budget() {
    let hm = HierarchicalMemory::default().await.unwrap();
    assert!(hm.is_within_budget());
}

#[tokio::test]
async fn test_hierarchical_memory_compress_no_op() {
    let mut hm = HierarchicalMemory::default().await.unwrap();
    // When under budget, no compression needed
    let compressed = hm.compress_if_needed().await.unwrap();
    assert!(!compressed);
}

#[tokio::test]
async fn test_hierarchical_memory_clone() {
    let hm = HierarchicalMemory::default().await.unwrap();
    let _hm2 = hm.clone();
}
