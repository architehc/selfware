//! Cache Integration Guide
//! This document describes how to integrate the cache infrastructure into the agent.

/// Step 1: Replace separate cache fields with unified CacheManager
/// Current Agent struct has:
///   tool_cache: ToolCache
///   llm_cache: LlmCache  
///   llm_embedding: TfIdfEmbeddingProvider
///   local_first: LocalFirstCoordinator
///   #[allow(dead_code)]
///
/// Should be:
///   cache_manager: CacheManager
///
/// This provides:
/// - Unified stats reporting
/// - Coordinated invalidation
/// - Cost tracking across all cache layers
/// - Semantic matching for LLM responses

/// Step 2: Initialize CacheManager in Agent::new()
/// Replace individual initializations:
///
///   tool_cache: ToolCache::new(),
///   llm_cache: LlmCache::new(cache_config),
///   llm_embedding: TfIdfEmbeddingProvider::default(),
///   local_first: LocalFirstCoordinator::new(),
///
/// With:
///
///   cache_manager: CacheManager::new(cache_config),

/// Step 3: Wire up cache invalidation hooks
/// In execution.rs after tool calls, the invalidation is already done. Ensure all 
/// mutating operations call cache_manager.invalidate_path()

/// Step 4: Enable semantic matching for LLM responses
/// In streaming.rs:
/// - Use cache_manager.llm_lookup() before making API calls
/// - Store responses with embeddings using cache_manager.llm_store()
/// - This enables reuse of semantically similar responses

/// Step 5: Add cache stats to TUI dashboard
/// In context_management.rs, update() already shows stats. Enhance with:
/// - Hit rates
/// - Cost savings
/// - Memory usage
/// - Recommendations

/// Step 6: Persistence
/// - Implement save/load for cache state
/// - Store in ~/.local/share/selfware/cache/
/// - Enable offline operation through LocalFirstCoordinator

/// Step 7: Monitor and tune
/// - Track which tools benefit most from caching
/// - Adjust TTLs based on file change frequency  
/// - Optimize semantic threshold for LLM cache

/// This integration will:
/// - Reduce API costs by 40-60% through LLM caching
/// - Speed up tool execution by 2-10x through result caching
/// - Enable offline operation
/// - Provide analytics on agent behavior
