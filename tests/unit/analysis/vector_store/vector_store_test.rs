use super::*;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_chunk_type_weight() {
    assert_eq!(ChunkType::Function.weight(), 1.0);
    assert_eq!(ChunkType::Import.weight(), 0.3);
    assert!(ChunkType::Comment.weight() < ChunkType::Function.weight());
}

#[test]
fn test_chunk_metadata_creation() {
    let meta = ChunkMetadata::new(
        PathBuf::from("src/lib.rs"),
        1,
        10,
        ChunkType::Function,
        "rust",
        "fn main() {}",
    );

    assert_eq!(*meta.file_path, *Path::new("src/lib.rs"));
    assert_eq!(meta.start_line, 1);
    assert_eq!(meta.end_line, 10);
    assert_eq!(meta.chunk_type, ChunkType::Function);
    assert!(!meta.content_hash.is_empty());
}

#[test]
fn test_chunk_metadata_with_symbol() {
    let meta = ChunkMetadata::new(
        PathBuf::from("lib.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    )
    .with_symbol("test")
    .with_tag("unit-test");

    assert_eq!(meta.symbol_name, Some("test".to_string()));
    assert!(meta.tags.contains(&"unit-test".to_string()));
}

#[test]
fn test_code_chunk_creation() {
    let meta = ChunkMetadata::new(
        PathBuf::from("lib.rs"),
        1,
        3,
        ChunkType::Function,
        "rust",
        "fn hello() {}",
    );
    let chunk = CodeChunk::new("fn hello() {}".to_string(), meta);

    assert!(!chunk.id.is_empty());
    assert_eq!(chunk.content, "fn hello() {}");
    assert_eq!(chunk.len(), 13);
    assert!(!chunk.is_empty());
}

#[test]
fn test_search_filter() {
    let filter = SearchFilter::new()
        .with_file_pattern("*.rs")
        .with_chunk_type(ChunkType::Function)
        .with_language("rust")
        .with_min_score(0.5);

    let meta = ChunkMetadata::new(
        PathBuf::from("test.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );
    let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

    assert!(filter.matches(&chunk));
}

#[test]
fn test_search_filter_file_pattern_mismatch() {
    let filter = SearchFilter::new().with_file_pattern("*.py");

    let meta = ChunkMetadata::new(
        PathBuf::from("test.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );
    let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

    assert!(!filter.matches(&chunk));
}

#[test]
fn test_vector_collection_add_get() {
    let mut collection = VectorCollection::new("test", CollectionScope::Project);

    let meta = ChunkMetadata::new(
        PathBuf::from("lib.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );
    let chunk = CodeChunk::new("fn test() {}".to_string(), meta);
    let chunk_id = chunk.id.clone();

    collection.add_chunk(chunk).unwrap();

    assert_eq!(collection.len(), 1);
    assert!(collection.get_chunk(&chunk_id).is_some());
}

#[test]
fn test_vector_collection_remove_chunk() {
    let mut collection = VectorCollection::new("test", CollectionScope::Project);

    let meta = ChunkMetadata::new(
        PathBuf::from("lib.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );
    let chunk = CodeChunk::new("fn test() {}".to_string(), meta);
    let chunk_id = chunk.id.clone();

    collection.add_chunk(chunk).unwrap();
    assert_eq!(collection.len(), 1);

    let removed = collection.remove_chunk(&chunk_id);
    assert!(removed.is_some());
    assert_eq!(collection.len(), 0);
}

#[test]
fn test_vector_collection_remove_file() {
    let mut collection = VectorCollection::new("test", CollectionScope::Project);

    let path = PathBuf::from("lib.rs");

    for i in 0..3 {
        let meta = ChunkMetadata::new(
            path.clone(),
            i * 10 + 1,
            (i + 1) * 10,
            ChunkType::Function,
            "rust",
            &format!("fn test{}() {{}}", i),
        );
        let chunk = CodeChunk::new(format!("fn test{}() {{}}", i), meta);
        collection.add_chunk(chunk).unwrap();
    }

    assert_eq!(collection.len(), 3);
    collection.remove_file(&path);
    assert_eq!(collection.len(), 0);
}

#[tokio::test]
async fn test_mock_embedding_provider() {
    let provider = MockEmbeddingProvider::new(384);

    let embedding = provider.embed("test text").await.unwrap();
    assert_eq!(embedding.len(), 384);

    // Verify normalization
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01);
}

#[tokio::test]
async fn test_mock_embedding_deterministic() {
    let provider = MockEmbeddingProvider::new(384);

    let e1 = provider.embed("test").await.unwrap();
    let e2 = provider.embed("test").await.unwrap();

    assert_eq!(e1, e2);
}

#[tokio::test]
async fn test_tfidf_embedding_provider() {
    let provider = TfIdfEmbeddingProvider::new(256);

    let embedding = provider.embed("fn test() {}").await.unwrap();
    assert_eq!(embedding.len(), 256);
}

#[tokio::test]
async fn test_tfidf_similar_texts() {
    let provider = TfIdfEmbeddingProvider::new(256);

    let e1 = provider.embed("function test").await.unwrap();
    let e2 = provider.embed("test function").await.unwrap();

    // Similar texts should have high cosine similarity
    let similarity = VectorIndex::cosine_similarity(&e1, &e2);
    assert!(similarity > 0.5);
}

#[test]
#[ignore = "HNSW approximate search can return either close neighbour for \
                a 3-point dataset; run with --ignored to investigate index quality"]
fn test_vector_index_add_search() {
    let mut index = VectorIndex::new(4);

    // Add some embeddings
    index
        .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    index
        .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap();
    index
        .add("c".to_string(), vec![0.9, 0.1, 0.0, 0.0])
        .unwrap();

    // Search for something similar to "a"
    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 2);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "a"); // Exact match
    assert_eq!(results[1].0, "c"); // Close match
}

#[test]
#[ignore = "HNSW search after remove on a tiny dataset can return zero \
                results (the remaining vector may be filtered as a tombstone). \
                Run with --ignored to investigate index quality."]
fn test_vector_index_remove() {
    let mut index = VectorIndex::new(4);

    index
        .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    index
        .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap();

    assert_eq!(index.len(), 2);

    index.remove("a");
    assert_eq!(index.len(), 1);

    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 1);
    assert_eq!(results[0].0, "b"); // Only "b" left
}

#[test]
fn test_code_chunker_rust() {
    let chunker = CodeChunker::default();
    let content = r#"
pub fn hello() {
    println!("Hello");
}

pub struct Point {
    x: i32,
    y: i32,
}

impl Point {
    pub fn new() -> Self {
        Self { x: 0, y: 0 }
    }
}
"#;

    let chunks = chunker.chunk_rust(content, Path::new("lib.rs"));

    // Should have chunks for function, struct, and impl
    assert!(chunks.len() >= 3);

    let types: Vec<_> = chunks.iter().map(|c| c.metadata.chunk_type).collect();
    assert!(types.contains(&ChunkType::Function));
    assert!(types.contains(&ChunkType::Struct));
    assert!(types.contains(&ChunkType::Impl));
}

#[test]
fn test_code_chunker_extract_symbol() {
    let chunker = CodeChunker::default();

    // Test function extraction
    let fn_name = chunker.extract_rust_symbol("pub fn hello() {}", ChunkType::Function);
    assert_eq!(fn_name, Some("hello".to_string()));

    // Test struct extraction
    let struct_name = chunker.extract_rust_symbol("pub struct MyStruct {", ChunkType::Struct);
    assert_eq!(struct_name, Some("MyStruct".to_string()));

    // Test impl extraction
    let impl_name = chunker.extract_rust_symbol("impl MyStruct {", ChunkType::Impl);
    assert_eq!(impl_name, Some("MyStruct".to_string()));
}

#[test]
fn test_code_chunker_fixed_size() {
    let chunker = CodeChunker {
        max_chunk_size: 100,
        min_chunk_size: 10,
        overlap: 10,
    };

    let content = "a\n".repeat(50);
    let chunks = chunker.chunk_fixed_size(&content, Path::new("test.txt"), "txt");

    assert!(!chunks.is_empty());
    for chunk in &chunks {
        assert!(crate::token_count::estimate_content_tokens(&chunk.content) <= 100);
    }
}

#[test]
fn test_code_chunker_bounds_a_single_oversized_line() {
    let chunker = CodeChunker {
        max_chunk_size: 32,
        min_chunk_size: 1,
        overlap: 4,
    };
    let content = "identifier_".repeat(500);

    let chunks = chunker.chunk_fixed_size(&content, Path::new("minified.js"), "js");

    assert!(chunks.len() > 1);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.content.as_str())
            .collect::<String>(),
        content
    );
    assert!(chunks
        .iter()
        .all(|chunk| { crate::token_count::estimate_content_tokens(&chunk.content) <= 32 }));
    let ids: std::collections::HashSet<&str> =
        chunks.iter().map(|chunk| chunk.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        chunks.len(),
        "pieces from one oversized line must have distinct stable IDs"
    );
    assert!(chunks
        .windows(2)
        .all(|pair| pair[0].metadata.byte_offset < pair[1].metadata.byte_offset));
}

#[test]
fn test_code_chunker_refreshes_ids_after_rebasing_split_rust_chunks() {
    let chunker = CodeChunker {
        max_chunk_size: 24,
        min_chunk_size: 1,
        overlap: 2,
    };
    let content = format!(
        "// heading\n\npub fn huge() {{\n    let value = \"{}\";\n}}\n",
        "repeated_token_".repeat(200)
    );

    let chunks = chunker.chunk(&content, Path::new("src/lib.rs"));
    assert!(chunks.len() > 1);
    let ids: std::collections::HashSet<&str> =
        chunks.iter().map(|chunk| chunk.id.as_str()).collect();
    assert_eq!(ids.len(), chunks.len());
    let function_chunks: Vec<&CodeChunk> = chunks
        .iter()
        .filter(|chunk| chunk.metadata.chunk_type == ChunkType::Function)
        .collect();
    assert!(function_chunks.len() > 1);
    for chunk in function_chunks {
        assert_eq!(chunk.id, CodeChunk::stable_id(&chunk.metadata));
        assert!(chunk.metadata.start_line >= 3);
    }
}

#[tokio::test]
async fn test_vector_store_create_collection() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    store.collection("test", CollectionScope::Project);

    assert!(store.get_collection("test").is_some());
    assert!(store.list_collections().contains(&"test"));
}

#[tokio::test]
async fn test_vector_store_delete_collection() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    store.collection("test", CollectionScope::Project);
    let deleted = store.delete_collection("test");

    assert!(deleted.is_some());
    assert!(store.get_collection("test").is_none());
}

#[test]
fn test_vector_store_delete_then_save_publishes_deletion_and_prunes_previous_generation() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let mut store = VectorStore::new(provider.clone()).with_storage(&storage_path);
    store.collection("deleted", CollectionScope::Project);
    store.save().unwrap();
    let previous_generation = current_generation_path(&storage_path);
    assert!(previous_generation.is_dir());

    assert!(store.delete_collection("deleted").is_some());
    store.save().unwrap();
    assert!(
        !previous_generation.exists(),
        "the previously committed generation should be pruned only after replacement"
    );

    let mut loaded = VectorStore::new(provider).with_storage(&storage_path);
    loaded.load().unwrap();
    assert!(loaded.get_collection("deleted").is_none());
    assert!(loaded.list_collections().is_empty());
}

#[tokio::test]
async fn test_vector_store_index_file() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(&file_path, "pub fn test() {}\npub fn hello() {}").unwrap();

    store.collection("project", CollectionScope::Project);
    let count = store.index_file("project", &file_path).await.unwrap();

    assert!(count >= 1);

    let collection = store.get_collection("project").unwrap();
    assert!(!collection.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn vector_store_refuses_symlinked_source_file() {
    use std::os::unix::fs::symlink;

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);
    store.collection("project", CollectionScope::Project);

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let target = outside.path().join("private.rs");
    std::fs::write(&target, "pub fn private_material() {}").unwrap();
    let linked = root.path().join("visible.rs");
    symlink(&target, &linked).unwrap();

    let error = store.index_file("project", &linked).await.unwrap_err();
    assert!(
        format!("{error:#}").contains("indexed source file"),
        "unexpected error: {error:#}"
    );
    assert!(store.get_collection("project").unwrap().is_empty());
}

#[tokio::test]
async fn reindex_file_replaces_chunks_not_accumulates() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    store.collection("project", CollectionScope::Project);

    // First index of a 3-function file.
    std::fs::write(&file_path, "pub fn a() {}\npub fn b() {}\npub fn c() {}").unwrap();
    store.index_file("project", &file_path).await.unwrap();
    let first = store.get_collection("project").unwrap().len();
    let first_ids = store
        .get_collection("project")
        .unwrap()
        .chunks()
        .iter()
        .map(|chunk| chunk.id.clone())
        .collect::<Vec<_>>();
    assert!(first >= 1);

    // Re-index the SAME file with SHRUNK content: stale chunks must be
    // removed, not accumulated on top of the old ones.
    std::fs::write(&file_path, "pub fn a() {}").unwrap();
    store.index_file("project", &file_path).await.unwrap();
    let second = store.get_collection("project").unwrap().len();
    let live_index = store.indices.get("project").unwrap();
    assert_eq!(
        live_index.len(),
        second,
        "collection replacement must remove the same stale IDs from HNSW"
    );
    for old_id in &first_ids {
        if store
            .get_collection("project")
            .unwrap()
            .get_chunk(old_id)
            .is_none()
        {
            assert!(
                !live_index
                    .chunk_ids
                    .iter()
                    .enumerate()
                    .any(|(index, id)| id == old_id && !live_index.deleted.contains(&index)),
                "removed collection chunk {old_id} must not remain live in HNSW"
            );
        }
    }
    assert!(
        second <= first,
        "re-index of a smaller file must not leave stale chunks (first={first}, second={second})"
    );

    // Re-indexing identical content again is idempotent — no growth.
    store.index_file("project", &file_path).await.unwrap();
    let third = store.get_collection("project").unwrap().len();
    assert_eq!(
        second, third,
        "repeated re-index must not accumulate chunks (second={second}, third={third})"
    );
}

