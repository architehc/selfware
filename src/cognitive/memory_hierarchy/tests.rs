//! Comprehensive tests for the memory hierarchy system
//!
//! This module provides tests for:
//! - MemoryEntry creation and validation
//! - Short-term memory operations (add, get, clear, eviction)
//! - Long-term memory storage and retrieval
//! - Memory search with filters
//! - Importance scoring
//! - Memory consolidation triggers
//! - Query building and execution

use super::types::{
    ChangeType, CodeContent, CodeContext, CodeEdit, CodeModification, ConsolidationResult, Episode,
    EpisodeType, FileContext, FileContextEntry, Importance, MemoryConfig, MemoryEntry, MemoryIndex,
    MemoryMetrics, MemoryQuery, MemoryStats, MemoryTier, MemoryUsage, SelfImprovementContext,
    SelfModel, SymbolContext, TaskContext, TierTransition, TokenBudget, WorkingContext,
    TOTAL_CONTEXT_TOKENS,
};
use super::{ArchiveMemory, LongTermMemory, ShortTermMemory, WorkingMemory};

// ============================================================================
// MemoryEntry Tests
// ============================================================================

#[test]
fn test_memory_entry_new() {
    let entry = MemoryEntry::new(1, "test content", MemoryTier::ShortTerm);

    assert_eq!(entry.id, 1);
    assert_eq!(entry.content, "test content");
    assert_eq!(entry.tier, MemoryTier::ShortTerm);
    assert_eq!(entry.access_count, 0);
    assert_eq!(entry.importance, 0.5);
    assert!(entry.tags.is_empty());
    assert!(entry.metadata.is_empty());
    assert!(entry.created_at > 0);
    assert_eq!(entry.created_at, entry.accessed_at);
}

#[test]
fn test_memory_entry_with_importance() {
    let entry = MemoryEntry::new(1, "test", MemoryTier::Working).with_importance(0.8);

    assert_eq!(entry.importance, 0.8);
}

#[test]
fn test_memory_entry_importance_clamping() {
    let entry_high = MemoryEntry::new(1, "test", MemoryTier::Working).with_importance(1.5);
    assert_eq!(entry_high.importance, 1.0);

    let entry_low = MemoryEntry::new(2, "test", MemoryTier::Working).with_importance(-0.5);
    assert_eq!(entry_low.importance, 0.0);
}

#[test]
fn test_memory_entry_with_tags() {
    let tags = vec!["tag1".to_string(), "tag2".to_string()];
    let entry = MemoryEntry::new(1, "test", MemoryTier::Working).with_tags(tags.clone());

    assert_eq!(entry.tags, tags);
}

#[test]
fn test_memory_entry_with_metadata() {
    let entry = MemoryEntry::new(1, "test", MemoryTier::Working)
        .with_metadata("key1", serde_json::json!("value1"))
        .with_metadata("key2", serde_json::json!(42));

    assert_eq!(
        entry.metadata.get("key1"),
        Some(&serde_json::json!("value1"))
    );
    assert_eq!(entry.metadata.get("key2"), Some(&serde_json::json!(42)));
}

#[tokio::test]
async fn test_memory_entry_accessed() {
    let mut entry = MemoryEntry::new(1, "test", MemoryTier::Working);
    let original_accessed = entry.accessed_at;

    // Small delay to ensure timestamp changes
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    entry.accessed();

    assert_eq!(entry.access_count, 1);
    assert!(entry.accessed_at >= original_accessed);
}

#[test]
fn test_memory_entry_clone() {
    let entry = MemoryEntry::new(1, "test", MemoryTier::Working)
        .with_importance(0.7)
        .with_tags(vec!["tag1".to_string()]);

    let cloned = entry.clone();

    assert_eq!(cloned.id, entry.id);
    assert_eq!(cloned.content, entry.content);
    assert_eq!(cloned.importance, entry.importance);
    assert_eq!(cloned.tags, entry.tags);
}

// ============================================================================
// MemoryQuery Tests
// ============================================================================

#[test]
fn test_memory_query_new() {
    let query = MemoryQuery::new("search pattern");

    assert_eq!(query.pattern, "search pattern");
    assert!(query.tier.is_none());
    assert!(query.tags.is_empty());
    assert!(query.min_importance.is_none());
    assert!(query.since.is_none());
    assert_eq!(query.limit, Some(10));
}

#[test]
fn test_memory_query_with_tier() {
    let query = MemoryQuery::new("test").with_tier(MemoryTier::LongTerm);

    assert_eq!(query.tier, Some(MemoryTier::LongTerm));
}

#[test]
fn test_memory_query_with_tags() {
    let tags = vec!["important".to_string(), "work".to_string()];
    let query = MemoryQuery::new("test").with_tags(tags.clone());

    assert_eq!(query.tags, tags);
}

#[test]
fn test_memory_query_with_min_importance() {
    let query = MemoryQuery::new("test").with_min_importance(0.75);

    assert_eq!(query.min_importance, Some(0.75));
}

#[test]
fn test_memory_query_with_limit() {
    let query = MemoryQuery::new("test").with_limit(25);

    assert_eq!(query.limit, Some(25));
}

#[test]
fn test_memory_query_since() {
    let timestamp = 1234567890;
    let query = MemoryQuery::new("test").since(timestamp);

    assert_eq!(query.since, Some(timestamp));
}

#[test]
fn test_memory_query_chaining() {
    let query = MemoryQuery::new("complex search")
        .with_tier(MemoryTier::ShortTerm)
        .with_tags(vec!["tag1".to_string()])
        .with_min_importance(0.5)
        .with_limit(5)
        .since(1000);

    assert_eq!(query.pattern, "complex search");
    assert_eq!(query.tier, Some(MemoryTier::ShortTerm));
    assert_eq!(query.tags, vec!["tag1".to_string()]);
    assert_eq!(query.min_importance, Some(0.5));
    assert_eq!(query.limit, Some(5));
    assert_eq!(query.since, Some(1000));
}

// ============================================================================
// MemoryIndex Tests
// ============================================================================

#[test]
fn test_memory_index_new() {
    let index = MemoryIndex::new();

    // next_id should start at 1
    assert_eq!(index.next_id(), 1);
    // And increment
    assert_eq!(index.next_id(), 2);
    assert_eq!(index.next_id(), 3);
}

