use super::*;

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
        model: "test".into(),
        input_tokens: 10,
        output_tokens: 5,
        created_at: 0,
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
    assert_eq!(result.unwrap().response, "test response");
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
        model: "alpha".into(),
        input_tokens: 10,
        output_tokens: 5,
        created_at: 0,
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
        model: "m".into(),
        input_tokens: 10,
        output_tokens: 5,
        created_at: 0,
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
