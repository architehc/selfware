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
#[path = "../../tests/unit/bench_harness/config/config_test.rs"]
mod tests;