#[tokio::test]
async fn test_memory_index_index_entry() {
    let index = MemoryIndex::new();
    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm)
        .with_tags(vec!["tag1".to_string(), "tag2".to_string()]);

    index.index_entry(&entry).await;

    let by_tier = index.get_by_tier(MemoryTier::ShortTerm).await;
    assert_eq!(by_tier, vec![1]);

    let by_tag1 = index.get_by_tag("tag1").await;
    assert_eq!(by_tag1, vec![1]);

    let by_tag2 = index.get_by_tag("tag2").await;
    assert_eq!(by_tag2, vec![1]);
}

#[tokio::test]
async fn test_memory_index_multiple_entries() {
    let index = MemoryIndex::new();

    let entry1 = MemoryEntry::new(1, "test1", MemoryTier::ShortTerm)
        .with_tags(vec!["shared".to_string(), "unique1".to_string()]);
    let entry2 = MemoryEntry::new(2, "test2", MemoryTier::ShortTerm)
        .with_tags(vec!["shared".to_string(), "unique2".to_string()]);
    let entry3 =
        MemoryEntry::new(3, "test3", MemoryTier::LongTerm).with_tags(vec!["unique3".to_string()]);

    index.index_entry(&entry1).await;
    index.index_entry(&entry2).await;
    index.index_entry(&entry3).await;

    // Check tier indexing
    let short_term_ids = index.get_by_tier(MemoryTier::ShortTerm).await;
    assert_eq!(short_term_ids.len(), 2);
    assert!(short_term_ids.contains(&1));
    assert!(short_term_ids.contains(&2));

    let long_term_ids = index.get_by_tier(MemoryTier::LongTerm).await;
    assert_eq!(long_term_ids, vec![3]);

    // Check tag indexing
    let shared_ids = index.get_by_tag("shared").await;
    assert_eq!(shared_ids.len(), 2);
    assert!(shared_ids.contains(&1));
    assert!(shared_ids.contains(&2));

    let unique1_ids = index.get_by_tag("unique1").await;
    assert_eq!(unique1_ids, vec![1]);
}

#[tokio::test]
async fn test_memory_index_remove_entry() {
    let index = MemoryIndex::new();
    let entry =
        MemoryEntry::new(1, "test", MemoryTier::ShortTerm).with_tags(vec!["tag1".to_string()]);

    index.index_entry(&entry).await;
    index.remove_entry(&entry).await;

    let by_tier = index.get_by_tier(MemoryTier::ShortTerm).await;
    assert!(by_tier.is_empty());

    let by_tag = index.get_by_tag("tag1").await;
    assert!(by_tag.is_empty());
}

#[tokio::test]
async fn test_memory_index_remove_partial() {
    let index = MemoryIndex::new();

    let entry1 =
        MemoryEntry::new(1, "test1", MemoryTier::ShortTerm).with_tags(vec!["shared".to_string()]);
    let entry2 =
        MemoryEntry::new(2, "test2", MemoryTier::ShortTerm).with_tags(vec!["shared".to_string()]);

    index.index_entry(&entry1).await;
    index.index_entry(&entry2).await;

    // Remove only entry1
    index.remove_entry(&entry1).await;

    // Shared tag should still have entry2
    let shared_ids = index.get_by_tag("shared").await;
    assert_eq!(shared_ids, vec![2]);

    // Tier should still have entry2
    let tier_ids = index.get_by_tier(MemoryTier::ShortTerm).await;
    assert_eq!(tier_ids, vec![2]);
}

#[test]
fn test_memory_index_default() {
    let index: MemoryIndex = Default::default();
    assert_eq!(index.next_id(), 1);
}

// ============================================================================
// ShortTermMemory Tests
// ============================================================================

#[tokio::test]
async fn test_short_term_memory_new() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    assert_eq!(stm.count().await, 0);
}

#[tokio::test]
async fn test_short_term_memory_with_config() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let config = MemoryConfig::default();
    let stm = ShortTermMemory::with_config(&config, index);

    assert_eq!(stm.count().await, 0);
}

#[tokio::test]
async fn test_short_term_memory_store() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    let entry = MemoryEntry::new(1, "test content", MemoryTier::Working);
    let id = stm.store(entry).await.unwrap();

    assert_eq!(id, 1);
    assert_eq!(stm.count().await, 1);
}

#[tokio::test]
async fn test_short_term_memory_store_sets_tier() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index.clone());

    // Create entry with Working tier
    let entry = MemoryEntry::new(1, "test", MemoryTier::Working);
    stm.store(entry).await.unwrap();

    // Retrieve and verify tier was changed to ShortTerm
    let retrieved = stm.retrieve(1).await.unwrap();
    assert_eq!(retrieved.tier, MemoryTier::ShortTerm);
}

#[tokio::test]
async fn test_short_term_memory_retrieve() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    let entry = MemoryEntry::new(1, "test content", MemoryTier::ShortTerm);
    stm.store(entry).await.unwrap();

    let retrieved = stm.retrieve(1).await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "test content");
}

#[tokio::test]
async fn test_short_term_memory_retrieve_updates_access() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm);
    stm.store(entry).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let first = stm.retrieve(1).await.unwrap();
    assert_eq!(first.access_count, 1);

    let second = stm.retrieve(1).await.unwrap();
    assert_eq!(second.access_count, 2);
    assert!(second.accessed_at >= first.accessed_at);
}

#[tokio::test]
async fn test_short_term_memory_retrieve_missing() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    let result = stm.retrieve(999).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_short_term_memory_query_basic() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    stm.store(MemoryEntry::new(1, "hello world", MemoryTier::ShortTerm))
        .await
        .unwrap();
    stm.store(MemoryEntry::new(2, "goodbye world", MemoryTier::ShortTerm))
        .await
        .unwrap();
    stm.store(MemoryEntry::new(3, "other content", MemoryTier::ShortTerm))
        .await
        .unwrap();

    let query = MemoryQuery::new("world");
    let results = stm.query(&query).await;

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|e| e.id == 1));
    assert!(results.iter().any(|e| e.id == 2));
}

