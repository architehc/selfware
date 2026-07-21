//! Minimal cache layer for tool results and LLM responses
//!
//! This module provides basic caching for:
//! - Tool results (exact matching)
//! - LLM responses (semantic matching via embeddings)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cache entry with value and expiration
#[derive(Clone)]
struct CacheEntry {
    value: Value,
    created_at: Instant,
    ttl: Duration,
    file_mtime: Option<std::time::SystemTime>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }

    fn is_file_stale(&self, current_mtime: Option<std::time::SystemTime>) -> bool {
        match (self.file_mtime, current_mtime) {
            (Some(cached), Some(current)) => cached != current,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
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
            default_ttl: Duration::from_secs(300),
            max_entries: 1000,
        }
    }

    /// Generate a cache key from tool name and arguments
    pub fn cache_key(tool_name: &str, args: &Value) -> String {
        let args_str = serde_json::to_string(args).unwrap_or_default();
        format!("{}:{}", tool_name, args_str)
    }

    /// Get a cached result if available and not expired
    pub async fn get(&self, tool_name: &str, args: &Value) -> Option<Value> {
        let key = Self::cache_key(tool_name, args);
        let entries = self.entries.read().await;

        if let Some(entry) = entries.get(&key) {
            if !entry.is_expired() {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let current_mtime = tokio::fs::metadata(path)
                        .await
                        .ok()
                        .and_then(|m| m.modified().ok());

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
    pub async fn set(&self, tool_name: &str, args: &Value, value: Value) {
        self.set_with_ttl(tool_name, args, value, self.default_ttl)
            .await;
    }

    /// Store a result with a custom TTL
    pub async fn set_with_ttl(&self, tool_name: &str, args: &Value, value: Value, ttl: Duration) {
        let key = Self::cache_key(tool_name, args);

        let file_mtime = if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            tokio::fs::metadata(path)
                .await
                .ok()
                .and_then(|m| m.modified().ok())
        } else {
            None
        };

        let entry = CacheEntry {
            value,
            created_at: Instant::now(),
            ttl,
            file_mtime,
        };

        let mut entries = self.entries.write().await;
        if entries.len() >= self.max_entries {
            self.evict_expired(&mut entries);
        }
        entries.insert(key, entry);
    }

    /// Remove expired entries
    fn evict_expired(&self, entries: &mut HashMap<String, CacheEntry>) {
        entries.retain(|_, entry| !entry.is_expired());

        if entries.len() >= self.max_entries {
            let mut items: Vec<_> = entries
                .iter()
                .map(|(k, v)| (k.clone(), v.created_at))
                .collect();
            items.sort_by_key(|a| a.1);

            let to_remove = self.max_entries / 10;
            for (key, _) in items.iter().take(to_remove) {
                entries.remove(key);
            }
        }
    }

    /// Invalidate entries related to a specific file path
    pub async fn invalidate_path(&self, path: &str) {
        let mut entries = self.entries.write().await;
        entries.retain(|key, _| !key.contains(path));
    }

    /// Clear all entries
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        CacheStats {
            entries: entries.len(),
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
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub entries: usize,
    pub max_entries: usize,
    pub default_ttl_secs: u64,
}

/// Check if a tool is cacheable (read-only operations)
pub fn is_cacheable(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read"
            | "directory_tree"
            | "git_status"
            | "git_diff"
            | "git_log"
            | "git_show"
            | "grep_search"
            | "glob_find"
            | "symbol_search"
            | "read_file"
    )
}

/// Check if a tool invalidates cache entries (mutating operations)
pub fn invalidates_cache(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_write"
            | "file_edit"
            | "file_delete"
            | "file_multi_edit"
            | "git_commit"
            | "git_checkout"
            | "git_reset"
            | "shell_exec"
            | "pty_shell"
            | "patch_apply"
            | "write_file"
            | "edit_file"
    )
}

/// Configuration for LLM response caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCacheConfig {
    pub enabled: bool,
    pub semantic_matching: bool,
    pub similarity_threshold: f32,
    pub max_entries: usize,
    pub ttl_secs: u64,
    pub cost_per_1k_input: f64,
    pub cost_per_1k_output: f64,
}

impl Default for LlmCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            semantic_matching: true,
            similarity_threshold: 0.85,
            max_entries: 500,
            ttl_secs: 3600,
            cost_per_1k_input: 0.003,
            cost_per_1k_output: 0.015,
        }
    }
}

/// Entry in the LLM response cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCacheEntry {
    pub id: String,
    pub prompt: String,
    pub embedding: Vec<f32>,
    pub response: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub created_at: u64,
    pub hit_count: u64,
    pub context_hash: u64,
    pub file_paths: Vec<String>,
}

impl LlmCacheEntry {
    /// Calculate the estimated cost of this entry
    #[allow(dead_code)] // Used in tests; useful API for cost tracking
    pub fn estimated_cost(&self, config: &LlmCacheConfig) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1000.0) * config.cost_per_1k_input;
        let output_cost = (self.output_tokens as f64 / 1000.0) * config.cost_per_1k_output;
        input_cost + output_cost
    }
}

/// LLM response cache with semantic matching via cosine similarity
pub struct LlmCache {
    config: LlmCacheConfig,
    entries: RwLock<HashMap<String, LlmCacheEntry>>,
    embeddings: RwLock<HashMap<String, Vec<f32>>>,
}

impl LlmCache {
    /// Create a new LLM cache
    pub fn new(config: LlmCacheConfig) -> Self {
        Self {
            config,
            entries: RwLock::new(HashMap::new()),
            embeddings: RwLock::new(HashMap::new()),
        }
    }

