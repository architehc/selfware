use super::*;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[tokio::test]
async fn test_tool_cache_basic() {
    let cache = ToolCache::new();
    let args = serde_json::json!({"path": "test.txt"});

    assert!(cache.get("file_read", &args).await.is_none());

    cache
        .set("file_read", &args, serde_json::json!("content"))
        .await;
    assert_eq!(
        cache.get("file_read", &args).await,
        Some(serde_json::json!("content"))
    );
}

#[test]
fn test_cache_key_generation() {
    let key1 = ToolCache::cache_key("file_read", &serde_json::json!({"path": "test.txt"}));
    let key2 = ToolCache::cache_key("file_read", &serde_json::json!({"path": "test.txt"}));
    let key3 = ToolCache::cache_key("file_read", &serde_json::json!({"path": "other.txt"}));

    assert_eq!(key1, key2);
    assert_ne!(key1, key3);
}

#[test]
fn test_is_cacheable() {
    assert!(is_cacheable("file_read"));
    assert!(is_cacheable("git_status"));
    assert!(!is_cacheable("file_write"));
    assert!(!is_cacheable("shell_exec"));
}

#[test]
fn test_invalidates_cache() {
    assert!(invalidates_cache("file_write"));
    assert!(invalidates_cache("git_commit"));
    // These mutating tools previously invalidated NOTHING, so the agent
    // was served pre-edit git_status/git_diff/grep results after edits.
    assert!(invalidates_cache("file_multi_edit"));
    assert!(invalidates_cache("patch_apply"));
    assert!(invalidates_cache("pty_shell"));
    assert!(!invalidates_cache("file_read"));
}

#[test]
fn test_llm_cache_config_default() {
    let config = LlmCacheConfig::default();
    assert!(config.enabled);
    assert!(config.semantic_matching);
    assert_eq!(config.similarity_threshold, 0.85);
}

#[test]
fn test_llm_cache_entry_cost() {
    let entry = LlmCacheEntry {
        id: "test".into(),
        prompt: "test".into(),
        embedding: vec![0.1, 0.2, 0.3],
        response: "response".into(),
        reasoning: None,
        model: "test".into(),
        input_tokens: 1000,
        output_tokens: 500,
        created_at: 0,
        hit_count: 0,
        context_hash: 0,
        file_paths: vec![],
    };

    let config = LlmCacheConfig::default();
    let cost = entry.estimated_cost(&config);
    assert!(cost > 0.0);
}

#[tokio::test]
async fn test_llm_cache_lookup_and_store() {
    let cache = LlmCache::default();

    // Should return None for empty cache
    let result = cache.lookup("test", &[0.1, 0.2, 0.3], 0, "test").await;
    assert!(result.is_none());

    // Store an entry
    let entry = LlmCacheEntry {
        id: "test-id".into(),
        prompt: "test prompt".into(),
        embedding: vec![1.0, 0.0, 0.0],
        response: "test response".into(),
        reasoning: None,
        model: "test".into(),
        input_tokens: 10,
        output_tokens: 5,
        created_at: now_secs(),
        hit_count: 0,
        context_hash: 0,
        file_paths: vec![],
    };
    cache.store(entry).await;

    // Should find with exact match
    let result = cache
        .lookup("test prompt", &[1.0, 0.0, 0.0], 0, "test")
        .await;
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.response, "test response");
    assert_eq!(result.hit_count, 1);
}

#[tokio::test]
async fn test_cache_manager_new() {
    let manager = CacheManager::default();
    assert_eq!(manager.tool_cache.stats().await.entries, 0);
}

#[test]
fn test_cosine_similarity() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    let sim = LlmCache::cosine_similarity(&a, &b);
    assert!((sim - 1.0).abs() < 0.001);

    let c = vec![0.0, 1.0, 0.0];
    let sim_ortho = LlmCache::cosine_similarity(&a, &c);
    assert!(sim_ortho < 0.001);
}

