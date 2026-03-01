//! Intelligent Caching Layer for LLM Responses and Tool Results
//!
//! This module provides multi-tier caching with:
//! - Semantic similarity matching for LLM response reuse
//! - Context-aware cache invalidation
//! - Cost tracking and savings analytics
//! - Hit rate optimization strategies
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Cache Manager                            │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │  ToolCache  │  │  LlmCache   │  │  SemanticMatcher    │  │
//! │  │  (exact)    │  │  (semantic) │  │  (embeddings)       │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! │                          │                                   │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │ CostTracker │  │ Analytics   │  │  Invalidator        │  │
//! │  │ (savings)   │  │ (hit rate)  │  │  (context-aware)    │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Cache entry with value and expiration
#[derive(Clone)]
struct CacheEntry {
    value: Value,
    created_at: Instant,
    ttl: Duration,
    /// File modification time (if applicable)
    file_mtime: Option<std::time::SystemTime>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }

    fn is_file_stale(&self, current_mtime: Option<std::time::SystemTime>) -> bool {
        match (self.file_mtime, current_mtime) {
            (Some(cached), Some(current)) => cached != current,
            (None, Some(_)) => true, // File didn't exist before, now it does
            (Some(_), None) => true, // File existed before, now it doesn't
            (None, None) => false,   // File still doesn't exist
        }
    }
}

/// Thread-safe tool result cache
pub struct ToolCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    default_ttl: Duration,
    max_entries: usize,
}

impl ToolCache {
    /// Create a new cache with default settings
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            default_ttl: Duration::from_secs(300), // 5 minutes default
            max_entries: 1000,
        }
    }

    /// Create a cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            default_ttl: ttl,
            max_entries: 1000,
        }
    }

    /// Generate a cache key from tool name and arguments
    pub fn cache_key(tool_name: &str, args: &Value) -> String {
        // Normalize the args to ensure consistent keys
        let args_str = serde_json::to_string(args).unwrap_or_default();
        format!("{}:{}", tool_name, args_str)
    }

    /// Get a cached result if available and not expired
    pub fn get(&self, tool_name: &str, args: &Value) -> Option<Value> {
        let key = Self::cache_key(tool_name, args);
        let entries = self.entries.read().unwrap_or_else(|poisoned| {
            warn!("ToolCache read lock poisoned, recovering");
            poisoned.into_inner()
        });

        if let Some(entry) = entries.get(&key) {
            if !entry.is_expired() {
                // For file operations, check if file has been modified
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let current_mtime =
                        std::fs::metadata(path).ok().and_then(|m| m.modified().ok());

                    if entry.is_file_stale(current_mtime) {
                        return None;
                    }
                }
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Store a result in the cache
    pub fn set(&self, tool_name: &str, args: &Value, value: Value) {
        self.set_with_ttl(tool_name, args, value, self.default_ttl);
    }

    /// Store a result with a custom TTL
    pub fn set_with_ttl(&self, tool_name: &str, args: &Value, value: Value, ttl: Duration) {
        let key = Self::cache_key(tool_name, args);

        // Get file modification time if applicable
        let file_mtime = args
            .get("path")
            .and_then(|v| v.as_str())
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|m| m.modified().ok());

        let entry = CacheEntry {
            value,
            created_at: Instant::now(),
            ttl,
            file_mtime,
        };

        {
            let mut entries = self.entries.write().unwrap_or_else(|poisoned| {
                warn!("ToolCache write lock poisoned, recovering");
                poisoned.into_inner()
            });
            // Evict old entries if at capacity
            if entries.len() >= self.max_entries {
                self.evict_expired(&mut entries);
            }
            entries.insert(key, entry);
        }
    }

    /// Remove expired entries
    fn evict_expired(&self, entries: &mut HashMap<String, CacheEntry>) {
        entries.retain(|_, entry| !entry.is_expired());

        // If still at capacity, remove oldest entries
        if entries.len() >= self.max_entries {
            let mut items: Vec<_> = entries
                .iter()
                .map(|(k, v)| (k.clone(), v.created_at))
                .collect();
            items.sort_by(|a, b| a.1.cmp(&b.1));

            // Remove oldest 10%
            let to_remove = self.max_entries / 10;
            for (key, _) in items.iter().take(to_remove) {
                entries.remove(key);
            }
        }
    }

    /// Invalidate a specific cache entry
    pub fn invalidate(&self, tool_name: &str, args: &Value) {
        let key = Self::cache_key(tool_name, args);
        let mut entries = self.entries.write().unwrap_or_else(|poisoned| {
            warn!("ToolCache write lock poisoned, recovering");
            poisoned.into_inner()
        });
        entries.remove(&key);
    }

    /// Invalidate all entries for a specific tool
    pub fn invalidate_tool(&self, tool_name: &str) {
        let prefix = format!("{}:", tool_name);
        if let Ok(mut entries) = self.entries.write() {
            entries.retain(|key, _| !key.starts_with(&prefix));
        }
    }

    /// Invalidate entries related to a specific file path
    pub fn invalidate_path(&self, path: &str) {
        if let Ok(mut entries) = self.entries.write() {
            entries.retain(|key, _| !key.contains(path));
        }
    }

    /// Clear all cached entries
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let entries = self.entries.read().map(|e| e.len()).unwrap_or(0);
        CacheStats {
            entries,
            max_entries: self.max_entries,
            default_ttl_secs: self.default_ttl.as_secs(),
        }
    }
}

impl Default for ToolCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub max_entries: usize,
    pub default_ttl_secs: u64,
}

/// Tools that are safe to cache (read-only operations)
pub fn is_cacheable(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read"
            | "directory_tree"
            | "git_status"
            | "git_diff"
            | "grep_search"
            | "glob_find"
            | "symbol_search"
    )
}

