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
fn max_cost_usd_rejects_nonpositive_and_nonfinite() {
    for bad in [0.0_f64, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut cfg = valid_config();
        cfg.agent.max_cost_usd = Some(bad);
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("max_cost_usd"),
            "max_cost_usd={bad} must be rejected, got: {err}"
        );
    }
}

#[test]
fn max_cost_usd_accepts_positive_finite_and_none() {
    let mut cfg = valid_config();
    cfg.agent.max_cost_usd = Some(0.25);
    assert!(cfg.validate().is_ok(), "a positive cap should validate");

    let mut cfg = valid_config();
    cfg.agent.max_cost_usd = None; // unset is fine (cap disabled)
    assert!(cfg.validate().is_ok(), "no cap should validate");
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

// ──────────────────────────────────────────────
// Sentinel values that crash or disable the API client (review finding #9)
// ──────────────────────────────────────────────

#[test]
fn max_wall_secs_rejects_sentinel_overflow() {
    // u64::MAX panics on `Instant + Duration` at the first billable request.
    let mut cfg = valid_config();
    cfg.agent.max_wall_secs = Some(u64::MAX);
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("max_wall_secs"),
        "u64::MAX must be rejected, got: {err}"
    );

    // Sane budgets still pass (None = disabled, 1h = ordinary).
    let mut cfg = valid_config();
    cfg.agent.max_wall_secs = None;
    assert!(cfg.validate().is_ok(), "no budget should validate");
    let mut cfg = valid_config();
    cfg.agent.max_wall_secs = Some(3600);
    assert!(cfg.validate().is_ok(), "a 1h budget should validate");
}

#[test]
fn stream_stall_timeout_rejects_zero() {
    // 0 would time out every stream on the first chunk wait — every
    // streamed request fails and the retry loop re-bills it.
    let mut cfg = valid_config();
    cfg.agent.stream_stall_timeout_secs = Some(0);
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("stream_stall_timeout_secs"),
        "0 must be rejected, got: {err}"
    );

    let mut cfg = valid_config();
    cfg.agent.stream_stall_timeout_secs = Some(300);
    assert!(cfg.validate().is_ok(), "a positive stall timeout validates");
    let mut cfg = valid_config();
    cfg.agent.stream_stall_timeout_secs = None;
    assert!(cfg.validate().is_ok(), "unset keeps the legacy default");
}

#[test]
fn max_retries_rejects_sentinel_overflow() {
    // u32::MAX overflows `max_retries + 1` (debug panic; release wraps to
    // zero attempts).
    let mut cfg = valid_config();
    cfg.retry.max_retries = u32::MAX;
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("max_retries"),
        "u32::MAX must be rejected, got: {err}"
    );

    let mut cfg = valid_config();
    cfg.retry.max_retries = 10;
    assert!(cfg.validate().is_ok(), "an ordinary retry count validates");
}

#[test]
fn profile_max_retries_rejects_sentinel_overflow() {
    // The per-profile override feeds the same `max_retries + 1` arithmetic
    // on both chat paths — same overflow class, same rejection.
    let mut cfg = valid_config();
    cfg.models.insert(
        "default".to_string(),
        crate::config::ModelProfile {
            endpoint: cfg.endpoint.clone(),
            model: cfg.model.clone(),
            api_key: None,
            max_tokens: cfg.max_tokens,
            temperature: cfg.temperature,
            modalities: vec!["text".to_string()],
            context_length: cfg.context_length,
            extra_body: None,
            native_function_calling: None,
            max_retries: Some(u32::MAX),
            response_timeout_floor_secs: None,
        },
    );
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("models.default.max_retries"),
        "profile u32::MAX must be rejected, got: {err}"
    );
}

