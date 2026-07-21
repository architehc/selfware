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
#[path = "../../tests/unit/vlm_bench/config/config_test.rs"]
mod tests;
