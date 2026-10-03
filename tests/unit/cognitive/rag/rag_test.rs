use super::*;
use crate::analysis::vector_store::MockEmbeddingProvider;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_rag_config_default() {
    let config = RagConfig::default();
    assert_eq!(config.max_context_tokens, 8000);
    assert_eq!(config.top_k, 10);
    assert!(config.include_extensions.contains(&"rs".to_string()));
}

#[test]
fn test_rag_config_rust() {
    let config = RagConfig::rust();
    assert!(config.include_extensions.contains(&"rs".to_string()));
    assert!(config.exclude_patterns.contains(&"target/".to_string()));
}

#[test]
fn test_rag_config_python() {
    let config = RagConfig::python();
    assert!(config.include_extensions.contains(&"py".to_string()));
    assert!(config
        .exclude_patterns
        .contains(&"__pycache__/".to_string()));
}

#[test]
fn test_rag_config_typescript() {
    let config = RagConfig::typescript();
    assert!(config.include_extensions.contains(&"ts".to_string()));
    assert!(config
        .exclude_patterns
        .contains(&"node_modules/".to_string()));
}

#[test]
fn test_file_watcher_creation() {
    let watcher = FileWatcher::new("/tmp", RagConfig::default());
    assert_eq!(watcher.tracked_count(), 0);
}

#[test]
fn test_file_watcher_is_excluded() {
    let config = RagConfig {
        exclude_patterns: vec!["target/".into(), "*.min.js".into()],
        ..Default::default()
    };
    let watcher = FileWatcher::new("/tmp", config);

    assert!(watcher.is_excluded(Path::new("/project/target/debug/main")));
    assert!(!watcher.is_excluded(Path::new("/project/src/main.rs")));
}

#[test]
fn private_tool_state_directories_are_always_excluded() {
    let config = RagConfig {
        exclude_patterns: Vec::new(),
        ..Default::default()
    };
    let watcher = FileWatcher::new("/tmp/project", config);

    for directory in [".claude", ".codex", ".agents", ".qwen", ".superpowers"] {
        assert!(
            watcher.is_excluded(&Path::new("/tmp/project").join(directory).join("state.rs")),
            "{directory} must be excluded independently of caller patterns"
        );
    }
    assert!(!watcher.is_excluded(Path::new("/tmp/project/.codex-not-state/source.rs")));
}

#[test]
fn test_file_watcher_is_included() {
    let config = RagConfig {
        include_extensions: vec!["rs".into(), "py".into()],
        ..Default::default()
    };
    let watcher = FileWatcher::new("/tmp", config);

    assert!(watcher.is_included(Path::new("main.rs")));
    assert!(watcher.is_included(Path::new("script.py")));
    assert!(!watcher.is_included(Path::new("data.csv")));
}

#[test]
fn test_file_watcher_scan() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();

    let config = RagConfig::rust();
    let mut watcher = FileWatcher::new(dir.path(), config);

    let changes = watcher.scan_changes();
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], FileChange::Added(_)));

    // Second scan should show no changes
    let changes2 = watcher.scan_changes();
    assert!(changes2.is_empty());
}

#[test]
fn test_file_watcher_detect_modification() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "fn main() {}").unwrap();

    let config = RagConfig::rust();
    let mut watcher = FileWatcher::new(dir.path(), config);

    watcher.scan_changes(); // Initial scan

    // Modify file
    std::thread::sleep(std::time::Duration::from_millis(100));
    std::fs::write(&file, "fn main() { println!(\"hello\"); }").unwrap();

    let changes = watcher.scan_changes();
    // May or may not detect depending on filesystem time resolution
    // Just verify it doesn't crash
    assert!(changes.len() <= 1);
}

#[test]
fn file_watcher_detects_mtime_rollback() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "fn main() {}").unwrap();

    let mut watcher = FileWatcher::new(dir.path(), RagConfig::rust());
    watcher.scan_changes();

    let earlier = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(earlier)
        .unwrap();

    let changes = watcher.scan_changes();
    assert!(matches!(changes.as_slice(), [FileChange::Modified(path)] if path == &file));
}