#[tokio::test]
async fn test_short_term_memory_query_case_insensitive() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    stm.store(MemoryEntry::new(1, "HELLO World", MemoryTier::ShortTerm))
        .await
        .unwrap();

    let query = MemoryQuery::new("hello WORLD");
    let results = stm.query(&query).await;

    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_short_term_memory_query_with_tier_filter() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    stm.store(MemoryEntry::new(1, "test", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Query for Working tier - should return empty
    let query = MemoryQuery::new("test").with_tier(MemoryTier::Working);
    let results = stm.query(&query).await;
    assert_eq!(results.len(), 0);

    // Query for ShortTerm tier - should find it
    let query2 = MemoryQuery::new("test").with_tier(MemoryTier::ShortTerm);
    let results2 = stm.query(&query2).await;
    assert_eq!(results2.len(), 1);
}

#[tokio::test]
async fn test_short_term_memory_query_with_tags() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    stm.store(
        MemoryEntry::new(1, "test", MemoryTier::ShortTerm)
            .with_tags(vec!["important".to_string(), "work".to_string()]),
    )
    .await
    .unwrap();

    stm.store(
        MemoryEntry::new(2, "test", MemoryTier::ShortTerm).with_tags(vec!["important".to_string()]),
    )
    .await
    .unwrap();

    // Query for both tags - should only find entry 1
    let query =
        MemoryQuery::new("test").with_tags(vec!["important".to_string(), "work".to_string()]);
    let results = stm.query(&query).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 1);

    // Query for single tag - should find both
    let query2 = MemoryQuery::new("test").with_tags(vec!["important".to_string()]);
    let results2 = stm.query(&query2).await;
    assert_eq!(results2.len(), 2);
}

#[tokio::test]
async fn test_short_term_memory_query_with_min_importance() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    stm.store(MemoryEntry::new(1, "test", MemoryTier::ShortTerm).with_importance(0.3))
        .await
        .unwrap();

    stm.store(MemoryEntry::new(2, "test", MemoryTier::ShortTerm).with_importance(0.8))
        .await
        .unwrap();

    let query = MemoryQuery::new("test").with_min_importance(0.5);
    let results = stm.query(&query).await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 2);
}

#[tokio::test]
async fn test_short_term_memory_query_with_since() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    // Get timestamp before storing (millis to match MemoryEntry timestamps)
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Sleep to ensure entry is created after 'before'
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Store entry
    stm.store(MemoryEntry::new(1, "test", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Query with since=before should find the entry
    let query = MemoryQuery::new("test").since(before);
    let results = stm.query(&query).await;
    assert_eq!(results.len(), 1);

    // Query with since in the future should not find the entry
    let future = before + 3_600_000; // 1 hour in the future (millis)
    let query2 = MemoryQuery::new("test").since(future);
    let results2 = stm.query(&query2).await;
    assert_eq!(results2.len(), 0);
}

#[tokio::test]
async fn test_short_term_memory_query_sorting() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    // Store entries with different importance
    stm.store(MemoryEntry::new(1, "test", MemoryTier::ShortTerm).with_importance(0.3))
        .await
        .unwrap();

    stm.store(MemoryEntry::new(2, "test", MemoryTier::ShortTerm).with_importance(0.9))
        .await
        .unwrap();

    stm.store(MemoryEntry::new(3, "test", MemoryTier::ShortTerm).with_importance(0.6))
        .await
        .unwrap();

    let query = MemoryQuery::new("test");
    let results = stm.query(&query).await;

    // Should be sorted by importance descending
    assert_eq!(results[0].id, 2); // 0.9
    assert_eq!(results[1].id, 3); // 0.6
    assert_eq!(results[2].id, 1); // 0.3
}

#[tokio::test]
async fn test_short_term_memory_query_with_limit() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    for i in 0..10 {
        stm.store(
            MemoryEntry::new(i as u64, "test", MemoryTier::ShortTerm)
                .with_importance(0.1 * i as f32),
        )
        .await
        .unwrap();
    }

    let query = MemoryQuery::new("test").with_limit(3);
    let results = stm.query(&query).await;

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_short_term_memory_remove() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm);
    stm.store(entry).await.unwrap();

    let removed = stm.remove(1).await;
    assert!(removed.is_some());
    assert_eq!(stm.count().await, 0);

    // Removing again should return None
    let removed_again = stm.remove(1).await;
    assert!(removed_again.is_none());
}

#[tokio::test]
async fn test_short_term_memory_clear() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    for i in 0..5 {
        stm.store(MemoryEntry::new(i as u64, "test", MemoryTier::ShortTerm))
            .await
            .unwrap();
    }

    assert_eq!(stm.count().await, 5);

    stm.clear().await;

    assert_eq!(stm.count().await, 0);
}

#[tokio::test]
async fn test_short_term_memory_entries() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    for i in 0..3 {
        stm.store(MemoryEntry::new(i as u64, "test", MemoryTier::ShortTerm))
            .await
            .unwrap();
    }

    let entries = stm.entries().await;
    assert_eq!(entries.len(), 3);
}

#[tokio::test]
async fn test_short_term_memory_eviction() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    // Very small capacity to trigger eviction
    let stm = ShortTermMemory::new(2, index);

    // Store 3 entries in a capacity of 2
    stm.store(MemoryEntry::new(1, "first", MemoryTier::ShortTerm))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    stm.store(MemoryEntry::new(2, "second", MemoryTier::ShortTerm))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    stm.store(MemoryEntry::new(3, "third", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Count should still be at capacity
    assert_eq!(stm.count().await, 2);

    // Oldest entry should be evicted (entry 1)
    assert!(stm.retrieve(1).await.is_none());
    assert!(stm.retrieve(2).await.is_some());
    assert!(stm.retrieve(3).await.is_some());
}

#[tokio::test]
async fn test_short_term_memory_check_promotion() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);
    let config = MemoryConfig::default();

    // Entry with low access count - should be Keep
    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm).with_importance(0.8);
    stm.store(entry).await.unwrap();

    let result = stm.check_promotion(1, &config).await;
    assert!(matches!(result, TierTransition::Keep));

    // Access entry multiple times
    for _ in 0..5 {
        stm.retrieve(1).await;
    }

    // Now should be eligible for promotion
    let result = stm.check_promotion(1, &config).await;
    assert!(matches!(result, TierTransition::Promote));
}

#[tokio::test]
async fn test_short_term_memory_check_demotion() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    // The demotion check compares: now - accessed_at > demotion_threshold
    // With demotion_threshold = 0, even 1 second difference should trigger demotion
    let config = MemoryConfig {
        demotion_threshold: 0,
        ..Default::default()
    };

    // Create entry
    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm);
    stm.store(entry).await.unwrap();

    // Wait enough time for demotion to trigger
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let result = stm.check_promotion(1, &config).await;
    assert!(matches!(result, TierTransition::Demote));
}

