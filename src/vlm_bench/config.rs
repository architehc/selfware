//! Configuration for the VLM benchmark suite.

use super::Difficulty;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for a VLM benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmBenchConfig {
    /// VLM API endpoint (OpenAI-compatible).
    pub endpoint: String,
    /// Model name to request.
    pub model: String,
    /// Maximum concurrent predictions (bounded by hardware).
    pub max_concurrent: usize,
    /// Maximum tokens per response.
    pub max_tokens: usize,
    /// Sampling temperature (low for deterministic evaluation).
    pub temperature: f32,
    /// Timeout per request in seconds.
    pub timeout_secs: u64,
    /// Which difficulty levels to run.
    pub levels: Vec<Difficulty>,
    /// Directory containing fixture images.
    pub fixtures_dir: PathBuf,
    /// Directory for output reports.
    pub output_dir: PathBuf,
}

impl Default for VlmBenchConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://192.168.1.99:1234/v1".into(),
            model: "qwen/qwen3.5-9b".into(),
            max_concurrent: 4,
            max_tokens: 4096,
            temperature: 0.2,
            timeout_secs: 120,
            levels: vec![
                Difficulty::Easy,
                Difficulty::Medium,
                Difficulty::Hard,
                Difficulty::VeryHard,
                Difficulty::Extreme,
                Difficulty::Mega,
            ],
            fixtures_dir: PathBuf::from("vlm_fixtures"),
            output_dir: PathBuf::from("vlm_results"),
        }
    }
}

impl VlmBenchConfig {
    /// Create a config targeting a specific endpoint and model.
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    /// Only run levels at or below the given difficulty.
    pub fn with_max_difficulty(mut self, max: Difficulty) -> Self {
        self.levels.retain(|d| *d <= max);
        self
    }

    /// Set concurrency limit.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrent = n.max(1);
        self
    }

    /// Validate that the configuration is sane.
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.is_empty() {
            return Err("endpoint must not be empty".into());
        }
        if self.model.is_empty() {
            return Err("model must not be empty".into());
        }
        if self.max_concurrent == 0 {
            return Err("max_concurrent must be >= 1".into());
        }
        if self.max_tokens == 0 {
            return Err("max_tokens must be >= 1".into());
        }
        if self.timeout_secs == 0 {
            return Err("timeout_secs must be >= 1".into());
        }
        if self.levels.is_empty() {
            return Err("at least one difficulty level must be selected".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = VlmBenchConfig::default();
        assert_eq!(cfg.endpoint, "http://192.168.1.99:1234/v1");
        assert_eq!(cfg.model, "qwen/qwen3.5-9b");
        assert_eq!(cfg.max_concurrent, 4);
        assert_eq!(cfg.max_tokens, 4096);
        assert!((cfg.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(cfg.timeout_secs, 120);
        assert_eq!(cfg.levels.len(), 6);
    }

    #[test]
    fn test_config_new() {
        let cfg = VlmBenchConfig::new("http://localhost:8080/v1", "test-model");
        assert_eq!(cfg.endpoint, "http://localhost:8080/v1");
        assert_eq!(cfg.model, "test-model");
        assert_eq!(cfg.max_concurrent, 4); // default
    }

    #[test]
    fn test_with_max_difficulty() {
        let cfg = VlmBenchConfig::default().with_max_difficulty(Difficulty::Medium);
        assert_eq!(cfg.levels.len(), 2);
        assert!(cfg.levels.contains(&Difficulty::Easy));
        assert!(cfg.levels.contains(&Difficulty::Medium));
        assert!(!cfg.levels.contains(&Difficulty::Hard));
    }

    #[test]
    fn test_with_concurrency() {
        let cfg = VlmBenchConfig::default().with_concurrency(8);
        assert_eq!(cfg.max_concurrent, 8);

        let cfg = VlmBenchConfig::default().with_concurrency(0);
        assert_eq!(cfg.max_concurrent, 1); // clamped
    }

    #[test]
    fn test_validate_ok() {
        assert!(VlmBenchConfig::default().validate().is_ok());
    }

    #[test]
    fn test_validate_empty_endpoint() {
        let mut cfg = VlmBenchConfig::default();
        cfg.endpoint = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_model() {
        let mut cfg = VlmBenchConfig::default();
        cfg.model = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_zero_concurrent() {
        let mut cfg = VlmBenchConfig::default();
        cfg.max_concurrent = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_levels() {
        let mut cfg = VlmBenchConfig::default();
        cfg.levels.clear();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = VlmBenchConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: VlmBenchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.endpoint, cfg.endpoint);
        assert_eq!(parsed.model, cfg.model);
        assert_eq!(parsed.levels.len(), cfg.levels.len());
    }
}