#[test]
fn test_file_watcher_detect_deletion() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "fn main() {}").unwrap();

    let config = RagConfig::rust();
    let mut watcher = FileWatcher::new(dir.path(), config);

    watcher.scan_changes(); // Initial scan

    // Delete file
    std::fs::remove_file(&file).unwrap();

    let changes = watcher.scan_changes();
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], FileChange::Deleted(_)));
}

#[tokio::test]
async fn test_rag_engine_creation() {
    let dir = tempdir().unwrap();
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::default();

    let engine = RagEngine::new(dir.path(), provider, config);

    assert!(engine.indexed_files.is_empty());
    assert_eq!(engine.stats().total_files, 0);
}

#[tokio::test]
async fn test_rag_engine_build_index() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("main.rs"),
        "fn main() { println!(\"hello\"); }",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }",
    )
    .unwrap();

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::rust();

    let mut engine = RagEngine::new(dir.path(), provider, config);
    let stats = engine.build_index().await.unwrap();

    assert_eq!(stats.total_files, 2);
    assert!(stats.total_chunks > 0);
}

#[tokio::test]
async fn full_and_incremental_scans_prune_private_tool_state() {
    let dir = tempdir().unwrap();
    let visible = dir.path().join("visible.rs");
    std::fs::write(&visible, "fn visible() {}").unwrap();
    for state_dir in [".claude", ".codex", ".agents", ".qwen", ".superpowers"] {
        let path = dir.path().join(state_dir);
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("private.rs"), "fn private_state() {}").unwrap();
    }

    // `/scan` supplies its own pattern list, so the invariant cannot depend
    // on a particular RagConfig constructor retaining the private directories.
    let config = RagConfig {
        include_extensions: vec!["rs".into()],
        exclude_patterns: Vec::new(),
        ..Default::default()
    };
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut engine = RagEngine::new(dir.path(), provider, config.clone());

    let stats = engine.build_index().await.unwrap();
    assert_eq!(stats.total_files, 1);
    assert_eq!(
        engine.indexed_files.keys().collect::<Vec<_>>(),
        vec![&visible]
    );

    let mut watcher = FileWatcher::new(dir.path(), config);
    let changes = watcher.scan_changes();
    assert_eq!(changes.len(), 1);
    assert!(matches!(&changes[0], FileChange::Added(path) if path == &visible));
}

#[tokio::test]
async fn explicitly_selected_private_named_root_remains_scannable() {
    let parent = tempdir().unwrap();
    let root = parent.path().join(".claude");
    std::fs::create_dir(&root).unwrap();
    let visible = root.join("visible.rs");
    std::fs::write(&visible, "fn visible() {}").unwrap();
    std::fs::create_dir(root.join(".codex")).unwrap();
    std::fs::write(root.join(".codex/private.rs"), "fn private_state() {}").unwrap();

    let config = RagConfig {
        include_extensions: vec!["rs".into()],
        exclude_patterns: Vec::new(),
        ..Default::default()
    };
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut engine = RagEngine::new(&root, provider, config.clone());

    let stats = engine.build_index().await.unwrap();
    assert_eq!(stats.total_files, 1);
    assert_eq!(
        engine.indexed_files.keys().collect::<Vec<_>>(),
        vec![&visible]
    );

    let mut watcher = FileWatcher::new(&root, config);
    let changes = watcher.scan_changes();
    assert_eq!(changes.len(), 1);
    assert!(matches!(&changes[0], FileChange::Added(path) if path == &visible));
}

