//! Memory compactor — runs parallel LLM summarization to produce consolidated
//! temporal records from short-term data.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use super::collector::{CollectedBatch, CollectedItem};
use super::config::ConsolidationConfig;
use super::multimodal::MultimodalRef;
use super::temporal::{CompactedContent, ConsolidationReport, RecordImportance, TemporalRecord};

/// Groups related items for batch summarization.
#[derive(Debug)]
struct ItemGroup {
    items: Vec<CollectedItem>,
    #[allow(dead_code)]
    primary_timestamp: chrono::DateTime<Utc>,
}

/// Compacts short-term data into consolidated temporal records using parallel LLM calls.
pub struct MemoryCompactor {
    config: ConsolidationConfig,
    client: Client,
    semaphore: Arc<Semaphore>,
    sequence_counter: Arc<AtomicU64>,
}

impl MemoryCompactor {
    pub fn new(config: ConsolidationConfig) -> Result<Self> {
        config
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid config: {e}"))?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .connect_timeout(std::time::Duration::from_secs(15))
            .pool_max_idle_per_host(config.max_concurrent_llm)
            .build()
            .context("Failed to build HTTP client")?;

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_llm));

        Ok(Self {
            config,
            client,
            semaphore,
            sequence_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Compact a collected batch into consolidated temporal records.
    pub async fn compact(
        &self,
        batch: CollectedBatch,
    ) -> Result<(Vec<TemporalRecord>, ConsolidationReport)> {
        let start = Instant::now();
        let started_at = Utc::now();
        let episodes_processed = batch.items.len();

        info!(
            "Starting compaction: {} items, {} concurrent streams",
            episodes_processed, self.config.max_concurrent_llm,
        );

        // 1. Group related items by temporal proximity and causal links
        let groups = self.group_items(batch.items);
        info!("Grouped into {} clusters", groups.len());

        // 2. Spawn parallel LLM summarization for each group
        let mut handles = Vec::with_capacity(groups.len());
        let total_tokens = Arc::new(AtomicU64::new(0));

        for group in groups {
            let sem = self.semaphore.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            let seq = self.sequence_counter.clone();
            let tokens = total_tokens.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                let result = summarize_group(&client, &config, &group, &seq, &tokens).await;
                result
            });
            handles.push(handle);
        }

        // 3. Collect results
        let mut records = Vec::new();
        let mut errors = Vec::new();

        for handle in handles {
            match handle.await {
                Ok(Ok(record)) => records.push(record),
                Ok(Err(e)) => {
                    warn!("Compaction group failed: {e}");
                    errors.push(format!("{e}"));
                }
                Err(e) => {
                    warn!("Compaction task panicked: {e}");
                    errors.push(format!("Task panic: {e}"));
                }
            }
        }

        // 4. Count causal links and multimodal refs
        let causal_links_created: usize = records
            .iter()
            .map(|r| r.causal_parents.len() + r.causal_children.len())
            .sum();
        let multimodal_refs_count: usize = records.iter().map(|r| r.multimodal_refs.len()).sum();

        let report = ConsolidationReport {
            started_at,
            ended_at: Utc::now(),
            episodes_processed,
            records_produced: records.len(),
            duplicates_removed: 0, // Dedup happens in the store layer
            tokens_used: total_tokens.load(Ordering::Relaxed),
            causal_links_created,
            multimodal_refs_count,
            errors,
        };

        info!(
            "Compaction complete: {} records from {} episodes in {:.1}s",
            report.records_produced,
            report.episodes_processed,
            start.elapsed().as_secs_f64(),
        );

        Ok((records, report))
    }

    /// Group items by temporal proximity and causal relationships.
    fn group_items(&self, mut items: Vec<CollectedItem>) -> Vec<ItemGroup> {
        if items.is_empty() {
            return Vec::new();
        }

        // Sort by timestamp
        items.sort_by_key(|i| i.timestamp);

        // Group items within a time window (30 minutes)
        let window_secs = 1800;
        let mut groups: Vec<ItemGroup> = Vec::new();
        let mut current_group: Vec<CollectedItem> = vec![items.remove(0)];

        for item in items {
            let last_ts = current_group.last().unwrap().timestamp;
            let gap = (item.timestamp - last_ts).num_seconds().abs();

            // Also check if items are causally related
            let causally_related = current_group.iter().any(|existing| {
                item.related_ids.contains(&existing.source_id)
                    || existing.related_ids.contains(&item.source_id)
            });

            if gap <= window_secs || causally_related {
                current_group.push(item);
            } else {
                let primary_ts = current_group[0].timestamp;
                groups.push(ItemGroup {
                    items: std::mem::take(&mut current_group),
                    primary_timestamp: primary_ts,
                });
                current_group.push(item);
            }
        }

        // Don't forget the last group
        if !current_group.is_empty() {
            let primary_ts = current_group[0].timestamp;
            groups.push(ItemGroup {
                items: current_group,
                primary_timestamp: primary_ts,
            });
        }

        groups
    }
}

