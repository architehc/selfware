//! Configuration for the memory consolidation ("sleep") system.

use serde::{Deserialize, Serialize};

/// Configuration for memory consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    /// How often to run consolidation in seconds (default: 3600 = hourly).
    pub interval_secs: u64,
    /// Maximum concurrent LLM summarization streams.
    pub max_concurrent_llm: usize,
    /// Maximum episodes to process per consolidation batch.
    pub batch_size: usize,
    /// Exponential decay half-life in hours (matches Episode::recency_score).
    pub decay_half_life_hours: f64,
    /// Minimum importance level to retain during consolidation.
    pub min_importance: u8,
    /// Similarity threshold for deduplication (0.0–1.0).
    pub dedup_similarity: f32,
    /// Maximum tokens per consolidated summary.
    pub summary_max_tokens: usize,
    /// LLM endpoint for summarization calls.
    pub endpoint: String,
    /// Model for summarization.
    pub model: String,
    /// Temperature for summarization (low for deterministic).
    pub temperature: f32,
    /// Timeout for each LLM call in seconds.
    pub timeout_secs: u64,
    /// Maximum age in hours — episodes older than this get consolidated first.
    pub max_episode_age_hours: f64,
    /// Whether to prune source episodes after consolidation.
    pub prune_after_consolidation: bool,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            max_concurrent_llm: 32,
            batch_size: 100,
            decay_half_life_hours: 24.0,
            min_importance: 1, // Importance::Low
            dedup_similarity: 0.95,
            summary_max_tokens: 1024,
            endpoint: "http://localhost:8000/v1".into(),
            model: "qwen3.5-27b".into(),
            temperature: 0.1,
            timeout_secs: 120,
            max_episode_age_hours: 72.0,
            prune_after_consolidation: false,
        }
    }
}

impl ConsolidationConfig {
    /// Create a config with a specific endpoint and model.
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set concurrency for LLM calls.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrent_llm = n.max(1);
        self
    }

    /// Validate configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.is_empty() {
            return Err("endpoint must not be empty".into());
        }
        if self.model.is_empty() {
            return Err("model must not be empty".into());
        }
        if self.max_concurrent_llm == 0 {
            return Err("max_concurrent_llm must be >= 1".into());
        }
        if self.batch_size == 0 {
            return Err("batch_size must be >= 1".into());
        }
        if self.decay_half_life_hours <= 0.0 {
            return Err("decay_half_life_hours must be positive".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = ConsolidationConfig::default();
        assert_eq!(cfg.interval_secs, 3600);
        assert_eq!(cfg.max_concurrent_llm, 32);
        assert_eq!(cfg.batch_size, 100);
        assert!((cfg.decay_half_life_hours - 24.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_ok() {
        assert!(ConsolidationConfig::default().validate().is_ok());
    }

    #[test]
    fn test_validate_empty_endpoint() {
        let cfg = ConsolidationConfig {
            endpoint: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_serde_roundtrip() {
        let cfg = ConsolidationConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: ConsolidationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.endpoint, cfg.endpoint);
        assert_eq!(parsed.max_concurrent_llm, cfg.max_concurrent_llm);
    }
}