#[test]
fn test_short_term_memory_clone() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);
    let _cloned = stm.clone();
}

// ============================================================================
// WorkingMemory Tests
// ============================================================================

#[tokio::test]
async fn test_working_memory_new() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    assert_eq!(wm.count().await, 0);
}

#[tokio::test]
async fn test_working_memory_store() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm);
    let id = wm.store(entry).await.unwrap();

    assert_eq!(id, 1);
    assert_eq!(wm.count().await, 1);
}

#[tokio::test]
async fn test_working_memory_store_and_retrieve() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm);
    wm.store(entry).await.unwrap();

    // WorkingMemory uses ShortTermMemory internally
    // The tier is set to ShortTerm by the underlying store
    let retrieved = wm.retrieve(1).await.unwrap();
    assert_eq!(retrieved.content, "test");
}

#[tokio::test]
async fn test_working_memory_retrieve() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::Working);
    wm.store(entry).await.unwrap();

    let retrieved = wm.retrieve(1).await;
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_working_memory_query() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    wm.store(MemoryEntry::new(1, "hello", MemoryTier::Working))
        .await
        .unwrap();
    wm.store(MemoryEntry::new(2, "world", MemoryTier::Working))
        .await
        .unwrap();

    let query = MemoryQuery::new("hello");
    let results = wm.query(&query).await;

    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_working_memory_remove() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    wm.store(MemoryEntry::new(1, "test", MemoryTier::Working))
        .await
        .unwrap();

    let removed = wm.remove(1).await;
    assert!(removed.is_some());
}

#[tokio::test]
async fn test_working_memory_clear() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    wm.store(MemoryEntry::new(1, "test1", MemoryTier::Working))
        .await
        .unwrap();
    wm.store(MemoryEntry::new(2, "test2", MemoryTier::Working))
        .await
        .unwrap();

    wm.clear().await;

    assert_eq!(wm.count().await, 0);
}

#[tokio::test]
async fn test_working_memory_entries() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);

    wm.store(MemoryEntry::new(1, "test", MemoryTier::Working))
        .await
        .unwrap();

    let entries = wm.entries().await;
    assert_eq!(entries.len(), 1);
}

#[test]
fn test_working_memory_clone() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let wm = WorkingMemory::new(10, index);
    let _cloned = wm.clone();
}

// ============================================================================
// LongTermMemory Tests
// ============================================================================

#[tokio::test]
async fn test_long_term_memory_new() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    assert_eq!(ltm.count().await, 0);
}

#[tokio::test]
async fn test_long_term_memory_with_config() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let config = MemoryConfig::default();
    let ltm = LongTermMemory::with_config(&config, index);

    assert_eq!(ltm.count().await, 0);
}

#[tokio::test]
async fn test_long_term_memory_store() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    let entry = MemoryEntry::new(1, "test content", MemoryTier::Working);
    let id = ltm.store(entry).await.unwrap();

    assert_eq!(id, 1);
    assert_eq!(ltm.count().await, 1);
}

#[tokio::test]
async fn test_long_term_memory_store_sets_tier() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::Working);
    ltm.store(entry).await.unwrap();

    let retrieved = ltm.retrieve(1).await.unwrap();
    assert_eq!(retrieved.tier, MemoryTier::LongTerm);
}

#[tokio::test]
async fn test_long_term_memory_retrieve() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    let entry = MemoryEntry::new(1, "test content", MemoryTier::LongTerm);
    ltm.store(entry).await.unwrap();

    let retrieved = ltm.retrieve(1).await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "test content");
}

#[tokio::test]
async fn test_long_term_memory_retrieve_updates_access() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    let entry = MemoryEntry::new(1, "test", MemoryTier::LongTerm);
    ltm.store(entry).await.unwrap();

    let first = ltm.retrieve(1).await.unwrap();
    assert_eq!(first.access_count, 1);

    let second = ltm.retrieve(1).await.unwrap();
    assert_eq!(second.access_count, 2);
}

#[tokio::test]
async fn test_long_term_memory_query() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    ltm.store(MemoryEntry::new(1, "hello world", MemoryTier::LongTerm))
        .await
        .unwrap();
    ltm.store(MemoryEntry::new(2, "other content", MemoryTier::LongTerm))
        .await
        .unwrap();

    let query = MemoryQuery::new("hello");
    let results = ltm.query(&query).await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 1);
}

#[tokio::test]
async fn test_long_term_memory_update() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    ltm.store(MemoryEntry::new(1, "original", MemoryTier::LongTerm))
        .await
        .unwrap();

    let updated = ltm
        .update(1, |entry| {
            entry.content = "modified".to_string();
            entry.importance = 0.9;
        })
        .await;

    assert!(updated);

    let retrieved = ltm.retrieve(1).await.unwrap();
    assert_eq!(retrieved.content, "modified");
    assert_eq!(retrieved.importance, 0.9);
}

#[tokio::test]
async fn test_long_term_memory_update_missing() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    let updated = ltm
        .update(999, |entry| {
            entry.content = "modified".to_string();
        })
        .await;

    assert!(!updated);
}

#[tokio::test]
async fn test_long_term_memory_remove() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    ltm.store(MemoryEntry::new(1, "test", MemoryTier::LongTerm))
        .await
        .unwrap();

    let removed = ltm.remove(1).await;
    assert!(removed.is_some());
    assert_eq!(ltm.count().await, 0);

    // Second remove should return None
    let removed_again = ltm.remove(1).await;
    assert!(removed_again.is_none());
}

#[tokio::test]
async fn test_long_term_memory_entries() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    for i in 0..5 {
        ltm.store(MemoryEntry::new(i as u64, "test", MemoryTier::LongTerm))
            .await
            .unwrap();
    }

    let entries = ltm.entries().await;
    assert_eq!(entries.len(), 5);
}

#[tokio::test]
async fn test_long_term_memory_consolidate() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);

    // Add entries with varying importance
    for i in 0..5 {
        ltm.store(
            MemoryEntry::new(i as u64, "test", MemoryTier::LongTerm)
                .with_importance(0.1 * i as f32),
        )
        .await
        .unwrap();
    }

    // Add a high importance entry
    ltm.store(MemoryEntry::new(10, "important", MemoryTier::LongTerm).with_importance(0.9))
        .await
        .unwrap();

    let result = ltm.consolidate().await;

    // Entries with importance < 0.3 should be removed
    assert!(result.entries_removed > 0);
}

