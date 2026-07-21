use super::*;
use crate::consolidation::temporal::{CompactedContent, RecordImportance};
use std::collections::HashMap;

fn make_record(id: &str) -> TemporalRecord {
    let now = chrono::Utc::now();
    TemporalRecord {
        id: id.into(),
        created_at: now,
        source_timestamps: vec![now],
        sequence_order: 1,
        causal_parents: vec![],
        causal_children: vec![],
        decay_score: 1.0,
        access_count: 0,
        last_accessed: now,
        content: CompactedContent {
            summary: "Test summary".into(),
            key_facts: vec![],
            entities: vec![],
            actions: vec![],
            outcomes: vec![],
            insights: vec![],
        },
        multimodal_refs: vec![],
        source_ids: vec!["src-1".into()],
        tags: vec!["test".into()],
        importance: RecordImportance::Normal,
        session_id: None,
        metadata: HashMap::new(),
    }
}

/// Helper: create a record with a custom summary and optional causal links.
fn make_record_with(
    id: &str,
    summary: &str,
    parents: Vec<&str>,
    children: Vec<&str>,
) -> TemporalRecord {
    let mut r = make_record(id);
    r.content.summary = summary.into();
    r.causal_parents = parents.into_iter().map(String::from).collect();
    r.causal_children = children.into_iter().map(String::from).collect();
    r
}

#[test]
fn test_store_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    let records = vec![make_record("r1"), make_record("r2")];

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(store.store(&records)).unwrap();
    assert_eq!(result.stored, 2);
    assert!(result.errors.is_empty());

    let loaded = store.load_all().unwrap();
    assert_eq!(loaded.len(), 2);
}

#[test]
fn test_query_by_tag() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    let mut r1 = make_record("r1");
    r1.tags = vec!["important".into()];
    let mut r2 = make_record("r2");
    r2.tags = vec!["trivial".into()];

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(store.store(&[r1, r2])).unwrap();

    let results = store.query_by_tag("important").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "r1");
}

#[test]
fn test_load_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());
    let loaded = store.load_all().unwrap();
    assert!(loaded.is_empty());
}

// =========================================================================
// Hybrid retrieval tests (semantic vector search + causal graph expansion)
// =========================================================================

#[tokio::test]
async fn test_hybrid_retrieve_semantic_and_causal() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    // r1: best semantic match for "tokio async runtime scheduling"
    // r2: causal child of r1 (shares one token: "tokio")
    // r3: unrelated content (database)
    let r1 = make_record_with(
        "r1",
        "Rust async runtime tokio task scheduling",
        vec![],
        vec!["r2"],
    );
    let r2 = make_record_with(
        "r2",
        "Tokio task spawn and await futures",
        vec!["r1"],
        vec![],
    );
    let r3 = make_record_with(
        "r3",
        "Database connection pooling with SQLx",
        vec![],
        vec![],
    );

    store.store(&[r1, r2, r3]).await.unwrap();

    // Query that semantically matches r1 best.
    let results = store
        .retrieve("tokio async runtime scheduling", 2)
        .await
        .unwrap();

    // Should return at least 2 results (r1 semantic + r2 causal neighbor).
    assert!(
        results.len() >= 2,
        "Expected at least 2 results, got {}",
        results.len()
    );

    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();

    // r1 should be the first result (best semantic match).
    assert_eq!(
        ids[0], "r1",
        "r1 should be the top semantic hit, got order: {:?}",
        ids
    );

    // r2 (causal child of r1) should be included via graph expansion.
    assert!(
        ids.contains(&"r2"),
        "r2 (causal neighbor of r1) should be in results: {:?}",
        ids
    );

    // r3 (unrelated) should NOT be in the results.
    assert!(
        !ids.contains(&"r3"),
        "r3 (unrelated) should not be in results: {:?}",
        ids
    );
}