/// Tools that should invalidate file-related caches
pub fn invalidates_cache(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_write" | "file_edit" | "git_commit" | "git_checkout" | "shell_exec"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let key1 = ToolCache::cache_key("file_read", &serde_json::json!({"path": "test.txt"}));
        let key2 = ToolCache::cache_key("file_read", &serde_json::json!({"path": "test.txt"}));
        let key3 = ToolCache::cache_key("file_read", &serde_json::json!({"path": "other.txt"}));

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_cache_set_get() {
        let cache = ToolCache::new();
        let args = serde_json::json!({"path": "/tmp/test.txt"});
        let value = serde_json::json!({"content": "hello"});

        cache.set("file_read", &args, value.clone());
        let cached = cache.get("file_read", &args);

        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), value);
    }

    #[test]
    fn test_cache_miss() {
        let cache = ToolCache::new();
        let args = serde_json::json!({"path": "/tmp/nonexistent.txt"});

        let cached = cache.get("file_read", &args);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_expiration() {
        let cache = ToolCache::with_ttl(Duration::from_millis(10));
        let args = serde_json::json!({"path": "/tmp/test.txt"});
        let value = serde_json::json!({"content": "hello"});

        cache.set("file_read", &args, value);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(20));

        let cached = cache.get("file_read", &args);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_invalidate_path() {
        let cache = ToolCache::new();

        cache.set(
            "file_read",
            &serde_json::json!({"path": "/tmp/file1.txt"}),
            serde_json::json!({"content": "1"}),
        );
        cache.set(
            "file_read",
            &serde_json::json!({"path": "/tmp/file2.txt"}),
            serde_json::json!({"content": "2"}),
        );

        cache.invalidate_path("/tmp/file1.txt");

        assert!(cache
            .get("file_read", &serde_json::json!({"path": "/tmp/file1.txt"}))
            .is_none());
        assert!(cache
            .get("file_read", &serde_json::json!({"path": "/tmp/file2.txt"}))
            .is_some());
    }

    #[test]
    fn test_is_cacheable() {
        assert!(is_cacheable("file_read"));
        assert!(is_cacheable("directory_tree"));
        assert!(is_cacheable("grep_search"));

        assert!(!is_cacheable("file_write"));
        assert!(!is_cacheable("shell_exec"));
    }

    #[test]
    fn test_invalidates_cache() {
        assert!(invalidates_cache("file_write"));
        assert!(invalidates_cache("file_edit"));
        assert!(invalidates_cache("shell_exec"));

        assert!(!invalidates_cache("file_read"));
        assert!(!invalidates_cache("git_status"));
    }

    #[test]
    fn test_cache_stats() {
        let cache = ToolCache::new();
        cache.set("test", &serde_json::json!({}), serde_json::json!({}));

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.max_entries, 1000);
    }

    #[test]
    fn test_cache_clear() {
        let cache = ToolCache::new();
        cache.set("test", &serde_json::json!({}), serde_json::json!({}));
        cache.clear();

        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn test_cache_with_ttl_constructor() {
        let cache = ToolCache::with_ttl(Duration::from_secs(60));
        let stats = cache.stats();
        assert_eq!(stats.default_ttl_secs, 60);
    }

    #[test]
    fn test_cache_set_with_ttl() {
        let cache = ToolCache::new();
        let args = serde_json::json!({"path": "test.txt"});
        let value = serde_json::json!({"content": "hello"});

        cache.set_with_ttl("file_read", &args, value.clone(), Duration::from_secs(120));

        let cached = cache.get("file_read", &args);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), value);
    }

    #[test]
    fn test_cache_multiple_tools() {
        let cache = ToolCache::new();

        cache.set(
            "file_read",
            &serde_json::json!({"path": "a.txt"}),
            serde_json::json!({"content": "a"}),
        );
        cache.set(
            "git_status",
            &serde_json::json!({}),
            serde_json::json!({"branch": "main"}),
        );
        cache.set(
            "grep_search",
            &serde_json::json!({"pattern": "test"}),
            serde_json::json!({"matches": []}),
        );

        assert!(cache
            .get("file_read", &serde_json::json!({"path": "a.txt"}))
            .is_some());
        assert!(cache.get("git_status", &serde_json::json!({})).is_some());
        assert!(cache
            .get("grep_search", &serde_json::json!({"pattern": "test"}))
            .is_some());
    }

    #[test]
    fn test_is_cacheable_all_types() {
        // All cacheable tools
        assert!(is_cacheable("file_read"));
        assert!(is_cacheable("directory_tree"));
        assert!(is_cacheable("git_status"));
        assert!(is_cacheable("git_diff"));
        assert!(is_cacheable("grep_search"));
        assert!(is_cacheable("glob_find"));
        assert!(is_cacheable("symbol_search"));

        // Non-cacheable tools
        assert!(!is_cacheable("file_write"));
        assert!(!is_cacheable("file_edit"));
        assert!(!is_cacheable("git_commit"));
        assert!(!is_cacheable("shell_exec"));
        assert!(!is_cacheable("unknown_tool"));
    }

    #[test]
    fn test_invalidates_cache_all_types() {
        assert!(invalidates_cache("file_write"));
        assert!(invalidates_cache("file_edit"));
        assert!(invalidates_cache("git_commit"));
        assert!(invalidates_cache("git_checkout"));
        assert!(invalidates_cache("shell_exec"));

        assert!(!invalidates_cache("file_read"));
        assert!(!invalidates_cache("git_status"));
        assert!(!invalidates_cache("grep_search"));
    }

    #[test]
    fn test_cache_stats_fields() {
        let cache = ToolCache::new();
        let stats = cache.stats();

        assert_eq!(stats.entries, 0);
        assert_eq!(stats.max_entries, 1000);
        assert_eq!(stats.default_ttl_secs, 300); // 5 minutes default
    }
}

// ============================================================================
// LLM Response Cache with Semantic Matching
// ============================================================================

/// Configuration for LLM response caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCacheConfig {
    /// Enable semantic similarity matching
    pub semantic_matching: bool,
    /// Minimum similarity threshold for cache hit (0.0 - 1.0)
    pub similarity_threshold: f32,
    /// Maximum number of cached responses
    pub max_entries: usize,
    /// Time-to-live for cached responses
    pub ttl_secs: u64,
    /// Enable cost tracking
    pub track_costs: bool,
    /// Model-specific cost per 1K input tokens
    pub input_cost_per_1k: f64,
    /// Model-specific cost per 1K output tokens
    pub output_cost_per_1k: f64,
}

impl Default for LlmCacheConfig {
    fn default() -> Self {
        Self {
            semantic_matching: true,
            similarity_threshold: 0.85,
            max_entries: 500,
            ttl_secs: 3600, // 1 hour
            track_costs: true,
            input_cost_per_1k: 0.003, // Default pricing
            output_cost_per_1k: 0.015,
        }
    }
}