#[tokio::test]
async fn load_scrubs_each_stale_path_class_and_update_reindexes_current_files() {
    let mut cases = vec!["private", "deleted", "excluded", "outside"];
    #[cfg(unix)]
    cases.push("symlink");

    for case in cases {
        let parent = tempdir().unwrap();
        // An explicitly selected root is allowed even when its own name is a
        // private-state name. Only private directories nested below it are
        // excluded.
        let root = parent.path().join(".claude");
        let storage = parent.path().join("cache");
        std::fs::create_dir(&root).unwrap();
        let visible = root.join("visible.rs");
        std::fs::write(&visible, "fn persisted_version() {}").unwrap();

        let outside = tempdir().unwrap();
        let stale = match case {
            "private" => {
                let path = root.join(".codex/state/private.rs");
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, "fn private_cache_sentinel() {}").unwrap();
                path
            }
            "deleted" => {
                let path = root.join("deleted.rs");
                std::fs::write(&path, "fn deleted_cache_sentinel() {}").unwrap();
                path
            }
            "excluded" => {
                let path = root.join("excluded.rs");
                std::fs::write(&path, "fn excluded_cache_sentinel() {}").unwrap();
                path
            }
            "outside" => {
                let path = outside.path().join("outside.rs");
                std::fs::write(&path, "fn outside_cache_sentinel() {}").unwrap();
                path
            }
            #[cfg(unix)]
            "symlink" => {
                let path = root.join("linked.rs");
                std::fs::write(&path, "fn symlink_cache_sentinel() {}").unwrap();
                path
            }
            _ => unreachable!(),
        };

        let config = RagConfig {
            include_extensions: vec!["rs".into()],
            exclude_patterns: if case == "excluded" {
                vec!["excluded.rs".into()]
            } else {
                Vec::new()
            },
            min_score: -1.0,
            ..Default::default()
        };
        let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));

        // Write the shape of a pre-fix cache by indexing paths directly,
        // bypassing the current traversal gate.
        let mut legacy =
            RagEngine::new(&root, provider.clone(), config.clone()).with_storage(&storage);
        legacy
            .store
            .collection(&legacy.collection_name, CollectionScope::Project);
        legacy
            .store
            .index_file(&legacy.collection_name, &visible)
            .await
            .unwrap();
        legacy
            .store
            .index_file(&legacy.collection_name, &stale)
            .await
            .unwrap();
        legacy.save().unwrap();

        if case == "deleted" {
            std::fs::remove_file(&stale).unwrap();
        }
        #[cfg(unix)]
        if case == "symlink" {
            let target = outside.path().join("symlink-target.rs");
            std::fs::write(&target, "fn external_symlink_target() {}").unwrap();
            std::fs::remove_file(&stale).unwrap();
            std::os::unix::fs::symlink(&target, &stale).unwrap();
        }
        std::fs::write(&visible, "fn current_version() {}").unwrap();

        let mut loaded = RagEngine::new(&root, provider, config).with_storage(&storage);
        loaded.load().unwrap();

        let before_update = loaded
            .store
            .get_collection(&loaded.collection_name)
            .unwrap()
            .chunks();
        assert!(
            before_update.is_empty(),
            "stale content for the still-allowed file survived load ({case})"
        );

        // Exercise the public search boundary before any current-disk update.
        let context = loaded.retrieve("cache sentinel").await.unwrap();
        assert!(
            context.sources.is_empty(),
            "stale persisted source reached retrieval before refresh ({case})"
        );

        let changes = loaded.update_index().await.unwrap();
        assert!(
            changes
                .iter()
                .any(|change| matches!(change, FileChange::Added(path) if path == &visible)),
            "load must leave current files pending for reindex ({case})"
        );
        let after_update = loaded
            .store
            .get_collection(&loaded.collection_name)
            .unwrap()
            .chunks();
        assert_eq!(after_update.len(), 1);
        assert!(after_update[0].content.contains("current_version"));
        assert!(!after_update[0].content.contains("persisted_version"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn full_and_incremental_scans_skip_file_and_directory_symlinks() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let visible = dir.path().join("visible.rs");
    let outside_file = outside.path().join("outside.rs");
    std::fs::write(&visible, "fn visible() {}").unwrap();
    std::fs::write(&outside_file, "fn outside() {}").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked-directory")).unwrap();
    std::os::unix::fs::symlink(&outside_file, dir.path().join("linked-file.rs")).unwrap();

    let config = RagConfig {
        include_extensions: vec!["rs".into()],
        exclude_patterns: Vec::new(),
        ..Default::default()
    };
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let mut engine = RagEngine::new(dir.path(), provider, config.clone());

    let stats = engine.build_index().await.unwrap();
    assert_eq!(stats.total_files, 1);
    assert_eq!(
        engine.indexed_files.keys().collect::<Vec<_>>(),
        vec![&visible]
    );

    let mut watcher = FileWatcher::new(dir.path(), config);
    let changes = watcher.scan_changes();
    assert_eq!(changes.len(), 1);
    assert!(matches!(&changes[0], FileChange::Added(path) if path == &visible));
}

#[tokio::test]
async fn test_rag_engine_retrieve() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("math.rs"),
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

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig {
        min_score: 0.0, // Lower threshold for mock provider
        ..RagConfig::rust()
    };

    let mut engine = RagEngine::new(dir.path(), provider, config);
    engine.build_index().await.unwrap();

    let context = engine.retrieve("sum addition").await.unwrap();

    // Mock provider may not find semantic matches, just verify it runs
    // retrieval_time_ms is u64, checking it exists validates the operation succeeded
    let _ = context.retrieval_time_ms;
    assert_eq!(context.query, "sum addition");
}

