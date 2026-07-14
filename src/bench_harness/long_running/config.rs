//! Configuration for long-running system tests.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for a long-running system test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongRunningConfig {
    /// OpenAI-compatible API endpoint.
    pub endpoint: String,
    /// Model name to request.
    pub model: String,
    /// Maximum duration for the entire test run.
    pub max_duration: Duration,
    /// Timeout per project in seconds.
    pub timeout_per_project_secs: u64,
    /// Maximum iterations per project.
    pub max_iterations: usize,
    /// Directory containing project templates.
    pub templates_dir: PathBuf,
    /// Directory for output results.
    pub output_dir: PathBuf,
    /// Maximum concurrent projects.
    pub max_concurrent: usize,
    /// Maximum tokens per response.
    pub max_tokens: usize,
    /// Sampling temperature.
    pub temperature: f32,
    /// Context length for the model.
    pub context_length: usize,
    /// Extra body parameters for API requests.
    #[serde(default)]
    pub extra_body: serde_json::Value,
    /// Whether to enable YOLO mode (auto-approve).
    pub yolo_mode: bool,
    /// Whether to require verification before completion.
    pub require_verification: bool,
}

impl Default for LongRunningConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:1234/v1".into(),
            model: "qwen3.5-27b".into(),
            max_duration: Duration::from_secs(8 * 3600), // 8 hours
            timeout_per_project_secs: 900,               // 15 minutes
            max_iterations: 80,
            templates_dir: PathBuf::from("system_tests/projecte2e/templates"),
            output_dir: PathBuf::from("long_run_results"),
            max_concurrent: 1, // Sequential by default for system tests
            max_tokens: 16384,
            temperature: 0.6,
            context_length: 262144,
            extra_body: serde_json::json!({
                "chat_template_kwargs": {"enable_thinking": false}
            }),
            yolo_mode: true,
            require_verification: true,
        }
    }
}

