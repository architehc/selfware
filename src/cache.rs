//! Result cache for selfware
//! Caches task results to avoid redundant API calls

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

/// Cache entry with TTL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub result: T,
    pub created_at: Instant,
    pub access_count: u64,
}

impl<T> CacheEntry<T> {
    pub fn new(result: T) -> Self {
        Self {
            result,
            created_at: Instant::now(),
            access_count: 1,
        }
    }
    
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// Task signature for cache keys
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskSignature {
    /// Normalized task description
    pub normalized_prompt: String,
    /// Task type/category
    pub task_type: String,
    /// Model used
    pub model: String,
}

impl TaskSignature {
    /// Create signature from raw prompt
    pub fn from_prompt(prompt: &str, model: &str) -> Self {
        Self {
            normalized_prompt: normalize_prompt(prompt),
            task_type: classify_task(prompt),
            model: model.to_string(),
        }
    }
}

/// Result cache with LRU eviction
pub struct ResultCache<T: Clone + Send + Sync> {
    cache: Arc<RwLock<HashMap<TaskSignature, CacheEntry<T>>>>,
    max_entries: usize,
    default_ttl: Duration,
    hits: Arc<RwLock<u64>>,
    misses: Arc<RwLock<u64>>,
}

impl<T: Clone + Send + Sync> ResultCache<T> {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::with_capacity(max_entries))),
            max_entries,
            default_ttl: Duration::from_secs(ttl_secs),
            hits: Arc::new(RwLock::new(0)),
            misses: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Get cached result if available
    pub async fn get(&self, signature: &TaskSignature) -> Option<T> {
        let cache = self.cache.read().await;
        
        if let Some(entry) = cache.get(signature) {
            if !entry.is_expired(self.default_ttl) {
                // Cache hit
                drop(cache);
                *self.hits.write().await += 1;
                return Some(entry.result.clone());
            }
        }
        
        // Cache miss
        *self.misses.write().await += 1;
        None
    }
    
    /// Store result in cache
    pub async fn put(&self, signature: TaskSignature, result: T) {
        let mut cache = self.cache.write().await;
        
        // Evict oldest if at capacity
        if cache.len() >= self.max_entries {
            if let Some(oldest) = cache.iter()
                .min_by_key(|(_, e)| e.created_at)
                .map(|(k, _)| k.clone()) {
                cache.remove(&oldest);
            }
        }
        
        cache.insert(signature, CacheEntry::new(result));
    }
    
    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let hits = *self.hits.read().await;
        let misses = *self.misses.read().await;
        let cache = self.cache.read().await;
        
        CacheStats {
            entries: cache.len(),
            hits,
            misses,
            hit_rate: if hits + misses > 0 {
                hits as f64 / (hits + misses) as f64
            } else {
                0.0
            },
        }
    }
    
    /// Clear expired entries
    pub async fn cleanup(&self) {
        let mut cache = self.cache.write().await;
        cache.retain(|_, entry| !entry.is_expired(self.default_ttl));
    }
}

/// Cache statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// Normalize prompt for cache key
fn normalize_prompt(prompt: &str) -> String {
    prompt.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Classify task type from prompt
fn classify_task(prompt: &str) -> String {
    let prompt_lower = prompt.to_lowercase();
    
    if prompt_lower.contains("python") {
        "python"
    } else if prompt_lower.contains("rust") {
        "rust"
    } else if prompt_lower.contains("javascript") || prompt_lower.contains("js") {
        "javascript"
    } else if prompt_lower.contains("sql") || prompt_lower.contains("database") {
        "database"
    } else if prompt_lower.contains("docker") || prompt_lower.contains("container") {
        "container"
    } else if prompt_lower.contains("kubernetes") || prompt_lower.contains("k8s") {
        "kubernetes"
    } else if prompt_lower.contains("test") || prompt_lower.contains("pytest") {
        "testing"
    } else if prompt_lower.contains("api") || prompt_lower.contains("rest") {
        "api"
    } else if prompt_lower.contains("web") || prompt_lower.contains("http") {
        "web"
    } else if prompt_lower.contains("ml") || prompt_lower.contains("neural") {
        "ml"
    } else {
        "general"
    }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_prompt() {
        assert_eq!(
            normalize_prompt("Write a Python function!!!"),
            "write a python function"
        );
    }
    
    #[test]
    fn test_classify_task() {
        assert_eq!(classify_task("Write Python code"), "python");
        assert_eq!(classify_task("Create Rust struct"), "rust");
        assert_eq!(classify_task("Build web server"), "web");
    }
    
    #[tokio::test]
    async fn test_cache_basic() {
        let cache = ResultCache::<String>::new(100, 3600);
        
        let sig = TaskSignature::from_prompt("Test prompt", "model");
        
        // Miss
        assert!(cache.get(&sig).await.is_none());
        
        // Store
        cache.put(sig.clone(), "result".to_string()).await;
        
        // Hit
        assert_eq!(cache.get(&sig).await, Some("result".to_string()));
        
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }
}