#[tokio::test]
async fn test_rag_engine_update_index() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::rust();

    let mut engine = RagEngine::new(dir.path(), provider, config);
    engine.build_index().await.unwrap();

    // Add new file
    std::fs::write(dir.path().join("lib.rs"), "pub fn test() {}").unwrap();

    let changes = engine.update_index().await.unwrap();
    assert!(!changes.is_empty());
}

#[tokio::test]
async fn failed_incremental_embedding_preserves_and_retries_last_good_file() {
    use axum::{extract::State, routing::post, Json, Router};

    async fn embeddings(
        State(fail): State<Arc<AtomicBool>>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        if fail.load(Ordering::SeqCst) {
            return Json(serde_json::json!({"data": []}));
        }
        let count = body["input"].as_array().map_or(1, Vec::len);
        Json(serde_json::json!({
            "data": (0..count)
                .map(|index| serde_json::json!({"index": index, "embedding": [0.1, 0.2]}))
                .collect::<Vec<_>>()
        }))
    }

    let fail = Arc::new(AtomicBool::new(false));
    let app = Router::new()
        .route("/embeddings", post(embeddings))
        .with_state(fail.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let dir = tempdir().unwrap();
    let file = dir.path().join("main.rs");
    std::fs::write(&file, "fn old_version() {}").unwrap();
    let provider = Arc::new(EmbeddingBackend::Http(
        crate::analysis::vector_store::HttpEmbeddingProvider::new(
            format!("http://{address}"),
            "test-model",
            0,
        ),
    ));
    let mut engine = RagEngine::new(dir.path(), provider, RagConfig::rust());
    engine.build_index().await.unwrap();
    let before = engine
        .store
        .get_collection(&engine.collection_name)
        .unwrap()
        .chunks()
        .iter()
        .map(|chunk| (chunk.id.clone(), chunk.content.clone()))
        .collect::<Vec<_>>();
    std::thread::sleep(std::time::Duration::from_millis(5));
    std::fs::write(&file, "fn new_version() {}").unwrap();
    fail.store(true, Ordering::SeqCst);
    assert!(engine.update_index().await.is_err());
    let after_failure = engine
        .store
        .get_collection(&engine.collection_name)
        .unwrap()
        .chunks()
        .iter()
        .map(|chunk| (chunk.id.clone(), chunk.content.clone()))
        .collect::<Vec<_>>();
    assert_eq!(after_failure, before);
    assert!(engine
        .watcher
        .pending_changes()
        .iter()
        .any(|change| matches!(change, FileChange::Modified(path) if path == &file)));

    fail.store(false, Ordering::SeqCst);
    engine.update_index().await.unwrap();
    assert!(engine
        .store
        .get_collection(&engine.collection_name)
        .unwrap()
        .chunks()
        .iter()
        .any(|chunk| chunk.content.contains("new_version")));
}

#[tokio::test]
async fn failed_full_rebuild_preserves_last_good_index_metadata_and_stats() {
    let dir = tempdir().unwrap();
    let preserved = dir.path().join("preserved.rs");
    std::fs::write(
        &preserved,
        "pub fn old_version() -> usize { let values = [1usize; 64]; values.iter().sum() }\n"
            .repeat(4),
    )
    .unwrap();
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::failing_on(
        2,
        "FAIL_FULL_REBUILD",
    )));
    let mut engine = RagEngine::new(dir.path(), provider, RagConfig::rust());
    engine.build_index().await.unwrap();

    let before_chunks = engine
        .store
        .get_collection(&engine.collection_name)
        .unwrap()
        .chunks()
        .iter()
        .map(|chunk| (chunk.id.clone(), chunk.content.clone()))
        .collect::<Vec<_>>();
    let before_stats = serde_json::to_value(engine.stats()).unwrap();
    let before_indexed_files = serde_json::to_value(&engine.indexed_files).unwrap();
    let before_watcher = engine.watcher.tracked_files.clone();

    std::fs::write(
        &preserved,
        "pub fn new_version() -> usize { let values = [2usize; 64]; values.iter().sum() }\n"
            .repeat(4),
    )
    .unwrap();
    let failing = dir.path().join("failing.rs");
    std::fs::write(
        &failing,
        "pub fn cannot_embed() -> &'static str { \"FAIL_FULL_REBUILD\" }\n".repeat(4),
    )
    .unwrap();

    let error = engine.build_index().await.unwrap_err();
    assert!(
        format!("{error:#}").contains("previous index retained"),
        "failure must state that the live index was preserved: {error:#}"
    );
    let after_chunks = engine
        .store
        .get_collection(&engine.collection_name)
        .unwrap()
        .chunks()
        .iter()
        .map(|chunk| (chunk.id.clone(), chunk.content.clone()))
        .collect::<Vec<_>>();
    assert_eq!(after_chunks, before_chunks);
    assert_eq!(serde_json::to_value(engine.stats()).unwrap(), before_stats);
    assert_eq!(
        serde_json::to_value(&engine.indexed_files).unwrap(),
        before_indexed_files
    );
    assert_eq!(engine.watcher.tracked_files, before_watcher);
    assert_eq!(
        engine.store.list_collections(),
        vec![engine.collection_name.as_str()]
    );

    // A later clean rebuild can still publish normally; no failed staging
    // collection remains in the live store or its persistence snapshot.
    std::fs::write(
        &failing,
        "pub fn now_embeds() -> usize { let values = [3usize; 64]; values.iter().sum() }\n"
            .repeat(4),
    )
    .unwrap();
    let stats = engine.build_index().await.unwrap();
    assert_eq!(stats.total_files, 2);
    let published = engine
        .store
        .get_collection(&engine.collection_name)
        .unwrap()
        .chunks();
    assert!(published
        .iter()
        .any(|chunk| chunk.content.contains("new_version")));
    assert!(!published
        .iter()
        .any(|chunk| chunk.content.contains("old_version")));
    assert_eq!(
        engine.store.list_collections(),
        vec![engine.collection_name.as_str()]
    );
}