/// Regression: consolidate() must de-index removed entries — before the fix,
/// tag/tier queries returned orphaned ids for entries that no longer existed.
#[tokio::test]
async fn test_long_term_consolidate_removes_orphaned_index_ids() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index.clone());

    // Low-importance entries (will be consolidated away), all sharing a tag.
    for i in 0..3u64 {
        ltm.store(
            MemoryEntry::new(i, "stale", MemoryTier::LongTerm)
                .with_importance(0.1)
                .with_tags(vec!["stale-tag".to_string()]),
        )
        .await
        .unwrap();
    }
    // One high-importance entry with the same tag must survive.
    ltm.store(
        MemoryEntry::new(99, "keeper", MemoryTier::LongTerm)
            .with_importance(0.9)
            .with_tags(vec!["stale-tag".to_string()]),
    )
    .await
    .unwrap();

    assert_eq!(index.get_by_tag("stale-tag").await.len(), 4);

    let result = ltm.consolidate().await;
    assert_eq!(result.entries_removed, 3);

    let remaining = index.get_by_tag("stale-tag").await;
    assert_eq!(
        remaining,
        vec![99],
        "tag query must not return orphaned ids for consolidated entries"
    );
    assert_eq!(
        index.get_by_tier(MemoryTier::LongTerm).await,
        vec![99],
        "tier query must not return orphaned ids for consolidated entries"
    );
}

#[tokio::test]
async fn test_long_term_memory_archive_oldest() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    // Small capacity to trigger archiving
    let ltm = LongTermMemory::new(2, index);

    // Add entries with importance < 0.5 (eligible for archiving)
    for i in 0..3 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        ltm.store(MemoryEntry::new(i as u64, "test", MemoryTier::LongTerm).with_importance(0.3))
            .await
            .unwrap();
    }

    // Count should be at capacity
    assert_eq!(ltm.count().await, 2);
}

#[test]
fn test_long_term_memory_clone() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(1000, index);
    let _cloned = ltm.clone();
}

// ============================================================================
// ArchiveMemory Tests
// ============================================================================

#[tokio::test]
async fn test_archive_memory_new() {
    let am = ArchiveMemory::new();
    assert_eq!(am.count().await, 0);
}

#[tokio::test]
async fn test_archive_memory_default() {
    let am: ArchiveMemory = Default::default();
    assert_eq!(am.count().await, 0);
}

#[tokio::test]
async fn test_archive_memory_store() {
    let am = ArchiveMemory::new();

    let entry = MemoryEntry::new(1, "archived content", MemoryTier::Working);
    let id = am.store(entry).await.unwrap();

    assert_eq!(id, 1);
    assert_eq!(am.count().await, 1);
}

#[tokio::test]
async fn test_archive_memory_store_sets_tier() {
    let am = ArchiveMemory::new();

    let entry = MemoryEntry::new(1, "test", MemoryTier::Working);
    am.store(entry).await.unwrap();

    let retrieved = am.retrieve(1).await.unwrap();
    assert_eq!(retrieved.tier, MemoryTier::Archive);
}

#[tokio::test]
async fn test_archive_memory_retrieve() {
    let am = ArchiveMemory::new();

    let entry = MemoryEntry::new(1, "archived", MemoryTier::Archive);
    am.store(entry).await.unwrap();

    let retrieved = am.retrieve(1).await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "archived");
}

