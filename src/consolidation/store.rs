//! Long-term store — persists consolidated records into VectorStore and KnowledgeGraph.

use anyhow::Result;
use tracing::{debug, info};

use super::temporal::TemporalRecord;

/// Persists consolidated temporal records into long-term storage backends.
///
/// Uses VectorStore for embedding-based retrieval and KnowledgeGraph for
/// relationship traversal (causal chains, temporal ordering).
pub struct LongTermStore {
    /// Collection name in the vector store.
    collection_name: String,
    /// Directory for persisting records as JSON.
    storage_dir: std::path::PathBuf,
}

impl LongTermStore {
    pub fn new(storage_dir: std::path::PathBuf) -> Self {
        Self {
            collection_name: "consolidated".into(),
            storage_dir,
        }
    }

    /// Store consolidated records.
    ///
    /// 1. Persist each record as a JSON file
    /// 2. In the future: embed text in VectorStore, create KnowledgeGraph nodes
    pub async fn store(&self, records: &[TemporalRecord]) -> Result<StoreResult> {
        std::fs::create_dir_all(&self.storage_dir)?;

        let mut stored = 0;
        let mut errors = Vec::new();

        for record in records {
            let path = self
                .storage_dir
                .join(format!("{}.json", record.id));

            match serde_json::to_string_pretty(record) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&path, &json) {
                        errors.push(format!("Failed to write {}: {e}", record.id));
                    } else {
                        stored += 1;
                        debug!(
                            record_id = %record.id,
                            sources = record.source_ids.len(),
                            "Stored consolidated record"
                        );
                    }
                }
                Err(e) => {
                    errors.push(format!("Failed to serialize {}: {e}", record.id));
                }
            }
        }

        info!(
            "Stored {stored}/{} records to {}",
            records.len(),
            self.storage_dir.display(),
        );

        Ok(StoreResult {
            stored,
            errors,
            collection: self.collection_name.clone(),
        })
    }

    /// Load all stored records from disk.
    pub fn load_all(&self) -> Result<Vec<TemporalRecord>> {
        let mut records = Vec::new();

        if !self.storage_dir.exists() {
            return Ok(records);
        }

        for entry in std::fs::read_dir(&self.storage_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = std::fs::read_to_string(&path)?;
                match serde_json::from_str::<TemporalRecord>(&content) {
                    Ok(record) => records.push(record),
                    Err(e) => {
                        tracing::warn!("Failed to parse {}: {e}", path.display());
                    }
                }
            }
        }

        // Sort by sequence order
        records.sort_by_key(|r| r.sequence_order);

        info!("Loaded {} consolidated records", records.len());
        Ok(records)
    }

    /// Query records by tag.
    pub fn query_by_tag(&self, tag: &str) -> Result<Vec<TemporalRecord>> {
        let all = self.load_all()?;
        Ok(all
            .into_iter()
            .filter(|r| r.tags.iter().any(|t| t == tag))
            .collect())
    }

    /// Get records with active causal links to a given record ID.
    pub fn causal_neighbors(&self, record_id: &str) -> Result<Vec<TemporalRecord>> {
        let all = self.load_all()?;
        Ok(all
            .into_iter()
            .filter(|r| {
                r.causal_parents.iter().any(|p| p == record_id)
                    || r.causal_children.iter().any(|c| c == record_id)
                    || r.id == record_id
            })
            .collect())
    }
}

/// Result of a store operation.
#[derive(Debug, Clone)]
pub struct StoreResult {
    pub stored: usize,
    pub errors: Vec<String>,
    pub collection: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consolidation::temporal::{CompactedContent, RecordImportance};
    use std::collections::HashMap;

    fn make_record(id: &str) -> TemporalRecord {
        let now = chrono::Utc::now();
        TemporalRecord {
            id: id.into(),
            created_at: now,
            source_timestamps: vec![now],
            sequence_order: 1,
            causal_parents: vec![],
            causal_children: vec![],
            decay_score: 1.0,
            access_count: 0,
            last_accessed: now,
            content: CompactedContent {
                summary: "Test summary".into(),
                key_facts: vec![],
                entities: vec![],
                actions: vec![],
                outcomes: vec![],
                insights: vec![],
            },
            multimodal_refs: vec![],
            source_ids: vec!["src-1".into()],
            tags: vec!["test".into()],
            importance: RecordImportance::Normal,
            session_id: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_store_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = LongTermStore::new(dir.path().to_path_buf());

        let records = vec![make_record("r1"), make_record("r2")];

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(store.store(&records)).unwrap();
        assert_eq!(result.stored, 2);
        assert!(result.errors.is_empty());

        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_query_by_tag() {
        let dir = tempfile::tempdir().unwrap();
        let store = LongTermStore::new(dir.path().to_path_buf());

        let mut r1 = make_record("r1");
        r1.tags = vec!["important".into()];
        let mut r2 = make_record("r2");
        r2.tags = vec!["trivial".into()];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(store.store(&[r1, r2])).unwrap();

        let results = store.query_by_tag("important").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
    }

    #[test]
    fn test_load_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = LongTermStore::new(dir.path().to_path_buf());
        let loaded = store.load_all().unwrap();
        assert!(loaded.is_empty());
    }
}
