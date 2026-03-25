//! Configuration for the concurrent benchmark harness.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for a benchmark harness run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessConfig {
    /// OpenAI-compatible API endpoint.
    pub endpoint: String,
    /// Model name to request.
    pub model: String,
    /// Maximum concurrent inference streams.
    pub max_concurrent: usize,
    /// Maximum tokens per response.
    pub max_tokens: usize,
    /// Sampling temperature.
    pub temperature: f32,
    /// Timeout per request in seconds.
    pub timeout_secs: u64,
    /// Directory for output reports.
    pub output_dir: PathBuf,
    /// Extra body parameters for the API request (e.g., chat_template_kwargs).
    #[serde(default)]
    pub extra_body: serde_json::Value,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8000/v1".into(),
            model: "qwen3.5-27b".into(),
            max_concurrent: 32,
            max_tokens: 65536,
            temperature: 0.2,
            timeout_secs: 300,
            output_dir: PathBuf::from("bench_results"),
            extra_body: serde_json::json!({
                "chat_template_kwargs": {"enable_thinking": false}
            }),
        }
    }
}

impl HarnessConfig {
    /// Create a config targeting a specific endpoint and model.
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set concurrency limit.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrent = n.max(1);
        self
    }

    /// Set max tokens per response.
    pub fn with_max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = n.max(1);
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = HarnessConfig::default();
        assert_eq!(cfg.endpoint, "http://localhost:8000/v1");
        assert_eq!(cfg.model, "qwen3.5-27b");
        assert_eq!(cfg.max_concurrent, 32);
        assert_eq!(cfg.max_tokens, 65536);
        assert!((cfg.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(cfg.timeout_secs, 300);
    }

    #[test]
    fn test_config_new() {
        let cfg = HarnessConfig::new("http://example.com/v1", "test-model");
        assert_eq!(cfg.endpoint, "http://example.com/v1");
        assert_eq!(cfg.model, "test-model");
        assert_eq!(cfg.max_concurrent, 32);
    }

    #[test]
    fn test_with_concurrency() {
        let cfg = HarnessConfig::default().with_concurrency(8);
        assert_eq!(cfg.max_concurrent, 8);

        let cfg = HarnessConfig::default().with_concurrency(0);
        assert_eq!(cfg.max_concurrent, 1);
    }

    #[test]
    fn test_validate_ok() {
        assert!(HarnessConfig::default().validate().is_ok());
    }

    #[test]
    fn test_validate_empty_endpoint() {
        let mut cfg = HarnessConfig::default();
        cfg.endpoint = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_model() {
        let mut cfg = HarnessConfig::default();
        cfg.model = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = HarnessConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: HarnessConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.endpoint, cfg.endpoint);
        assert_eq!(parsed.model, cfg.model);
        assert_eq!(parsed.max_concurrent, cfg.max_concurrent);
    }
}