#[test]
fn test_extra_body_rejects_top_level_xhigh_and_high_for_qwen_on_sglang() {
    let mut cfg = valid_config();
    cfg.endpoint = "https://llm.selfware.design/v1".to_string();
    cfg.model = "qwen38-flash-next".to_string();

    // xhigh rejected on SGLang serving deployment
    let mut extra = serde_json::Map::new();
    extra.insert("reasoning_effort".to_string(), serde_json::json!("xhigh"));
    cfg.extra_body = Some(extra.clone());
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("extra_body.reasoning_effort cannot be 'xhigh' at top-level on SGLang serving deployments"),
        "validation must reject top-level xhigh reasoning_effort on SGLang: {err}"
    );

    // high rejected for Qwen on SGLang serving deployment
    let mut extra_high = serde_json::Map::new();
    extra_high.insert("reasoning_effort".to_string(), serde_json::json!("high"));
    cfg.extra_body = Some(extra_high.clone());
    let err_high = cfg.validate().unwrap_err().to_string();
    assert!(
        err_high.contains("extra_body.reasoning_effort cannot be 'high' for Qwen models"),
        "validation must reject top-level high reasoning_effort for Qwen on SGLang: {err_high}"
    );

    // low and medium accepted on SGLang serving deployment
    for allowed in ["low", "medium"] {
        let mut extra_ok = serde_json::Map::new();
        extra_ok.insert("reasoning_effort".to_string(), serde_json::json!(allowed));
        cfg.extra_body = Some(extra_ok);
        assert!(
            cfg.validate().is_ok(),
            "validation should accept top-level {allowed} for Qwen on SGLang"
        );
    }

    // Test the SAME model against a different capability configuration:
    // On OpenRouter or generic OpenAI endpoint, top-level xhigh is allowed.
    let mut openrouter_cfg = valid_config();
    openrouter_cfg.endpoint = "https://openrouter.ai/api/v1".to_string();
    openrouter_cfg.model = "qwen38-flash-next".to_string();
    openrouter_cfg.extra_body = Some(extra);
    assert!(
        openrouter_cfg.validate().is_ok(),
        "same Qwen model on non-SGLang endpoint must allow top-level xhigh"
    );

    // But 'high' is rejected for Qwen even on OpenRouter (Qwen template refuses high everywhere)
    openrouter_cfg.extra_body = Some(extra_high);
    let or_high_err = openrouter_cfg.validate().unwrap_err().to_string();
    assert!(
        or_high_err.contains("cannot be 'high' for Qwen models"),
        "validation must reject high for Qwen on any endpoint: {or_high_err}"
    );

    // Non-Qwen allows high
    let mut non_qwen_cfg = valid_config();
    non_qwen_cfg.model = "gpt-4o".to_string();
    let mut extra_gpt = serde_json::Map::new();
    extra_gpt.insert("reasoning_effort".to_string(), serde_json::json!("high"));
    non_qwen_cfg.extra_body = Some(extra_gpt);
    assert!(
        non_qwen_cfg.validate().is_ok(),
        "non-Qwen models should allow top-level high"
    );
}

#[test]
fn test_model_profile_extra_body_rejects_top_level_xhigh() {
    let mut cfg = valid_config();
    let mut extra = serde_json::Map::new();
    extra.insert("reasoning_effort".to_string(), serde_json::json!("xhigh"));
    cfg.models.insert(
        "qwen".to_string(),
        crate::config::ModelProfile {
            endpoint: "https://llm.selfware.design/v1".to_string(),
            model: "qwen38-flash-next".to_string(),
            api_key: None,
            max_tokens: cfg.max_tokens,
            temperature: cfg.temperature,
            modalities: vec!["text".to_string()],
            context_length: cfg.context_length,
            extra_body: Some(extra.clone()),
            native_function_calling: None,
            max_retries: None,
            response_timeout_floor_secs: None,
        },
    );

    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("models.qwen.extra_body.reasoning_effort cannot be 'xhigh' at top-level on SGLang serving deployments"),
        "profile validation must reject top-level xhigh on SGLang: {err}"
    );

    // Same profile model on OpenRouter endpoint succeeds
    cfg.models.get_mut("qwen").unwrap().endpoint = "https://openrouter.ai/api/v1".to_string();
    assert!(
        cfg.validate().is_ok(),
        "same profile model on non-SGLang endpoint must accept top-level xhigh"
    );
}

