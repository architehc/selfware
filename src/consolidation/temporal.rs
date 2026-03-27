//! Temporal records — the core data structure for consolidated long-term memory.
//!
//! Preserves properties that don't appear in raw text tokens:
//! - Temporal ordering and timestamps
//! - Causal chains (what caused what)
//! - Decay curves (exponential with access reinforcement)
//! - Multimodal references (screenshots, interaction traces, spatial layouts)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::multimodal::MultimodalRef;

/// Compacted content produced by LLM summarization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedContent {
    /// Summarized text content.
    pub summary: String,
    /// Key facts extracted from the source material.
    pub key_facts: Vec<String>,
    /// Entities mentioned (people, tools, files, concepts).
    pub entities: Vec<String>,
    /// Actions that were taken.
    pub actions: Vec<String>,
    /// Outcomes and results.
    pub outcomes: Vec<String>,
    /// Lessons learned or insights.
    pub insights: Vec<String>,
}

impl CompactedContent {
    /// Estimate token count of the compacted content.
    pub fn estimated_tokens(&self) -> usize {
        let text_len = self.summary.len()
            + self.key_facts.iter().map(|s| s.len()).sum::<usize>()
            + self.entities.iter().map(|s| s.len()).sum::<usize>()
            + self.actions.iter().map(|s| s.len()).sum::<usize>()
            + self.outcomes.iter().map(|s| s.len()).sum::<usize>()
            + self.insights.iter().map(|s| s.len()).sum::<usize>();
        // Rough estimate: ~4 chars per token
        text_len / 4
    }
}

/// Importance level for temporal records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[derive(Default)]
pub enum RecordImportance {
    Low = 1,
    #[default]
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// A consolidated long-term memory record with rich temporal and multimodal metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalRecord {
    /// Unique identifier (SHA-256 prefix of content + timestamp).
    pub id: String,
    /// When this consolidated record was created.
    pub created_at: DateTime<Utc>,
    /// Original timestamps from the source episodes.
    pub source_timestamps: Vec<DateTime<Utc>>,
    /// Global monotonic sequence counter for ordering.
    pub sequence_order: u64,
    /// IDs of records that causally preceded this one.
    pub causal_parents: Vec<String>,
    /// IDs of records caused by this one.
    pub causal_children: Vec<String>,
    /// Current decay score (recomputed on access).
    pub decay_score: f64,
    /// Number of times this record has been accessed/referenced.
    pub access_count: u32,
    /// Last time this record was accessed.
    pub last_accessed: DateTime<Utc>,
    /// The compacted content.
    pub content: CompactedContent,
    /// References to non-text features (screenshots, traces, spatial layouts).
    pub multimodal_refs: Vec<MultimodalRef>,
    /// IDs of source episodes that were compacted into this record.
    pub source_ids: Vec<String>,
    /// Tags for categorization and retrieval.
    pub tags: Vec<String>,
    /// Importance level.
    pub importance: RecordImportance,
    /// Session ID where the source data originated.
    pub session_id: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

impl TemporalRecord {
    /// Compute the decay score at a given time.
    ///
    /// Uses exponential decay with a configurable half-life, boosted by access count.
    /// Each access partially resets the decay — more accessed memories persist longer.
    pub fn decay_score_at(&self, now: DateTime<Utc>, half_life_hours: f64) -> f64 {
        let age_hours = (now - self.created_at).num_seconds() as f64 / 3600.0;
        let base_decay = (-age_hours * (2.0_f64.ln()) / half_life_hours).exp();

        // Access reinforcement: each access adds a bonus that also decays
        let access_bonus = if self.access_count > 0 {
            let since_access = (now - self.last_accessed).num_seconds() as f64 / 3600.0;
            let access_decay = (-since_access * (2.0_f64.ln()) / half_life_hours).exp();
            // Diminishing returns on access count
            let access_factor = (self.access_count as f64).ln_1p() * 0.1;
            access_factor * access_decay
        } else {
            0.0
        };

        // Importance multiplier
        let importance_mult = match self.importance {
            RecordImportance::Low => 0.5,
            RecordImportance::Normal => 1.0,
            RecordImportance::High => 1.5,
            RecordImportance::Critical => 2.0,
        };

        ((base_decay + access_bonus) * importance_mult).min(1.0)
    }

    /// Record an access (updates access_count and last_accessed).
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
        self.decay_score = self.decay_score_at(Utc::now(), 24.0);
    }

    /// Get the time span covered by source data.
    pub fn time_span(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        if self.source_timestamps.is_empty() {
            return None;
        }
        let min = self.source_timestamps.iter().min().copied()?;
        let max = self.source_timestamps.iter().max().copied()?;
        Some((min, max))
    }