/// A cached LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCacheEntry {
    /// Unique identifier
    pub id: String,
    /// Original prompt/query
    pub prompt: String,
    /// Prompt embedding for similarity matching
    pub embedding: Vec<f32>,
    /// The cached response
    pub response: String,
    /// Model that generated the response
    pub model: String,
    /// Input token count
    pub input_tokens: u32,
    /// Output token count
    pub output_tokens: u32,
    /// Cache creation timestamp (Unix seconds)
    pub created_at: u64,
    /// Number of times this cache entry was hit
    pub hit_count: u32,
    /// Context hash for invalidation
    pub context_hash: u64,
    /// Associated file paths (for invalidation)
    pub file_paths: Vec<String>,
}

impl LlmCacheEntry {
    /// Calculate the estimated cost of this response
    pub fn estimated_cost(&self, config: &LlmCacheConfig) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1000.0) * config.input_cost_per_1k;
        let output_cost = (self.output_tokens as f64 / 1000.0) * config.output_cost_per_1k;
        input_cost + output_cost
    }

    /// Check if entry is expired
    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now > self.created_at + ttl_secs
    }
}

/// LLM response cache with semantic similarity matching
pub struct LlmCache {
    config: LlmCacheConfig,
    entries: RwLock<HashMap<String, LlmCacheEntry>>,
    embeddings: RwLock<Vec<(String, Vec<f32>)>>, // (id, embedding) pairs for search
    cost_tracker: Arc<CostTracker>,
    analytics: Arc<CacheAnalytics>,
    invalidator: Arc<CacheInvalidator>,
}

impl LlmCache {
    /// Create a new LLM cache
    pub fn new(config: LlmCacheConfig) -> Self {
        Self {
            config,
            entries: RwLock::new(HashMap::new()),
            embeddings: RwLock::new(Vec::new()),
            cost_tracker: Arc::new(CostTracker::new()),
            analytics: Arc::new(CacheAnalytics::new()),
            invalidator: Arc::new(CacheInvalidator::new()),
        }
    }

    /// Look up a cached response by semantic similarity
    pub fn lookup(
        &self,
        _prompt: &str,
        embedding: &[f32],
        context_hash: u64,
    ) -> Option<LlmCacheEntry> {
        self.analytics.record_request();

        // Check invalidation first
        if self.invalidator.should_invalidate(context_hash) {
            return None;
        }

        let entries = self.entries.read().unwrap_or_else(|poisoned| {
            warn!("LlmCache entries read lock poisoned, recovering");
            poisoned.into_inner()
        });
        let embeddings = self.embeddings.read().unwrap_or_else(|poisoned| {
            warn!("LlmCache embeddings read lock poisoned, recovering");
            poisoned.into_inner()
        });

        // Normalize query once; stored embeddings are already normalized.
        let mut normed_query = embedding.to_vec();
        l2_normalize(&mut normed_query);

        // Find best matching entry by semantic similarity
        let mut best_match: Option<(&str, f32)> = None;
        for (id, entry_embedding) in embeddings.iter() {
            let similarity = cosine_similarity(&normed_query, entry_embedding);
            if similarity >= self.config.similarity_threshold
                && (best_match.is_none() || similarity > best_match.unwrap().1)
            {
                best_match = Some((id.as_str(), similarity));
            }
        }

        if let Some((id, _similarity)) = best_match {
            if let Some(entry) = entries.get(id) {
                // Check if expired
                if entry.is_expired(self.config.ttl_secs) {
                    return None;
                }

                // Record hit
                self.analytics.record_hit();
                self.cost_tracker
                    .record_savings(entry.estimated_cost(&self.config));

                return Some(entry.clone());
            }
        }

        self.analytics.record_miss();
        None
    }

    /// Store a response in the cache
    pub fn store(&self, entry: LlmCacheEntry) {
        let entry_id = entry.id.clone();
        self.invalidator.remove_entry(&entry_id);

        // Register file paths for invalidation tracking
        for path in &entry.file_paths {
            self.invalidator.register_path(&entry_id, path);
        }

        // Store L2-normalized embedding for similarity search so lookup
        // can use a simple dot product instead of full cosine formula.
        if let Ok(mut embeddings) = self.embeddings.write() {
            // Replace existing embedding for the same entry ID to avoid orphan growth.
            embeddings.retain(|(id, _)| id != &entry_id);

            // Hard cap: if embeddings grew beyond max_entries (e.g. due to
            // lock contention with eviction), trim the oldest.
            while embeddings.len() >= self.config.max_entries {
                embeddings.remove(0);
            }

            let mut normed = entry.embedding.clone();
            l2_normalize(&mut normed);
            embeddings.push((entry_id.clone(), normed));
        }

        // Store the entry
        if let Ok(mut entries) = self.entries.write() {
            // Evict if at capacity
            if entries.len() >= self.config.max_entries {
                self.evict_oldest(&mut entries);
            }
            entries.insert(entry_id, entry);
        }

        self.analytics.record_store();
    }

    /// Evict the oldest entries
    fn evict_oldest(&self, entries: &mut HashMap<String, LlmCacheEntry>) {
        let mut by_age: Vec<_> = entries
            .iter()
            .map(|(k, v)| (k.clone(), v.created_at))
            .collect();
        by_age.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        // Remove oldest 10%
        let to_remove = (self.config.max_entries / 10).max(1);
        let ids_to_remove: Vec<String> = by_age
            .into_iter()
            .take(to_remove)
            .map(|(id, _)| id)
            .collect();

        for id in &ids_to_remove {
            entries.remove(id);
            self.invalidator.remove_entry(id);
        }

        // Also remove from embeddings
        if let Ok(mut embeddings) = self.embeddings.write() {
            embeddings.retain(|(e_id, _)| !ids_to_remove.contains(e_id));
        }
    }

    /// Invalidate cache entries for a file path
    pub fn invalidate_path(&self, path: &str) {
        let ids_to_remove = self.invalidator.get_entries_for_path(path);
        if let Ok(mut entries) = self.entries.write() {
            for id in &ids_to_remove {
                entries.remove(id);
                self.invalidator.remove_entry(id);
            }
        }
        if let Ok(mut embeddings) = self.embeddings.write() {
            embeddings.retain(|(id, _)| !ids_to_remove.contains(id));
        }
    }