#[test]
fn test_extra_body_allows_nested_chat_template_kwargs_xhigh() {
    let mut cfg = valid_config();
    cfg.model = "qwen38-flash-next".to_string();
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({
            "enable_thinking": true,
            "preserve_thinking": true,
            "reasoning_effort": "xhigh"
        }),
    );
    cfg.extra_body = Some(extra);

    assert!(
        cfg.validate().is_ok(),
        "nested chat_template_kwargs reasoning_effort=xhigh must be accepted"
    );
}

#[test]
fn test_reject_non_string_reasoning_effort() {
    let mut cfg = valid_config();
    cfg.endpoint = "https://llm.selfware.design/v1".to_string();
    cfg.model = "qwen38-flash-next".to_string();

    // Top-level non-string (integer)
    let mut extra = serde_json::Map::new();
    extra.insert("reasoning_effort".to_string(), serde_json::json!(42));
    cfg.extra_body = Some(extra);

    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("extra_body.reasoning_effort must be a string"),
        "must reject integer reasoning_effort: {err}"
    );

    // Profile non-string (boolean)
    let mut cfg2 = valid_config();
    let mut prof_extra = serde_json::Map::new();
    prof_extra.insert("reasoning_effort".to_string(), serde_json::json!(true));
    cfg2.models.insert(
        "qwen".to_string(),
        crate::config::ModelProfile {
            endpoint: "https://llm.selfware.design/v1".to_string(),
            model: "qwen38-flash-next".to_string(),
            api_key: None,
            max_tokens: cfg2.max_tokens,
            temperature: cfg2.temperature,
            modalities: vec!["text".to_string()],
            context_length: cfg2.context_length,
            extra_body: Some(prof_extra),
            native_function_calling: None,
            max_retries: None,
            response_timeout_floor_secs: None,
        },
    );

    let err2 = cfg2.validate().unwrap_err().to_string();
    assert!(
        err2.contains("models.qwen.extra_body.reasoning_effort must be a string"),
        "must reject boolean reasoning_effort in profile: {err2}"
    );
}

#[test]
fn test_reject_unknown_reasoning_effort() {
    let mut cfg = valid_config();
    let mut extra = serde_json::Map::new();
    extra.insert("reasoning_effort".to_string(), serde_json::json!("hig"));
    cfg.extra_body = Some(extra);

    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("extra_body.reasoning_effort must be one of 'low', 'medium', 'high', 'xhigh'"),
        "must reject 'hig': {err}"
    );

    let mut cfg2 = valid_config();
    let mut prof_extra = serde_json::Map::new();
    prof_extra.insert("reasoning_effort".to_string(), serde_json::json!("x-high"));
    cfg2.models.insert(
        "qwen".to_string(),
        crate::config::ModelProfile {
            endpoint: "https://llm.selfware.design/v1".to_string(),
            model: "qwen38-flash-next".to_string(),
            api_key: None,
            max_tokens: cfg2.max_tokens,
            temperature: cfg2.temperature,
            modalities: vec!["text".to_string()],
            context_length: cfg2.context_length,
            extra_body: Some(prof_extra),
            native_function_calling: None,
            max_retries: None,
            response_timeout_floor_secs: None,
        },
    );

    let err2 = cfg2.validate().unwrap_err().to_string();
    assert!(
        err2.contains("models.qwen.extra_body.reasoning_effort must be one of 'low', 'medium', 'high', 'xhigh'"),
        "must reject 'x-high' in profile: {err2}"
    );
}