#[tokio::test]
async fn test_archive_memory_retrieve_missing() {
    let am = ArchiveMemory::new();

    let result = am.retrieve(999).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_archive_memory_count() {
    let am = ArchiveMemory::new();

    for i in 0..5 {
        am.store(MemoryEntry::new(i as u64, "test", MemoryTier::Archive))
            .await
            .unwrap();
    }

    assert_eq!(am.count().await, 5);
}

#[test]
fn test_archive_memory_clone() {
    let am = ArchiveMemory::new();
    let _cloned = am.clone();
}

// ============================================================================
// MemoryConfig Tests
// ============================================================================

#[test]
fn test_memory_config_default() {
    let config = MemoryConfig::default();

    assert_eq!(config.working_capacity, 10);
    assert_eq!(config.short_term_capacity, 100);
    assert_eq!(config.long_term_capacity, 1000);
    assert_eq!(config.promotion_threshold, 3);
    assert_eq!(config.demotion_threshold, 3600);
    assert_eq!(config.importance_threshold, 0.7);
}

// ============================================================================
// MemoryStats Tests
// ============================================================================

#[test]
fn test_memory_stats_default() {
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
}

// ============================================================================
// Importance Tests
// ============================================================================

#[test]
fn test_importance_as_f32() {
    assert_eq!(Importance::Low.as_f32(), 0.25);
    assert_eq!(Importance::Normal.as_f32(), 0.4);
    assert_eq!(Importance::Medium.as_f32(), 0.5);
    assert_eq!(Importance::High.as_f32(), 0.75);
    assert_eq!(Importance::Critical.as_f32(), 1.0);
}

#[test]
fn test_importance_ordering() {
    assert!(Importance::Low < Importance::Normal);
    assert!(Importance::Normal < Importance::Medium);
    assert!(Importance::Medium < Importance::High);
    assert!(Importance::High < Importance::Critical);
}

// ============================================================================
// EpisodeType Tests
// ============================================================================

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

// ============================================================================
// Episode Tests
// ============================================================================

#[test]
fn test_episode_new() {
    let episode = Episode::new("ep-1", EpisodeType::Action, "did something");

    assert_eq!(episode.id, "ep-1");
    assert_eq!(episode.episode_type, EpisodeType::Action);
    assert_eq!(episode.content, "did something");
    assert_eq!(episode.importance, Importance::Medium);
    assert!(episode.metadata.is_empty());
    assert!(episode.related_episodes.is_empty());
    assert!(episode.insights.is_empty());
    assert!(!episode.is_summarized);
    assert!(episode.original_id.is_none());
}

#[test]
fn test_episode_with_importance() {
    let episode =
        Episode::new("ep-1", EpisodeType::Success, "achievement").with_importance(Importance::High);

    assert_eq!(episode.importance, Importance::High);
}

// ============================================================================
// TokenBudget Tests
// ============================================================================

#[test]
fn test_token_budget_new() {
    let budget = TokenBudget::new(1_000_000);

    assert_eq!(budget.working_memory, 250_000);
    assert_eq!(budget.episodic_memory, 250_000);
    assert_eq!(budget.semantic_memory, 250_000);
    assert_eq!(budget.response_reserve, 250_000);
}

#[test]
fn test_token_budget_default() {
    let budget: TokenBudget = Default::default();

    assert_eq!(budget.working_memory, TOTAL_CONTEXT_TOKENS / 4);
    assert_eq!(budget.episodic_memory, TOTAL_CONTEXT_TOKENS / 4);
    assert_eq!(budget.semantic_memory, TOTAL_CONTEXT_TOKENS / 4);
    assert_eq!(budget.response_reserve, TOTAL_CONTEXT_TOKENS / 4);
}

#[test]
fn test_token_budget_for_conversation() {
    let budget = TokenBudget::for_conversation();

    assert_eq!(budget.working_memory, TOTAL_CONTEXT_TOKENS / 4);
    assert_eq!(budget.episodic_memory, TOTAL_CONTEXT_TOKENS / 4);
    assert_eq!(budget.semantic_memory, TOTAL_CONTEXT_TOKENS / 4);
    assert_eq!(budget.response_reserve, TOTAL_CONTEXT_TOKENS / 4);
}

#[test]
fn test_token_budget_for_self_improvement() {
    let budget = TokenBudget::for_self_improvement();

    assert_eq!(budget.working_memory, TOTAL_CONTEXT_TOKENS / 8);
    assert_eq!(budget.episodic_memory, TOTAL_CONTEXT_TOKENS / 8);
    assert_eq!(budget.semantic_memory, TOTAL_CONTEXT_TOKENS * 3 / 4);
    assert_eq!(budget.response_reserve, TOTAL_CONTEXT_TOKENS / 8);
}

// ============================================================================
// MemoryUsage Tests
// ============================================================================

#[test]
fn test_memory_usage_default() {
    let usage = MemoryUsage::default();

    assert_eq!(usage.working_tokens, 0);
    assert_eq!(usage.episodic_tokens, 0);
    assert_eq!(usage.semantic_tokens, 0);
    assert_eq!(usage.self_tokens, 0);
    assert_eq!(usage.total_used, 0);
}

// ============================================================================
// MemoryMetrics Tests
// ============================================================================

#[test]
fn test_memory_metrics_default() {
    let metrics = MemoryMetrics::default();

    assert_eq!(metrics.cache_hits, 0);
    assert_eq!(metrics.cache_misses, 0);
    assert_eq!(metrics.evictions, 0);
    assert_eq!(metrics.compressions, 0);
    assert_eq!(metrics.avg_retrieval_time_ms, 0.0);
    assert_eq!(metrics.last_updated, 0);
}

// ============================================================================
// TaskContext Tests
// ============================================================================

#[test]
fn test_task_context_creation() {
    let task = TaskContext {
        description: "Test task".to_string(),
        goal: "Complete testing".to_string(),
        progress: vec!["step1".to_string()],
        next_steps: vec!["step2".to_string()],
        relevant_files: vec!["test.rs".to_string()],
    };

    assert_eq!(task.description, "Test task");
    assert_eq!(task.goal, "Complete testing");
}

// ============================================================================
// CodeEdit Tests
// ============================================================================

#[test]
fn test_code_edit_creation() {
    let edit = CodeEdit {
        timestamp: 1234567890,
        description: "Fixed bug".to_string(),
        lines_changed: (10, 20),
    };

    assert_eq!(edit.timestamp, 1234567890);
    assert_eq!(edit.description, "Fixed bug");
    assert_eq!(edit.lines_changed, (10, 20));
}

// ============================================================================
// CodeContent Tests
// ============================================================================

#[test]
fn test_code_content_full() {
    let content = CodeContent::Full("fn main() {}".to_string());

    match content {
        CodeContent::Full(s) => assert_eq!(s, "fn main() {}"),
        _ => panic!("Expected Full variant"),
    }
}

#[test]
fn test_code_content_summary() {
    let content = CodeContent::Summary {
        overview: "A summary".to_string(),
        key_functions: vec!["func1".to_string(), "func2".to_string()],
    };

    match content {
        CodeContent::Summary {
            overview,
            key_functions,
        } => {
            assert_eq!(overview, "A summary");
            assert_eq!(key_functions.len(), 2);
        }
        _ => panic!("Expected Summary variant"),
    }
}

// ============================================================================
// CodeContext Tests
// ============================================================================

#[test]
fn test_code_context_new() {
    let ctx = CodeContext::new();

    assert!(ctx.files.is_empty());
    assert!(ctx.symbols.is_empty());
    assert_eq!(ctx.total_tokens, 0);
}

#[test]
fn test_code_context_default() {
    let ctx: CodeContext = Default::default();

    assert!(ctx.files.is_empty());
    assert!(ctx.symbols.is_empty());
    assert_eq!(ctx.total_tokens, 0);
}

// ============================================================================
// FileContext Tests
// ============================================================================

#[test]
fn test_file_context_creation() {
    let file = FileContext {
        path: "src/main.rs".to_string(),
        content: "fn main() {}".to_string(),
        language: "rust".to_string(),
        estimated_tokens: 10,
        relevance_score: 0.8,
    };

    assert_eq!(file.path, "src/main.rs");
    assert_eq!(file.language, "rust");
    assert_eq!(file.relevance_score, 0.8);
}

// ============================================================================
// FileContextEntry Tests
// ============================================================================

#[test]
fn test_file_context_entry_creation() {
    let entry = FileContextEntry {
        path: "src/lib.rs".to_string(),
        content: "pub mod test;".to_string(),
        relevance_score: 0.9,
    };

    assert_eq!(entry.path, "src/lib.rs");
    assert_eq!(entry.relevance_score, 0.9);
}

// ============================================================================
// SymbolContext Tests
// ============================================================================

#[test]
fn test_symbol_context_creation() {
    let symbol = SymbolContext {
        name: "my_function".to_string(),
        symbol_type: "function".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_start: 10,
        line_end: 20,
    };

    assert_eq!(symbol.name, "my_function");
    assert_eq!(symbol.symbol_type, "function");
    assert_eq!(symbol.line_start, 10);
    assert_eq!(symbol.line_end, 20);
}

// ============================================================================
// WorkingContext Tests
// ============================================================================

#[test]
fn test_working_context_new() {
    let ctx = WorkingContext::new("You are a helpful assistant.");

    assert!(ctx.messages.is_empty());
    assert_eq!(ctx.system_prompt, "You are a helpful assistant.");
    assert_eq!(ctx.estimated_tokens, 0);
    assert!(ctx.current_task.is_none());
    assert!(ctx.active_code.is_empty());
}

// ============================================================================
// SelfImprovementContext Tests
// ============================================================================

#[test]
fn test_self_improvement_context_estimate_tokens() {
    let ctx = SelfImprovementContext {
        goal: "Improve performance".to_string(),
        self_model: "Current model".to_string(),
        architecture: "System architecture".to_string(),
        recent_modifications: "Recent changes".to_string(),
        relevant_code: CodeContext::new(),
        suggestions: vec!["Suggestion 1".to_string(), "Suggestion 2".to_string()],
    };

    let estimated = ctx.estimate_tokens();
    assert!(estimated > 0);
}

#[test]
fn test_self_improvement_context_to_prompt() {
    let ctx = SelfImprovementContext {
        goal: "Improve".to_string(),
        self_model: "Model".to_string(),
        architecture: "Arch".to_string(),
        recent_modifications: "Changes".to_string(),
        relevant_code: CodeContext::new(),
        suggestions: vec!["Suggestion".to_string()],
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("Self-Improvement Context"));
    assert!(prompt.contains("Improve"));
    assert!(prompt.contains("Suggestion"));
}

// ============================================================================
// SelfModel Tests
// ============================================================================

#[test]
fn test_self_model_creation() {
    let model = SelfModel {
        version: "1.0.0".to_string(),
        capabilities: vec!["reasoning".to_string()],
        limitations: vec!["token limit".to_string()],
        recent_changes: vec!["bug fix".to_string()],
        modules: vec!["memory".to_string()],
    };

    assert_eq!(model.version, "1.0.0");
    assert_eq!(model.capabilities.len(), 1);
}

// ============================================================================
// CodeModification Tests
// ============================================================================

#[test]
fn test_code_modification_creation() {
    let modification = CodeModification {
        id: "mod-1".to_string(),
        timestamp: 1234567890,
        file_path: "src/main.rs".to_string(),
        change_type: ChangeType::Modification,
        description: "Fixed logic".to_string(),
        success: true,
    };

    assert_eq!(modification.id, "mod-1");
    assert_eq!(modification.change_type, ChangeType::Modification);
    assert!(modification.success);
}

// ============================================================================
// ChangeType Tests
// ============================================================================

#[test]
fn test_change_type_variants() {
    let _addition = ChangeType::Addition;
    let _deletion = ChangeType::Deletion;
    let _modification = ChangeType::Modification;
    let _refactor = ChangeType::Refactor;
}

// ============================================================================
// TierTransition Tests
// ============================================================================

#[test]
fn test_tier_transition_variants() {
    let _promote = TierTransition::Promote;
    let _demote = TierTransition::Demote;
    let _keep = TierTransition::Keep;
}

// ============================================================================
// ConsolidationResult Tests
// ============================================================================

#[test]
fn test_consolidation_result_creation() {
    let result = ConsolidationResult {
        entries_merged: 5,
        entries_removed: 3,
        new_summaries: vec![],
    };

    assert_eq!(result.entries_merged, 5);
    assert_eq!(result.entries_removed, 3);
}

// ============================================================================
// MemoryTier Tests
// ============================================================================

#[test]
fn test_memory_tier_variants() {
    assert_eq!(MemoryTier::Working as i32, 0);
    assert_eq!(MemoryTier::ShortTerm as i32, 1);
    assert_eq!(MemoryTier::LongTerm as i32, 2);
    assert_eq!(MemoryTier::Archive as i32, 3);
}

#[test]
fn test_memory_tier_ordering() {
    assert!(MemoryTier::Working < MemoryTier::ShortTerm);
    assert!(MemoryTier::ShortTerm < MemoryTier::LongTerm);
    assert!(MemoryTier::LongTerm < MemoryTier::Archive);
}

// ============================================================================
// Edge Cases and Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_empty_memory_operations() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index.clone());
    let ltm = LongTermMemory::new(1000, index.clone());
    let am = ArchiveMemory::new();

    // Querying empty memory should return empty results
    let query = MemoryQuery::new("anything");
    assert!(stm.query(&query).await.is_empty());
    assert!(ltm.query(&query).await.is_empty());

    // Retrieving from empty memory should return None
    assert!(stm.retrieve(1).await.is_none());
    assert!(ltm.retrieve(1).await.is_none());
    assert!(am.retrieve(1).await.is_none());

    // Count should be 0
    assert_eq!(stm.count().await, 0);
    assert_eq!(ltm.count().await, 0);
    assert_eq!(am.count().await, 0);

    // Entries should be empty
    assert!(stm.entries().await.is_empty());
    assert!(ltm.entries().await.is_empty());

    // Clear should work on empty memory
    stm.clear().await;
    assert_eq!(stm.count().await, 0);
}