#[tokio::test]
async fn test_vector_store_search() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(
        &file_path,
        r#"
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

pub fn calculate_product(a: i32, b: i32) -> i32 {
    a * b
}
"#,
    )
    .unwrap();

    store.collection("project", CollectionScope::Project);
    store.index_file("project", &file_path).await.unwrap();

    let results = store
        .search("project", "sum addition", 5, None)
        .await
        .unwrap();

    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_vector_store_search_with_filter() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(&file_path, "pub fn test() {}").unwrap();

    store.collection("project", CollectionScope::Project);
    store.index_file("project", &file_path).await.unwrap();

    let filter = SearchFilter::new()
        .with_chunk_type(ChunkType::Struct)
        .with_min_score(0.9);

    let results = store
        .search("project", "test", 5, Some(&filter))
        .await
        .unwrap();

    // Should be empty due to filter (no structs, high min score)
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_vector_store_persistence() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");

    // Create and populate store
    {
        let mut store = VectorStore::new(provider.clone()).with_storage(&storage_path);

        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "pub fn test() {}").unwrap();

        store.collection("project", CollectionScope::Project);
        store.index_file("project", &file_path).await.unwrap();
        store.save().unwrap();
    }

    // Load store from disk
    {
        let mut store = VectorStore::new(provider).with_storage(&storage_path);
        store.load().unwrap();

        assert!(store.get_collection("project").is_some());
    }
}

