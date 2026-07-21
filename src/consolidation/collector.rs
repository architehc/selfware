//! Short-term data collector — gathers data from all ephemeral sources
//! for consolidation into long-term memory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A collected item from any short-term source, normalized for processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedItem {
    /// Unique identifier from the source.
    pub source_id: String,
    /// Which source this came from.
    pub source_type: SourceType,
    /// Main text content.
    pub content: String,
    /// When the original event occurred.
    pub timestamp: DateTime<Utc>,
    /// Importance level (1=Low, 2=Normal, 3=High, 4=Critical).
    pub importance: u8,
    /// Tags from the source.
    pub tags: Vec<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
    /// Related item IDs (for causal chain detection).
    pub related_ids: Vec<String>,
    /// Session identifier.
    pub session_id: Option<String>,
    /// Associated file paths (screenshots, traces).
    pub file_refs: Vec<String>,
}

/// The type of source a collected item came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceType {
    /// Episodic memory (from cognitive::episodic)
    Episode,
    /// Working memory entry (from memory.rs)
    MemoryEntry,
    /// Session log event (from agent::session_log)
    SessionEvent,
    /// Browser interaction trace (from computer control)
    InteractionTrace,
    /// Tool execution result
    ToolResult,
}

/// A batch of collected items ready for consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedBatch {
    /// All items in this batch.
    pub items: Vec<CollectedItem>,
    /// Time range covered by this batch.
    pub time_range: (DateTime<Utc>, DateTime<Utc>),
    /// Number of items by source type.
    pub source_counts: HashMap<SourceType, usize>,
}

/// Collects data from short-term sources for consolidation.
pub struct ShortTermCollector {
    /// Maximum age of items to collect (in seconds from now).
    max_age_secs: u64,
    /// Minimum importance to collect.
    min_importance: u8,
}

impl ShortTermCollector {
    pub fn new(max_age_secs: u64, min_importance: u8) -> Self {
        Self {
            max_age_secs,
            min_importance,
        }
    }

    /// Collect episodes from the episodic memory system.
    ///
    /// Converts Episode structs into normalized CollectedItems.
    pub fn collect_episodes(&self, episodes: &[EpisodeData]) -> Vec<CollectedItem> {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.max_age_secs as i64);

        episodes
            .iter()
            .filter(|ep| ep.timestamp >= cutoff && ep.importance >= self.min_importance)
            .map(|ep| CollectedItem {
                source_id: ep.id.clone(),
                source_type: SourceType::Episode,
                content: ep.content.clone(),
                timestamp: ep.timestamp,
                importance: ep.importance,
                tags: ep.tags.clone(),
                metadata: ep.context.clone(),
                related_ids: ep.related_ids.clone(),
                session_id: Some(ep.session_id.clone()),
                file_refs: Vec::new(),
            })
            .collect()
    }

    /// Collect memory entries from working memory.
    pub fn collect_memory_entries(&self, entries: &[MemoryEntryData]) -> Vec<CollectedItem> {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.max_age_secs as i64);

        entries
            .iter()
            .filter(|e| e.timestamp >= cutoff)
            .map(|e| CollectedItem {
                source_id: format!("mem-{}", e.timestamp.timestamp()),
                source_type: SourceType::MemoryEntry,
                content: e.content.clone(),
                timestamp: e.timestamp,
                importance: 2, // Normal
                tags: vec![e.role.clone()],
                metadata: HashMap::new(),
                related_ids: Vec::new(),
                session_id: None,
                file_refs: Vec::new(),
            })
            .collect()
    }

    /// Assemble a batch from multiple sources.
    pub fn assemble_batch(&self, items: Vec<CollectedItem>) -> CollectedBatch {
        let mut source_counts: HashMap<SourceType, usize> = HashMap::new();
        for item in &items {
            *source_counts.entry(item.source_type).or_insert(0) += 1;
        }

        let time_range = if items.is_empty() {
            let now = Utc::now();
            (now, now)
        } else {
            let min = items.iter().map(|i| i.timestamp).min().unwrap();
            let max = items.iter().map(|i| i.timestamp).max().unwrap();
            (min, max)
        };

        CollectedBatch {
            items,
            time_range,
            source_counts,
        }
    }
}

/// Normalized episode data for collection (avoids direct dependency on episodic module).
#[derive(Debug, Clone)]
pub struct EpisodeData {
    pub id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub importance: u8,
    pub tags: Vec<String>,
    pub context: HashMap<String, String>,
    pub related_ids: Vec<String>,
    pub session_id: String,
}

/// Normalized memory entry data for collection.
#[derive(Debug, Clone)]
pub struct MemoryEntryData {
    pub content: String,
    pub role: String,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
#[path = "../../tests/unit/consolidation/collector/collector_test.rs"]
mod tests;