/// Summarize a group of items using the LLM.
async fn summarize_group(
    client: &Client,
    config: &ConsolidationConfig,
    group: &ItemGroup,
    sequence: &AtomicU64,
    total_tokens: &AtomicU64,
) -> Result<TemporalRecord> {
    // Build the consolidation prompt
    let mut context_parts = Vec::new();
    for item in group.items.iter() {
        context_parts.push(format!(
            "[{}] ({:?}) [{}] {}\n  Tags: {}\n  Metadata: {:?}",
            item.timestamp.format("%H:%M:%S"),
            item.source_type,
            importance_label(item.importance),
            item.content,
            item.tags.join(", "),
            item.metadata,
        ));
    }
    let context_text = context_parts.join("\n\n");

    let prompt = format!(
        r#"You are a memory consolidation system. Analyze the following sequence of events/memories and produce a consolidated summary.

## Source Events

{context_text}

## Instructions

Produce a JSON response with these fields:
- "summary": A concise paragraph summarizing what happened, preserving temporal ordering and causal relationships
- "key_facts": Array of important facts (max 5)
- "entities": Array of entities mentioned (tools, files, concepts, people)
- "actions": Array of actions that were taken
- "outcomes": Array of results/outcomes
- "insights": Array of lessons learned or patterns noticed
- "causal_chain": Array of [cause_id, effect_id] pairs showing causal relationships between the source events
- "importance": One of "low", "normal", "high", "critical"
- "tags": Array of categorization tags

Respond with only valid JSON, no markdown fences."#
    );

    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));

    let body = json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": "You are a precise memory consolidation system. Output only valid JSON."},
            {"role": "user", "content": prompt}
        ],
        "max_tokens": config.summary_max_tokens,
        "temperature": config.temperature,
        "stream": false,
    });

    debug!(
        "Sending consolidation request for {} items",
        group.items.len()
    );

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Consolidation LLM request failed")?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .context("Failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {status}: {body_text}"));
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&body_text).context("Failed to parse API response")?;

    let content_str = parsed["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    let tokens_used = parsed["usage"]["total_tokens"].as_u64().unwrap_or(0);
    total_tokens.fetch_add(tokens_used, Ordering::Relaxed);

    // Parse the LLM's structured response
    let llm_output: serde_json::Value =
        serde_json::from_str(content_str.trim()).unwrap_or_else(|_| {
            // Try extracting JSON from markdown fences
            let cleaned = extract_json(content_str);
            serde_json::from_str(cleaned).unwrap_or(json!({
                "summary": content_str,
                "key_facts": [],
                "entities": [],
                "actions": [],
                "outcomes": [],
                "insights": [],
            }))
        });

    let content = CompactedContent {
        summary: llm_output["summary"]
            .as_str()
            .unwrap_or("No summary generated")
            .to_string(),
        key_facts: json_string_array(&llm_output["key_facts"]),
        entities: json_string_array(&llm_output["entities"]),
        actions: json_string_array(&llm_output["actions"]),
        outcomes: json_string_array(&llm_output["outcomes"]),
        insights: json_string_array(&llm_output["insights"]),
    };

    // Build causal links from LLM output and source relationships
    let mut causal_parents = Vec::new();
    if let Some(chains) = llm_output["causal_chain"].as_array() {
        for pair in chains {
            if let Some(cause) = pair[0].as_str() {
                causal_parents.push(cause.to_string());
            }
        }
    }
    // Also inherit causal links from source items
    for item in &group.items {
        for related in &item.related_ids {
            if !causal_parents.contains(related) {
                causal_parents.push(related.clone());
            }
        }
    }

    // Determine importance
    let importance = match llm_output["importance"].as_str().unwrap_or("normal") {
        "low" => RecordImportance::Low,
        "high" => RecordImportance::High,
        "critical" => RecordImportance::Critical,
        _ => RecordImportance::Normal,
    };

    // Generate ID
    let seq = sequence.fetch_add(1, Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(content.summary.as_bytes());
    hasher.update(seq.to_le_bytes());
    let id = hex::encode(&hasher.finalize()[..8]);

    // Collect source timestamps and IDs
    let source_timestamps: Vec<_> = group.items.iter().map(|i| i.timestamp).collect();
    let source_ids: Vec<_> = group.items.iter().map(|i| i.source_id.clone()).collect();

    // Collect multimodal refs from source items
    let multimodal_refs: Vec<MultimodalRef> = group
        .items
        .iter()
        .flat_map(|item| {
            item.file_refs.iter().map(|path| {
                MultimodalRef::Screenshot {
                    path: std::path::PathBuf::from(path),
                    timestamp: item.timestamp,
                    description: format!("Screenshot from {}", item.source_id),
                    dimensions: (0, 0), // Unknown at collection time
                }
            })
        })
        .collect();

    // Tags from LLM output merged with source tags
    let mut tags: Vec<String> = json_string_array(&llm_output["tags"]);
    for item in &group.items {
        for tag in &item.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
    }

    let now = Utc::now();
    let record = TemporalRecord {
        id,
        created_at: now,
        source_timestamps,
        sequence_order: seq,
        causal_parents,
        causal_children: Vec::new(), // Filled in later by cross-referencing
        decay_score: 1.0,
        access_count: 0,
        last_accessed: now,
        content,
        multimodal_refs,
        source_ids,
        tags,
        importance,
        session_id: group.items.first().and_then(|i| i.session_id.clone()),
        metadata: std::collections::HashMap::new(),
    };

    Ok(record)
}

fn importance_label(level: u8) -> &'static str {
    match level {
        1 => "LOW",
        2 => "NORMAL",
        3 => "HIGH",
        4 => "CRITICAL",
        _ => "UNKNOWN",
    }
}