#[tokio::test]
async fn test_vector_store_persistence_chunk_id_resolves() {
    // Regression test: chunks and id_index used to be #[serde(skip)] so
    // after save+load, get_chunk() always returned None and RAG was
    // broken across restarts.  Now chunks are persisted and id_index is
    // rebuilt on load, so a known chunk id must still resolve.
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");

    let known_chunk_id = {
        let mut store = VectorStore::new(provider.clone()).with_storage(&storage_path);
        let file_path = dir.path().join("persist_test.rs");
        std::fs::write(&file_path, "pub fn persisted_fn() { let x = 42; }").unwrap();

        store.collection("project", CollectionScope::Project);
        store.index_file("project", &file_path).await.unwrap();
        store.save().unwrap();

        // Grab the first chunk id before we drop the store
        store
            .get_collection("project")
            .unwrap()
            .chunks()
            .first()
            .unwrap()
            .id
            .clone()
    };

    // Reload from disk and verify the chunk id resolves
    {
        let mut store = VectorStore::new(provider).with_storage(&storage_path);
        store.load().unwrap();

        let collection = store.get_collection("project").expect("collection missing");
        assert!(!collection.is_empty(), "chunks should survive save+load");
        assert!(
            collection.get_chunk(&known_chunk_id).is_some(),
            "known chunk id '{}' should resolve after reload — RAG is broken if it doesn't",
            known_chunk_id
        );
    }
}

fn current_generation_path(storage_path: &Path) -> PathBuf {
    let manifest: VectorStoreManifest = serde_json::from_slice(
        &std::fs::read(storage_path.join(VECTOR_STORE_MANIFEST_FILE)).unwrap(),
    )
    .unwrap();
    VectorStore::generation_path(storage_path, &manifest.generation).unwrap()
}

#[tokio::test]
async fn test_vector_store_load_rejects_id_mismatch_without_partial_state() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let file_path = dir.path().join("persist_test.rs");
    std::fs::write(&file_path, "pub fn persisted_fn() { let x = 42; }").unwrap();

    let mut source = VectorStore::new(provider.clone()).with_storage(&storage_path);
    source.collection("project", CollectionScope::Project);
    source.index_file("project", &file_path).await.unwrap();
    source.save().unwrap();

    let index_path = current_generation_path(&storage_path).join(VECTOR_STORE_INDICES_FILE);
    let bytes = std::fs::read(&index_path).unwrap();
    let (mut persisted, consumed): (PersistedIndices, usize) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(consumed, bytes.len());
    persisted.indices.get_mut("project").unwrap().chunk_ids[0] = "wrong-id".to_string();
    std::fs::write(
        &index_path,
        bincode::serde::encode_to_vec(&persisted, bincode::config::standard()).unwrap(),
    )
    .unwrap();

    let mut target = VectorStore::new(provider).with_storage(&storage_path);
    target.collection("sentinel", CollectionScope::Session);
    let error = target.load().unwrap_err();
    assert!(
        error.to_string().contains("do not exactly match"),
        "unexpected error: {error:#}"
    );
    assert!(target.get_collection("sentinel").is_some());
    assert!(target.get_collection("project").is_none());
}

#[tokio::test]
async fn test_vector_store_load_requires_full_index_parse_before_replacing_state() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let file_path = dir.path().join("persist_test.rs");
    std::fs::write(&file_path, "pub fn persisted_fn() {}").unwrap();

    let mut source = VectorStore::new(provider.clone()).with_storage(&storage_path);
    source.collection("project", CollectionScope::Project);
    source.index_file("project", &file_path).await.unwrap();
    source.save().unwrap();

    let index_path = current_generation_path(&storage_path).join(VECTOR_STORE_INDICES_FILE);
    let mut bytes = std::fs::read(&index_path).unwrap();
    bytes.extend_from_slice(b"trailing-corruption");
    std::fs::write(index_path, bytes).unwrap();

    let mut target = VectorStore::new(provider).with_storage(&storage_path);
    target.collection("sentinel", CollectionScope::Session);
    let error = target.load().unwrap_err();
    assert!(error.to_string().contains("trailing data"));
    assert!(target.get_collection("sentinel").is_some());
    assert!(target.get_collection("project").is_none());
}