    /// Invalidate all entries matching a context hash
    pub fn invalidate_context(&self, context_hash: u64) {
        self.invalidator.mark_invalidated(context_hash);
    }

    /// Get cost tracker
    pub fn cost_tracker(&self) -> &Arc<CostTracker> {
        &self.cost_tracker
    }

    /// Get analytics
    pub fn analytics(&self) -> &Arc<CacheAnalytics> {
        &self.analytics
    }

    /// Get current cache size
    pub fn size(&self) -> usize {
        self.entries
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("LlmCache entries read lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len()
    }

    /// Clear the entire cache
    pub fn clear(&self) {
        {
            let mut entries = self.entries.write().unwrap_or_else(|p| {
                warn!("LlmCache entries write lock poisoned, recovering");
                p.into_inner()
            });
            entries.clear();
        }
        {
            let mut embeddings = self.embeddings.write().unwrap_or_else(|p| {
                warn!("LlmCache embeddings write lock poisoned, recovering");
                p.into_inner()
            });
            embeddings.clear();
        }
        self.invalidator.clear();
    }
}

impl Default for LlmCache {
    fn default() -> Self {
        Self::new(LlmCacheConfig::default())
    }
}

// ============================================================================
// Semantic Similarity Matching
// ============================================================================

/// L2-normalize a vector in place.  Zero vectors are left unchanged.
fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Calculate cosine similarity between two vectors.
///
/// When both inputs are already L2-normalized (as stored embeddings are),
/// this reduces to a simple dot product without any sqrt.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Semantic matcher for prompt similarity
pub struct SemanticMatcher {
    /// Similarity threshold for matching
    threshold: f32,
    /// Cached embeddings for comparison
    embeddings: RwLock<Vec<(String, Vec<f32>)>>,
}

impl SemanticMatcher {
    /// Create a new semantic matcher
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold: threshold.clamp(0.0, 1.0),
            embeddings: RwLock::new(Vec::new()),
        }
    }

    /// Add an embedding for future matching
    pub fn add(&self, id: &str, embedding: Vec<f32>) {
        let mut embeddings = self.embeddings.write().unwrap_or_else(|p| {
            warn!("SemanticMatcher write lock poisoned, recovering");
            p.into_inner()
        });
        embeddings.push((id.to_string(), embedding));
    }

    /// Find the best matching entry ID
    pub fn find_match(&self, embedding: &[f32]) -> Option<(String, f32)> {
        let embeddings = self.embeddings.read().unwrap_or_else(|poisoned| {
            warn!("SemanticMatcher read lock poisoned, recovering");
            poisoned.into_inner()
        });

        let mut best: Option<(String, f32)> = None;
        for (id, stored) in embeddings.iter() {
            let similarity = cosine_similarity(embedding, stored);
            if similarity >= self.threshold
                && (best.is_none() || similarity > best.as_ref().unwrap().1)
            {
                best = Some((id.clone(), similarity));
            }
        }

        best
    }

    /// Remove an embedding
    pub fn remove(&self, id: &str) {
        if let Ok(mut embeddings) = self.embeddings.write() {
            embeddings.retain(|(e_id, _)| e_id != id);
        }
    }

    /// Clear all embeddings
    pub fn clear(&self) {
        if let Ok(mut embeddings) = self.embeddings.write() {
            embeddings.clear();
        }
    }

    /// Get current embedding count
    pub fn count(&self) -> usize {
        self.embeddings.read().map(|e| e.len()).unwrap_or(0)
    }
}

impl Default for SemanticMatcher {
    fn default() -> Self {
        Self::new(0.85)
    }
}

// ============================================================================
// Cost Tracking
// ============================================================================

/// Tracks API cost savings from cache hits
pub struct CostTracker {
    /// Total savings from cache hits (in dollars)
    total_savings: AtomicU64, // Stored as microdollars (1/1,000,000)
    /// Number of cache hits that saved money
    hits_with_savings: AtomicU64,
    /// Total API calls that would have been made
    total_calls_avoided: AtomicU64,
    /// Cost history for trending
    history: RwLock<VecDeque<CostRecord>>,
}

/// A cost savings record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
    /// Amount saved in dollars
    pub amount: f64,
    /// Cumulative savings
    pub cumulative: f64,
}

impl CostTracker {
    /// Create a new cost tracker
    pub fn new() -> Self {
        Self {
            total_savings: AtomicU64::new(0),
            hits_with_savings: AtomicU64::new(0),
            total_calls_avoided: AtomicU64::new(0),
            history: RwLock::new(VecDeque::with_capacity(1000)),
        }
    }

    /// Record a cost savings
    pub fn record_savings(&self, amount: f64) {
        // Convert to microdollars for atomic storage
        let microdollars = (amount * 1_000_000.0) as u64;
        self.total_savings
            .fetch_add(microdollars, Ordering::Relaxed);
        self.hits_with_savings.fetch_add(1, Ordering::Relaxed);
        self.total_calls_avoided.fetch_add(1, Ordering::Relaxed);

        // Record in history
        {
            let mut history = self.history.write().unwrap_or_else(|p| {
                warn!("CostTracker write lock poisoned, recovering");
                p.into_inner()
            });
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let cumulative = self.total_savings();
            history.push_back(CostRecord {
                timestamp: now,
                amount,
                cumulative,
            });

            // Keep only last 1000 records
            while history.len() > 1000 {
                history.pop_front();
            }
        }
    }