    /// L2-normalize a vector for cosine similarity
    fn l2_normalize(v: &[f32]) -> Vec<f32> {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter().map(|x| x / norm).collect()
        } else {
            v.to_vec()
        }
    }

    /// Calculate cosine similarity between two normalized vectors (dot product)
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Look up a cached response by prompt similarity
    pub async fn lookup(
        &self,
        _prompt: &str,
        embedding: &[f32],
        context_hash: u64,
        model: &str,
    ) -> Option<LlmCacheEntry> {
        if !self.config.enabled {
            return None;
        }

        // Normalize the query embedding
        let normalized_query = Self::l2_normalize(embedding);

        let entries = self.entries.read().await;
        let embeddings = self.embeddings.read().await;

        let mut best_match: Option<(String, f32)> = None;

        for (id, stored_embedding) in embeddings.iter() {
            if stored_embedding.len() != normalized_query.len() {
                continue;
            }
            let similarity = Self::cosine_similarity(&normalized_query, stored_embedding);

            if similarity >= self.config.similarity_threshold {
                if let Some(entry) = entries.get(id) {
                    // Require both the same context_hash AND the same model
                    // before accepting a hit. This prevents semantic matches
                    // across different causal contexts or different models.
                    if entry.context_hash == context_hash
                        && entry.model == model
                        && (best_match.is_none() || similarity > best_match.as_ref().unwrap().1)
                    {
                        best_match = Some((id.clone(), similarity));
                    }
                }
            }
        }

        if let Some((id, _)) = best_match {
            if let Some(entry) = entries.get(&id).cloned() {
                return Some(entry);
            }
        }

        None
    }

    /// Store a response in the cache
    pub async fn store(&self, entry: LlmCacheEntry) {
        if !self.config.enabled {
            return;
        }

        let id = entry.id.clone();
        // Store normalized embedding for cosine similarity
        let embedding = Self::l2_normalize(&entry.embedding);

        let mut entries = self.entries.write().await;
        let mut embeddings = self.embeddings.write().await;

        if entries.len() >= self.config.max_entries && !entries.contains_key(&id) {
            self.evict_oldest(&mut entries, &mut embeddings);
        }

        entries.insert(id.clone(), entry);
        embeddings.insert(id, embedding);
    }

    /// Evict oldest entries when at capacity
    fn evict_oldest(
        &self,
        entries: &mut HashMap<String, LlmCacheEntry>,
        embeddings: &mut HashMap<String, Vec<f32>>,
    ) {
        let mut items: Vec<_> = entries
            .iter()
            .map(|(k, v)| (k.clone(), v.created_at))
            .collect();
        items.sort_by_key(|a| a.1);

        let to_remove = self.config.max_entries / 10;
        let ids_to_remove: Vec<_> = items
            .iter()
            .take(to_remove)
            .map(|(k, _)| k.clone())
            .collect();

        for id in &ids_to_remove {
            entries.remove(id);
            embeddings.remove(id);
        }
    }

    /// Invalidate entries for a file path
    pub async fn invalidate_path(&self, path: &str) {
        let ids_to_remove: Vec<_> = {
            let entries = self.entries.read().await;
            entries
                .iter()
                .filter(|(_, entry)| entry.file_paths.iter().any(|p| p.contains(path)))
                .map(|(id, _)| id.clone())
                .collect()
        };

        {
            let mut entries = self.entries.write().await;
            for id in &ids_to_remove {
                entries.remove(id);
            }
        }

        {
            let mut embeddings = self.embeddings.write().await;
            for id in &ids_to_remove {
                embeddings.remove(id);
            }
        }
    }

    /// Clear all entries
    #[allow(dead_code)]
    pub async fn clear(&self) {
        {
            let mut entries = self.entries.write().await;
            entries.clear();
        }
        {
            let mut embeddings = self.embeddings.write().await;
            embeddings.clear();
        }
    }

    /// Get cache statistics
    #[allow(dead_code)]
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        CacheStats {
            entries: entries.len(),
            max_entries: self.config.max_entries,
            default_ttl_secs: self.config.ttl_secs,
        }
    }
}

impl Default for LlmCache {
    fn default() -> Self {
        Self::new(LlmCacheConfig::default())
    }
}

/// Unified cache manager combining tool and LLM caches
pub struct CacheManager {
    /// Tool result cache (exact matching)
    pub tool_cache: ToolCache,
    /// LLM response cache (semantic matching)
    pub llm_cache: LlmCache,
    /// Local-first coordinator for offline support
    pub local_first: crate::session::local_first::LocalFirstCoordinator,
    /// Embedding provider for LLM cache similarity matching
    pub llm_embedding: crate::analysis::vector_store::TfIdfEmbeddingProvider,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(llm_config: LlmCacheConfig) -> Self {
        let llm_cache = LlmCache::new(llm_config);

        Self {
            tool_cache: ToolCache::new(),
            llm_cache,
            local_first: crate::session::local_first::LocalFirstCoordinator::new(),
            llm_embedding: crate::analysis::vector_store::TfIdfEmbeddingProvider::default(),
        }
    }

    /// Invalidate caches for a file path
    pub async fn invalidate_path(&self, path: &str) {
        self.tool_cache.invalidate_path(path).await;
        self.llm_cache.invalidate_path(path).await;
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new(LlmCacheConfig::default())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session/cache/cache_test.rs"]
mod tests;