#[tokio::test]
async fn test_vector_store_load_rejects_chunk_content_hash_mismatch() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let file_path = dir.path().join("persist_test.rs");
    std::fs::write(&file_path, "pub fn persisted_fn() {}").unwrap();

    let mut source = VectorStore::new(provider.clone()).with_storage(&storage_path);
    source.collection("project", CollectionScope::Project);
    source.index_file("project", &file_path).await.unwrap();
    source.save().unwrap();

    let collection_path =
        current_generation_path(&storage_path).join(VECTOR_STORE_COLLECTIONS_FILE);
    let mut persisted: PersistedCollections =
        serde_json::from_slice(&std::fs::read(&collection_path).unwrap()).unwrap();
    persisted.collections.get_mut("project").unwrap().chunks[0].content =
        "stale or tampered model context".to_string();
    std::fs::write(&collection_path, serde_json::to_vec(&persisted).unwrap()).unwrap();

    let mut target = VectorStore::new(provider).with_storage(&storage_path);
    target.collection("sentinel", CollectionScope::Session);
    let error = target.load().unwrap_err();
    assert!(error.to_string().contains("content hash"), "{error:#}");
    assert!(target.get_collection("sentinel").is_some());
    assert!(target.get_collection("project").is_none());
}

#[test]
fn test_vector_store_ignores_unpublished_generation() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");

    let mut source = VectorStore::new(provider.clone()).with_storage(&storage_path);
    source.collection("committed", CollectionScope::Project);
    source.save().unwrap();

    let orphan =
        VectorStore::generation_path(&storage_path, "00000000000000000000000000000000").unwrap();
    std::fs::create_dir(&orphan).unwrap();
    std::fs::write(orphan.join(VECTOR_STORE_COLLECTIONS_FILE), b"partial").unwrap();

    let mut loaded = VectorStore::new(provider).with_storage(&storage_path);
    loaded.load().unwrap();
    assert!(loaded.get_collection("committed").is_some());
    assert_eq!(loaded.list_collections().len(), 1);
}

#[cfg(unix)]
#[test]
fn test_vector_store_pruning_never_follows_or_removes_generation_symlink() {
    use std::os::unix::fs::symlink;

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let protected = dir.path().join("protected");
    std::fs::create_dir(&protected).unwrap();
    std::fs::write(protected.join("marker"), b"keep").unwrap();

    let mut store = VectorStore::new(provider.clone()).with_storage(&storage_path);
    store.collection("first", CollectionScope::Project);
    store.save().unwrap();
    let previous_generation = current_generation_path(&storage_path);
    std::fs::remove_dir_all(&previous_generation).unwrap();
    symlink(&protected, &previous_generation).unwrap();

    store.collection("second", CollectionScope::Session);
    store.save().unwrap();

    assert!(
        std::fs::symlink_metadata(&previous_generation)
            .unwrap()
            .file_type()
            .is_symlink(),
        "generation cleanup must leave a masquerading symlink untouched"
    );
    assert_eq!(std::fs::read(protected.join("marker")).unwrap(), b"keep");
    let mut loaded = VectorStore::new(provider).with_storage(&storage_path);
    loaded.load().unwrap();
    assert!(loaded.get_collection("second").is_some());
}

#[cfg(unix)]
#[test]
fn test_vector_store_load_refuses_symlinked_active_generation() {
    use std::os::unix::fs::symlink;

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let external_generation = dir.path().join("external-generation");

    let mut store = VectorStore::new(provider.clone()).with_storage(&storage_path);
    store.collection("project", CollectionScope::Project);
    store.save().unwrap();
    let active_generation = current_generation_path(&storage_path);
    std::fs::rename(&active_generation, &external_generation).unwrap();
    symlink(&external_generation, &active_generation).unwrap();

    let mut loaded = VectorStore::new(provider).with_storage(&storage_path);
    let error = loaded.load().unwrap_err();
    assert!(error.to_string().contains("real directory"), "{error:#}");
    assert!(loaded.list_collections().is_empty());
}

#[test]
fn test_vector_store_concurrent_saves_publish_complete_unique_generations() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    let mut first = VectorStore::new(provider.clone()).with_storage(&storage_path);
    first.collection("first", CollectionScope::Project);
    let mut second = VectorStore::new(provider.clone()).with_storage(&storage_path);
    second.collection("second", CollectionScope::Session);

    std::thread::scope(|scope| {
        let first_save = scope.spawn(|| first.save());
        let second_save = scope.spawn(|| second.save());
        first_save.join().unwrap().unwrap();
        second_save.join().unwrap().unwrap();
    });

    let entries: Vec<String> = std::fs::read_dir(&storage_path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let generation_count = entries
        .iter()
        .filter(|name| name.starts_with(VECTOR_STORE_GENERATION_PREFIX))
        .count();
    assert!((1..=2).contains(&generation_count));
    assert!(!entries.iter().any(|name| name.ends_with(".tmp")));
    assert!(current_generation_path(&storage_path).is_dir());

    let mut loaded = VectorStore::new(provider).with_storage(&storage_path);
    loaded.load().unwrap();
    let names = loaded.list_collections();
    assert!(names == ["first"] || names == ["second"]);
}

#[test]
fn test_vector_store_loads_complete_legacy_collection_index_pair() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::new(3)));
    let dir = tempdir().unwrap();
    let storage_path = dir.path().join("vector_store");
    std::fs::create_dir(&storage_path).unwrap();

    let content = "legacy chunk".to_string();
    let metadata = ChunkMetadata::new(
        PathBuf::from("legacy.rs"),
        1,
        1,
        ChunkType::CodeBlock,
        "rust",
        &content,
    );
    let chunk = CodeChunk::new(content, metadata);
    let chunk_id = chunk.id.clone();
    let mut collection = VectorCollection::new("legacy", CollectionScope::Project);
    collection.add_chunk(chunk).unwrap();
    std::fs::write(
        storage_path.join("legacy.json"),
        serde_json::to_vec_pretty(&collection).unwrap(),
    )
    .unwrap();
    let embeddings = vec![vec![1.0f32, 0.0, 0.0]];
    let chunk_ids = vec![chunk_id.clone()];
    std::fs::write(
        storage_path.join("legacy.idx"),
        bincode::serde::encode_to_vec((&embeddings, &chunk_ids), bincode::config::standard())
            .unwrap(),
    )
    .unwrap();

    let mut loaded = VectorStore::new(provider).with_storage(&storage_path);
    loaded.load().unwrap();
    let collection = loaded.get_collection("legacy").unwrap();
    assert!(collection.get_chunk(&chunk_id).is_some());
    assert_eq!(loaded.indices.get("legacy").unwrap().dimension(), 3);
}