#[test]
fn test_l2_normalize() {
    let v = vec![3.0, 4.0, 0.0];
    let normalized = LlmCache::l2_normalize(&v);
    let norm: f32 = normalized.iter().map(|x| x * x).sum();
    assert!((norm - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn test_llm_cache_different_model_no_hit() {
    let cache = LlmCache::default();

    // Store an entry with model "alpha"
    let entry = LlmCacheEntry {
        id: "model-alpha".into(),
        prompt: "same prompt".into(),
        embedding: vec![1.0, 0.0, 0.0],
        response: "alpha response".into(),
        reasoning: None,
        model: "alpha".into(),
        input_tokens: 10,
        output_tokens: 5,
        created_at: now_secs(),
        hit_count: 0,
        context_hash: 42,
        file_paths: vec![],
    };
    cache.store(entry).await;

    // Same embedding + context_hash but DIFFERENT model -> no hit
    let result = cache
        .lookup("same prompt", &[1.0, 0.0, 0.0], 42, "beta")
        .await;
    assert!(
        result.is_none(),
        "cache should NOT hit for a different model"
    );

    // Same model -> hit
    let result = cache
        .lookup("same prompt", &[1.0, 0.0, 0.0], 42, "alpha")
        .await;
    assert!(result.is_some());
}

#[tokio::test]
async fn test_llm_cache_different_context_hash_no_hit() {
    let cache = LlmCache::default();

    let entry = LlmCacheEntry {
        id: "hash-100".into(),
        prompt: "prompt".into(),
        embedding: vec![1.0, 0.0, 0.0],
        response: "response".into(),
        reasoning: None,
        model: "m".into(),
        input_tokens: 10,
        output_tokens: 5,
        created_at: now_secs(),
        hit_count: 0,
        context_hash: 100,
        file_paths: vec![],
    };
    cache.store(entry).await;

    // Same model, same embedding, DIFFERENT context_hash -> no hit
    let result = cache.lookup("prompt", &[1.0, 0.0, 0.0], 200, "m").await;
    assert!(
        result.is_none(),
        "cache should NOT hit for a different context_hash"
    );
}

#[tokio::test]
async fn llm_cache_expires_entries_and_removes_their_embeddings() {
    let cache = LlmCache::new(LlmCacheConfig {
        ttl_secs: 1,
        ..Default::default()
    });
    cache
        .store(LlmCacheEntry {
            id: "expired".into(),
            prompt: "prompt".into(),
            embedding: vec![1.0, 0.0],
            response: "stale".into(),
            reasoning: None,
            model: "m".into(),
            input_tokens: 1,
            output_tokens: 1,
            created_at: now_secs().saturating_sub(2),
            hit_count: 0,
            context_hash: 7,
            file_paths: vec![],
        })
        .await;

    assert!(cache.lookup("prompt", &[1.0, 0.0], 7, "m").await.is_none());
    assert_eq!(cache.stats().await.entries, 0);
}

#[tokio::test]
async fn llm_cache_honors_small_and_zero_capacities() {
    let cache = LlmCache::new(LlmCacheConfig {
        max_entries: 2,
        ..Default::default()
    });
    for n in 0..3_u64 {
        cache
            .store(LlmCacheEntry {
                id: format!("id-{n}"),
                prompt: format!("prompt-{n}"),
                embedding: vec![1.0, n as f32],
                response: String::new(),
                reasoning: None,
                model: "m".into(),
                input_tokens: 0,
                output_tokens: 0,
                created_at: now_secs() + n,
                hit_count: 0,
                context_hash: n,
                file_paths: vec![],
            })
            .await;
    }
    assert_eq!(cache.stats().await.entries, 2);

    let disabled = LlmCache::new(LlmCacheConfig {
        max_entries: 0,
        ..Default::default()
    });
    disabled
        .store(LlmCacheEntry {
            id: "ignored".into(),
            prompt: String::new(),
            embedding: vec![],
            response: String::new(),
            reasoning: None,
            model: "m".into(),
            input_tokens: 0,
            output_tokens: 0,
            created_at: now_secs(),
            hit_count: 0,
            context_hash: 0,
            file_paths: vec![],
        })
        .await;
    assert_eq!(disabled.stats().await.entries, 0);
}

#[test]
fn test_text_tool_call_not_cached_via_parse() {
    // Verify that parse_tool_calls detects XML tool calls so that
    // cache_response will skip caching such responses.
    let content = "<tool>\n<name>shell_exec</name>\n<arguments>{\"command\":\"echo hello\"}</arguments>\n</tool>";
    let parsed = crate::tool_parser::parse_tool_calls(content);
    assert!(
        !parsed.tool_calls.is_empty(),
        "text/XML tool call must be detected so it is not cached"
    );

    // A plain text response should NOT be detected as a tool call
    let plain = "This is a normal response with no tool calls.";
    let parsed_plain = crate::tool_parser::parse_tool_calls(plain);
    assert!(
        parsed_plain.tool_calls.is_empty(),
        "plain text should not be detected as a tool call"
    );
}

// ---- Recursive search results do not survive an edit beneath their root ----

#[tokio::test]
async fn an_edit_invalidates_a_recursive_search_rooted_above_it() {
    let manager = CacheManager::default();
    let cache = &manager.tool_cache;
    let grep = serde_json::json!({"pattern": "fn answer", "path": "."});
    let glob = serde_json::json!({"pattern": "**/*.rs", "path": "src"});
    let symbols = serde_json::json!({"query": "answer"});
    let tree = serde_json::json!({"path": "."});
    let other_file = serde_json::json!({"path": "README.md"});
    cache
        .set("grep_search", &grep, serde_json::json!(["old hit"]))
        .await;
    cache
        .set("glob_find", &glob, serde_json::json!(["src/lib.rs"]))
        .await;
    cache
        .set("symbol_search", &symbols, serde_json::json!(["old"]))
        .await;
    cache
        .set("directory_tree", &tree, serde_json::json!(["src/"]))
        .await;
    cache
        .set("file_read", &other_file, serde_json::json!("readme"))
        .await;
    assert!(cache.get("grep_search", &grep).await.is_some());

    // The dispatcher's path for `file_edit {path: "src/x.rs"}`.
    manager.invalidate_path_and_git("src/x.rs").await;

    assert!(
        cache.get("grep_search", &grep).await.is_none(),
        "a grep rooted at `.` covers src/x.rs and must re-run after the edit"
    );
    assert!(cache.get("glob_find", &glob).await.is_none());
    assert!(cache.get("symbol_search", &symbols).await.is_none());
    assert!(
        cache.get("directory_tree", &tree).await.is_none(),
        "a new file changes the listing"
    );
    // An unrelated single-file read is not a tree-scoped result; its own
    // mtime check covers it.
    assert!(cache.get("file_read", &other_file).await.is_some());
}

#[tokio::test]
async fn invalidate_path_also_drops_tree_scoped_entries() {
    let cache = ToolCache::new();
    let grep = serde_json::json!({"pattern": "x", "path": "/abs/project"});
    cache
        .set("grep_search", &grep, serde_json::json!(["hit"]))
        .await;
    cache.invalidate_path("/abs/project/src/deep/file.rs").await;
    assert!(cache.get("grep_search", &grep).await.is_none());
}

#[tokio::test]
async fn tool_cache_is_namespaced_by_active_workspace_root() {
    use crate::tools::workspace_root::{self, WorkspaceRoot};

    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    std::fs::write(first.path().join("same.txt"), "first").unwrap();
    std::fs::write(second.path().join("same.txt"), "second").unwrap();
    let first_root = WorkspaceRoot::fixed(first.path());
    let second_root = WorkspaceRoot::fixed(second.path());
    let cache = ToolCache::new();
    let args = serde_json::json!({"path": "same.txt"});

    workspace_root::scope(first_root.clone(), async {
        cache
            .set("file_read", &args, serde_json::json!("from first"))
            .await;
        assert_eq!(
            cache.get("file_read", &args).await,
            Some(serde_json::json!("from first"))
        );
    })
    .await;

    workspace_root::scope(second_root, async {
        assert!(
            cache.get("file_read", &args).await.is_none(),
            "the same relative arguments in another worktree must miss"
        );
        cache
            .set("file_read", &args, serde_json::json!("from second"))
            .await;
    })
    .await;

    workspace_root::scope(first_root, async {
        assert_eq!(
            cache.get("file_read", &args).await,
            Some(serde_json::json!("from first")),
            "switching back may reuse only that workspace's own entry"
        );
    })
    .await;
}

#[tokio::test]
async fn invalidation_only_drops_entries_for_the_active_workspace() {
    use crate::tools::workspace_root::{self, WorkspaceRoot};

    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_root = WorkspaceRoot::fixed(first.path());
    let second_root = WorkspaceRoot::fixed(second.path());
    let cache = ToolCache::new();
    let tree = serde_json::json!({"path": "."});

    workspace_root::scope(first_root.clone(), async {
        cache
            .set("directory_tree", &tree, serde_json::json!(["first.rs"]))
            .await;
    })
    .await;
    workspace_root::scope(second_root.clone(), async {
        cache
            .set("directory_tree", &tree, serde_json::json!(["second.rs"]))
            .await;
        cache.invalidate_path("src/edited.rs").await;
        assert!(cache.get("directory_tree", &tree).await.is_none());
    })
    .await;

    workspace_root::scope(first_root, async {
        assert_eq!(
            cache.get("directory_tree", &tree).await,
            Some(serde_json::json!(["first.rs"])),
            "an edit in another worktree must not evict this root's listing"
        );
    })
    .await;
}

#[tokio::test]
async fn relative_path_mtime_is_checked_inside_the_active_workspace() {
    use crate::tools::workspace_root::{self, WorkspaceRoot};

    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("only-in-workspace.txt");
    std::fs::write(&file, "present").unwrap();
    let root = WorkspaceRoot::fixed(workspace.path());
    let cache = ToolCache::new();
    let args = serde_json::json!({"path": "only-in-workspace.txt"});

    workspace_root::scope(root.clone(), async {
        cache
            .set("file_read", &args, serde_json::json!("present"))
            .await;
        assert!(cache.get("file_read", &args).await.is_some());
    })
    .await;

    std::fs::remove_file(file).unwrap();
    workspace_root::scope(root, async {
        assert!(
            cache.get("file_read", &args).await.is_none(),
            "relative-path staleness must be checked under the workspace, not process cwd"
        );
    })
    .await;
}

#[test]
fn package_tools_invalidate_the_cache() {
    for tool in ["npm_install", "yarn_install", "pip_install", "npm_run"] {
        assert!(invalidates_cache(tool), "{tool}");
    }
    assert!(invalidates_cache("cargo_clippy"));
}