    /// Get total savings in dollars
    pub fn total_savings(&self) -> f64 {
        self.total_savings.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Get number of hits that saved money
    pub fn hits_with_savings(&self) -> u64 {
        self.hits_with_savings.load(Ordering::Relaxed)
    }

    /// Get total calls avoided
    pub fn calls_avoided(&self) -> u64 {
        self.total_calls_avoided.load(Ordering::Relaxed)
    }

    /// Get average savings per hit
    pub fn average_savings(&self) -> f64 {
        let hits = self.hits_with_savings() as f64;
        if hits > 0.0 {
            self.total_savings() / hits
        } else {
            0.0
        }
    }

    /// Get cost history
    pub fn history(&self) -> Vec<CostRecord> {
        self.history
            .read()
            .unwrap_or_else(|p| {
                warn!("CostTracker read lock poisoned, recovering");
                p.into_inner()
            })
            .iter()
            .cloned()
            .collect()
    }

    /// Get summary stats
    pub fn summary(&self) -> CostSummary {
        CostSummary {
            total_savings: self.total_savings(),
            hits_with_savings: self.hits_with_savings(),
            calls_avoided: self.calls_avoided(),
            average_per_hit: self.average_savings(),
        }
    }

    /// Reset all tracking
    pub fn reset(&self) {
        self.total_savings.store(0, Ordering::Relaxed);
        self.hits_with_savings.store(0, Ordering::Relaxed);
        self.total_calls_avoided.store(0, Ordering::Relaxed);
        {
            let mut history = self.history.write().unwrap_or_else(|p| {
                warn!("CostTracker write lock poisoned, recovering");
                p.into_inner()
            });
            history.clear();
        }
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Cost tracking summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub total_savings: f64,
    pub hits_with_savings: u64,
    pub calls_avoided: u64,
    pub average_per_hit: f64,
}

// ============================================================================
// Cache Analytics
// ============================================================================

/// Tracks cache performance analytics
pub struct CacheAnalytics {
    /// Total requests
    requests: AtomicU64,
    /// Cache hits
    hits: AtomicU64,
    /// Cache misses
    misses: AtomicU64,
    /// Entries stored
    stores: AtomicU64,
    /// Hit rate history for trending
    history: RwLock<VecDeque<HitRateRecord>>,
}

/// A hit rate record for trending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitRateRecord {
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
    /// Hit rate at this point (0.0 - 1.0)
    pub hit_rate: f32,
    /// Total requests at this point
    pub total_requests: u64,
}

impl CacheAnalytics {
    /// Create new analytics tracker
    pub fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            stores: AtomicU64::new(0),
            history: RwLock::new(VecDeque::with_capacity(100)),
        }
    }

    /// Record a cache request
    pub fn record_request(&self) {
        self.requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache hit
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
        self.maybe_record_history();
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a store operation
    pub fn record_store(&self) {
        self.stores.fetch_add(1, Ordering::Relaxed);
    }

    /// Maybe record a history point (every 10 requests)
    fn maybe_record_history(&self) {
        let requests = self.requests.load(Ordering::Relaxed);
        if requests.is_multiple_of(10) {
            let mut history = self.history.write().unwrap_or_else(|p| {
                warn!("CacheAnalytics write lock poisoned, recovering");
                p.into_inner()
            });
            {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                history.push_back(HitRateRecord {
                    timestamp: now,
                    hit_rate: self.hit_rate(),
                    total_requests: requests,
                });

                // Keep only last 100 records
                while history.len() > 100 {
                    history.pop_front();
                }
            }
        }
    }

    /// Get total requests
    pub fn total_requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Get hit count
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Get miss count
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Get store count
    pub fn stores(&self) -> u64 {
        self.stores.load(Ordering::Relaxed)
    }

    /// Calculate hit rate (0.0 - 1.0)
    pub fn hit_rate(&self) -> f32 {
        let requests = self.total_requests() as f32;
        if requests > 0.0 {
            self.hits() as f32 / requests
        } else {
            0.0
        }
    }

    /// Get history for trending
    pub fn history(&self) -> Vec<HitRateRecord> {
        self.history
            .read()
            .unwrap_or_else(|p| {
                warn!("CacheAnalytics read lock poisoned, recovering");
                p.into_inner()
            })
            .iter()
            .cloned()
            .collect()
    }

    /// Get optimization suggestions based on analytics
    pub fn optimization_suggestions(&self) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();
        let hit_rate = self.hit_rate();

        if hit_rate < 0.3 && self.total_requests() > 100 {
            suggestions.push(OptimizationSuggestion {
                category: "Threshold".into(),
                message: "Low hit rate. Consider lowering similarity threshold.".into(),
                priority: OptimizationPriority::High,
            });
        }

        if hit_rate > 0.9 {
            suggestions.push(OptimizationSuggestion {
                category: "Efficiency".into(),
                message: "Excellent hit rate! Cache is working efficiently.".into(),
                priority: OptimizationPriority::Low,
            });
        }

        let misses = self.misses();
        if misses > 1000 && hit_rate < 0.5 {
            suggestions.push(OptimizationSuggestion {
                category: "Capacity".into(),
                message: "Many misses. Consider increasing cache size.".into(),
                priority: OptimizationPriority::Medium,
            });
        }

        suggestions
    }

    /// Get summary stats
    pub fn summary(&self) -> AnalyticsSummary {
        AnalyticsSummary {
            total_requests: self.total_requests(),
            hits: self.hits(),
            misses: self.misses(),
            stores: self.stores(),
            hit_rate: self.hit_rate(),
        }
    }

    /// Reset all analytics
    pub fn reset(&self) {
        self.requests.store(0, Ordering::Relaxed);
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.stores.store(0, Ordering::Relaxed);
        {
            let mut history = self.history.write().unwrap_or_else(|p| {
                warn!("CacheAnalytics write lock poisoned, recovering");
                p.into_inner()
            });
            history.clear();
        }
    }
}

impl Default for CacheAnalytics {
    fn default() -> Self {
        Self::new()
    }
}

/// Analytics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSummary {
    pub total_requests: u64,
    pub hits: u64,
    pub misses: u64,
    pub stores: u64,
    pub hit_rate: f32,
}

/// Optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    pub category: String,
    pub message: String,
    pub priority: OptimizationPriority,
}

/// Priority level for optimization suggestions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationPriority {
    Low,
    Medium,
    High,
}

// ============================================================================
// Cache Invalidation
// ============================================================================

/// Maximum number of tracked file paths in the cache invalidator.
const MAX_INVALIDATOR_PATHS: usize = 5_000;

/// Context-aware cache invalidator
pub struct CacheInvalidator {
    /// File path to cache entry IDs mapping
    path_to_entries: RwLock<HashMap<String, Vec<String>>>,
    /// Invalidated context hashes
    invalidated_contexts: RwLock<VecDeque<u64>>,
    /// File modification times for staleness check
    file_mtimes: RwLock<HashMap<String, u64>>,
}