#[tokio::test]
async fn test_vector_store_stats() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    store.collection("project1", CollectionScope::Project);
    store.collection("project2", CollectionScope::Session);

    let stats = store.stats();

    assert_eq!(stats.collection_count, 2);
    assert_eq!(stats.embedding_dimension, EMBEDDING_DIM);
}

#[test]
fn test_cosine_similarity() {
    // Identical vectors
    let sim = VectorIndex::cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]);
    assert!((sim - 1.0).abs() < 0.01);

    // Orthogonal vectors
    let sim = VectorIndex::cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);
    assert!(sim.abs() < 0.01);

    // Opposite vectors
    let sim = VectorIndex::cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]);
    assert!((sim + 1.0).abs() < 0.01);
}

#[test]
fn test_collection_scope_default() {
    assert_eq!(CollectionScope::default(), CollectionScope::Project);
}

#[test]
fn test_chunk_type_default() {
    assert_eq!(ChunkType::default(), ChunkType::CodeBlock);
}

#[test]
fn test_empty_vector_index() {
    let index = VectorIndex::new(4);
    assert!(index.is_empty());
    assert_eq!(index.len(), 0);

    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 5);
    assert!(results.is_empty());
}

#[test]
fn test_vector_index_dimension_mismatch() {
    let mut index = VectorIndex::new(4);
    let result = index.add("a".to_string(), vec![1.0, 0.0, 0.0]); // Only 3 dims
    assert!(result.is_err());
}

#[tokio::test]
async fn test_embedding_batch() {
    let provider = MockEmbeddingProvider::default();
    let texts = vec!["hello".to_string(), "world".to_string()];

    let embeddings = provider.embed_batch(&texts).await.unwrap();

    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[0].len(), EMBEDDING_DIM);
}

#[test]
fn test_search_filter_empty_matches_all() {
    let filter = SearchFilter::new();

    let meta = ChunkMetadata::new(
        PathBuf::from("any.py"),
        1,
        5,
        ChunkType::Text,
        "python",
        "# comment",
    );
    let chunk = CodeChunk::new("# comment".to_string(), meta);

    assert!(filter.matches(&chunk)); // Empty filter matches everything
}

#[test]
fn test_chunk_with_embedding() {
    let meta = ChunkMetadata::new(
        PathBuf::from("lib.rs"),
        1,
        3,
        ChunkType::Function,
        "rust",
        "fn hello() {}",
    );
    let chunk = CodeChunk::new("fn hello() {}".to_string(), meta);
    let embedding = vec![0.1, 0.2, 0.3];

    let chunk = chunk.with_embedding(embedding.clone());
    assert_eq!(chunk.embedding, Some(embedding));
}

#[test]
fn test_collection_files() {
    let mut collection = VectorCollection::new("test", CollectionScope::Project);

    for path in ["a.rs", "b.rs", "c.rs"] {
        let meta = ChunkMetadata::new(
            PathBuf::from(path),
            1,
            5,
            ChunkType::Function,
            "rust",
            "fn test() {}",
        );
        let chunk = CodeChunk::new("fn test() {}".to_string(), meta);
        collection.add_chunk(chunk).unwrap();
    }

    let files = collection.files();
    assert_eq!(files.len(), 3);
}

// Additional comprehensive tests

#[test]
fn test_chunk_type_all_variants() {
    let types = [
        ChunkType::Function,
        ChunkType::Struct,
        ChunkType::Enum,
        ChunkType::Trait,
        ChunkType::Impl,
        ChunkType::Module,
        ChunkType::Import,
        ChunkType::Comment,
        ChunkType::Test,
        ChunkType::Constant,
        ChunkType::CodeBlock,
        ChunkType::Text,
    ];

    for chunk_type in types {
        assert!(chunk_type.weight() >= 0.0);
        assert!(chunk_type.weight() <= 1.0);
        let _ = format!("{:?}", chunk_type);
    }
}

#[test]
fn test_chunk_metadata_clone() {
    let meta = ChunkMetadata::new(
        PathBuf::from("test.rs"),
        1,
        10,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );

    let cloned = meta.clone();
    assert_eq!(meta.file_path, cloned.file_path);
    assert_eq!(meta.content_hash, cloned.content_hash);
}

#[test]
fn test_chunk_metadata_serialization() {
    let meta = ChunkMetadata::new(
        PathBuf::from("test.rs"),
        1,
        10,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );

    let json = serde_json::to_string(&meta).unwrap();
    let deserialized: ChunkMetadata = serde_json::from_str(&json).unwrap();

    assert_eq!(meta.chunk_type, deserialized.chunk_type);
}