impl LongRunningConfig {
    /// Create a config targeting a specific endpoint and model.
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set the maximum duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.max_duration = duration;
        self
    }

    /// Set the maximum number of concurrent projects.
    pub fn with_max_concurrent(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent = max_concurrent.max(1);
        self
    }

    /// Set timeout per project.
    pub fn with_project_timeout(mut self, secs: u64) -> Self {
        self.timeout_per_project_secs = secs;
        self
    }

    /// Set max iterations per project.
    pub fn with_max_iterations(mut self, iters: usize) -> Self {
        self.max_iterations = iters;
        self
    }

    /// Set templates directory.
    pub fn with_templates_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.templates_dir = dir.into();
        self
    }

    /// Set output directory.
    pub fn with_output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = dir.into();
        self
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.is_empty() {
            return Err("endpoint must not be empty".into());
        }
        if self.model.is_empty() {
            return Err("model must not be empty".into());
        }
        if self.timeout_per_project_secs == 0 {
            return Err("timeout_per_project_secs must be >= 1".into());
        }
        if self.max_iterations == 0 {
            return Err("max_iterations must be >= 1".into());
        }
        if !self.templates_dir.exists() {
            return Err(format!(
                "templates_dir does not exist: {}",
                self.templates_dir.display()
            ));
        }
        Ok(())
    }

    /// Generate a selfware.toml config string for this configuration.
    pub fn to_selfware_toml(&self) -> String {
        format!(
            r#"endpoint = "{endpoint}"
model = "{model}"
max_tokens = {max_tokens}
context_length = {context_length}
temperature = {temperature}

[safety]
allowed_paths = ["./**", "/tmp/**"]

[agent]
max_iterations = {max_iterations}
step_timeout_secs = {step_timeout}
native_function_calling = false
streaming = true
min_completion_steps = 5
require_verification_before_completion = {require_verification}

[continuous_work]
enabled = true
checkpoint_interval_tools = 5
auto_recovery = true
max_recovery_attempts = 3

[retry]
max_retries = 3
base_delay_ms = 2000
max_delay_ms = 30000

{extra_body}"#,
            endpoint = self.endpoint,
            model = self.model,
            max_tokens = self.max_tokens,
            context_length = self.context_length,
            temperature = self.temperature,
            max_iterations = self.max_iterations,
            step_timeout = self.timeout_per_project_secs,
            require_verification = self.require_verification,
            extra_body = self.extra_body_toml_section(),
        )
    }

    /// Serialize `extra_body` as a valid TOML `[extra_body]` section.
    ///
    /// The previous implementation dumped serde_json's pretty-printed JSON
    /// under a hand-written `[extra_body]` header, which is NOT valid TOML — so
    /// every spawned child died at config parse and long-test was broken out of
    /// the box. Serialize through the `toml` crate instead (emitted last so its
    /// nested sub-tables cannot bleed into a following section). Returns an
    /// empty string when there is nothing to emit.
    fn extra_body_toml_section(&self) -> String {
        let is_empty = self
            .extra_body
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(true);
        if is_empty {
            return String::new();
        }
        let mut wrapper = serde_json::Map::new();
        wrapper.insert("extra_body".to_string(), self.extra_body.clone());
        toml::to_string(&serde_json::Value::Object(wrapper)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = LongRunningConfig::default();
        assert_eq!(cfg.endpoint, "http://127.0.0.1:1234/v1");
        assert_eq!(cfg.model, "qwen3.5-27b");
        assert_eq!(cfg.max_iterations, 80);
        assert_eq!(cfg.timeout_per_project_secs, 900);
    }

    #[test]
    fn test_config_new() {
        let cfg = LongRunningConfig::new("http://example.com/v1", "test-model");
        assert_eq!(cfg.endpoint, "http://example.com/v1");
        assert_eq!(cfg.model, "test-model");
    }

    #[test]
    fn to_selfware_toml_is_valid_and_round_trips_extra_body() {
        // Regression: the default extra_body was emitted as pretty JSON under a
        // [extra_body] header — invalid TOML, so every spawned child died at
        // config parse and long-test was broken out of the box.
        let cfg = LongRunningConfig::default();
        let toml_str = cfg.to_selfware_toml();

        // Must now parse as valid TOML.
        let parsed: toml::Value = toml::from_str(&toml_str)
            .unwrap_or_else(|e| panic!("generated config is not valid TOML: {e}\n---\n{toml_str}"));

        // extra_body round-trips as a nested table.
        let enable_thinking = parsed
            .get("extra_body")
            .and_then(|v| v.get("chat_template_kwargs"))
            .and_then(|v| v.get("enable_thinking"))
            .and_then(|v| v.as_bool());
        assert_eq!(
            enable_thinking,
            Some(false),
            "extra_body nesting lost:\n{toml_str}"
        );

        // And selfware's own config loader accepts the generated config.
        let loaded: std::result::Result<crate::config::Config, _> = toml::from_str(&toml_str);
        assert!(
            loaded.is_ok(),
            "selfware rejected the generated config: {:?}\n---\n{toml_str}",
            loaded.err()
        );
    }

    #[test]
    fn to_selfware_toml_handles_empty_extra_body() {
        let cfg = LongRunningConfig {
            extra_body: serde_json::json!({}),
            ..Default::default()
        };
        let toml_str = cfg.to_selfware_toml();
        let parsed: std::result::Result<toml::Value, _> = toml::from_str(&toml_str);
        assert!(parsed.is_ok(), "empty extra_body must still be valid TOML");
    }

    #[test]
    fn test_validate_empty_endpoint() {
        let cfg = LongRunningConfig {
            endpoint: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_toml_generation() {
        let cfg = LongRunningConfig::default();
        let toml = cfg.to_selfware_toml();
        assert!(toml.contains("endpoint = "));
        assert!(toml.contains("model = "));
        assert!(toml.contains("max_iterations = 80"));
    }
}
