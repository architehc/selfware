//! Long-term memory management

use super::types::{
    ConsolidationResult, MemoryConfig, MemoryEntry, MemoryIndex, MemoryQuery, MemoryTier,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Long-term memory storage
pub struct LongTermMemory {
    entries: Arc<RwLock<HashMap<u64, MemoryEntry>>>,
    capacity: usize,
    index: Arc<MemoryIndex>,
}

impl LongTermMemory {
    pub fn new(capacity: usize, index: Arc<MemoryIndex>) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            capacity,
            index,
        }
    }

    pub fn with_config(config: &MemoryConfig, index: Arc<MemoryIndex>) -> Self {
        Self::new(config.long_term_capacity, index)
    }

    /// Store an entry
    pub async fn store(&self, mut entry: MemoryEntry) -> anyhow::Result<u64> {
        entry.tier = MemoryTier::LongTerm;

        let mut entries = self.entries.write().await;

        if entries.len() >= self.capacity && !entries.is_empty() {
            self.archive_oldest(&mut entries).await?;
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
            .filter(|e| self.matches_query(e, query))
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

    /// Update an entry
    pub async fn update(&self, id: u64, f: impl FnOnce(&mut MemoryEntry)) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&id) {
            f(entry);
            return true;
        }
        false
    }

    /// Remove an entry
    pub async fn remove(&self, id: u64) -> Option<MemoryEntry> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.remove(&id) {
            self.index.remove_entry(&entry).await;
            return Some(entry);
        }
        None
    }

    /// Get count of entries
    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Get all entries
    pub async fn entries(&self) -> Vec<MemoryEntry> {
        self.entries.read().await.values().cloned().collect()
    }

    /// Consolidate similar memories
    pub async fn consolidate(&self) -> ConsolidationResult {
        let mut entries = self.entries.write().await;
        let to_consolidate: Vec<u64> = entries
            .values()
            .filter(|e| e.importance < 0.3)
            .map(|e| e.id)
            .collect();

        let mut merged = 0;
        let mut removed = 0;

        for id in &to_consolidate {
            if entries.remove(id).is_some() {
                removed += 1;
            }
        }

        ConsolidationResult {
            entries_merged: merged,
            entries_removed: removed,
            new_summaries: Vec::new(),
        }
    }

    fn matches_query(&self, entry: &MemoryEntry, query: &MemoryQuery) -> bool {
        if !entry
            .content
            .to_lowercase()
            .contains(&query.pattern.to_lowercase())
        {
            return false;
        }

        if let Some(tier) = query.tier {
            if entry.tier != tier {
                return false;
            }
        }

        if !query.tags.is_empty() {
            if !query.tags.iter().all(|t| entry.tags.contains(t)) {
                return false;
            }
        }

        if let Some(min_importance) = query.min_importance {
            if entry.importance < min_importance {
                return false;
            }
        }

        if let Some(since) = query.since {
            if entry.created_at < since {
                return false;
            }
        }

        true
    }

    async fn archive_oldest(
        &self,
        entries: &mut tokio::sync::RwLockWriteGuard<'_, HashMap<u64, MemoryEntry>>,
    ) -> anyhow::Result<()> {
        let oldest = entries
            .values()
            .filter(|e| e.importance < 0.5)
            .min_by_key(|e| e.accessed_at)
            .map(|e| e.id);

        if let Some(id) = oldest {
            if let Some(entry) = entries.remove(&id) {
                self.index.remove_entry(&entry).await;
            }
        }

        Ok(())
    }
}

impl Clone for LongTermMemory {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            capacity: self.capacity,
            index: self.index.clone(),
        }
    }
}

/// Archive memory (cold storage)
pub struct ArchiveMemory {
    entries: Arc<RwLock<HashMap<u64, MemoryEntry>>>,
}

impl ArchiveMemory {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store(&self, mut entry: MemoryEntry) -> anyhow::Result<u64> {
        entry.tier = MemoryTier::Archive;
        let mut entries = self.entries.write().await;
        let id = entry.id;
        entries.insert(id, entry);
        Ok(id)
    }

    pub async fn retrieve(&self, id: u64) -> Option<MemoryEntry> {
        self.entries.read().await.get(&id).cloned()
    }

    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }
}

impl Default for ArchiveMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ArchiveMemory {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}