impl CacheInvalidator {
    /// Create a new invalidator
    pub fn new() -> Self {
        Self {
            path_to_entries: RwLock::new(HashMap::new()),
            invalidated_contexts: RwLock::new(VecDeque::new()),
            file_mtimes: RwLock::new(HashMap::new()),
        }
    }

    /// Register a file path for a cache entry.
    ///
    /// When the number of tracked paths exceeds MAX_INVALIDATOR_PATHS,
    /// arbitrary old entries are evicted to stay within the limit.
    pub fn register_path(&self, entry_id: &str, path: &str) {
        if let Ok(mut map) = self.path_to_entries.write() {
            map.entry(path.to_string())
                .or_default()
                .push(entry_id.to_string());

            // Evict entries if over capacity
            if map.len() > MAX_INVALIDATOR_PATHS {
                let to_remove = map.len() - MAX_INVALIDATOR_PATHS;
                let keys: Vec<String> = map.keys().take(to_remove).cloned().collect();
                for key in keys {
                    map.remove(&key);
                }
            }
        }

        // Store current mtime
        if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(mtime) = metadata.modified() {
                if let Ok(duration) = mtime.duration_since(UNIX_EPOCH) {
                    if let Ok(mut mtimes) = self.file_mtimes.write() {
                        mtimes.insert(path.to_string(), duration.as_secs());

                        if mtimes.len() > MAX_INVALIDATOR_PATHS {
                            let to_remove = mtimes.len() - MAX_INVALIDATOR_PATHS;
                            let keys: Vec<String> =
                                mtimes.keys().take(to_remove).cloned().collect();
                            for key in keys {
                                mtimes.remove(&key);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get cache entry IDs associated with a file path
    pub fn get_entries_for_path(&self, path: &str) -> Vec<String> {
        self.path_to_entries
            .read()
            .ok()
            .and_then(|map| map.get(path).cloned())
            .unwrap_or_default()
    }

    /// Remove an entry ID from all path mappings.
    pub fn remove_entry(&self, entry_id: &str) {
        if let Ok(mut map) = self.path_to_entries.write() {
            for ids in map.values_mut() {
                ids.retain(|id| id != entry_id);
            }
            map.retain(|_, ids| !ids.is_empty());
        }
    }

    /// Mark a context as invalidated
    pub fn mark_invalidated(&self, context_hash: u64) {
        if let Ok(mut contexts) = self.invalidated_contexts.write() {
            if !contexts.contains(&context_hash) {
                contexts.push_back(context_hash);
                // Keep only last 100 invalidated contexts
                while contexts.len() > 100 {
                    contexts.pop_front();
                }
            }
        }
    }

    /// Check if a context should be invalidated
    pub fn should_invalidate(&self, context_hash: u64) -> bool {
        self.invalidated_contexts
            .read()
            .map(|contexts| contexts.contains(&context_hash))
            .unwrap_or(false)
    }

    /// Check if a file has been modified since caching
    pub fn is_file_stale(&self, path: &str) -> bool {
        let cached_mtime = self
            .file_mtimes
            .read()
            .ok()
            .and_then(|mtimes| mtimes.get(path).copied());

        if let Some(cached) = cached_mtime {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(current) = metadata.modified() {
                    if let Ok(duration) = current.duration_since(UNIX_EPOCH) {
                        return duration.as_secs() != cached;
                    }
                }
            }
            // File might not exist anymore
            return true;
        }

        false
    }

    /// Clear all tracking
    pub fn clear(&self) {
        if let Ok(mut map) = self.path_to_entries.write() {
            map.clear();
        }
        if let Ok(mut contexts) = self.invalidated_contexts.write() {
            contexts.clear();
        }
        if let Ok(mut mtimes) = self.file_mtimes.write() {
            mtimes.clear();
        }
    }
}

impl Default for CacheInvalidator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Unified Cache Manager
// ============================================================================

/// Unified cache manager combining tool and LLM caches
pub struct CacheManager {
    /// Tool result cache (exact matching)
    pub tool_cache: ToolCache,
    /// LLM response cache (semantic matching)
    pub llm_cache: LlmCache,
    /// Shared cost tracker
    cost_tracker: Arc<CostTracker>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(llm_config: LlmCacheConfig) -> Self {
        let llm_cache = LlmCache::new(llm_config);
        let cost_tracker = llm_cache.cost_tracker.clone();

        Self {
            tool_cache: ToolCache::new(),
            llm_cache,
            cost_tracker,
        }
    }

    /// Get the cost tracker
    pub fn cost_tracker(&self) -> &Arc<CostTracker> {
        &self.cost_tracker
    }

    /// Clear all caches
    pub fn clear_all(&self) {
        self.tool_cache.clear();
        self.llm_cache.clear();
    }

    /// Invalidate caches for a file path
    pub fn invalidate_path(&self, path: &str) {
        self.tool_cache.invalidate_path(path);
        self.llm_cache.invalidate_path(path);
    }

    /// Get combined stats
    pub fn stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            tool_cache: self.tool_cache.stats(),
            llm_analytics: self.llm_cache.analytics().summary(),
            cost_summary: self.cost_tracker.summary(),
            llm_cache_size: self.llm_cache.size(),
        }
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new(LlmCacheConfig::default())
    }
}

/// Combined cache manager statistics
#[derive(Debug, Clone)]
pub struct CacheManagerStats {
    pub tool_cache: CacheStats,
    pub llm_analytics: AnalyticsSummary,
    pub cost_summary: CostSummary,
    pub llm_cache_size: usize,
}

// ============================================================================
// Additional Tests
// ============================================================================

#[cfg(test)]
mod llm_cache_tests {
    use super::*;

    #[test]
    fn test_llm_cache_config_default() {
        let config = LlmCacheConfig::default();
        assert!(config.semantic_matching);
        assert_eq!(config.similarity_threshold, 0.85);
        assert_eq!(config.max_entries, 500);
        assert_eq!(config.ttl_secs, 3600);
    }

    #[test]
    fn test_llm_cache_entry_cost() {
        let config = LlmCacheConfig::default();
        let entry = LlmCacheEntry {
            id: "test".into(),
            prompt: "test prompt".into(),
            embedding: vec![0.1, 0.2, 0.3],
            response: "test response".into(),
            model: "test-model".into(),
            input_tokens: 1000,
            output_tokens: 500,
            created_at: 0,
            hit_count: 0,
            context_hash: 0,
            file_paths: vec![],
        };

        let cost = entry.estimated_cost(&config);
        // 1000/1000 * 0.003 + 500/1000 * 0.015 = 0.003 + 0.0075 = 0.0105
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_llm_cache_store_lookup() {
        let cache = LlmCache::default();

        let embedding = vec![0.5, 0.5, 0.5, 0.5];
        let entry = LlmCacheEntry {
            id: "test-1".into(),
            prompt: "What is Rust?".into(),
            embedding: embedding.clone(),
            response: "Rust is a systems programming language.".into(),
            model: "test".into(),
            input_tokens: 10,
            output_tokens: 20,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hit_count: 0,
            context_hash: 12345,
            file_paths: vec![],
        };

        cache.store(entry);

        // Should find similar query
        let result = cache.lookup("What is Rust?", &embedding, 12345);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().response,
            "Rust is a systems programming language."
        );
    }

    #[test]
    fn test_llm_cache_semantic_miss() {
        let cache = LlmCache::default();

        let embedding1 = vec![1.0, 0.0, 0.0, 0.0];
        let entry = LlmCacheEntry {
            id: "test-1".into(),
            prompt: "Question about Rust".into(),
            embedding: embedding1,
            response: "Answer about Rust".into(),
            model: "test".into(),
            input_tokens: 10,
            output_tokens: 20,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hit_count: 0,
            context_hash: 12345,
            file_paths: vec![],
        };

        cache.store(entry);

        // Very different embedding should miss
        let different_embedding = vec![0.0, 1.0, 0.0, 0.0];
        let result = cache.lookup("Different question", &different_embedding, 12345);
        assert!(result.is_none());
    }

    #[test]
    fn test_llm_cache_invalidation() {
        let cache = LlmCache::default();

        let embedding = vec![0.5, 0.5, 0.5, 0.5];
        let entry = LlmCacheEntry {
            id: "test-1".into(),
            prompt: "Query".into(),
            embedding: embedding.clone(),
            response: "Response".into(),
            model: "test".into(),
            input_tokens: 10,
            output_tokens: 20,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hit_count: 0,
            context_hash: 12345,
            file_paths: vec!["/tmp/test.txt".into()],
        };

        cache.store(entry);
        assert_eq!(cache.size(), 1);

        cache.invalidate_path("/tmp/test.txt");
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_llm_cache_context_invalidation() {
        let cache = LlmCache::default();

        let embedding = vec![0.5, 0.5, 0.5, 0.5];
        let entry = LlmCacheEntry {
            id: "test-1".into(),
            prompt: "Query".into(),
            embedding: embedding.clone(),
            response: "Response".into(),
            model: "test".into(),
            input_tokens: 10,
            output_tokens: 20,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hit_count: 0,
            context_hash: 12345,
            file_paths: vec![],
        };

        cache.store(entry);

        // Invalidate context
        cache.invalidate_context(12345);

        // Should not find due to invalidated context
        let result = cache.lookup("Query", &embedding, 12345);
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod semantic_matcher_tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_semantic_matcher_add_find() {
        let matcher = SemanticMatcher::new(0.9);
        matcher.add("entry-1", vec![1.0, 0.0, 0.0]);
        matcher.add("entry-2", vec![0.0, 1.0, 0.0]);

        let result = matcher.find_match(&[1.0, 0.0, 0.0]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "entry-1");
    }

    #[test]
    fn test_semantic_matcher_no_match() {
        let matcher = SemanticMatcher::new(0.9);
        matcher.add("entry-1", vec![1.0, 0.0, 0.0]);

        let result = matcher.find_match(&[0.0, 1.0, 0.0]);
        assert!(result.is_none());
    }

    #[test]
    fn test_semantic_matcher_remove() {
        let matcher = SemanticMatcher::new(0.9);
        matcher.add("entry-1", vec![1.0, 0.0, 0.0]);
        assert_eq!(matcher.count(), 1);

        matcher.remove("entry-1");
        assert_eq!(matcher.count(), 0);
    }

    #[test]
    fn test_semantic_matcher_clear() {
        let matcher = SemanticMatcher::new(0.9);
        matcher.add("entry-1", vec![1.0, 0.0, 0.0]);
        matcher.add("entry-2", vec![0.0, 1.0, 0.0]);
        assert_eq!(matcher.count(), 2);

        matcher.clear();
        assert_eq!(matcher.count(), 0);
    }

    #[test]
    fn test_semantic_matcher_default() {
        let matcher = SemanticMatcher::default();
        assert_eq!(matcher.count(), 0);
    }
}

#[cfg(test)]
mod cost_tracker_tests {
    use super::*;

    #[test]
    fn test_cost_tracker_new() {
        let tracker = CostTracker::new();
        assert_eq!(tracker.total_savings(), 0.0);
        assert_eq!(tracker.hits_with_savings(), 0);
        assert_eq!(tracker.calls_avoided(), 0);
    }

    #[test]
    fn test_cost_tracker_record_savings() {
        let tracker = CostTracker::new();
        tracker.record_savings(0.005);
        tracker.record_savings(0.003);

        assert!((tracker.total_savings() - 0.008).abs() < 0.0001);
        assert_eq!(tracker.hits_with_savings(), 2);
    }

    #[test]
    fn test_cost_tracker_average_savings() {
        let tracker = CostTracker::new();
        tracker.record_savings(0.010);
        tracker.record_savings(0.020);

        assert!((tracker.average_savings() - 0.015).abs() < 0.0001);
    }

    #[test]
    fn test_cost_tracker_average_savings_empty() {
        let tracker = CostTracker::new();
        assert_eq!(tracker.average_savings(), 0.0);
    }

    #[test]
    fn test_cost_tracker_summary() {
        let tracker = CostTracker::new();
        tracker.record_savings(0.01);

        let summary = tracker.summary();
        assert!((summary.total_savings - 0.01).abs() < 0.0001);
        assert_eq!(summary.hits_with_savings, 1);
        assert_eq!(summary.calls_avoided, 1);
    }

    #[test]
    fn test_cost_tracker_reset() {
        let tracker = CostTracker::new();
        tracker.record_savings(0.01);
        tracker.reset();

        assert_eq!(tracker.total_savings(), 0.0);
        assert_eq!(tracker.hits_with_savings(), 0);
    }

    #[test]
    fn test_cost_tracker_history() {
        let tracker = CostTracker::new();
        tracker.record_savings(0.01);
        tracker.record_savings(0.02);

        let history = tracker.history();
        assert_eq!(history.len(), 2);
    }
}

#[cfg(test)]
mod analytics_tests {
    use super::*;

    #[test]
    fn test_analytics_new() {
        let analytics = CacheAnalytics::new();
        assert_eq!(analytics.total_requests(), 0);
        assert_eq!(analytics.hits(), 0);
        assert_eq!(analytics.misses(), 0);
    }

    #[test]
    fn test_analytics_record() {
        let analytics = CacheAnalytics::new();
        analytics.record_request();
        analytics.record_hit();
        analytics.record_request();
        analytics.record_miss();

        assert_eq!(analytics.total_requests(), 2);
        assert_eq!(analytics.hits(), 1);
        assert_eq!(analytics.misses(), 1);
    }

    #[test]
    fn test_analytics_hit_rate() {
        let analytics = CacheAnalytics::new();
        for _ in 0..10 {
            analytics.record_request();
        }
        for _ in 0..7 {
            analytics.record_hit();
        }
        for _ in 0..3 {
            analytics.record_miss();
        }

        assert!((analytics.hit_rate() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_analytics_hit_rate_zero_requests() {
        let analytics = CacheAnalytics::new();
        assert_eq!(analytics.hit_rate(), 0.0);
    }

    #[test]
    fn test_analytics_stores() {
        let analytics = CacheAnalytics::new();
        analytics.record_store();
        analytics.record_store();
        assert_eq!(analytics.stores(), 2);
    }

    #[test]
    fn test_analytics_summary() {
        let analytics = CacheAnalytics::new();
        analytics.record_request();
        analytics.record_hit();

        let summary = analytics.summary();
        assert_eq!(summary.total_requests, 1);
        assert_eq!(summary.hits, 1);
        assert_eq!(summary.hit_rate, 1.0);
    }

    #[test]
    fn test_analytics_reset() {
        let analytics = CacheAnalytics::new();
        analytics.record_request();
        analytics.record_hit();
        analytics.reset();

        assert_eq!(analytics.total_requests(), 0);
        assert_eq!(analytics.hits(), 0);
    }

    #[test]
    fn test_optimization_suggestions_low_hit_rate() {
        let analytics = CacheAnalytics::new();
        // Simulate 200 requests with 20% hit rate
        for _ in 0..200 {
            analytics.record_request();
        }
        for _ in 0..40 {
            analytics.record_hit();
        }
        for _ in 0..160 {
            analytics.record_miss();
        }

        let suggestions = analytics.optimization_suggestions();
        assert!(!suggestions.is_empty());
        assert!(suggestions
            .iter()
            .any(|s| s.priority == OptimizationPriority::High));
    }

    #[test]
    fn test_optimization_suggestions_high_hit_rate() {
        let analytics = CacheAnalytics::new();
        // Simulate 100 requests with 95% hit rate
        for _ in 0..100 {
            analytics.record_request();
            analytics.record_hit();
        }

        let suggestions = analytics.optimization_suggestions();
        assert!(suggestions
            .iter()
            .any(|s| s.priority == OptimizationPriority::Low));
    }
}

#[cfg(test)]
mod invalidator_tests {
    use super::*;

    #[test]
    fn test_invalidator_new() {
        let inv = CacheInvalidator::new();
        assert!(inv.get_entries_for_path("/test").is_empty());
    }

    #[test]
    fn test_invalidator_register_path() {
        let inv = CacheInvalidator::new();
        inv.register_path("entry-1", "/tmp/test.txt");
        inv.register_path("entry-2", "/tmp/test.txt");

        let entries = inv.get_entries_for_path("/tmp/test.txt");
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&"entry-1".to_string()));
        assert!(entries.contains(&"entry-2".to_string()));
    }

    #[test]
    fn test_invalidator_context() {
        let inv = CacheInvalidator::new();
        assert!(!inv.should_invalidate(12345));

        inv.mark_invalidated(12345);
        assert!(inv.should_invalidate(12345));
    }

    #[test]
    fn test_invalidator_clear() {
        let inv = CacheInvalidator::new();
        inv.register_path("entry-1", "/tmp/test.txt");
        inv.mark_invalidated(12345);

        inv.clear();

        assert!(inv.get_entries_for_path("/tmp/test.txt").is_empty());
        assert!(!inv.should_invalidate(12345));
    }
}

#[cfg(test)]
mod cache_manager_tests {
    use super::*;

    #[test]
    fn test_cache_manager_new() {
        let manager = CacheManager::default();
        let stats = manager.stats();
        assert_eq!(stats.tool_cache.entries, 0);
        assert_eq!(stats.llm_cache_size, 0);
    }

    #[test]
    fn test_cache_manager_tool_cache() {
        let manager = CacheManager::default();
        manager.tool_cache.set(
            "file_read",
            &serde_json::json!({"path": "test.txt"}),
            serde_json::json!({"content": "hello"}),
        );

        let stats = manager.stats();
        assert_eq!(stats.tool_cache.entries, 1);
    }

    #[test]
    fn test_cache_manager_clear_all() {
        let manager = CacheManager::default();
        manager
            .tool_cache
            .set("test", &serde_json::json!({}), serde_json::json!({}));

        manager.clear_all();

        let stats = manager.stats();
        assert_eq!(stats.tool_cache.entries, 0);
    }

    #[test]
    fn test_cache_manager_invalidate_path() {
        let manager = CacheManager::default();
        manager.tool_cache.set(
            "file_read",
            &serde_json::json!({"path": "/tmp/test.txt"}),
            serde_json::json!({}),
        );

        manager.invalidate_path("/tmp/test.txt");

        assert!(manager
            .tool_cache
            .get("file_read", &serde_json::json!({"path": "/tmp/test.txt"}))
            .is_none());
    }

    #[test]
    fn test_cache_manager_cost_tracker() {
        let manager = CacheManager::default();
        manager.cost_tracker().record_savings(0.01);

        let stats = manager.stats();
        assert!((stats.cost_summary.total_savings - 0.01).abs() < 0.0001);
    }
}