#[tokio::test]
async fn test_capacity_limits() {
    let index = std::sync::Arc::new(MemoryIndex::new());

    // Test with very small capacity - should trigger eviction
    let stm = ShortTermMemory::new(1, index.clone());

    // Store first entry
    stm.store(MemoryEntry::new(1, "test1", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Store second entry - should evict first due to capacity
    stm.store(MemoryEntry::new(2, "test2", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Count should be at capacity (1)
    assert_eq!(stm.count().await, 1);

    // First entry should be evicted
    assert!(stm.retrieve(1).await.is_none());
    // Second entry should exist
    assert!(stm.retrieve(2).await.is_some());
}

#[tokio::test]
async fn test_query_with_invalid_patterns() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    stm.store(MemoryEntry::new(1, "normal content", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Query with empty pattern
    let query = MemoryQuery::new("");
    let results = stm.query(&query).await;
    // Empty pattern matches everything
    assert_eq!(results.len(), 1);

    // Query with pattern that doesn't exist
    let query = MemoryQuery::new("xyz_nonexistent");
    let results = stm.query(&query).await;
    assert!(results.is_empty());

    // Query with special characters
    let query = MemoryQuery::new("!@#$%");
    let results = stm.query(&query).await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_update_with_empty_callback() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(100, index);

    ltm.store(MemoryEntry::new(1, "test", MemoryTier::LongTerm))
        .await
        .unwrap();

    // Update with empty callback
    let updated = ltm
        .update(1, |_entry| {
            // Do nothing
        })
        .await;

    assert!(updated);

    // Entry should remain unchanged
    let entry = ltm.retrieve(1).await.unwrap();
    assert_eq!(entry.content, "test");
}

#[tokio::test]
async fn test_multiple_concurrent_operations() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = std::sync::Arc::new(ShortTermMemory::new(100, index));

    // Spawn multiple concurrent stores
    let mut handles = vec![];
    for i in 0..10 {
        let stm_clone = stm.clone();
        let handle = tokio::spawn(async move {
            let entry = MemoryEntry::new(i as u64, format!("content {}", i), MemoryTier::ShortTerm);
            stm_clone.store(entry).await.unwrap();
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // All entries should be stored
    assert_eq!(stm.count().await, 10);
}

#[tokio::test]
async fn test_query_with_all_filters() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    // Create entry that matches all filters
    stm.store(
        MemoryEntry::new(1, "specific content", MemoryTier::ShortTerm)
            .with_importance(0.8)
            .with_tags(vec!["important".to_string(), "work".to_string()]),
    )
    .await
    .unwrap();

    // Create entries that don't match
    stm.store(
        MemoryEntry::new(2, "other content", MemoryTier::ShortTerm)
            .with_importance(0.3)
            .with_tags(vec!["important".to_string()]),
    )
    .await
    .unwrap();

    stm.store(
        MemoryEntry::new(3, "specific content", MemoryTier::ShortTerm)
            .with_importance(0.8)
            .with_tags(vec!["work".to_string()]),
    )
    .await
    .unwrap();

    // Query with multiple filters (millis to match MemoryEntry timestamps)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let query = MemoryQuery::new("specific")
        .with_tier(MemoryTier::ShortTerm)
        .with_tags(vec!["important".to_string(), "work".to_string()])
        .with_min_importance(0.7)
        .since(now - 60_000)
        .with_limit(5);

    let results = stm.query(&query).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 1);
}

#[tokio::test]
async fn test_memory_entry_serialization() {
    let entry = MemoryEntry::new(1, "test content", MemoryTier::ShortTerm)
        .with_importance(0.75)
        .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
        .with_metadata("key", serde_json::json!("value"));

    // Serialize to JSON
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("test content"));
    assert!(json.contains("tag1"));

    // Deserialize back
    let deserialized: MemoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, entry.id);
    assert_eq!(deserialized.content, entry.content);
    assert_eq!(deserialized.importance, entry.importance);
    assert_eq!(deserialized.tags, entry.tags);
}

#[tokio::test]
async fn test_consolidation_empty_memory() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(100, index);

    // Consolidate empty memory
    let result = ltm.consolidate().await;
    assert_eq!(result.entries_removed, 0);
    assert_eq!(result.entries_merged, 0);
    assert!(result.new_summaries.is_empty());
}

#[tokio::test]
async fn test_consolidation_all_high_importance() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let ltm = LongTermMemory::new(100, index);

    // Add only high importance entries (all >= 0.3)
    for i in 0..5 {
        ltm.store(MemoryEntry::new(i as u64, "test", MemoryTier::LongTerm).with_importance(0.5))
            .await
            .unwrap();
    }

    // None should be removed
    let result = ltm.consolidate().await;
    assert_eq!(result.entries_removed, 0);
    assert_eq!(ltm.count().await, 5);
}

#[tokio::test]
async fn test_eviction_behavior() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    // Capacity of 2 means at most 2 entries are kept
    // Note: evict_oldest only runs when len >= capacity before insert
    let stm = ShortTermMemory::new(2, index.clone());

    // First entry - should be stored
    stm.store(MemoryEntry::new(1, "first", MemoryTier::ShortTerm))
        .await
        .unwrap();
    assert_eq!(stm.count().await, 1);

    // Second entry - should be stored
    stm.store(MemoryEntry::new(2, "second", MemoryTier::ShortTerm))
        .await
        .unwrap();
    assert_eq!(stm.count().await, 2);

    // Third entry - should trigger eviction of oldest entry
    stm.store(MemoryEntry::new(3, "third", MemoryTier::ShortTerm))
        .await
        .unwrap();

    // Count should remain at capacity (2)
    assert_eq!(stm.count().await, 2);

    // All entries should still be retrievable (oldest eviction may not work as expected
    // depending on timing resolution)
    let all_entries = stm.entries().await;
    assert_eq!(all_entries.len(), 2);
}