fn json_string_array(val: &serde_json::Value) -> Vec<String> {
    val.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_json(text: &str) -> &str {
    if let Some(start) = text.find("```json") {
        let inner = &text[start + 7..];
        if let Some(end) = inner.find("```") {
            return inner[..end].trim();
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return &text[start..=end];
        }
    }
    text.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consolidation::collector::{CollectedItem, SourceType};
    use std::collections::HashMap;

    fn make_item(id: &str, minutes_ago: i64) -> CollectedItem {
        CollectedItem {
            source_id: id.into(),
            source_type: SourceType::Episode,
            content: format!("Event {id}"),
            timestamp: Utc::now() - chrono::Duration::minutes(minutes_ago),
            importance: 2,
            tags: vec!["test".into()],
            metadata: HashMap::new(),
            related_ids: Vec::new(),
            session_id: Some("session-1".into()),
            file_refs: Vec::new(),
        }
    }

    #[test]
    fn test_group_items_by_time() {
        let config = ConsolidationConfig::default();
        let compactor = MemoryCompactor::new(config).unwrap();

        let items = vec![
            make_item("a", 10), // recent cluster
            make_item("b", 8),
            make_item("c", 120), // separate cluster (2h gap)
            make_item("d", 118),
        ];

        let groups = compactor.group_items(items);
        assert_eq!(groups.len(), 2, "Should create 2 groups with >30min gap");
    }

    #[test]
    fn test_group_items_causal() {
        let config = ConsolidationConfig::default();
        let compactor = MemoryCompactor::new(config).unwrap();

        let mut items = vec![
            make_item("a", 100),
            make_item("b", 5), // far in time from 'a'
        ];
        // But 'b' is causally related to 'a'
        items[1].related_ids.push("a".into());

        let groups = compactor.group_items(items);
        // They should still be grouped because of causal link
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn test_group_items_empty() {
        let config = ConsolidationConfig::default();
        let compactor = MemoryCompactor::new(config).unwrap();
        let groups = compactor.group_items(Vec::new());
        assert!(groups.is_empty());
    }

    #[test]
    fn test_importance_label() {
        assert_eq!(importance_label(1), "LOW");
        assert_eq!(importance_label(2), "NORMAL");
        assert_eq!(importance_label(3), "HIGH");
        assert_eq!(importance_label(4), "CRITICAL");
    }

    #[test]
    fn test_json_string_array() {
        let val = json!(["a", "b", "c"]);
        assert_eq!(json_string_array(&val), vec!["a", "b", "c"]);

        let empty = json!(null);
        assert!(json_string_array(&empty).is_empty());
    }

    #[test]
    fn test_extract_json() {
        assert_eq!(extract_json(r#"{"a":1}"#), r#"{"a":1}"#);
        assert_eq!(
            extract_json("text\n```json\n{\"a\":1}\n```\nmore"),
            r#"{"a":1}"#
        );
        assert_eq!(extract_json("before {\"x\":2} after"), r#"{"x":2}"#);
    }
}
