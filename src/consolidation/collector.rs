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
mod tests {
    use super::*;

    fn make_episode(id: &str, hours_ago: i64, importance: u8) -> EpisodeData {
        EpisodeData {
            id: id.into(),
            content: format!("Episode {id}"),
            timestamp: Utc::now() - chrono::Duration::hours(hours_ago),
            importance,
            tags: vec!["test".into()],
            context: HashMap::new(),
            related_ids: Vec::new(),
            session_id: "session-1".into(),
        }
    }

    #[test]
    fn test_collect_episodes_filters_by_age() {
        let collector = ShortTermCollector::new(86400, 1); // 24 hours, min importance 1
        let episodes = vec![
            make_episode("recent", 1, 2),
            make_episode("old", 48, 2),
        ];

        let collected = collector.collect_episodes(&episodes);
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].source_id, "recent");
    }

    #[test]
    fn test_collect_episodes_filters_by_importance() {
        let collector = ShortTermCollector::new(86400 * 7, 3); // 7 days, min High
        let episodes = vec![
            make_episode("low", 1, 1),
            make_episode("high", 1, 3),
            make_episode("critical", 1, 4),
        ];

        let collected = collector.collect_episodes(&episodes);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_assemble_batch() {
        let collector = ShortTermCollector::new(86400, 1);
        let items = vec![
            CollectedItem {
                source_id: "ep-1".into(),
                source_type: SourceType::Episode,
                content: "test".into(),
                timestamp: Utc::now(),
                importance: 2,
                tags: vec![],
                metadata: HashMap::new(),
                related_ids: vec![],
                session_id: None,
                file_refs: vec![],
            },
            CollectedItem {
                source_id: "mem-1".into(),
                source_type: SourceType::MemoryEntry,
                content: "test".into(),
                timestamp: Utc::now(),
                importance: 2,
                tags: vec![],
                metadata: HashMap::new(),
                related_ids: vec![],
                session_id: None,
                file_refs: vec![],
            },
        ];

        let batch = collector.assemble_batch(items);
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.source_counts[&SourceType::Episode], 1);
        assert_eq!(batch.source_counts[&SourceType::MemoryEntry], 1);
    }
}