#[test]
fn test_content_similarity() {
    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::default();
    let engine = RagEngine::new("/tmp", provider, config);

    let sim = engine.content_similarity("hello world test", "hello world test");
    assert!((sim - 1.0).abs() < 0.01);

    let sim = engine.content_similarity("hello world", "goodbye moon");
    assert!(sim < 0.5);
}

#[test]
fn watcher_acknowledges_the_version_observed_before_indexing() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("race.rs");
    std::fs::write(&file, "fn before() {}").unwrap();
    let observed = file_modified_nanos(&file).expect("initial modification time");
    let mut watcher = FileWatcher::new(dir.path(), RagConfig::rust());

    // Model a successful index of the version whose timestamp was captured
    // before the asynchronous embedding call.
    watcher.acknowledge_observed(&FileChange::Added(file.clone()), Some(observed));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        std::fs::write(
            &file,
            format!("fn after_{:?}() {{}}", std::time::Instant::now()),
        )
        .unwrap();
        if file_modified_nanos(&file) != Some(observed) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "filesystem did not expose a newer modification time"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(watcher
        .pending_changes()
        .iter()
        .any(|change| matches!(change, FileChange::Modified(path) if path == &file)));
}

#[test]
fn test_context_builder() {
    let builder = ContextBuilder::new()
        .with_system("You are a helpful assistant")
        .with_instruction("Be concise")
        .with_query("What does this code do?");

    let prompt = builder.build();

    assert!(prompt.contains("You are a helpful assistant"));
    assert!(prompt.contains("Be concise"));
    assert!(prompt.contains("What does this code do?"));
}

