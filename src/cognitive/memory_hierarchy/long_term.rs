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
    archive: ArchiveMemory,
}

impl LongTermMemory {
    pub fn new(capacity: usize, index: Arc<MemoryIndex>) -> Self {
        let archive = ArchiveMemory::with_index(index.clone());
        Self::with_archive(capacity, index, archive)
    }

    pub fn with_archive(capacity: usize, index: Arc<MemoryIndex>, archive: ArchiveMemory) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            capacity,
            index,
            archive,
        }
    }

    pub fn with_config(config: &MemoryConfig, index: Arc<MemoryIndex>) -> Self {
        Self::new(config.long_term_capacity, index)
    }

    /// Store an entry
    pub async fn store(&self, mut entry: MemoryEntry) -> anyhow::Result<u64> {
        anyhow::ensure!(self.capacity > 0, "long-term memory capacity is zero");
        entry.tier = MemoryTier::LongTerm;

        let mut entries = self.entries.write().await;

        if entries.len() >= self.capacity && !entries.contains_key(&entry.id) {
            self.archive_oldest(&mut entries).await?;
        }

        let id = entry.id;
        let replaced = entries.insert(id, entry.clone());

        if let Some(previous) = replaced {
            self.index.remove_entry(&previous).await;
        }
        self.index.index_entry(&entry).await;
        drop(entries);

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

    /// Update an entry
    pub async fn update(&self, id: u64, f: impl FnOnce(&mut MemoryEntry)) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&id) {
            let previous = entry.clone();
            f(entry);
            // A memory remains in the tier that owns it; callers may update
            // content, tags, and importance but cannot create index ghosts in
            // another tier through this API.
            entry.id = id;
            entry.tier = MemoryTier::LongTerm;
            let updated = entry.clone();
            self.index.remove_entry(&previous).await;
            self.index.index_entry(&updated).await;
            drop(entries);
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

        let merged = 0;
        let mut removed = 0;

        for id in &to_consolidate {
            if let Some(entry) = entries.remove(id) {
                // Keep the shared index in sync: dropping the entry without
                // de-indexing it would leave orphaned ids in tag/tier queries.
                self.index.remove_entry(&entry).await;
                removed += 1;
            }
        }

        ConsolidationResult {
            entries_merged: merged,
            entries_removed: removed,
            new_summaries: Vec::new(),
        }
    }

    async fn archive_oldest(
        &self,
        entries: &mut tokio::sync::RwLockWriteGuard<'_, HashMap<u64, MemoryEntry>>,
    ) -> anyhow::Result<()> {
        let oldest = entries
            .values()
            .filter(|e| e.importance < 0.5)
            .min_by_key(|e| e.accessed_at)
            .or_else(|| entries.values().min_by_key(|e| e.accessed_at))
            .map(|e| e.id);

        if let Some(id) = oldest {
            if let Some(mut entry) = entries.remove(&id) {
                let original = entry.clone();
                entry.tier = MemoryTier::Archive;
                if let Err(error) = self.archive.store(entry).await {
                    entries.insert(id, original.clone());
                    self.index.index_entry(&original).await;
                    return Err(error);
                }
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
            archive: self.archive.clone(),
        }
    }
}

/// Archive memory (cold storage)
pub struct ArchiveMemory {
    entries: Arc<RwLock<HashMap<u64, MemoryEntry>>>,
    index: Arc<MemoryIndex>,
}

impl ArchiveMemory {
    pub fn new() -> Self {
        Self::with_index(Arc::new(MemoryIndex::new()))
    }

    pub fn with_index(index: Arc<MemoryIndex>) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            index,
        }
    }

    pub async fn store(&self, mut entry: MemoryEntry) -> anyhow::Result<u64> {
        entry.tier = MemoryTier::Archive;
        let mut entries = self.entries.write().await;
        let id = entry.id;
        let replaced = entries.insert(id, entry.clone());
        if let Some(previous) = replaced {
            self.index.remove_entry(&previous).await;
        }
        self.index.index_entry(&entry).await;
        drop(entries);
        Ok(id)
    }

    pub async fn retrieve(&self, id: u64) -> Option<MemoryEntry> {
        self.entries.read().await.get(&id).cloned()
    }

    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }

    pub async fn query(&self, query: &MemoryQuery) -> Vec<MemoryEntry> {
        let entries = self.entries.read().await;
        let mut results: Vec<_> = entries
            .values()
            .filter(|entry| super::types::matches_query(entry, query))
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

    pub async fn remove(&self, id: u64) -> Option<MemoryEntry> {
        let mut entries = self.entries.write().await;
        let removed = entries.remove(&id);
        if let Some(entry) = &removed {
            self.index.remove_entry(entry).await;
        }
        drop(entries);
        removed
    }

    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        let removed: Vec<_> = entries.values().cloned().collect();
        entries.clear();
        for entry in &removed {
            self.index.remove_entry(entry).await;
        }
        drop(entries);
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
            index: self.index.clone(),
        }
    }
}