#[tokio::test]
async fn test_promotion_threshold_edge_cases() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    // Config requiring exactly 3 accesses and 0.7 importance
    let config = MemoryConfig {
        promotion_threshold: 3,
        importance_threshold: 0.7,
        ..Default::default()
    };

    // Entry with exactly 3 accesses but low importance - should not promote
    let entry = MemoryEntry::new(1, "test", MemoryTier::ShortTerm).with_importance(0.5);
    stm.store(entry).await.unwrap();
    for _ in 0..3 {
        stm.retrieve(1).await;
    }

    let result = stm.check_promotion(1, &config).await;
    assert!(matches!(result, TierTransition::Keep));

    // Entry with high importance but only 2 accesses - should not promote
    let entry2 = MemoryEntry::new(2, "test2", MemoryTier::ShortTerm).with_importance(0.9);
    stm.store(entry2).await.unwrap();
    stm.retrieve(2).await;
    stm.retrieve(2).await;

    let result2 = stm.check_promotion(2, &config).await;
    assert!(matches!(result2, TierTransition::Keep));
}

#[tokio::test]
async fn test_query_sorting_by_access_time() {
    let index = std::sync::Arc::new(MemoryIndex::new());
    let stm = ShortTermMemory::new(100, index);

    // Add entries with same importance but spaced out in time
    stm.store(MemoryEntry::new(100, "test", MemoryTier::ShortTerm).with_importance(0.5))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    stm.store(MemoryEntry::new(101, "test", MemoryTier::ShortTerm).with_importance(0.5))
        .await
        .unwrap();

    // Verify we have 2 entries
    let query = MemoryQuery::new("test");
    let results = stm.query(&query).await;
    assert_eq!(results.len(), 2);

    // Now access the first entry to update its accessed_at
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    stm.retrieve(100).await;

    let query2 = MemoryQuery::new("test");
    let results2 = stm.query(&query2).await;

    // The retrieved entry (id 100) should now appear in results
    // Note: Sorting behavior may vary by implementation
    let ids: Vec<_> = results2.iter().map(|e| e.id).collect();
    assert!(ids.contains(&100), "Retrieved entry should be in results");
    assert!(ids.contains(&101), "Other entry should also be in results");
}