#[tokio::test]
async fn test_hybrid_retrieve_causal_parent_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    // r2 is the semantic hit; r1 is its causal parent.
    let r1 = make_record_with(
        "r1",
        "Setting up the Rust project with cargo init",
        vec![],
        vec!["r2"],
    );
    let r2 = make_record_with(
        "r2",
        "Cargo build dependencies and features configuration",
        vec!["r1"],
        vec![],
    );
    let r3 = make_record_with(
        "r3",
        "Database connection pooling with SQLx",
        vec![],
        vec![],
    );

    store.store(&[r1, r2, r3]).await.unwrap();

    // Query matches r2 (cargo build dependencies).
    let results = store.retrieve("cargo build dependencies", 1).await.unwrap();

    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();

    // r2 should be the semantic hit.
    assert!(ids.contains(&"r2"), "r2 should be in results: {:?}", ids);

    // r1 (causal parent of r2) should be included via graph expansion.
    assert!(
        ids.contains(&"r1"),
        "r1 (causal parent of r2) should be in results: {:?}",
        ids
    );
}

#[tokio::test]
async fn test_retrieve_empty_index_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    let results = store.retrieve("anything", 5).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_retrieve_k_zero_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    store.store(&[make_record("r1")]).await.unwrap();

    let results = store.retrieve("test", 0).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_index_existing_after_reload() {
    let dir = tempfile::tempdir().unwrap();

    // Store records with one LongTermStore instance.
    {
        let store = LongTermStore::new(dir.path().to_path_buf());
        let r1 = make_record_with(
            "r1",
            "Rust async runtime tokio task scheduling",
            vec![],
            vec!["r2"],
        );
        let r2 = make_record_with(
            "r2",
            "Tokio task spawn and await futures",
            vec!["r1"],
            vec![],
        );
        store.store(&[r1, r2]).await.unwrap();
    }

    // Create a new LongTermStore (simulating restart) and index existing.
    let store = LongTermStore::new(dir.path().to_path_buf());
    let count = store.index_existing().await.unwrap();
    assert_eq!(count, 2, "Should index 2 existing records");

    // Retrieval should work after indexing.
    let results = store.retrieve("tokio async runtime", 2).await.unwrap();
    assert!(
        !results.is_empty(),
        "Retrieval should return results after index_existing"
    );

    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    assert!(
        ids.contains(&"r1"),
        "r1 should be retrieved after index_existing: {:?}",
        ids
    );
    // r2 is a causal neighbor of r1 and should be included.
    assert!(
        ids.contains(&"r2"),
        "r2 (causal neighbor) should be retrieved: {:?}",
        ids
    );
}

#[tokio::test]
async fn test_index_existing_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());
    let count = store.index_existing().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_retrieve_auto_indexes_on_empty() {
    // retrieve() on an empty in-memory index should auto-populate from disk.
    let dir = tempfile::tempdir().unwrap();

    // Store with one instance.
    {
        let store = LongTermStore::new(dir.path().to_path_buf());
        let r1 = make_record_with("r1", "Rust async runtime tokio", vec![], vec![]);
        store.store(&[r1]).await.unwrap();
    }

    // New instance — don't call index_existing, just retrieve directly.
    let store = LongTermStore::new(dir.path().to_path_buf());
    let results = store.retrieve("rust async", 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "r1");
}

#[tokio::test]
async fn test_store_indexes_for_retrieval() {
    // After store(), retrieve() should work without a separate index call.
    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::new(dir.path().to_path_buf());

    let r1 = make_record_with("r1", "Rust async runtime tokio", vec![], vec![]);
    let r2 = make_record_with("r2", "Database SQLx pooling", vec![], vec![]);

    store.store(&[r1, r2]).await.unwrap();

    let results = store.retrieve("async tokio runtime", 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "r1");
}

#[tokio::test]
async fn test_with_embedder_custom_provider() {
    use crate::analysis::vector_store::MockEmbeddingProvider;

    let dir = tempfile::tempdir().unwrap();
    let store = LongTermStore::with_embedder(
        dir.path().to_path_buf(),
        Arc::new(MockEmbeddingProvider::new(128)),
    );

    let r1 = make_record_with("r1", "test content one", vec![], vec!["r2"]);
    let r2 = make_record_with("r2", "test content two", vec!["r1"], vec![]);

    store.store(&[r1, r2]).await.unwrap();

    // With mock embedder, semantic ranking is hash-based, but causal
    // expansion should still work. With k=2, both records are semantic
    // hits, so both should be returned.
    let results = store.retrieve("test", 2).await.unwrap();
    assert_eq!(results.len(), 2);

    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"r1"));
    assert!(ids.contains(&"r2"));
}
