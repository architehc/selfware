use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Result};

use super::api_key::is_local_endpoint;
use super::Config;

static SGLANG_CAPABILITY_CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

fn sglang_cache() -> &'static Mutex<HashMap<String, bool>> {
    SGLANG_CAPABILITY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Query the capability cache for an endpoint.
pub fn get_sglang_capability(endpoint: &str) -> Option<bool> {
    let base = normalize_endpoint_base(endpoint);
    sglang_cache().lock().ok()?.get(&base).copied()
}

/// Explicitly cache the SGLang capability for an endpoint.
pub fn set_sglang_capability(endpoint: &str, is_sglang: bool) {
    let base = normalize_endpoint_base(endpoint);
    if let Ok(mut cache) = sglang_cache().lock() {
        cache.insert(base, is_sglang);
    }
}

/// Clear the capability cache for testing or runtime reset.
pub fn clear_sglang_capability_cache() {
    if let Ok(mut cache) = sglang_cache().lock() {
        cache.clear();
    }
}

/// Normalize an endpoint URL to its base host/root path (stripping /v1 and trailing slashes).
pub fn normalize_endpoint_base(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

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
        // A run-level USD cost cap, when set, must be a positive finite amount —
        // a NaN/infinite/zero/negative cap would silently disable enforcement or
        // abort immediately.
        if let Some(max_cost) = self.agent.max_cost_usd {
            if !max_cost.is_finite() {
                bail!(
                    "Config error: agent.max_cost_usd must be a finite number, got: {}",
                    max_cost
                );
            }
            if max_cost <= 0.0 {
                bail!(
                    "Config error: agent.max_cost_usd must be greater than 0 when set, got: {}",
                    max_cost
                );
            }
        }

        // --- Retry settings: base_delay_ms should not exceed max_delay_ms ---
        if self.retry.base_delay_ms > self.retry.max_delay_ms {
            bail!(
                "Config error: retry.base_delay_ms ({}) must not exceed retry.max_delay_ms ({})",
                self.retry.base_delay_ms,
                self.retry.max_delay_ms
            );
        }

        // --- Sentinel values that crash or disable the API client ---
        // u64::MAX panics on `Instant + Duration` at the first billable
        // request; anything past 30 days is a misconfiguration, not a budget.
        const MAX_WALL_SECS_LIMIT: u64 = 30 * 24 * 60 * 60;
        if let Some(max_wall) = self.agent.max_wall_secs {
            if max_wall > MAX_WALL_SECS_LIMIT {
                bail!(
                    "Config error: agent.max_wall_secs ({}) exceeds maximum allowed ({} = 30 days)",
                    max_wall,
                    MAX_WALL_SECS_LIMIT
                );
            }
        }
        // A zero stall timeout times out every stream on the first chunk
        // wait — every streamed request fails and the retry loop re-bills it.
        if let Some(stall) = self.agent.stream_stall_timeout_secs {
            if stall == 0 {
                bail!(
                    "Config error: agent.stream_stall_timeout_secs must be greater than 0 when set \
                     (0 would time out every stream immediately)"
                );
            }
        }
        // u32::MAX overflows `max_retries + 1` (debug panic; release wraps to
        // zero attempts). Past 100 the exponential backoff is meaningless.
        const MAX_RETRIES_LIMIT: u32 = 100;
        if self.retry.max_retries > MAX_RETRIES_LIMIT {
            bail!(
                "Config error: retry.max_retries ({}) exceeds maximum allowed ({})",
                self.retry.max_retries,
                MAX_RETRIES_LIMIT
            );
        }
        // Same overflow class through the per-profile override, which feeds
        // the same `max_retries + 1` arithmetic on both chat paths.
        for (name, profile) in &self.models {
            if let Some(retries) = profile.max_retries {
                if retries > MAX_RETRIES_LIMIT {
                    bail!(
                        "Config error: models.{}.max_retries ({}) exceeds maximum allowed ({})",
                        name,
                        retries,
                        MAX_RETRIES_LIMIT
                    );
                }
            }
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

        // --- extra_body sampling parameters ---
        // Catch out-of-range sampling values here rather than letting the
        // provider reject them mid-run. top_p/top_k/min_p are probabilities.
        if let Some(extra) = &self.extra_body {
            for key in ["top_p", "min_p"] {
                if let Some(v) = extra.get(key).and_then(|v| v.as_f64()) {
                    if !(0.0..=1.0).contains(&v) {
                        bail!(
                            "Config error: extra_body.{} must be in [0.0, 1.0], got: {}",
                            key,
                            v
                        );
                    }
                }
            }
            if let Some(v) = extra.get("top_k").and_then(|v| v.as_i64()) {
                if v < 0 {
                    bail!("Config error: extra_body.top_k must be >= 0, got: {}", v);
                }
            }
        }

        // Reasoning effort validation:
        // Top-level extra_body reasoning_effort
        if let Some(extra) = &self.extra_body {
            if let Some(val_raw) = extra.get("reasoning_effort") {
                let Some(val) = val_raw.as_str() else {
                    bail!(
                        "Config error: extra_body.reasoning_effort must be a string, got: {}",
                        val_raw
                    );
                };
                let val_lower = val.to_ascii_lowercase();
                if !matches!(val_lower.as_str(), "low" | "medium" | "high" | "xhigh") {
                    bail!(
                        "Config error: extra_body.reasoning_effort must be one of 'low', 'medium', 'high', 'xhigh', got: '{}'",
                        val
                    );
                }

                // 1. Unconditional check for Qwen: 'high' is refused by the Qwen chat template on any serving stack
                if self.model.to_ascii_lowercase().contains("qwen") && val_lower == "high" {
                    bail!(
                        "Config error: extra_body.reasoning_effort cannot be 'high' for Qwen models. \
                         The Qwen chat template refuses 'high' on all serving stacks (accepted values: 'low', 'medium', or default/xhigh via chat_template_kwargs)."
                    );
                }

                // 2. Behavioral SGLang check: 'xhigh' is rejected by SGLang schema at top-level
                if val_lower == "xhigh" && is_sglang_backend(&self.endpoint) {
                    bail!(
                        "Config error: extra_body.reasoning_effort cannot be 'xhigh' at top-level on SGLang serving deployments. \
                         Top-level reasoning_effort only accepts 'low' or 'medium' ('xhigh' is rejected by the endpoint schema). \
                         For xhigh reasoning, place it under [extra_body.chat_template_kwargs.reasoning_effort] \
                         or omit the field (default is xhigh)."
                    );
                }
            }
        }

        // Validate model profile extra_body reasoning_effort
        for (name, profile) in &self.models {
            if let Some(extra) = &profile.extra_body {
                if let Some(val_raw) = extra.get("reasoning_effort") {
                    let Some(val) = val_raw.as_str() else {
                        bail!(
                            "Config error: models.{}.extra_body.reasoning_effort must be a string, got: {}",
                            name,
                            val_raw
                        );
                    };
                    let val_lower = val.to_ascii_lowercase();
                    if !matches!(val_lower.as_str(), "low" | "medium" | "high" | "xhigh") {
                        bail!(
                            "Config error: models.{}.extra_body.reasoning_effort must be one of 'low', 'medium', 'high', 'xhigh', got: '{}'",
                            name,
                            val
                        );
                    }

                    let is_qwen_profile = profile.model.to_ascii_lowercase().contains("qwen");
                    if is_qwen_profile && val_lower == "high" {
                        bail!(
                            "Config error: models.{}.extra_body.reasoning_effort cannot be 'high' for Qwen models. \
                             The Qwen chat template refuses 'high' on all serving stacks (accepted values: 'low', 'medium', or default/xhigh via chat_template_kwargs).",
                            name
                        );
                    }

                    if val_lower == "xhigh" && is_sglang_backend(&profile.endpoint) {
                        bail!(
                            "Config error: models.{}.extra_body.reasoning_effort cannot be 'xhigh' at top-level on SGLang serving deployments. \
                             Top-level reasoning_effort only accepts 'low' or 'medium' ('xhigh' is rejected by the endpoint schema). \
                             For xhigh reasoning, place it under [models.{}.extra_body.chat_template_kwargs.reasoning_effort] \
                             or omit the field (default is xhigh).",
                            name, name
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Bounded async discovery of backend capabilities for all endpoints
    /// where `reasoning_effort == "xhigh"`.
    pub async fn discover_sglang_capabilities(&self) {
        let mut endpoints_to_probe = Vec::new();

        if let Some(extra) = &self.extra_body {
            if let Some(val) = extra.get("reasoning_effort").and_then(|v| v.as_str()) {
                if val.eq_ignore_ascii_case("xhigh") {
                    endpoints_to_probe.push(&self.endpoint);
                }
            }
        }

        for profile in self.models.values() {
            if let Some(extra) = &profile.extra_body {
                if let Some(val) = extra.get("reasoning_effort").and_then(|v| v.as_str()) {
                    if val.eq_ignore_ascii_case("xhigh") {
                        endpoints_to_probe.push(&profile.endpoint);
                    }
                }
            }
        }

        endpoints_to_probe.sort();
        endpoints_to_probe.dedup();

        for ep in endpoints_to_probe {
            probe_sglang_backend_async(ep).await;
        }
    }

    /// Async validation step: runs bounded capability discovery before validating invariants.
    pub async fn validate_async(&self) -> Result<()> {
        self.discover_sglang_capabilities().await;
        self.validate()
    }
}

/// Validates whether the response body from /get_server_info corresponds to an SGLang deployment.
/// Requires backend-specific evidence (e.g. `sglang_version`, `tool_call_parser`, `reasoning_parser`,
/// or explicit mention of `sglang` in the payload) to prevent misclassifying generic servers or models.
pub fn is_sglang_server_info_body(body: &str) -> bool {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(obj) = json.as_object() {
            // 1. SGLang-specific fields
            if obj.contains_key("sglang_version")
                || obj.contains_key("tool_call_parser")
                || obj.contains_key("reasoning_parser")
            {
                return true;
            }

            // 2. Version string explicitly mentioning sglang
            if let Some(ver) = obj.get("version").and_then(|v| v.as_str()) {
                if ver.to_ascii_lowercase().contains("sglang") {
                    return true;
                }
            }

            // 3. Any key or string value explicitly mentioning sglang
            let lower_body = body.to_ascii_lowercase();
            if lower_body.contains("sglang") {
                return true;
            }
        }
    }
    false
}

/// Bounded async discovery for SGLang backend via `/get_server_info`.
/// Probes both HTTP and HTTPS, validates the response body, and caches the result.
/// Transient failures (timeouts, connection errors, server errors, auth failures)
/// preserve an unknown state and are NOT cached as negative results.
pub async fn probe_sglang_backend_async(endpoint: &str) -> bool {
    let base = normalize_endpoint_base(endpoint);

    // 1. Check cache
    if let Some(cached) = get_sglang_capability(&base) {
        return cached;
    }

    // 2. Fast hostname heuristic
    if is_sglang_serving_deployment(&base) {
        set_sglang_capability(&base, true);
        return true;
    }

    // 3. Bounded HTTP probe
    let url = format!("{}/get_server_info", base);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .connect_timeout(std::time::Duration::from_millis(300))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                if let Ok(body) = resp.text().await {
                    let is_sg = is_sglang_server_info_body(&body);
                    set_sglang_capability(&base, is_sg);
                    is_sg
                } else {
                    // Failed to read body - transient error, do not cache
                    false
                }
            } else if status.as_u16() == 404 {
                // Route does not exist on this server - conclusive non-SGLang
                set_sglang_capability(&base, false);
                false
            } else {
                // Transient status (401/403 auth, 408 timeout, 5xx server error) - do not cache
                false
            }
        }
        Err(_) => {
            // Connection refused, timeout, DNS resolution error - transient failure, do not cache
            false
        }
    }
}

fn probe_sglang_backend_blocking_direct(base: &str) -> (bool, bool) {
    let url = format!("{}/get_server_info", base);
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .connect_timeout(std::time::Duration::from_millis(300))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (false, false),
    };

    match client.get(&url).send() {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                if let Ok(body) = resp.text() {
                    let is_sg = is_sglang_server_info_body(&body);
                    (is_sg, true)
                } else {
                    (false, false)
                }
            } else if status.as_u16() == 404 {
                (false, true)
            } else {
                (false, false)
            }
        }
        Err(_) => (false, false),
    }
}

