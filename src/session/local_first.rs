//! Minimal local-first optimization module
//!
//! Provides basic offline support and response caching.

use std::collections::HashMap;

/// Local-first coordinator for offline support
#[derive(Debug)]
pub struct LocalFirstCoordinator {
    /// Cached responses
    response_cache: LocalCache<String>,
}

impl LocalFirstCoordinator {
    /// Create a new coordinator
    pub fn new() -> Self {
        Self {
            response_cache: LocalCache::new(),
        }
    }

    /// Cache a response
    pub fn cache_response(&mut self, key: &str, response: String, size_bytes: usize) {
        let entry = LocalCacheEntry {
            key: key.to_string(),
            value: response,
            size_bytes,
        };
        self.response_cache.put(entry);
    }

    /// Retrieve a cached response and record a hit/miss
    pub fn get_response(&mut self, key: &str) -> Option<&str> {
        self.response_cache.get(key)
    }

    /// Record a cache miss from an upstream cache layer
    pub fn record_miss(&mut self) {
        self.response_cache.record_miss();
    }

    /// Invalidate cached git status and diff responses
    pub fn invalidate_git(&mut self) {
        self.response_cache.invalidate_git();
    }

    /// Invalidate cached responses for a file path, tree-scoped tools, and git status/diff
    pub fn invalidate_git_and_path(&mut self, path: &str) {
        self.response_cache.invalidate_git_and_path(path);
    }

    /// Clear all cached responses
    pub fn clear(&mut self) {
        self.response_cache.clear();
    }

    /// Get statistics
    pub fn stats(&self) -> LocalFirstStats {
        let cache_stats = self.response_cache.stats();
        LocalFirstStats {
            offline_status: OfflineStatus::Online,
            bandwidth_saved_bytes: cache_stats.total_size_bytes,
            cache_stats,
        }
    }
}

impl Default for LocalFirstCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple local cache for generic values
#[derive(Debug)]
pub struct LocalCache<T> {
    entries: HashMap<String, LocalCacheEntry<T>>,
    max_entries: usize,
    hits: usize,
    misses: usize,
}

#[derive(Debug, Clone)]
pub struct LocalCacheEntry<T> {
    key: String,
    value: T,
    size_bytes: usize,
}

fn is_tree_scoped_key(key: &str) -> bool {
    key.split_once(':')
        .is_some_and(|(tool, _)| crate::session::cache::TREE_SCOPED_TOOLS.contains(&tool))
}

impl LocalCache<String> {
    /// Create a new cache with default settings
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 1000,
            hits: 0,
            misses: 0,
        }
    }

    /// Insert an entry
    pub fn put(&mut self, entry: LocalCacheEntry<String>) {
        if self.entries.len() >= self.max_entries && !self.entries.contains_key(&entry.key) {
            // Simple eviction: remove first entry
            if let Some(first_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&first_key);
            }
        }
        self.entries.insert(entry.key.clone(), entry);
    }

    /// Look up an entry by key and update hit/miss counters
    pub fn get(&mut self, key: &str) -> Option<&str> {
        if let Some(entry) = self.entries.get(key) {
            self.hits += 1;
            Some(entry.value.as_str())
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn record_miss(&mut self) {
        self.misses += 1;
    }

    /// Invalidate git status and git diff entries
    pub fn invalidate_git(&mut self) {
        self.entries
            .retain(|key, _| !key.starts_with("git_status") && !key.starts_with("git_diff"));
    }

    /// Invalidate entries related to a file path, tree-scoped tools, and git status/diff
    pub fn invalidate_git_and_path(&mut self, path: &str) {
        self.entries.retain(|key, _| {
            !key.contains(path)
                && !key.starts_with("git_status")
                && !key.starts_with("git_diff")
                && !is_tree_scoped_key(key)
        });
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_size_bytes: usize = self.entries.values().map(|e| e.size_bytes).sum();
        let total_lookups = self.hits + self.misses;
        let hit_rate = if total_lookups > 0 {
            self.hits as f64 / total_lookups as f64
        } else {
            0.0
        };
        CacheStats {
            entry_count: self.entries.len(),
            total_size_bytes,
            hit_rate,
        }
    }
}

impl Default for LocalCache<String> {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: usize,
    pub total_size_bytes: usize,
    pub hit_rate: f64,
}

/// Offline status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineStatus {
    Online,
    #[allow(dead_code)]
    Offline,
}

impl std::fmt::Display for OfflineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfflineStatus::Online => write!(f, "Online"),
            OfflineStatus::Offline => write!(f, "Offline"),
        }
    }
}

/// Local-first statistics
#[derive(Debug, Clone)]
pub struct LocalFirstStats {
    pub offline_status: OfflineStatus,
    pub bandwidth_saved_bytes: usize,
    pub cache_stats: CacheStats,
}

#[cfg(test)]
#[path = "../../tests/unit/session/local_first/local_first_test.rs"]
mod tests;