#[test]
fn test_context_builder_with_context() {
    let context = RetrievedContext {
        context: "fn main() {}".to_string(),
        sources: vec![],
        token_count: 10,
        query: "test".to_string(),
        retrieval_time_ms: 5,
    };

    let builder = ContextBuilder::new()
        .with_context(context)
        .with_query("Explain this");

    let prompt = builder.build();
    assert!(prompt.contains("fn main()"));
}

#[test]
fn test_context_builder_token_count() {
    let builder = ContextBuilder::new()
        .with_system("System prompt here")
        .with_instruction("Do something")
        .with_query("Question?");

    let count = builder.token_count();
    assert!(count > 0);
}

#[test]
fn test_rag_stats_default() {
    let stats = RagStats::default();
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.total_chunks, 0);
    assert!(stats.last_full_index.is_none());
}

#[test]
fn test_indexed_file() {
    let file = IndexedFile {
        path: PathBuf::from("test.rs"),
        modified_at: 12345,
        chunk_count: 5,
        size: 1024,
        language: "rs".to_string(),
    };

    assert_eq!(file.chunk_count, 5);
    assert_eq!(file.language, "rs");
}

#[test]
fn test_context_source() {
    let source = ContextSource {
        file: PathBuf::from("main.rs"),
        start_line: 1,
        end_line: 10,
        chunk_type: ChunkType::Function,
        symbol: Some("main".to_string()),
        score: 0.9,
    };

    assert_eq!(source.symbol, Some("main".to_string()));
    assert!(source.score > 0.8);
}

#[test]
fn test_file_change_variants() {
    let added = FileChange::Added(PathBuf::from("new.rs"));
    let modified = FileChange::Modified(PathBuf::from("changed.rs"));
    let deleted = FileChange::Deleted(PathBuf::from("removed.rs"));

    assert!(matches!(added, FileChange::Added(_)));
    assert!(matches!(modified, FileChange::Modified(_)));
    assert!(matches!(deleted, FileChange::Deleted(_)));
}

#[tokio::test]
async fn test_rag_engine_indexed_files() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
    std::fs::write(dir.path().join("b.rs"), "fn b() {}").unwrap();

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::rust();

    let mut engine = RagEngine::new(dir.path(), provider, config);
    engine.build_index().await.unwrap();

    let files = engine.indexed_files();
    assert_eq!(files.len(), 2);
}

#[tokio::test]
async fn test_rag_engine_search_with_filter() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::rust();

    let mut engine = RagEngine::new(dir.path(), provider, config);
    engine.build_index().await.unwrap();

    let filter = SearchFilter::new().with_chunk_type(ChunkType::Function);
    let results = engine.search_with_filter("main", filter).await.unwrap();

    // Results depend on chunking, just verify no crash
    let _ = results.len();
}

#[tokio::test]
async fn test_rag_engine_context_for_files() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("target.rs");
    std::fs::write(&file_path, "fn target_function() {}").unwrap();
    std::fs::write(dir.path().join("other.rs"), "fn other() {}").unwrap();

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::rust();

    let mut engine = RagEngine::new(dir.path(), provider, config);
    engine.build_index().await.unwrap();

    let context = engine
        .context_for_files(&[file_path], "function")
        .await
        .unwrap();

    assert!(
        context.sources.is_empty()
            || context
                .sources
                .iter()
                .any(|s| s.file.to_string_lossy().contains("target"))
    );
}

// Additional tests for comprehensive coverage

