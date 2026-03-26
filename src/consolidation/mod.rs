//! Memory Consolidation ("Sleep") System
//!
//! Periodic batch compaction of short-term experience into long-term storage,
//! inspired by human sleep-based memory consolidation.
//!
//! # Architecture
//!
//! ```text
//! Short-Term Sources          Consolidation Pipeline          Long-Term Storage
//! ┌──────────────┐           ┌──────────────────┐           ┌───────────────┐
//! │ Episodes     │──┐        │                  │           │ VectorStore   │
//! │ Memory       │──┼──────► │ Collector ──────►│──────────►│ (embeddings)  │
//! │ Session Logs │──┤        │                  │           │               │
//! │ Traces       │──┘        │ Compactor ──────►│──────────►│ KnowledgeGraph│
//! └──────────────┘           │ (32 LLM streams) │           │ (relationships│
//!                            │                  │           │               │
//!                            │ Store ──────────►│──────────►│ JSON files    │
//!                            └──────────────────┘           └───────────────┘
//! ```
//!
//! # Key Features
//!
//! - **Temporal preservation**: Timestamps, ordering, causal chains, decay curves
//! - **Multimodal references**: Screenshots, interaction traces, spatial layouts
//! - **Parallel processing**: Uses 32 concurrent LLM streams for summarization
//! - **Configurable decay**: Exponential with access reinforcement (24h half-life)
//!
//! # Example
//!
//! ```ignore
//! use selfware::consolidation::*;
//!
//! let config = ConsolidationConfig::new("http://localhost:8000/v1", "qwen3.5-27b");
//! let mut engine = ConsolidationEngine::new(config)?;
//! let report = engine.consolidate().await?;
//! println!("Consolidated {} episodes into {} records", report.episodes_processed, report.records_produced);
//! ```

pub mod collector;
pub mod compactor;
pub mod config;
pub mod multimodal;
pub mod store;
pub mod temporal;

pub use collector::{
    CollectedBatch, CollectedItem, EpisodeData, MemoryEntryData, ShortTermCollector, SourceType,
};
pub use compactor::MemoryCompactor;
pub use config::ConsolidationConfig;
pub use multimodal::MultimodalRef;
pub use store::LongTermStore;
pub use temporal::{CompactedContent, ConsolidationReport, RecordImportance, TemporalRecord};

use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

/// Main orchestrator for memory consolidation.
pub struct ConsolidationEngine {
    #[allow(dead_code)]
    config: ConsolidationConfig,
    collector: ShortTermCollector,
    compactor: MemoryCompactor,
    store: LongTermStore,
}

impl ConsolidationEngine {
    /// Create a new consolidation engine.
    pub fn new(config: ConsolidationConfig) -> Result<Self> {
        let collector = ShortTermCollector::new(
            (config.max_episode_age_hours * 3600.0) as u64,
            config.min_importance,
        );
        let compactor = MemoryCompactor::new(config.clone())?;
        let store = LongTermStore::new(PathBuf::from("consolidated_memory"));

        Ok(Self {
            config,
            collector,
            compactor,
            store,
        })
    }

    /// Create with a custom storage directory.
    pub fn with_storage_dir(mut self, dir: PathBuf) -> Self {
        self.store = LongTermStore::new(dir);
        self
    }

    /// Run one consolidation cycle ("sleep" episode).
    ///
    /// 1. Collect short-term data from all sources
    /// 2. Compact via parallel LLM summarization
    /// 3. Store in long-term storage
    pub async fn consolidate_episodes(
        &mut self,
        episodes: Vec<EpisodeData>,
    ) -> Result<ConsolidationReport> {
        info!(
            "Starting consolidation cycle with {} episodes",
            episodes.len()
        );

        // 1. Collect and normalize
        let items = self.collector.collect_episodes(&episodes);
        if items.is_empty() {
            info!("No items to consolidate");
            let now = chrono::Utc::now();
            return Ok(ConsolidationReport {
                started_at: now,
                ended_at: now,
                episodes_processed: 0,
                records_produced: 0,
                duplicates_removed: 0,
                tokens_used: 0,
                causal_links_created: 0,
                multimodal_refs_count: 0,
                errors: vec![],
            });
        }

        let batch = self.collector.assemble_batch(items);

        // 2. Compact via parallel LLM calls
        let (records, mut report) = self.compactor.compact(batch).await?;

        // 3. Store in long-term storage
        let store_result = self.store.store(&records).await?;
        if !store_result.errors.is_empty() {
            report.errors.extend(store_result.errors);
        }

        info!(
            "Consolidation complete: {} episodes -> {} records, {} tokens used",
            report.episodes_processed, report.records_produced, report.tokens_used,
        );

        Ok(report)
    }

    /// Run one consolidation cycle with raw collected items.
    pub async fn consolidate_items(
        &mut self,
        items: Vec<CollectedItem>,
    ) -> Result<ConsolidationReport> {
        if items.is_empty() {
            let now = chrono::Utc::now();
            return Ok(ConsolidationReport {
                started_at: now,
                ended_at: now,
                episodes_processed: 0,
                records_produced: 0,
                duplicates_removed: 0,
                tokens_used: 0,
                causal_links_created: 0,
                multimodal_refs_count: 0,
                errors: vec![],
            });
        }

        let batch = self.collector.assemble_batch(items);
        let (records, mut report) = self.compactor.compact(batch).await?;
        let store_result = self.store.store(&records).await?;
        if !store_result.errors.is_empty() {
            report.errors.extend(store_result.errors);
        }

        Ok(report)
    }

    /// Start periodic consolidation in the background.
    pub fn start_periodic(self, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let engine = self;
            let mut tick = tokio::time::interval(interval);

            loop {
                tick.tick().await;
                info!("Periodic consolidation triggered");

                // In periodic mode, we'd need a way to get episodes from the runtime.
                // For now, this is a placeholder that loads from the store and
                // reports on the state.
                match engine.store.load_all() {
                    Ok(records) => {
                        info!(
                            "Periodic check: {} consolidated records in store",
                            records.len()
                        );
                    }
                    Err(e) => {
                        tracing::error!("Periodic consolidation error: {e}");
                    }
                }
            }
        })
    }

    /// Get a reference to the long-term store for queries.
    pub fn store(&self) -> &LongTermStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let config = ConsolidationConfig::default();
        let engine = ConsolidationEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_engine_with_storage_dir() {
        let config = ConsolidationConfig::default();
        let engine = ConsolidationEngine::new(config)
            .unwrap()
            .with_storage_dir(PathBuf::from("/tmp/test_consolidation"));
        // Just verify it doesn't panic
        let _ = engine;
    }

    #[tokio::test]
    async fn test_consolidate_empty() {
        let config = ConsolidationConfig::default();
        let mut engine = ConsolidationEngine::new(config).unwrap();
        let report = engine.consolidate_episodes(vec![]).await.unwrap();
        assert_eq!(report.episodes_processed, 0);
        assert_eq!(report.records_produced, 0);
    }
}