#[test]
fn test_is_sglang_server_info_body_validation() {
    // SGLang JSON responses with backend-specific evidence
    let valid_sglang_json =
        r#"{"version": "0.4.3.post2", "tool_call_parser": "qwen", "reasoning_parser": "qwen3"}"#;
    assert!(is_sglang_server_info_body(valid_sglang_json));

    let sglang_version_json = r#"{"sglang_version": "0.4.0"}"#;
    assert!(is_sglang_server_info_body(sglang_version_json));

    let sglang_in_ver_json = r#"{"version": "0.4.0-sglang"}"#;
    assert!(is_sglang_server_info_body(sglang_in_ver_json));

    let sglang_backend_json = r#"{"backend": "sglang", "version": "1.0"}"#;
    assert!(is_sglang_server_info_body(sglang_backend_json));

    // Generic version responses (vLLM, Ollama, custom servers) — must NOT qualify without SGLang evidence
    let generic_version_json = r#"{"version": "0.4.0"}"#;
    assert!(!is_sglang_server_info_body(generic_version_json));

    let generic_v1_json = r#"{"version": "1.0.0"}"#;
    assert!(!is_sglang_server_info_body(generic_v1_json));

    // Model name in response (e.g. Qwen running on non-SGLang stack) — must NOT qualify
    let qwen_model_json = r#"{"version": "1.0", "model": "qwen2.5-72b"}"#;
    assert!(!is_sglang_server_info_body(qwen_model_json));

    // Generic HTML 200 (nginx, apache, captive portal) — must NOT be classified as SGLang
    let nginx_html =
        "<html><head><title>200 OK</title></head><body>Welcome to nginx!</body></html>";
    assert!(!is_sglang_server_info_body(nginx_html));

    // Generic JSON 200 without SGLang fields — must NOT be classified as SGLang
    let generic_json = r#"{"status": "ok", "message": "hello"}"#;
    assert!(!is_sglang_server_info_body(generic_json));
}

#[test]
fn test_is_sglang_serving_deployment_ports() {
    // Explicit SGLang identifiers and deployment domains
    assert!(is_sglang_serving_deployment(
        "https://llm.selfware.design/v1"
    ));
    assert!(is_sglang_serving_deployment(
        "http://sglang-cluster:8000/v1"
    ));
    assert!(is_sglang_serving_deployment("http://10.0.0.1:30000/v1"));

    // Common ports without SGLang evidence must NOT qualify without behavioral inspection
    assert!(!is_sglang_serving_deployment("http://localhost:8000/v1"));
    assert!(!is_sglang_serving_deployment("http://127.0.0.1:8000/v1"));
    assert!(!is_sglang_serving_deployment("http://localhost:8080/v1"));
}

#[test]
fn test_sglang_capability_cache() {
    clear_sglang_capability_cache();
    let ep = "http://custom-proxy.internal:9999/v1";
    assert_eq!(get_sglang_capability(ep), None);

    set_sglang_capability(ep, true);
    assert_eq!(get_sglang_capability(ep), Some(true));

    // Subpath variation normalizes to the same base
    assert_eq!(
        get_sglang_capability("http://custom-proxy.internal:9999"),
        Some(true)
    );

    clear_sglang_capability_cache();
    assert_eq!(get_sglang_capability(ep), None);
}