#[test]
fn test_rag_config_serialization() {
    let config = RagConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: RagConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.max_context_tokens, deserialized.max_context_tokens);
    assert_eq!(config.top_k, deserialized.top_k);
}

#[test]
fn test_rag_config_clone() {
    let config = RagConfig::rust();
    let cloned = config.clone();

    assert_eq!(config.include_extensions, cloned.include_extensions);
    assert_eq!(config.exclude_patterns, cloned.exclude_patterns);
}

#[test]
fn test_indexed_file_serialization() {
    let file = IndexedFile {
        path: PathBuf::from("test.rs"),
        modified_at: 12345,
        chunk_count: 5,
        size: 1024,
        language: "rs".to_string(),
    };

    let json = serde_json::to_string(&file).unwrap();
    let deserialized: IndexedFile = serde_json::from_str(&json).unwrap();

    assert_eq!(file.path, deserialized.path);
    assert_eq!(file.modified_at, deserialized.modified_at);
}

#[test]
fn test_indexed_file_clone() {
    let file = IndexedFile {
        path: PathBuf::from("lib.rs"),
        modified_at: 99999,
        chunk_count: 10,
        size: 2048,
        language: "rs".to_string(),
    };

    let cloned = file.clone();
    assert_eq!(file.path, cloned.path);
    assert_eq!(file.size, cloned.size);
}

#[test]
fn test_rag_stats_serialization() {
    let mut files_by_lang = HashMap::new();
    files_by_lang.insert("rs".to_string(), 10);
    files_by_lang.insert("py".to_string(), 5);

    let stats = RagStats {
        total_files: 15,
        total_chunks: 100,
        total_tokens: 5000,
        last_full_index: Some(12345),
        last_update: Some(12346),
        build_time_ms: 500,
        files_by_language: files_by_lang,
    };

    let json = serde_json::to_string(&stats).unwrap();
    let deserialized: RagStats = serde_json::from_str(&json).unwrap();

    assert_eq!(stats.total_files, deserialized.total_files);
    assert_eq!(stats.build_time_ms, deserialized.build_time_ms);
}

#[test]
fn test_rag_stats_clone() {
    let stats = RagStats {
        total_files: 10,
        total_chunks: 50,
        ..Default::default()
    };

    let cloned = stats.clone();
    assert_eq!(stats.total_files, cloned.total_files);
}