    /// Check if this record has causal connections.
    pub fn has_causal_links(&self) -> bool {
        !self.causal_parents.is_empty() || !self.causal_children.is_empty()
    }
}

/// Report from a consolidation cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    /// When the consolidation started.
    pub started_at: DateTime<Utc>,
    /// When it finished.
    pub ended_at: DateTime<Utc>,
    /// Number of source episodes processed.
    pub episodes_processed: usize,
    /// Number of consolidated records produced.
    pub records_produced: usize,
    /// Number of duplicates removed.
    pub duplicates_removed: usize,
    /// Total LLM tokens used for summarization.
    pub tokens_used: u64,
    /// Number of causal links established.
    pub causal_links_created: usize,
    /// Number of multimodal references preserved.
    pub multimodal_refs_count: usize,
    /// Any errors encountered.
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record() -> TemporalRecord {
        let now = Utc::now();
        TemporalRecord {
            id: "test-001".into(),
            created_at: now,
            source_timestamps: vec![
                now - chrono::Duration::hours(2),
                now - chrono::Duration::hours(1),
            ],
            sequence_order: 1,
            causal_parents: vec![],
            causal_children: vec![],
            decay_score: 1.0,
            access_count: 0,
            last_accessed: now,
            content: CompactedContent {
                summary: "Test summary".into(),
                key_facts: vec!["fact1".into()],
                entities: vec!["entity1".into()],
                actions: vec!["action1".into()],
                outcomes: vec!["outcome1".into()],
                insights: vec!["insight1".into()],
            },
            multimodal_refs: vec![],
            source_ids: vec!["ep-1".into(), "ep-2".into()],
            tags: vec!["test".into()],
            importance: RecordImportance::Normal,
            session_id: Some("session-1".into()),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_decay_score_fresh() {
        let record = make_record();
        let score = record.decay_score_at(Utc::now(), 24.0);
        // Fresh record should have score close to 1.0
        assert!(score > 0.9, "Fresh score should be > 0.9, got {score}");
    }

    #[test]
    fn test_decay_score_aged() {
        let mut record = make_record();
        record.created_at = Utc::now() - chrono::Duration::hours(48);
        record.last_accessed = record.created_at;
        let score = record.decay_score_at(Utc::now(), 24.0);
        // 48 hours = 2 half-lives, so ~0.25
        assert!(score < 0.3, "48h old score should be < 0.3, got {score}");
    }

    #[test]
    fn test_decay_score_with_access() {
        let mut record = make_record();
        record.created_at = Utc::now() - chrono::Duration::hours(24);
        record.last_accessed = Utc::now();
        record.access_count = 5;
        let score = record.decay_score_at(Utc::now(), 24.0);
        // Should be higher than a non-accessed record due to access bonus
        let mut plain = make_record();
        plain.created_at = record.created_at;
        plain.last_accessed = plain.created_at;
        let plain_score = plain.decay_score_at(Utc::now(), 24.0);
        assert!(
            score > plain_score,
            "Accessed score {score} should be > plain {plain_score}"
        );
    }

    #[test]
    fn test_importance_multiplier() {
        let mut low = make_record();
        low.importance = RecordImportance::Low;
        let mut critical = make_record();
        critical.importance = RecordImportance::Critical;

        let now = Utc::now();
        assert!(critical.decay_score_at(now, 24.0) > low.decay_score_at(now, 24.0));
    }

    #[test]
    fn test_time_span() {
        let record = make_record();
        let (min, max) = record.time_span().unwrap();
        assert!(min < max);
    }

    #[test]
    fn test_record_access() {
        let mut record = make_record();
        assert_eq!(record.access_count, 0);
        record.record_access();
        assert_eq!(record.access_count, 1);
        record.record_access();
        assert_eq!(record.access_count, 2);
    }

    #[test]
    fn test_serde_roundtrip() {
        let record = make_record();
        let json = serde_json::to_string(&record).unwrap();
        let parsed: TemporalRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, record.id);
        assert_eq!(parsed.source_ids.len(), 2);
        assert_eq!(parsed.importance, RecordImportance::Normal);
    }

    #[test]
    fn test_compacted_content_tokens() {
        let content = CompactedContent {
            summary: "A".repeat(400),
            key_facts: vec!["B".repeat(100)],
            entities: vec![],
            actions: vec![],
            outcomes: vec![],
            insights: vec![],
        };
        assert_eq!(content.estimated_tokens(), 125); // 500 / 4
    }
}
