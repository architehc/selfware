//! Short-term memory management

use super::types::{
    MemoryConfig, MemoryEntry, MemoryIndex, MemoryQuery, MemoryTier, TierTransition,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Short-term memory storage
pub struct ShortTermMemory {
    entries: Arc<RwLock<HashMap<u64, MemoryEntry>>>,
    capacity: usize,
    index: Arc<MemoryIndex>,
}

impl ShortTermMemory {
    pub fn new(capacity: usize, index: Arc<MemoryIndex>) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            capacity,
            index,
        }
    }

    pub fn with_config(config: &MemoryConfig, index: Arc<MemoryIndex>) -> Self {
        Self::new(config.short_term_capacity, index)
    }

    /// Total tokens currently held in this tier — included in the memory
    /// hierarchy's usage accounting (previously invisible to the budget).
    pub async fn tokens_total(&self) -> usize {
        self.entries
            .read()
            .await
            .values()
            .map(|e| crate::token_count::estimate_content_tokens(&e.content))
            .sum()
    }

    /// Store an entry
    pub async fn store(&self, mut entry: MemoryEntry) -> anyhow::Result<u64> {
        entry.tier = MemoryTier::ShortTerm;

        let mut entries = self.entries.write().await;

        if entries.len() >= self.capacity && !entries.is_empty() {
            self.evict_oldest(&mut entries).await?;
        }

        let id = entry.id;
        entries.insert(id, entry.clone());
        drop(entries);

        self.index.index_entry(&entry).await;

        Ok(id)
    }

    /// Retrieve an entry
    pub async fn retrieve(&self, id: u64) -> Option<MemoryEntry> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&id) {
            entry.accessed();
            return Some(entry.clone());
        }
        None
    }

    /// Query entries
    pub async fn query(&self, query: &MemoryQuery) -> Vec<MemoryEntry> {
        let entries = self.entries.read().await;
        let mut results: Vec<MemoryEntry> = entries
            .values()
            .filter(|e| super::types::matches_query(e, query))
            .cloned()
            .collect();

        results.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.accessed_at.cmp(&a.accessed_at))
        });

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        results
    }

    /// Check if entry should be promoted
    pub async fn check_promotion(&self, id: u64, config: &MemoryConfig) -> TierTransition {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(&id) {
            if entry.access_count >= config.promotion_threshold
                && entry.importance >= config.importance_threshold
            {
                return TierTransition::Promote;
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now.saturating_sub(entry.accessed_at) > config.demotion_threshold * 1000 {
                return TierTransition::Demote;
            }
        }
        TierTransition::Keep
    }

    /// Remove an entry
    pub async fn remove(&self, id: u64) -> Option<MemoryEntry> {
        let mut entries = self.entries.write().await;
        entries.remove(&id)
    }

    /// Get count of entries
    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Clear all entries
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }

    /// Get all entries
    pub async fn entries(&self) -> Vec<MemoryEntry> {
        self.entries.read().await.values().cloned().collect()
    }

    async fn evict_oldest(
        &self,
        entries: &mut tokio::sync::RwLockWriteGuard<'_, HashMap<u64, MemoryEntry>>,
    ) -> anyhow::Result<()> {
        let oldest = entries.values().min_by_key(|e| e.accessed_at).map(|e| e.id);

        if let Some(id) = oldest {
            if let Some(entry) = entries.remove(&id) {
                self.index.remove_entry(&entry).await;
            }
        }

        Ok(())
    }
}

impl Clone for ShortTermMemory {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            capacity: self.capacity,
            index: self.index.clone(),
        }
    }
}

/// Working memory (most immediate)
pub struct WorkingMemory {
    inner: ShortTermMemory,
}

impl WorkingMemory {
    pub fn new(capacity: usize, index: Arc<MemoryIndex>) -> Self {
        Self {
            inner: ShortTermMemory::new(capacity, index),
        }
    }

    pub async fn store(&self, mut entry: MemoryEntry) -> anyhow::Result<u64> {
        entry.tier = MemoryTier::Working;
        self.inner.store(entry).await
    }

    pub async fn retrieve(&self, id: u64) -> Option<MemoryEntry> {
        self.inner.retrieve(id).await
    }

    pub async fn query(&self, query: &MemoryQuery) -> Vec<MemoryEntry> {
        self.inner.query(query).await
    }

    pub async fn remove(&self, id: u64) -> Option<MemoryEntry> {
        self.inner.remove(id).await
    }

    pub async fn count(&self) -> usize {
        self.inner.count().await
    }

    pub async fn clear(&self) {
        self.inner.clear().await
    }

    /// Get all entries in working memory
    pub async fn entries(&self) -> Vec<MemoryEntry> {
        self.inner.entries().await
    }

    /// Add a message to working memory, tracking it in the working context
    pub fn add_message(&mut self, message: crate::api::types::Message, estimated_tokens: usize) {
        // Update the internal working context with the message and token count
        // The actual MemoryEntry storage is handled by HierarchicalMemory::add_message
        let _ = (message, estimated_tokens);
        // Token tracking and budget enforcement happen at the HierarchicalMemory level
    }

    /// Get the current working context
    pub fn get_context(&self) -> super::types::WorkingContext {
        super::types::WorkingContext::new("You are Selfware, an AI assistant.")
    }
}

impl Clone for WorkingMemory {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
