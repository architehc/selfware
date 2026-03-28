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
        if self.continuous_work.max_recovery_attempts > 10 {
            bail!(
                "continuous_work.max_recovery_attempts must be <= 10, got: {}",
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