/// Detects if an endpoint is an SGLang deployment behaviourally via `/get_server_info`,
/// falling back to hostname/port heuristics when offline or when probing cannot be performed.
/// Transient failures preserve an unknown state and are not cached as negative results.
pub fn is_sglang_backend(endpoint: &str) -> bool {
    let base = normalize_endpoint_base(endpoint);

    // 1. Cached capability
    if let Some(cached) = get_sglang_capability(&base) {
        return cached;
    }

    // 2. Fast hostname / known deployment heuristic
    if is_sglang_serving_deployment(&base) {
        set_sglang_capability(&base, true);
        return true;
    }

    // 3. Blocking probe (executed on an OS thread if inside Tokio to avoid blocking Tokio workers)
    let (is_sg, _is_conclusive) = if tokio::runtime::Handle::try_current().is_ok() {
        let base_clone = base.clone();
        std::thread::spawn(move || probe_sglang_backend_blocking_direct(&base_clone))
            .join()
            .unwrap_or((false, false))
    } else {
        probe_sglang_backend_blocking_direct(&base)
    };

    // Always cache the probe result (negative caching on failure/timeout)
    // so subsequent requests on the hot path do not repeatedly incur a blocking network probe.
    set_sglang_capability(&base, is_sg);

    is_sg
}

/// Returns true if the endpoint URL indicates an SGLang serving deployment
/// where OpenAI schema enforcement and SGLang chat templates diverge on top-level `reasoning_effort`.
pub fn is_sglang_serving_deployment(endpoint: &str) -> bool {
    let lower = endpoint.to_ascii_lowercase();
    lower.contains("sglang") || lower.contains("selfware.design") || lower.contains(":30000")
}

#[cfg(test)]
#[path = "../../tests/unit/config/validation/validation_test.rs"]
mod tests;
