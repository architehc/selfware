//! Configuration for the concurrent benchmark harness.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Configuration for a benchmark harness run.
///
/// `Debug` is implemented by hand (not derived) so the `api_key` never prints,
/// and `api_key` is `skip_serializing` so it never lands in an on-disk report.
#[derive(Clone, Serialize, Deserialize)]
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
    /// Maximum retry attempts for transient failures.
    #[serde(default)]
    pub max_retries: u32,
    /// Initial delay between retries in milliseconds.
    #[serde(default)]
    pub retry_delay_ms: u64,
    /// Directory for output reports.
    pub output_dir: PathBuf,
    /// Extra body parameters for the API request (e.g., chat_template_kwargs).
    #[serde(default)]
    pub extra_body: serde_json::Value,
    /// Optional API key. When set, requests send `Authorization: Bearer <key>`.
    /// Resolved from the main Config (keyring / SELFWARE_API_KEY / config file).
    /// `skip_serializing`: an in-memory secret that must never be written to a
    /// report/plan artifact. Still deserializes (default `None` when absent).
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
}

impl fmt::Debug for HarnessConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HarnessConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("max_concurrent", &self.max_concurrent)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("timeout_secs", &self.timeout_secs)
            .field("max_retries", &self.max_retries)
            .field("retry_delay_ms", &self.retry_delay_ms)
            .field("output_dir", &self.output_dir)
            .field("extra_body", &self.extra_body)
            // Never print the secret — only whether one is set.
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:1234/v1".into(),
            model: "qwen3.5-27b".into(),
            max_concurrent: 32,
            max_tokens: 65536,
            temperature: 0.2,
            timeout_secs: 300,
            max_retries: 3,
            retry_delay_ms: 500,
            output_dir: PathBuf::from("bench_results"),
            extra_body: serde_json::json!({
                "chat_template_kwargs": {"enable_thinking": false}
            }),
            api_key: None,
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
        assert_eq!(cfg.endpoint, "http://127.0.0.1:1234/v1");
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
        let cfg = HarnessConfig {
            endpoint: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_model() {
        let cfg = HarnessConfig {
            model: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn api_key_is_never_serialized_or_debug_printed() {
        assert!(HarnessConfig::default().api_key.is_none());
        let cfg = HarnessConfig {
            api_key: Some("sk-super-secret".into()),
            ..Default::default()
        };
        // Security: the key must NOT appear in the serialized report artifact.
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("sk-super-secret"),
            "api_key leaked into serialized HarnessConfig: {json}"
        );
        assert!(!json.contains("api_key"), "api_key field must be skipped");
        // Security: the key must NOT appear in Debug output either.
        let dbg = format!("{cfg:?}");
        assert!(
            !dbg.contains("sk-super-secret"),
            "api_key leaked into Debug: {dbg}"
        );
        assert!(dbg.contains("<redacted>"), "Debug should mark a set key");
        // Deserializing a report (no api_key field) yields None, not an error.
        let back: HarnessConfig = serde_json::from_str(&json).unwrap();
        assert!(back.api_key.is_none());
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