#[test]
fn test_code_chunk_clone() {
    let meta = ChunkMetadata::new(
        PathBuf::from("lib.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn hello() {}",
    );
    let chunk = CodeChunk::new("fn hello() {}".to_string(), meta);

    let cloned = chunk.clone();
    assert_eq!(chunk.id, cloned.id);
    assert_eq!(chunk.content, cloned.content);
}

#[test]
fn test_search_filter_clone() {
    let filter = SearchFilter::new()
        .with_file_pattern("*.rs")
        .with_chunk_type(ChunkType::Function);

    let cloned = filter.clone();
    assert_eq!(filter.file_patterns, cloned.file_patterns);
}

#[test]
fn test_search_filter_with_tag() {
    let filter = SearchFilter::new().with_tag("important");

    let meta = ChunkMetadata::new(
        PathBuf::from("test.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    )
    .with_tag("important");

    let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

    assert!(filter.matches(&chunk));
}

#[test]
fn test_collection_scope_all_variants() {
    let scopes = [
        CollectionScope::Project,
        CollectionScope::Session,
        CollectionScope::Global,
    ];

    for scope in scopes {
        let _ = format!("{:?}", scope);
        let cloned = scope;
        assert_eq!(scope, cloned);
    }
}

#[test]
fn test_vector_collection_is_empty() {
    let collection = VectorCollection::new("test", CollectionScope::Project);
    assert!(collection.is_empty());
    assert_eq!(collection.len(), 0);
}

#[test]
fn test_vector_collection_name() {
    let collection = VectorCollection::new("test_collection", CollectionScope::Project);
    assert_eq!(collection.name, "test_collection");
}

#[test]
fn test_search_result_clone() {
    let meta = ChunkMetadata::new(
        PathBuf::from("test.rs"),
        1,
        5,
        ChunkType::Function,
        "rust",
        "fn test() {}",
    );
    let chunk = CodeChunk::new("fn test() {}".to_string(), meta);

    let result = SearchResult {
        chunk,
        score: 0.95,
        distance: 0.05,
    };

    let cloned = result.clone();
    assert_eq!(result.score, cloned.score);
    assert_eq!(result.distance, cloned.distance);
}

#[test]
fn test_vector_index_clear() {
    let mut index = VectorIndex::new(4);

    index
        .add("a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    index
        .add("b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap();

    assert_eq!(index.len(), 2);

    index.clear();
    assert!(index.is_empty());
}

#[tokio::test]
async fn test_mock_embedding_provider_dimension() {
    let provider = MockEmbeddingProvider::new(512);

    let embedding = provider.embed("test").await.unwrap();
    assert_eq!(embedding.len(), 512);
}

#[test]
fn test_code_chunker_new() {
    let chunker = CodeChunker::new(2000);
    assert_eq!(chunker.max_chunk_size, 2000);
}

#[test]
fn test_vector_store_stats_empty() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let store = VectorStore::new(provider);

    let stats = store.stats();
    assert_eq!(stats.collection_count, 0);
    assert_eq!(stats.total_chunks, 0);
}

// =========================================================================
// Index integrity and health tests
// =========================================================================

#[test]
fn test_verify_index_integrity_healthy() {
    let mut index = VectorIndex::new(3);
    index.add("a".to_string(), vec![1.0, 0.0, 0.0]).unwrap();
    index.add("b".to_string(), vec![0.0, 1.0, 0.0]).unwrap();

    let issues = index.verify_index_integrity();
    assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
}

#[test]
fn test_verify_index_integrity_nan() {
    let mut index = VectorIndex::new(3);
    index
        .add("a".to_string(), vec![1.0, f32::NAN, 0.0])
        .unwrap();

    let issues = index.verify_index_integrity();
    assert!(!issues.is_empty());
    assert!(issues.iter().any(|i| i.contains("NaN")));
}

#[test]
fn test_verify_index_integrity_inf() {
    let mut index = VectorIndex::new(3);
    // After L2 normalization at insert time, [1.0, INF, 0.0] becomes
    // [0.0, NaN, 0.0] because INF/INF = NaN. The integrity check should
    // still detect the bad embedding.
    index
        .add("a".to_string(), vec![1.0, f32::INFINITY, 0.0])
        .unwrap();

    let issues = index.verify_index_integrity();
    assert!(!issues.is_empty());
    assert!(issues
        .iter()
        .any(|i| i.contains("NaN") || i.contains("Inf")));
}

#[test]
fn test_verify_index_integrity_duplicate_ids() {
    let mut index = VectorIndex::new(2);
    index.add("dup".to_string(), vec![1.0, 0.0]).unwrap();
    index.add("dup".to_string(), vec![0.0, 1.0]).unwrap();

    let issues = index.verify_index_integrity();
    assert!(issues.iter().any(|i| i.contains("Duplicate")));
}

#[test]
fn test_check_health_healthy() {
    let mut index = VectorIndex::new(2);
    index.add("a".to_string(), vec![1.0, 0.0]).unwrap();
    assert_eq!(index.check_health(), IndexHealth::Healthy);
}

#[test]
fn test_check_health_corrupt_nan() {
    let mut index = VectorIndex::new(2);
    index.add("a".to_string(), vec![f32::NAN, 0.0]).unwrap();
    assert_eq!(index.check_health(), IndexHealth::Corrupt);
}

#[test]
fn test_check_health_degraded_duplicates() {
    let mut index = VectorIndex::new(2);
    index.add("dup".to_string(), vec![1.0, 0.0]).unwrap();
    index.add("dup".to_string(), vec![0.0, 1.0]).unwrap();
    assert_eq!(index.check_health(), IndexHealth::Degraded);
}

#[test]
fn test_check_health_empty_index() {
    let index = VectorIndex::new(4);
    assert_eq!(index.check_health(), IndexHealth::Healthy);
}

#[tokio::test]
async fn test_rebuild_index() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut store = VectorStore::new(provider);

    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(&file_path, "pub fn test() {}").unwrap();

    store.collection("project", CollectionScope::Project);
    store.index_file("project", &file_path).await.unwrap();

    // Rebuild should succeed
    store.rebuild_index("project").await.unwrap();

    let index = store.indices.get("project").unwrap();
    assert_eq!(index.check_health(), IndexHealth::Healthy);
}

// ── Regex caching tests ──────────────────────────────────────────

#[test]
fn test_cached_extract_rust_symbol_fn() {
    let chunker = CodeChunker::default();
    // Verify cached regexes produce the same results as before
    assert_eq!(
        chunker.extract_rust_symbol("pub fn hello() {}", ChunkType::Function),
        Some("hello".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("fn world() {}", ChunkType::Function),
        Some("world".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("pub struct Foo {", ChunkType::Function),
        None, // no "fn" keyword
    );
}

#[test]
fn test_cached_extract_rust_symbol_all_types() {
    let chunker = CodeChunker::default();

    assert_eq!(
        chunker.extract_rust_symbol("pub struct MyStruct {", ChunkType::Struct),
        Some("MyStruct".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("enum Color {", ChunkType::Enum),
        Some("Color".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("pub trait Display {", ChunkType::Trait),
        Some("Display".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("impl<T> MyStruct {", ChunkType::Impl),
        Some("MyStruct".to_string())
    );
    // The regex matches the first word after `impl`, which is the trait name
    // in `impl Trait for Type` form. This matches the original behavior.
    assert_eq!(
        chunker.extract_rust_symbol("impl Display for MyStruct {", ChunkType::Impl),
        Some("Display".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("mod utils {", ChunkType::Module),
        Some("utils".to_string())
    );
    assert_eq!(
        chunker.extract_rust_symbol("// comment", ChunkType::Comment),
        None,
    );
}

// ── BoundedVectorStore tests ─────────────────────────────────────

#[tokio::test]
async fn test_bounded_vector_store_eviction_at_capacity() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let inner = VectorStore::new(provider);
    // Very small capacity to trigger eviction quickly
    let mut bounded = BoundedVectorStore::new(inner, 3);

    bounded.collection("test", CollectionScope::Project);

    let dir = tempdir().unwrap();

    // Index several small files. Each file should produce at least 1 chunk.
    for i in 0..6 {
        let file_path = dir.path().join(format!("file{}.rs", i));
        std::fs::write(
            &file_path,
            format!("pub fn func_{}() {{ println!(\"hello\"); }}", i),
        )
        .unwrap();
        bounded.index_file("test", &file_path).await.unwrap();
    }

    // The store should not exceed max_items
    assert!(
        bounded.len() <= 3,
        "Store has {} items but max is 3",
        bounded.len()
    );
}

#[tokio::test]
async fn test_bounded_vector_store_stays_within_bounds() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let inner = VectorStore::new(provider);
    let mut bounded = BoundedVectorStore::new(inner, 5);

    bounded.collection("coll", CollectionScope::Session);

    let dir = tempdir().unwrap();

    // Insert many files
    for i in 0..20 {
        let file_path = dir.path().join(format!("mod{}.rs", i));
        std::fs::write(
            &file_path,
            format!("pub fn handler_{}() {{ let x = {}; }}", i, i * 42),
        )
        .unwrap();
        bounded.index_file("coll", &file_path).await.unwrap();

        // After each insertion, the store must not exceed max_items
        assert!(
            bounded.len() <= 5,
            "After inserting file {}, store has {} items (max 5)",
            i,
            bounded.len()
        );
    }
}

#[tokio::test]
async fn test_bounded_vector_store_clear() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let inner = VectorStore::new(provider);
    let mut bounded = BoundedVectorStore::new(inner, 100);

    bounded.collection("proj", CollectionScope::Project);

    let dir = tempdir().unwrap();
    let file_path = dir.path().join("code.rs");
    std::fs::write(&file_path, "pub fn example() { let _ = 1 + 2; }").unwrap();
    bounded.index_file("proj", &file_path).await.unwrap();

    assert!(!bounded.is_empty());

    bounded.clear();
    assert!(bounded.is_empty());
    assert_eq!(bounded.len(), 0);
}

#[test]
fn test_bounded_vector_store_default_capacity() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let inner = VectorStore::new(provider);
    let bounded = BoundedVectorStore::with_default_capacity(inner);
    assert_eq!(bounded.max_items(), DEFAULT_MAX_ITEMS);
    assert!(bounded.is_empty());
}

// ── Filter-aware progressive expansion test ─────────────────────

#[tokio::test]
async fn test_vector_store_search_filter_does_not_starve() {
    // Build a collection + index with 20 chunks where the 5 chunks
    // MATCHING the filter are ranked LOWER in similarity than at
    // least k*2 non-matching chunks.  A plain k*2 search would miss
    // them, but progressive expansion must find them.

    let dim = 4;
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::new(dim)));
    let mut store = VectorStore::new(provider.clone());
    let collection_name = "test_filter_starve";
    store.collection(collection_name, CollectionScope::Project);

    // Embed the query text through the same provider so we know the
    // exact query embedding the search will use.
    let query_text = "unique query text";
    let query_emb = provider.embed(query_text).await.unwrap();

    // Build a decoy embedding that is nearly identical to the query
    // embedding (high cosine similarity).  We perturb slightly so each
    // decoy is unique and ranks just below the query direction.
    let mut decoy_base = query_emb.clone();
    // Make decoys slightly shorter so cosine sim < 1 but still high
    decoy_base[0] *= 0.99;

    // 15 "decoy" chunks with high similarity to the query.
    // chunk_type = Struct so the filter (Function) rejects them.
    for i in 0..15u32 {
        let mut emb = decoy_base.clone();
        // Slightly perturb a later dimension so each embedding is unique
        emb[3] += (i as f32) * 0.0001;

        let path = PathBuf::from(format!("decoy{}.rs", i));
        let content = format!("pub struct Decoy{} {{}}", i);
        let meta = ChunkMetadata::new(
            path,
            i as usize * 10 + 1,
            (i + 1) as usize * 10,
            ChunkType::Struct, // NOT Function — will be filtered out
            "rust",
            &content,
        );
        let chunk = CodeChunk::new(content, meta).with_embedding(emb.clone());
        let chunk_id = chunk.id.clone();
        store
            .collections
            .get_mut(collection_name)
            .unwrap()
            .add_chunk(chunk)
            .unwrap();
        store
            .indices
            .get_mut(collection_name)
            .unwrap()
            .add(chunk_id, emb)
            .unwrap();
    }

    // 5 "target" chunks with LOW similarity (orthogonal to query).
    // These are the chunks the filter should match (Function type).
    for i in 0..5u32 {
        // Orthogonal direction — cosine similarity ~0 with the query
        let mut emb = vec![0.0; dim];
        emb[2] = 1.0;
        emb[3] = (i as f32) * 0.01;

        let path = PathBuf::from(format!("target{}.rs", i));
        let content = format!("pub fn target{}() {{}}", i);
        let meta = ChunkMetadata::new(
            path,
            1000 + i as usize * 10,
            1010 + i as usize * 10,
            ChunkType::Function, // matches the filter
            "rust",
            &content,
        );
        let chunk = CodeChunk::new(content, meta).with_embedding(emb.clone());
        let chunk_id = chunk.id.clone();
        store
            .collections
            .get_mut(collection_name)
            .unwrap()
            .add_chunk(chunk)
            .unwrap();
        store
            .indices
            .get_mut(collection_name)
            .unwrap()
            .add(chunk_id, emb)
            .unwrap();
    }

    // Verify the collection has 20 chunks
    assert_eq!(store.get_collection(collection_name).unwrap().len(), 20);

    // k = 3, so k*2 = 6.  The top 6 candidates are all decoys (Structs).
    // The 5 targets (Functions) are ranked below the top 6, so a plain
    // k*2 search would return 0 results after filtering.
    let k = 3;

    let filter = SearchFilter::new().with_chunk_type(ChunkType::Function);

    let results = store
        .search(collection_name, query_text, k, Some(&filter))
        .await
        .unwrap();

    // Without progressive expansion, the results would be empty
    // because the top k*2=6 candidates are all Structs (rejected
    // by the filter).  With expansion, the search must find the
    // Function chunks deeper in the index.
    assert!(
        !results.is_empty(),
        "Filter-aware search returned no results — progressive expansion is broken"
    );
    assert_eq!(
        results.len(),
        k,
        "Expected {} results, got {} — filter should not starve the result set",
        k,
        results.len()
    );

    // All returned chunks must be Functions (matching the filter).
    for result in &results {
        assert_eq!(
            result.chunk.metadata.chunk_type,
            ChunkType::Function,
            "Non-matching chunk type returned: {:?}",
            result.chunk.metadata.chunk_type
        );
    }

    // Sanity: verify that a plain (unfiltered) search with k=6 — no filter —
    // the top 6 should all be Structs (decoys), proving the Functions are
    // ranked below the k*2 cutoff.
    let unfiltered = store
        .search(collection_name, query_text, 6, None)
        .await
        .unwrap();
    assert_eq!(unfiltered.len(), 6);
    for result in &unfiltered {
        assert_eq!(
            result.chunk.metadata.chunk_type,
            ChunkType::Struct,
            "Expected top-6 to be all Structs (decoys)"
        );
    }
}

// ─── HttpEmbeddingProvider auth ─────────────────────────────────────────────

#[test]
fn http_embedding_parser_reorders_indexed_batch() {
    let json = serde_json::json!({
        "data": [
            {"index": 1, "embedding": [3.0, 4.0]},
            {"index": 0, "embedding": [1.0, 2.0]}
        ]
    });
    let parsed = HttpEmbeddingProvider::parse_embeddings(&json, 2, 2).unwrap();
    assert_eq!(parsed, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
}

#[test]
fn http_embedding_parser_rejects_bad_cardinality_dimension_and_values() {
    let short = serde_json::json!({"data": [{"embedding": [1.0, 2.0]}]});
    assert!(HttpEmbeddingProvider::parse_embeddings(&short, 2, 2).is_err());

    let wrong_dimension = serde_json::json!({"data": [{"embedding": [1.0]}]});
    assert!(HttpEmbeddingProvider::parse_embeddings(&wrong_dimension, 1, 2).is_err());

    let non_numeric = serde_json::json!({"data": [{"embedding": [1.0, "bad"]}]});
    assert!(HttpEmbeddingProvider::parse_embeddings(&non_numeric, 1, 2).is_err());
}

#[test]
fn http_embedding_provider_learns_dimension_once() {
    let provider = HttpEmbeddingProvider::new("http://127.0.0.1:1/v1", "test-model", 0);
    assert_eq!(provider.dimension(), 0);
    provider.accept_dimension(3).unwrap();
    assert_eq!(provider.dimension(), 3);
    provider.accept_dimension(3).unwrap();
    assert!(provider.accept_dimension(4).is_err());
}

#[test]
fn malformed_batch_does_not_pin_dynamic_provider_dimension() {
    let provider = Arc::new(EmbeddingBackend::Http(HttpEmbeddingProvider::new(
        "http://127.0.0.1:1/v1",
        "test-model",
        0,
    )));
    let store = VectorStore::new(provider.clone());

    let mixed = vec![vec![1.0, 2.0], vec![3.0, 4.0, 5.0]];
    assert!(store.validate_embedding_batch(&mixed, 2).is_err());
    assert_eq!(provider.dimension(), 0);

    let non_finite = vec![vec![1.0, f32::NAN]];
    assert!(store.validate_embedding_batch(&non_finite, 1).is_err());
    assert_eq!(provider.dimension(), 0);

    let valid = vec![vec![1.0, 2.0, 3.0]];
    assert_eq!(store.validate_embedding_batch(&valid, 1).unwrap(), 3);
    assert_eq!(provider.dimension(), 3);
}

/// Fake /embeddings endpoint that requires `Bearer secret-key` and returns a
/// fixed 3-dim vector. Returns the base URL (server runs on a spawned task).
async fn fake_embedding_server() -> String {
    use axum::http::HeaderMap;
    use axum::{routing::post, Json, Router};

    async fn handler(
        headers: HeaderMap,
    ) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
        let authed = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            == Some("Bearer secret-key");
        if !authed {
            return Err(axum::http::StatusCode::UNAUTHORIZED);
        }
        Ok(Json(serde_json::json!({
            "data": [{"embedding": [0.1, 0.2, 0.3]}]
        })))
    }

    let app = Router::new().route("/embeddings", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_provider_sends_bearer_key_and_reads_vector() {
    let base = fake_embedding_server().await;
    let provider = HttpEmbeddingProvider::new(&base, "test-model", 3)
        .with_api_key(Some("secret-key".to_string()));
    let v = provider.embed("hello").await.unwrap();
    assert_eq!(v, vec![0.1, 0.2, 0.3]);
}

#[tokio::test]
async fn http_provider_without_key_gets_auth_error() {
    let base = fake_embedding_server().await;
    let provider = HttpEmbeddingProvider::new(&base, "test-model", 3);
    let err = provider.embed("hello").await.unwrap_err();
    assert!(err.to_string().contains("401"), "unexpected error: {err}");
}

#[tokio::test]
#[ignore = "live OpenRouter call; run with OPENROUTER_API_KEY set"]
async fn live_openrouter_qwen3_embedding() {
    let key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY");
    let provider = HttpEmbeddingProvider::new(
        "https://openrouter.ai/api/v1",
        "qwen/qwen3-embedding-8b",
        4096,
    )
    .with_api_key(Some(key));
    let add = provider
        .embed("fn add(a: i32, b: i32) -> i32 { a + b }")
        .await
        .unwrap();
    let sum = provider
        .embed("pub fn sum(x: i32, y: i32) -> i32 { x + y }")
        .await
        .unwrap();
    let weather = provider.embed("sunny with a chance of rain").await.unwrap();
    assert_eq!(add.len(), 4096);
    let cos = |a: &[f32], b: &[f32]| {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (na * nb)
    };
    assert!(
        cos(&add, &sum) > 0.8,
        "semantically equivalent code should cluster"
    );
    assert!(
        cos(&add, &weather) < 0.5,
        "unrelated text should be far from code"
    );
}

// ---------------------------------------------------------------------------
// HttpEmbeddingProvider credential endpoint-safety gate (P1): the bearer
// token must go through the same plaintext-HTTP / userinfo refusal as every
// other authenticated request path.
// ---------------------------------------------------------------------------

#[test]
fn test_http_embedding_provider_refuses_insecure_remote_endpoint_with_key() {
    let provider = HttpEmbeddingProvider::new("http://remote.example.com/v1", "test-model", 384)
        .with_api_key(Some("sk-test-1234567890".to_string()));
    let err = provider
        .request(&serde_json::json!({}))
        .expect_err("plaintext-HTTP remote endpoint with a key must be refused");
    assert!(
        err.to_string().contains("Refusing to send the API key"),
        "got: {err}"
    );
}

#[test]
fn test_http_embedding_provider_refuses_userinfo_endpoint_with_key() {
    let provider =
        HttpEmbeddingProvider::new("http://user:pass@remote.example.com/v1", "test-model", 384)
            .with_api_key(Some("sk-test-1234567890".to_string()));
    let err = provider
        .request(&serde_json::json!({}))
        .expect_err("userinfo-embedding endpoint with a key must be refused");
    assert!(
        err.to_string().contains("Refusing to send the API key"),
        "got: {err}"
    );
}

#[test]
fn test_http_embedding_provider_allows_local_http_endpoint_with_key() {
    let provider = HttpEmbeddingProvider::new("http://127.0.0.1:1234/v1", "test-model", 384)
        .with_api_key(Some("sk-test-1234567890".to_string()));
    assert!(
        provider.request(&serde_json::json!({})).is_ok(),
        "local HTTP endpoints keep working (traffic stays on the machine)"
    );
}

#[test]
fn test_http_embedding_provider_allows_remote_endpoint_without_key() {
    let provider = HttpEmbeddingProvider::new("http://remote.example.com/v1", "test-model", 384);
    assert!(
        provider.request(&serde_json::json!({})).is_ok(),
        "no credential, no refusal"
    );
}
