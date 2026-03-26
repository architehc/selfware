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

    /// Get statistics
    pub fn stats(&self) -> LocalFirstStats {
        let cache_stats = self.response_cache.stats();
        LocalFirstStats {
            offline_status: OfflineStatus::Online,
            pending_ops: 0,
            bandwidth_saved_bytes: cache_stats.total_size_bytes,
            edge_tasks_pending: 0,
            edge_tasks_completed: 0,
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
    max_size_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct LocalCacheEntry<T> {
    key: String,
    value: T,
    size_bytes: usize,
}

impl LocalCache<String> {
    /// Create a new cache with default settings
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 1000,
            max_size_bytes: 10 * 1024 * 1024, // 10MB
        }
    }

    /// Insert an entry
    pub fn put(&mut self, entry: LocalCacheEntry<String>) {
        if self.entries.len() >= self.max_entries {
            // Simple eviction: remove first entry
            if let Some(first_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&first_key);
            }
        }
        self.entries.insert(entry.key.clone(), entry);
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_size_bytes: usize = self.entries.values().map(|e| e.size_bytes).sum();
        CacheStats {
            entry_count: self.entries.len(),
            max_entries: self.max_entries,
            total_size_bytes,
            max_size_bytes: self.max_size_bytes,
            hit_rate: 0.0, // Simplified - no tracking in minimal version
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
    pub max_entries: usize,
    pub total_size_bytes: usize,
    pub max_size_bytes: usize,
    pub hit_rate: f64,
}

/// Offline status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineStatus {
    Online,
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
    pub pending_ops: usize,
    pub bandwidth_saved_bytes: usize,
    pub edge_tasks_pending: usize,
    pub edge_tasks_completed: usize,
    pub cache_stats: CacheStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_first_coordinator_new() {
        let coord = LocalFirstCoordinator::new();
        let stats = coord.stats();
        assert_eq!(stats.cache_stats.entry_count, 0);
        assert_eq!(stats.offline_status, OfflineStatus::Online);
    }

    #[test]
    fn test_cache_response() {
        let mut coord = LocalFirstCoordinator::new();
        coord.cache_response("key1", "value1".to_string(), 6);
        
        let stats = coord.stats();
        assert_eq!(stats.cache_stats.entry_count, 1);
        assert_eq!(stats.bandwidth_saved_bytes, 6);
    }

    #[test]
    fn test_local_cache_basic() {
        let mut cache = LocalCache::new();
        let entry = LocalCacheEntry {
            key: "test".to_string(),
            value: "value".to_string(),
            size_bytes: 5,
        };
        cache.put(entry);
        
        let stats = cache.stats();
        assert_eq!(stats.entry_count, 1);
        assert_eq!(stats.total_size_bytes, 5);
    }
}