async fn start_validation_mock_server(
    responses: Vec<(u16, String, Option<std::time::Duration>)>,
) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://{}/v1", addr);
    let handle = tokio::spawn(async move {
        for (status, body, delay) in responses {
            if let Ok((mut socket, _)) = listener.accept().await {
                if let Some(d) = delay {
                    tokio::time::sleep(d).await;
                }
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let wire = format!(
                    "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                let _ = socket.write_all(wire.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        }
    });
    (endpoint, handle)
}

#[tokio::test]
async fn test_non_sglang_server_on_common_port_accepts_xhigh() {
    clear_sglang_capability_cache();
    let generic_response = r#"{"version": "1.0.0", "status": "ok"}"#.to_string();
    let (endpoint, _task) = start_validation_mock_server(vec![(200, generic_response, None)]).await;

    let mut cfg = valid_config();
    cfg.endpoint = endpoint.clone();
    let mut extra = serde_json::Map::new();
    extra.insert("reasoning_effort".to_string(), serde_json::json!("xhigh"));
    cfg.extra_body = Some(extra);

    // Async validation discovers non-SGLang and accepts xhigh
    assert!(cfg.validate_async().await.is_ok());
    assert_eq!(get_sglang_capability(&endpoint), Some(false));
}

#[tokio::test]
async fn test_sglang_server_on_common_port_rejects_xhigh() {
    clear_sglang_capability_cache();
    let sglang_response = r#"{"sglang_version": "0.4.3", "tool_call_parser": "qwen"}"#.to_string();
    let (endpoint, _task) = start_validation_mock_server(vec![(200, sglang_response, None)]).await;

    let mut cfg = valid_config();
    cfg.endpoint = endpoint.clone();
    let mut extra = serde_json::Map::new();
    extra.insert("reasoning_effort".to_string(), serde_json::json!("xhigh"));
    cfg.extra_body = Some(extra);

    let err = cfg.validate_async().await.unwrap_err().to_string();
    assert!(
        err.contains("extra_body.reasoning_effort cannot be 'xhigh' at top-level on SGLang"),
        "must reject top-level xhigh for SGLang: {err}"
    );
    assert_eq!(get_sglang_capability(&endpoint), Some(true));
}

#[tokio::test]
async fn test_probe_timeout_preserves_unknown_and_retries() {
    clear_sglang_capability_cache();
    let sglang_body = r#"{"sglang_version": "0.4.3", "tool_call_parser": "qwen"}"#.to_string();
    // Request 1: 700ms delay causes reqwest 500ms timeout.
    // Request 2: immediate 200 OK with SGLang body.
    let (endpoint, _task) = start_validation_mock_server(vec![
        (
            200,
            String::new(),
            Some(std::time::Duration::from_millis(700)),
        ),
        (200, sglang_body, None),
    ])
    .await;

    // 1st probe: times out
    let first_result = probe_sglang_backend_async(&endpoint).await;
    assert!(!first_result, "first probe should fail on timeout");
    // Transient failure must NOT poison the cache as negative (preserve unknown state)
    assert_eq!(
        get_sglang_capability(&endpoint),
        None,
        "transient timeout must leave capability in unknown state"
    );

    // 2nd probe: retries and succeeds
    let second_result = probe_sglang_backend_async(&endpoint).await;
    assert!(second_result, "second probe should succeed on retry");
    assert_eq!(
        get_sglang_capability(&endpoint),
        Some(true),
        "successful probe should cache positive result"
    );
}

#[tokio::test]
async fn test_probe_transient_failures_and_conclusive_404() {
    clear_sglang_capability_cache();
    // 500 server error and 401 auth challenge must NOT cache false (transient/unknown)
    // 404 route missing IS conclusive non-SGLang
    let (endpoint, _task) = start_validation_mock_server(vec![
        (500, "Internal Server Error".to_string(), None),
        (401, "Unauthorized".to_string(), None),
        (404, "Not Found".to_string(), None),
    ])
    .await;

    // 1. 500 Server error
    assert!(!probe_sglang_backend_async(&endpoint).await);
    assert_eq!(
        get_sglang_capability(&endpoint),
        None,
        "500 must not be cached as negative"
    );

    // 2. 401 Unauthorized
    assert!(!probe_sglang_backend_async(&endpoint).await);
    assert_eq!(
        get_sglang_capability(&endpoint),
        None,
        "401 must not be cached as negative"
    );

    // 3. 404 Not Found
    assert!(!probe_sglang_backend_async(&endpoint).await);
    assert_eq!(
        get_sglang_capability(&endpoint),
        Some(false),
        "404 route missing is conclusive non-SGLang"
    );
}
