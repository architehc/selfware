//! Configuration validation logic.

use anyhow::{bail, Result};

use super::api_key::is_local_endpoint;
use super::Config;

impl Config {
    /// Validate configuration values, returning an error for truly invalid
    /// settings and logging warnings for suspicious-but-non-fatal ones.
    pub fn validate(&self) -> Result<()> {
        // --- Endpoint URL validation ---
        // Must start with http:// or https:// and contain a host component.
        if self.endpoint.is_empty() {
            bail!("Config error: endpoint must not be empty");
        }
        if !self.endpoint.starts_with("http://") && !self.endpoint.starts_with("https://") {
            bail!(
                "Config error: endpoint must start with http:// or https://, got: {}",
                self.endpoint
            );
        }
        // Quick structural check: after the scheme there should be a host
        let after_scheme = if self.endpoint.starts_with("https://") {
            &self.endpoint[8..]
        } else {
            &self.endpoint[7..]
        };
        if after_scheme.is_empty() || after_scheme.starts_with('/') {
            bail!("Config error: endpoint URL has no host: {}", self.endpoint);
        }
        // Warn if the endpoint uses plain HTTP to a remote host (unencrypted).
        // Local HTTP is fine — most local LLMs (ollama, vllm, sglang, llama.cpp) serve HTTP.
        if self.endpoint.starts_with("http://") && !is_local_endpoint(&self.endpoint) {
            eprintln!(
                "WARNING: endpoint '{}' uses plain HTTP to a remote host. API keys and data \
                 will be transmitted unencrypted. Consider using https:// instead.",
                self.endpoint
            );
        }

        // --- Model name ---
        if self.model.trim().is_empty() {
            bail!("Config error: model name must not be empty");
        }

        // --- Token limits ---
        if self.max_tokens == 0 {
            bail!("Config error: max_tokens must be greater than 0");
        }
        if self.context_length == 0 {
            bail!("Config error: context_length must be greater than 0");
        }
        const MAX_TOKEN_LIMIT: usize = 10_000_000;
        if self.max_tokens > MAX_TOKEN_LIMIT {
            bail!(
                "Config error: max_tokens ({}) exceeds maximum allowed ({})",
                self.max_tokens,
                MAX_TOKEN_LIMIT
            );
        }

        // --- Temperature ---
        if self.temperature < 0.0 {
            bail!(
                "Config error: temperature must be non-negative, got: {}",
                self.temperature
            );
        }
        if self.temperature > 10.0 {
            eprintln!(
                "Config warning: temperature {} is unusually high (typical range 0.0-2.0)",
                self.temperature
            );
        }

        // --- Agent config ---
        if self.agent.max_iterations == 0 {
            bail!("Config error: agent.max_iterations must be greater than 0");
        }
        if self.agent.step_timeout_secs == 0 {
            bail!("Config error: agent.step_timeout_secs must be greater than 0");
        }
        if self.agent.token_budget == 0 {
            bail!("Config error: agent.token_budget must be greater than 0");
        }
        if self.agent.token_budget > MAX_TOKEN_LIMIT {
            bail!(
                "Config error: agent.token_budget ({}) exceeds maximum allowed ({})",
                self.agent.token_budget,
                MAX_TOKEN_LIMIT
            );
        }
        // Validate token_safety_margin doesn't exceed token_budget
        if self.agent.token_safety_margin >= self.agent.token_budget {
            bail!(
                "Config error: agent.token_safety_margin ({}) must be less than agent.token_budget ({})",
                self.agent.token_safety_margin,
                self.agent.token_budget
            );
        }

        // --- Retry settings: base_delay_ms should not exceed max_delay_ms ---
        if self.retry.base_delay_ms > self.retry.max_delay_ms {
            bail!(
                "Config error: retry.base_delay_ms ({}) must not exceed retry.max_delay_ms ({})",
                self.retry.base_delay_ms,
                self.retry.max_delay_ms
            );
        }

        // --- UI animation speed ---
        if self.ui.animation_speed <= 0.0 {
            bail!(
                "Config error: ui.animation_speed must be positive, got: {}",
                self.ui.animation_speed
            );
        }
        if self.ui.animation_speed > 100.0 {
            eprintln!(
                "Config warning: ui.animation_speed {} is unusually high",
                self.ui.animation_speed
            );
        }

        // --- Warnings for suspicious but non-fatal values ---
        if self.agent.step_timeout_secs > 3600 {
            eprintln!(
                "Config warning: agent.step_timeout_secs ({}) exceeds 1 hour",
                self.agent.step_timeout_secs
            );
        }
        if let Some(ref key) = self.api_key {
            if key.expose().is_empty() {
                eprintln!("Config warning: api_key is set but empty");
            }
        }

        // --- Continuous work recovery settings ---
        if self.continuous_work.max_recovery_attempts > 100 {
            bail!(
                "continuous_work.max_recovery_attempts must be <= 100, got: {}",
                self.continuous_work.max_recovery_attempts
            );
        }

        // --- Continuous work checkpoint settings ---
        if self.continuous_work.checkpoint_interval_tools < 1 {
            bail!(
                "checkpoint_interval_tools must be >= 1, got: {}",
                self.continuous_work.checkpoint_interval_tools
            );
        }

        // --- Concurrency limits ---
        self.concurrency.validate()?;

        // --- Glob pattern validation ---
        // Fail fast on invalid patterns instead of deferring to runtime.
        for (label, patterns) in [
            ("allowed_paths", &self.safety.allowed_paths),
            ("denied_paths", &self.safety.denied_paths),
        ] {
            for pattern in patterns {
                if let Err(e) = glob::Pattern::new(pattern) {
                    bail!("Invalid glob in safety.{}: '{}' — {}", label, pattern, e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AgentConfig, ConcurrencyConfig, ContinuousWorkConfig, RedactedString, RetrySettings,
        SafetyConfig, UiConfig,
    };

    /// Helper: produce a `Config` that is known-valid.
    /// Tests start from this and mutate a single field to exercise edge cases.
    fn valid_config() -> Config {
        Config {
            endpoint: "https://api.example.com/v1".to_string(),
            model: "test-model".to_string(),
            max_tokens: 4096,
            context_length: 8192,
            temperature: 0.7,
            api_key: Some(RedactedString::new("sk-test-key")),
            ..Config::default()
        }
    }

    // ──────────────────────────────────────────────
    // Happy path
    // ──────────────────────────────────────────────

    #[test]
    fn valid_config_passes() {
        let cfg = valid_config();
        assert!(
            cfg.validate().is_ok(),
            "a well-formed config should validate"
        );
    }

    #[test]
    fn default_config_passes() {
        // Config::default() uses default_max_tokens() (65536) for agent.token_budget
        // and default_token_safety_margin() (8192), so it should pass validation.
        let cfg = Config::default();
        let result = cfg.validate();
        assert!(
            result.is_ok(),
            "Config::default() should validate, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn local_http_endpoint_passes() {
        let cfg = Config {
            endpoint: "http://localhost:8080/v1".to_string(),
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn remote_http_endpoint_passes_with_warning() {
        // Remote HTTP only emits a warning; it does not bail.
        let cfg = Config {
            endpoint: "http://api.example.com/v1".to_string(),
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Endpoint validation
    // ──────────────────────────────────────────────

    #[test]
    fn empty_endpoint_fails() {
        let cfg = Config {
            endpoint: String::new(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("endpoint must not be empty"),
            "expected empty-endpoint error, got: {err}"
        );
    }

    #[test]
    fn endpoint_missing_scheme_fails() {
        let cfg = Config {
            endpoint: "api.example.com/v1".to_string(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("must start with http:// or https://"),
            "expected scheme error, got: {err}"
        );
    }

    #[test]
    fn endpoint_no_host_after_scheme_fails() {
        let cfg = Config {
            endpoint: "https://".to_string(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("no host"),
            "expected no-host error, got: {err}"
        );
    }

    #[test]
    fn endpoint_slash_after_scheme_fails() {
        let cfg = Config {
            endpoint: "https:///path".to_string(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("no host"),
            "expected no-host error, got: {err}"
        );
    }

    // ──────────────────────────────────────────────
    // Model name validation
    // ──────────────────────────────────────────────

    #[test]
    fn empty_model_fails() {
        let cfg = Config {
            model: String::new(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("model name must not be empty"),
            "expected empty-model error, got: {err}"
        );
    }

    #[test]
    fn whitespace_only_model_fails() {
        let cfg = Config {
            model: "   \n\t ".to_string(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("model name must not be empty"),
            "expected empty-model error, got: {err}"
        );
    }

    // ──────────────────────────────────────────────
    // Token limits
    // ──────────────────────────────────────────────

    #[test]
    fn max_tokens_zero_fails() {
        let cfg = Config {
            max_tokens: 0,
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_tokens must be greater than 0"),
            "expected max_tokens==0 error, got: {err}"
        );
    }

    #[test]
    fn max_tokens_exceeds_limit_fails() {
        let cfg = Config {
            max_tokens: 10_000_001,
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("exceeds maximum allowed"),
            "expected max_tokens overflow error, got: {err}"
        );
    }

    #[test]
    fn max_tokens_at_limit_passes() {
        let cfg = Config {
            max_tokens: 10_000_000,
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn context_length_zero_fails() {
        let cfg = Config {
            context_length: 0,
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("context_length must be greater than 0"),
            "expected context_length==0 error, got: {err}"
        );
    }

    // ──────────────────────────────────────────────
    // Temperature
    // ──────────────────────────────────────────────

    #[test]
    fn negative_temperature_fails() {
        let cfg = Config {
            temperature: -0.1,
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("temperature must be non-negative"),
            "expected negative-temperature error, got: {err}"
        );
    }

    #[test]
    fn zero_temperature_passes() {
        let cfg = Config {
            temperature: 0.0,
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn very_high_temperature_passes() {
        // >10.0 only warns; it does not bail.
        let cfg = Config {
            temperature: 15.0,
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Agent config
    // ──────────────────────────────────────────────

    #[test]
    fn agent_max_iterations_zero_fails() {
        let cfg = Config {
            agent: AgentConfig {
                max_iterations: 0,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("agent.max_iterations must be greater than 0"),
            "expected max_iterations==0 error, got: {err}"
        );
    }

    #[test]
    fn agent_step_timeout_zero_fails() {
        let cfg = Config {
            agent: AgentConfig {
                step_timeout_secs: 0,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("agent.step_timeout_secs must be greater than 0"),
            "expected step_timeout==0 error, got: {err}"
        );
    }

    #[test]
    fn agent_token_budget_zero_fails() {
        let cfg = Config {
            agent: AgentConfig {
                token_budget: 0,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("agent.token_budget must be greater than 0"),
            "expected token_budget==0 error, got: {err}"
        );
    }

    #[test]
    fn agent_token_budget_exceeds_limit_fails() {
        let cfg = Config {
            agent: AgentConfig {
                token_budget: 10_000_001,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("agent.token_budget") && err.contains("exceeds maximum allowed"),
            "expected token_budget overflow error, got: {err}"
        );
    }

    #[test]
    fn agent_token_budget_at_limit_passes() {
        let cfg = Config {
            agent: AgentConfig {
                token_budget: 10_000_000,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn token_safety_margin_equal_to_budget_fails() {
        let cfg = Config {
            agent: AgentConfig {
                token_budget: 4096,
                token_safety_margin: 4096,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("token_safety_margin") && err.contains("must be less than"),
            "expected safety-margin >= budget error, got: {err}"
        );
    }

    #[test]
    fn token_safety_margin_greater_than_budget_fails() {
        let cfg = Config {
            agent: AgentConfig {
                token_budget: 1000,
                token_safety_margin: 2000,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn token_safety_margin_one_less_than_budget_passes() {
        let cfg = Config {
            agent: AgentConfig {
                token_budget: 4096,
                token_safety_margin: 4095,
                ..AgentConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Retry settings
    // ──────────────────────────────────────────────

    #[test]
    fn retry_base_exceeds_max_fails() {
        let cfg = Config {
            retry: RetrySettings {
                base_delay_ms: 5000,
                max_delay_ms: 1000,
                ..RetrySettings::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("base_delay_ms") && err.contains("must not exceed"),
            "expected base > max retry error, got: {err}"
        );
    }

    #[test]
    fn retry_base_equal_to_max_passes() {
        let cfg = Config {
            retry: RetrySettings {
                base_delay_ms: 1000,
                max_delay_ms: 1000,
                ..RetrySettings::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // UI animation speed
    // ──────────────────────────────────────────────

    #[test]
    fn animation_speed_zero_fails() {
        let cfg = Config {
            ui: UiConfig {
                animation_speed: 0.0,
                ..UiConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("animation_speed must be positive"),
            "expected animation_speed==0 error, got: {err}"
        );
    }

    #[test]
    fn animation_speed_negative_fails() {
        let cfg = Config {
            ui: UiConfig {
                animation_speed: -1.0,
                ..UiConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("animation_speed must be positive"),
            "expected negative animation_speed error, got: {err}"
        );
    }

    #[test]
    fn animation_speed_very_high_passes() {
        // >100.0 only warns; it does not bail.
        let cfg = Config {
            ui: UiConfig {
                animation_speed: 200.0,
                ..UiConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Continuous work
    // ──────────────────────────────────────────────

    #[test]
    fn recovery_attempts_over_100_fails() {
        let cfg = Config {
            continuous_work: ContinuousWorkConfig {
                max_recovery_attempts: 101,
                ..ContinuousWorkConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_recovery_attempts") && err.contains("<= 100"),
            "expected recovery attempts error, got: {err}"
        );
    }

    #[test]
    fn recovery_attempts_at_100_passes() {
        let cfg = Config {
            continuous_work: ContinuousWorkConfig {
                max_recovery_attempts: 100,
                ..ContinuousWorkConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn checkpoint_interval_zero_fails() {
        let cfg = Config {
            continuous_work: ContinuousWorkConfig {
                checkpoint_interval_tools: 0,
                ..ContinuousWorkConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("checkpoint_interval_tools") && err.contains(">= 1"),
            "expected checkpoint interval error, got: {err}"
        );
    }

    #[test]
    fn checkpoint_interval_one_passes() {
        let cfg = Config {
            continuous_work: ContinuousWorkConfig {
                checkpoint_interval_tools: 1,
                ..ContinuousWorkConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Concurrency
    // ──────────────────────────────────────────────

    #[test]
    fn concurrency_max_streams_zero_fails() {
        let cfg = Config {
            concurrency: ConcurrencyConfig {
                max_streams: 0,
                ..ConcurrencyConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_streams") && err.contains(">= 1"),
            "expected max_streams==0 error, got: {err}"
        );
    }

    #[test]
    fn concurrency_max_tools_over_256_fails() {
        let cfg = Config {
            concurrency: ConcurrencyConfig {
                max_tools: 257,
                ..ConcurrencyConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_tools") && err.contains("<= 256"),
            "expected max_tools overflow error, got: {err}"
        );
    }

    #[test]
    fn concurrency_max_global_zero_fails() {
        let cfg = Config {
            concurrency: ConcurrencyConfig {
                max_global: 0,
                ..ConcurrencyConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_global") && err.contains(">= 1"),
            "expected max_global==0 error, got: {err}"
        );
    }

    #[test]
    fn concurrency_at_limits_passes() {
        let cfg = Config {
            concurrency: ConcurrencyConfig {
                max_streams: 1,
                max_tools: 256,
                max_global: 1,
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Glob pattern validation
    // ──────────────────────────────────────────────

    #[test]
    fn invalid_glob_in_allowed_paths_fails() {
        let cfg = Config {
            safety: SafetyConfig {
                allowed_paths: vec!["[unclosed".to_string()],
                ..SafetyConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("Invalid glob") && err.contains("allowed_paths"),
            "expected invalid-glob error for allowed_paths, got: {err}"
        );
    }

    #[test]
    fn invalid_glob_in_denied_paths_fails() {
        let cfg = Config {
            safety: SafetyConfig {
                denied_paths: vec!["[unclosed".to_string()],
                ..SafetyConfig::default()
            },
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("Invalid glob") && err.contains("denied_paths"),
            "expected invalid-glob error for denied_paths, got: {err}"
        );
    }

    #[test]
    fn valid_globs_pass() {
        let cfg = Config {
            safety: SafetyConfig {
                allowed_paths: vec!["./**".to_string(), "src/**/*.rs".to_string()],
                denied_paths: vec!["**/.env".to_string(), "**/secrets/**".to_string()],
                ..SafetyConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn empty_glob_lists_pass() {
        let cfg = Config {
            safety: SafetyConfig {
                allowed_paths: vec![],
                denied_paths: vec![],
                ..SafetyConfig::default()
            },
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // API key edge case
    // ──────────────────────────────────────────────

    #[test]
    fn empty_api_key_passes_with_warning() {
        // An empty api_key only emits a warning; it does not bail.
        let cfg = Config {
            api_key: Some(RedactedString::new("")),
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn no_api_key_passes() {
        let cfg = Config {
            api_key: None,
            ..valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    // ──────────────────────────────────────────────
    // Error message content checks
    // ──────────────────────────────────────────────

    #[test]
    fn error_messages_contain_config_error_prefix() {
        // Most validation errors are prefixed with "Config error:" — verify
        // that the prefix is consistently applied for the main categories.
        let cases: Vec<(Config, &str)> = vec![
            (
                Config {
                    endpoint: String::new(),
                    ..valid_config()
                },
                "endpoint must not be empty",
            ),
            (
                Config {
                    model: String::new(),
                    ..valid_config()
                },
                "model name must not be empty",
            ),
            (
                Config {
                    max_tokens: 0,
                    ..valid_config()
                },
                "max_tokens must be greater than 0",
            ),
            (
                Config {
                    temperature: -1.0,
                    ..valid_config()
                },
                "temperature must be non-negative",
            ),
        ];

        for (cfg, needle) in cases {
            let err = cfg.validate().unwrap_err().to_string();
            assert!(
                err.contains(needle),
                "error should contain '{needle}', got: {err}"
            );
        }
    }

    // ──────────────────────────────────────────────
    // ConcurrencyConfig::validate (called by Config::validate)
    // ──────────────────────────────────────────────

    #[test]
    fn concurrency_validate_boundary_values() {
        // All at minimum (1)
        assert!(ConcurrencyConfig {
            max_streams: 1,
            max_tools: 1,
            max_global: 1,
        }
        .validate()
        .is_ok());

        // All at maximum (256)
        assert!(ConcurrencyConfig {
            max_streams: 256,
            max_tools: 256,
            max_global: 256,
        }
        .validate()
        .is_ok());

        // One below minimum
        assert!(ConcurrencyConfig {
            max_streams: 0,
            max_tools: 1,
            max_global: 1,
        }
        .validate()
        .is_err());

        // One above maximum
        assert!(ConcurrencyConfig {
            max_streams: 1,
            max_tools: 257,
            max_global: 1,
        }
        .validate()
        .is_err());
    }

    // ──────────────────────────────────────────────
    // First-error-wins ordering
    // ──────────────────────────────────────────────

    #[test]
    fn endpoint_error_takes_precedence_over_model_error() {
        // Both endpoint and model are invalid; endpoint is checked first.
        let cfg = Config {
            endpoint: String::new(),
            model: String::new(),
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("endpoint"),
            "endpoint error should be reported first, got: {err}"
        );
        assert!(
            !err.contains("model"),
            "model error should not appear when endpoint fails first, got: {err}"
        );
    }

    #[test]
    fn max_tokens_error_before_context_length() {
        // max_tokens is checked before context_length.
        let cfg = Config {
            max_tokens: 0,
            context_length: 0,
            ..valid_config()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_tokens"),
            "max_tokens error should be reported first, got: {err}"
        );
        assert!(
            !err.contains("context_length"),
            "context_length error should not appear when max_tokens fails first, got: {err}"
        );
    }
}