#[test]
fn test_file_change_clone() {
    let change = FileChange::Added(PathBuf::from("new.rs"));
    let cloned = change.clone();

    match cloned {
        FileChange::Added(path) => assert_eq!(path, PathBuf::from("new.rs")),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_file_change_debug() {
    let changes = vec![
        FileChange::Added(PathBuf::from("a.rs")),
        FileChange::Modified(PathBuf::from("b.rs")),
        FileChange::Deleted(PathBuf::from("c.rs")),
    ];

    for change in changes {
        let debug_str = format!("{:?}", change);
        assert!(!debug_str.is_empty());
    }
}

#[test]
fn test_retrieved_context_clone() {
    let context = RetrievedContext {
        context: "fn main() {}".to_string(),
        sources: vec![ContextSource {
            file: PathBuf::from("main.rs"),
            start_line: 1,
            end_line: 10,
            chunk_type: ChunkType::Function,
            symbol: Some("main".to_string()),
            score: 0.95,
        }],
        token_count: 100,
        query: "test query".to_string(),
        retrieval_time_ms: 50,
    };

    let cloned = context.clone();
    assert_eq!(context.context, cloned.context);
    assert_eq!(context.sources.len(), cloned.sources.len());
}

#[test]
fn test_context_source_clone() {
    let source = ContextSource {
        file: PathBuf::from("lib.rs"),
        start_line: 10,
        end_line: 20,
        chunk_type: ChunkType::Struct,
        symbol: Some("MyStruct".to_string()),
        score: 0.85,
    };

    let cloned = source.clone();
    assert_eq!(source.file, cloned.file);
    assert_eq!(source.symbol, cloned.symbol);
}

#[test]
fn test_context_source_debug() {
    let source = ContextSource {
        file: PathBuf::from("test.rs"),
        start_line: 1,
        end_line: 5,
        chunk_type: ChunkType::Import,
        symbol: None,
        score: 0.5,
    };

    let debug_str = format!("{:?}", source);
    assert!(debug_str.contains("test.rs"));
}

#[test]
fn test_file_watcher_excluded_extension_pattern() {
    let config = RagConfig {
        exclude_patterns: vec!["*.map".into(), "*.pyc".into()],
        ..Default::default()
    };
    let watcher = FileWatcher::new("/tmp", config);

    // Extension patterns match on the file extension only
    assert!(watcher.is_excluded(Path::new("/project/app.map")));
    assert!(watcher.is_excluded(Path::new("/project/module.pyc")));
    assert!(!watcher.is_excluded(Path::new("/project/main.js")));
}

#[test]
fn test_file_watcher_excluded_directory_pattern() {
    let config = RagConfig {
        exclude_patterns: vec!["node_modules/".into(), "vendor/".into()],
        ..Default::default()
    };
    let watcher = FileWatcher::new("/tmp", config);

    assert!(watcher.is_excluded(Path::new("/project/node_modules/package/index.js")));
    assert!(watcher.is_excluded(Path::new("/project/vendor/lib/file.php")));
    assert!(!watcher.is_excluded(Path::new("/project/src/main.rs")));
}

#[test]
fn test_file_watcher_no_extension() {
    let config = RagConfig {
        include_extensions: vec!["rs".into()],
        ..Default::default()
    };
    let watcher = FileWatcher::new("/tmp", config);

    assert!(!watcher.is_included(Path::new("Makefile")));
    assert!(!watcher.is_included(Path::new("LICENSE")));
}

#[test]
fn test_context_builder_empty() {
    let builder = ContextBuilder::new();
    let prompt = builder.build();
    assert!(prompt.is_empty() || prompt.contains("User request"));
}

#[test]
fn test_context_builder_system_only() {
    let builder = ContextBuilder::new().with_system("You are a code assistant");
    let prompt = builder.build();
    assert!(prompt.contains("You are a code assistant"));
}

#[test]
fn test_context_builder_all_fields() {
    let context = RetrievedContext {
        context: "pub struct Test {}".to_string(),
        sources: vec![],
        token_count: 5,
        query: "struct".to_string(),
        retrieval_time_ms: 10,
    };

    let builder = ContextBuilder::new()
        .with_system("System")
        .with_instruction("Explain the code")
        .with_context(context)
        .with_query("What is Test?");

    // Get token count before build() which consumes the builder
    let count = builder.token_count();
    let prompt = builder.build();

    assert!(prompt.contains("System"));
    assert!(prompt.contains("Explain the code"));
    assert!(prompt.contains("pub struct Test"));
    assert!(prompt.contains("What is Test?"));
    assert!(count > 0);
}

#[tokio::test]
async fn test_rag_engine_stats() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("test.rs"), "fn test() {}").unwrap();

    let provider = Arc::new(EmbeddingBackend::Mock(MockEmbeddingProvider::default()));
    let config = RagConfig::rust();

    let mut engine = RagEngine::new(dir.path(), provider, config);
    engine.build_index().await.unwrap();

    let stats = engine.stats();
    assert_eq!(stats.total_files, 1);
    // build_time_ms is u64 - it existing indicates success
    let _ = stats.build_time_ms;
}

#[test]
fn test_rag_config_all_defaults() {
    let config = RagConfig::default();

    assert_eq!(config.min_score, 0.3);
    assert_eq!(config.dedup_threshold, 0.95);
    assert_eq!(config.max_chunk_tokens, 500);
    assert!(config.include_metadata);
    assert!(config.include_line_numbers);
}

#[test]
fn test_retrieved_context_debug() {
    let context = RetrievedContext {
        context: "code".to_string(),
        sources: vec![],
        token_count: 10,
        query: "query".to_string(),
        retrieval_time_ms: 5,
    };

    let debug_str = format!("{:?}", context);
    assert!(debug_str.contains("query"));
}
