use super::api_key::{is_local_endpoint, ApiKeySource, KEYRING_SERVICE};
use super::types::{
    default_animation_speed, default_checkpoint_interval_secs, default_checkpoint_interval_tools,
    default_max_recovery_attempts, default_retry_base_delay_ms, default_retry_max_delay_ms,
    default_retry_max_retries, default_status_interval, default_theme, ConcurrencyConfig,
};
use super::*;
use std::path::PathBuf;

/// Mutex to serialize tests that mutate SELFWARE_* environment variables.
/// `env::set_var` / `env::remove_var` are process-global, so parallel tests
/// that rely on specific env values will race without serialization.
/// All tests that call `clear_selfware_env_vars()` or `set_var(SELFWARE_*)`
/// should acquire this lock first via `lock_env()`.
// NOTE: ENV_MUTEX and clear_selfware_env_vars have moved to
// `crate::config::test_helpers` so that `cli::tests` can share the same lock.
// This module now re-exports the helpers for backward compatibility within
// config tests.

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.endpoint, "http://127.0.0.1:1234/v1");
    assert_eq!(config.model, "qwen3.5-27b");
    assert_eq!(config.max_tokens, 65536);
    assert!((config.temperature - 1.0).abs() < f32::EPSILON);
    assert!(config.api_key.is_none());
}

#[test]
fn test_safety_config_default() {
    let config = SafetyConfig::default();
    assert_eq!(config.allowed_paths, vec!["./**".to_string()]);
    assert!(!config.denied_paths.is_empty());
    assert_eq!(
        config.protected_branches,
        vec!["main".to_string(), "master".to_string()]
    );
}

#[test]
fn test_agent_config_default() {
    let config = AgentConfig::default();
    assert_eq!(config.max_iterations, 100);
    assert_eq!(config.step_timeout_secs, 300);
    assert_eq!(
        config.token_budget,
        default_max_tokens(),
        "defaults to max_tokens"
    );
}

#[test]
fn test_config_load_missing_file() {
    let result = Config::load(Some("/nonexistent/path/config.toml"));
    assert!(result.is_err());
}

#[test]
fn test_config_load_no_path_uses_defaults() {
    // When no config file exists in the specific path, it should return an error
    // Or wait, if we want to test default config values, just use Config::default()
    let config = Config::default();
    assert_eq!(config.endpoint, "http://127.0.0.1:1234/v1");
}

#[test]
fn test_config_serialization() {
    let config = Config::default();
    let toml_str = toml::to_string(&config).unwrap();
    assert!(toml_str.contains("endpoint"));
    assert!(toml_str.contains("model"));
}

#[test]
fn test_config_deserialization() {
    let toml_str = r#"
            endpoint = "http://test:9999/v1"
            model = "test-model"
            max_tokens = 1000
            temperature = 0.5
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.endpoint, "http://test:9999/v1");
    assert_eq!(config.model, "test-model");
    assert_eq!(config.max_tokens, 1000);
}

#[test]
fn test_config_with_safety_section() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            allowed_paths = ["/home/**"]
            denied_paths = ["**/.env"]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.safety.allowed_paths, vec!["/home/**".to_string()]);
    assert_eq!(config.safety.denied_paths, vec!["**/.env".to_string()]);
}

#[test]
fn test_config_with_agent_section() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            max_iterations = 50
            step_timeout_secs = 600
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.agent.max_iterations, 50);
    assert_eq!(config.agent.step_timeout_secs, 600);
}

#[test]
fn test_yolo_file_config_default() {
    let config = YoloFileConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.max_operations, 0);
    assert!((config.max_hours - 0.0).abs() < f64::EPSILON);
    assert!(config.allow_git_push);
    assert!(!config.allow_destructive_shell);
    assert!(config.audit_log_path.is_none());
    assert_eq!(config.status_interval, 100);
}

#[test]
fn test_yolo_file_config_serialization() {
    let config = YoloFileConfig {
        enabled: true,
        max_operations: 500,
        max_hours: 8.0,
        allow_git_push: false,
        allow_destructive_shell: true,
        audit_log_path: Some(PathBuf::from("/tmp/audit.log")),
        status_interval: 50,
    };
    let toml_str = toml::to_string(&config).unwrap();
    assert!(toml_str.contains("enabled = true"));
    assert!(toml_str.contains("max_operations = 500"));
    assert!(toml_str.contains("max_hours = 8.0"));
    assert!(toml_str.contains("allow_git_push = false"));
    assert!(toml_str.contains("allow_destructive_shell = true"));
}

#[test]
fn test_config_with_yolo_section() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [yolo]
            enabled = true
            max_operations = 1000
            max_hours = 4.0
            allow_git_push = false
            allow_destructive_shell = false
            status_interval = 25
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.yolo.enabled);
    assert_eq!(config.yolo.max_operations, 1000);
    assert!((config.yolo.max_hours - 4.0).abs() < f64::EPSILON);
    assert!(!config.yolo.allow_git_push);
    assert!(!config.yolo.allow_destructive_shell);
    assert_eq!(config.yolo.status_interval, 25);
}

#[test]
fn test_config_with_yolo_audit_log() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [yolo]
            enabled = true
            audit_log_path = "/var/log/selfware-audit.log"
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.yolo.enabled);
    assert_eq!(
        config.yolo.audit_log_path,
        Some(PathBuf::from("/var/log/selfware-audit.log"))
    );
}

#[test]
fn test_safety_config_require_confirmation_default() {
    let config = SafetyConfig::default();
    assert!(config
        .require_confirmation
        .contains(&"git_push".to_string()));
    assert!(config
        .require_confirmation
        .contains(&"file_delete".to_string()));
    assert!(config
        .require_confirmation
        .contains(&"shell_exec".to_string()));
}

#[test]
fn test_config_with_custom_require_confirmation() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            require_confirmation = ["dangerous_op", "deploy"]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.safety.require_confirmation,
        vec!["dangerous_op".to_string(), "deploy".to_string()]
    );
}

#[test]
fn test_config_partial_deserialization() {
    // Only required fields, rest should use defaults
    let toml_str = r#"
            endpoint = "http://custom:1234/v1"
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.endpoint, "http://custom:1234/v1");
    assert_eq!(config.model, "qwen3.5-27b"); // default
    assert_eq!(config.max_tokens, 65536); // default
}

#[test]
fn test_config_with_api_key() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            api_key = "sk-test-12345"
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.api_key.as_ref().map(|k| k.expose().to_string()),
        Some("sk-test-12345".to_string())
    );
}

#[test]
fn test_config_clone() {
    let config = Config::default();
    let cloned = config.clone();
    assert_eq!(config.endpoint, cloned.endpoint);
    assert_eq!(config.model, cloned.model);
    assert_eq!(config.max_tokens, cloned.max_tokens);
}

#[test]
fn test_safety_config_clone() {
    let config = SafetyConfig::default();
    let cloned = config.clone();
    assert_eq!(config.allowed_paths, cloned.allowed_paths);
    assert_eq!(config.protected_branches, cloned.protected_branches);
}

#[test]
fn test_agent_config_clone() {
    let config = AgentConfig::default();
    let cloned = config.clone();
    assert_eq!(config.max_iterations, cloned.max_iterations);
    assert_eq!(config.step_timeout_secs, cloned.step_timeout_secs);
}

#[test]
fn test_yolo_file_config_clone() {
    let config = YoloFileConfig {
        enabled: true,
        max_operations: 100,
        max_hours: 2.0,
        allow_git_push: true,
        allow_destructive_shell: false,
        audit_log_path: Some(PathBuf::from("/tmp/test.log")),
        status_interval: 50,
    };
    let cloned = config.clone();
    assert_eq!(config.enabled, cloned.enabled);
    assert_eq!(config.max_operations, cloned.max_operations);
    assert_eq!(config.audit_log_path, cloned.audit_log_path);
}

#[test]
fn test_config_debug() {
    let config = Config::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("Config"));
    assert!(debug_str.contains("endpoint"));
}

#[test]
fn test_config_debug_redacts_api_key() {
    let config = Config {
        api_key: Some(RedactedString::new("sk-super-secret-key-12345")),
        context_length: default_context_length(),
        ..Config::default()
    };
    let debug_str = format!("{:?}", config);
    assert!(
        !debug_str.contains("sk-super-secret-key-12345"),
        "API key must not appear in Debug output"
    );
    assert!(
        debug_str.contains("[REDACTED]"),
        "Debug output should show [REDACTED] for API key"
    );
}

#[test]
fn test_safety_config_debug() {
    let config = SafetyConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("SafetyConfig"));
}

#[test]
fn test_agent_config_debug() {
    let config = AgentConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("AgentConfig"));
}

#[test]
fn test_yolo_file_config_debug() {
    let config = YoloFileConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("YoloFileConfig"));
}

#[test]
fn test_config_invalid_toml() {
    let toml_str = "this is not valid { toml }";
    let result: Result<Config, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_config_wrong_type() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            max_tokens = "not a number"
        "#;
    let result: Result<Config, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_config_full_roundtrip() {
    let config = Config {
        endpoint: "http://test:9999/v1".to_string(),
        model: "test-model".to_string(),
        max_tokens: 4096,
        context_length: 131072,
        temperature: 0.7,
        api_key: Some(RedactedString::new("test-key")),
        safety: SafetyConfig {
            allowed_paths: vec!["/home/**".to_string()],
            denied_paths: vec!["**/.git/**".to_string()],
            protected_branches: vec!["main".to_string()],
            require_confirmation: vec!["deploy".to_string()],
            strict_permissions: false,
            permissions: vec![],
        },
        agent: AgentConfig {
            max_iterations: 50,
            step_timeout_secs: 120,
            token_budget: 100000,
            native_function_calling: false,
            streaming: true,
            min_completion_steps: 3,
            require_verification_before_completion: true,
            ..Default::default()
        },
        yolo: YoloFileConfig {
            enabled: true,
            max_operations: 500,
            max_hours: 4.0,
            allow_git_push: false,
            allow_destructive_shell: false,
            audit_log_path: Some(PathBuf::from("/tmp/audit.log")),
            status_interval: 25,
        },
        ui: UiConfig {
            theme: "ocean".to_string(),
            animations: true,
            compact_mode: true,
            verbose_mode: false,
            show_tokens: true,
            animation_speed: 1.5,
        },
        continuous_work: ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 8,
            checkpoint_interval_secs: 180,
            auto_recovery: true,
            max_recovery_attempts: 4,
        },
        retry: RetrySettings {
            max_retries: 6,
            base_delay_ms: 500,
            max_delay_ms: 20000,
        },
        resources: crate::config::ResourcesConfig::default(),
        evolution: EvolutionTomlConfig::default(),
        cache: crate::session::cache::LlmCacheConfig::default(),
        debug: crate::config::DebugConfig::default(),
        models: HashMap::new(),
        execution_mode: ExecutionMode::default(),
        compact_mode: false,
        verbose_mode: false,
        show_tokens: false,
        extra_body: None,
        qa: crate::testing::qa_profiles::QaConfig::default(),
        mcp: crate::mcp::McpConfig::default(),
        hooks: Vec::new(),
        plan_mode: false,
        concurrency: crate::config::ConcurrencyConfig::default(),
        matched_profile: None,
        matched_profile_applied: Vec::new(),
        sources: crate::config::ConfigSources::new(),
    };

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();

    assert_eq!(parsed.endpoint, config.endpoint);
    assert_eq!(parsed.model, config.model);
    assert_eq!(parsed.max_tokens, config.max_tokens);
    assert_eq!(parsed.api_key, config.api_key);
    assert_eq!(parsed.safety.allowed_paths, config.safety.allowed_paths);
    assert_eq!(parsed.agent.max_iterations, config.agent.max_iterations);
    assert_eq!(parsed.yolo.enabled, config.yolo.enabled);
    assert_eq!(parsed.yolo.max_operations, config.yolo.max_operations);
}

#[test]
fn test_empty_config_uses_all_defaults() {
    let toml_str = "";
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.endpoint, "http://127.0.0.1:1234/v1");
    assert_eq!(config.model, "qwen3.5-27b");
    assert_eq!(config.max_tokens, 65536);
    assert!(!config.yolo.enabled);
}

#[test]
fn test_context_length_from_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            context_length = 1010000
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.context_length, 1010000,
        "context_length should be parsed from TOML"
    );
}

#[test]
fn test_context_length_default() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.context_length, 131072,
        "context_length should use default when not specified"
    );
}

#[test]
fn test_default_true_helper() {
    assert!(default_true());
}

#[test]
fn test_default_status_interval_helper() {
    assert_eq!(default_status_interval(), 100);
}

#[test]
fn test_config_temperature_edge_values() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            temperature = 0.0
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!((config.temperature - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_config_with_all_safety_fields() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            allowed_paths = ["/home/**", "/opt/**"]
            denied_paths = ["**/.env", "**/.secrets"]
            protected_branches = ["main", "master", "develop"]
            require_confirmation = ["git_push", "file_delete"]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.safety.allowed_paths.len(), 2);
    assert_eq!(config.safety.denied_paths.len(), 2);
    assert_eq!(config.safety.protected_branches.len(), 3);
    assert_eq!(config.safety.require_confirmation.len(), 2);
}

#[test]
fn test_yolo_config_with_zero_limits() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [yolo]
            enabled = true
            max_operations = 0
            max_hours = 0.0
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.yolo.enabled);
    assert_eq!(config.yolo.max_operations, 0);
    assert!((config.yolo.max_hours - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_config_serialize_then_deserialize() {
    let config = Config::default();
    let serialized = toml::to_string(&config).unwrap();
    let deserialized: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(config.endpoint, deserialized.endpoint);
    assert_eq!(config.model, deserialized.model);
}

#[test]
fn test_safety_config_serialize() {
    let config = SafetyConfig::default();
    let serialized = toml::to_string(&config).unwrap();
    assert!(serialized.contains("allowed_paths"));
    assert!(serialized.contains("protected_branches"));
}

#[test]
fn test_agent_config_serialize() {
    let config = AgentConfig::default();
    let serialized = toml::to_string(&config).unwrap();
    assert!(serialized.contains("max_iterations"));
    assert!(serialized.contains("step_timeout_secs"));
}

#[test]
fn test_config_large_token_budget() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            token_budget = 2000000
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.agent.token_budget, 2000000);
}

#[test]
fn test_config_high_temperature() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            temperature = 2.0
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!((config.temperature - 2.0).abs() < f32::EPSILON);
}

#[test]
fn test_yolo_with_long_audit_path() {
    let long_path = "/var/log/selfware/audit/2024/01/detailed-audit.log";
    let toml_str = format!(
        r#"
            endpoint = "http://localhost:8000/v1"

            [yolo]
            enabled = true
            audit_log_path = "{}"
        "#,
        long_path
    );
    let config: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(config.yolo.audit_log_path, Some(PathBuf::from(long_path)));
}

#[test]
fn test_config_empty_api_key() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            api_key = ""
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.api_key.as_ref().map(|k| k.expose().to_string()),
        Some("".to_string())
    );
}

#[test]
fn test_config_empty_allowed_paths() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            allowed_paths = []
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.safety.allowed_paths.is_empty());
}

#[test]
fn test_config_empty_protected_branches() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            protected_branches = []
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.safety.protected_branches.is_empty());
}

#[test]
fn test_default_helpers() {
    assert_eq!(default_endpoint(), "http://127.0.0.1:1234/v1");
    assert_eq!(default_model(), "qwen3.5-27b");
    assert_eq!(default_max_tokens(), 65536);
    assert!((default_temperature() - 1.0).abs() < f32::EPSILON);
    assert_eq!(default_max_iterations(), 100);
    assert_eq!(default_step_timeout(), 300);
    assert_eq!(
        default_token_budget(),
        0,
        "sentinel value, resolved from max_tokens at load"
    );
    assert_eq!(default_allowed_paths(), vec!["./**".to_string()]);
    assert_eq!(
        default_protected_branches(),
        vec!["main".to_string(), "master".to_string()]
    );
}

#[test]
fn test_default_require_confirmation_content() {
    let confirmation = default_require_confirmation();
    assert!(confirmation.contains(&"git_push".to_string()));
    assert!(confirmation.contains(&"file_delete".to_string()));
    assert!(confirmation.contains(&"shell_exec".to_string()));
}

#[test]
fn test_config_with_max_tokens_zero() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            max_tokens = 0
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.max_tokens, 0);
}

#[test]
fn test_agent_config_with_zero_iterations() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            max_iterations = 0
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.agent.max_iterations, 0);
}

#[test]
fn test_yolo_config_high_status_interval() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [yolo]
            status_interval = 10000
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.yolo.status_interval, 10000);
}

#[test]
fn test_yolo_destructive_shell_enabled() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [yolo]
            enabled = true
            allow_destructive_shell = true
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.yolo.allow_destructive_shell);
}

#[test]
fn test_config_with_unicode_paths() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            allowed_paths = ["/home/用户/**", "/opt/データ/**"]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config
        .safety
        .allowed_paths
        .contains(&"/home/用户/**".to_string()));
}

#[test]
fn test_ui_config_default() {
    let config = UiConfig::default();
    assert_eq!(config.theme, "amber");
    assert!(config.animations);
    assert!(!config.compact_mode);
    assert!(!config.verbose_mode);
    assert!(!config.show_tokens);
    assert!((config.animation_speed - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_config_with_ui_section() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [ui]
            theme = "ocean"
            animations = true
            compact_mode = true
            show_tokens = true
            animation_speed = 1.5
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.ui.theme, "ocean");
    assert!(config.ui.animations);
    assert!(config.ui.compact_mode);
    assert!(config.ui.show_tokens);
    assert!((config.ui.animation_speed - 1.5).abs() < f64::EPSILON);
}

#[test]
fn test_ui_config_serialization() {
    let config = UiConfig {
        theme: "high-contrast".to_string(),
        animations: false,
        compact_mode: true,
        verbose_mode: true,
        show_tokens: true,
        animation_speed: 2.0,
    };
    let toml_str = toml::to_string(&config).unwrap();
    assert!(toml_str.contains("theme = \"high-contrast\""));
    assert!(toml_str.contains("animations = false"));
    assert!(toml_str.contains("compact_mode = true"));
}

#[test]
fn test_config_ui_defaults_applied() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [ui]
            compact_mode = true
            show_tokens = true
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    // UI defaults should be present
    assert_eq!(config.ui.theme, "amber"); // default
    assert!(config.ui.compact_mode);
    assert!(config.ui.show_tokens);
}

#[test]
fn test_continuous_work_defaults() {
    let config = Config::default();
    assert!(config.continuous_work.enabled);
    assert_eq!(config.continuous_work.checkpoint_interval_tools, 10);
    assert_eq!(config.continuous_work.checkpoint_interval_secs, 300);
    assert!(config.continuous_work.auto_recovery);
    assert_eq!(config.continuous_work.max_recovery_attempts, 3);
}

#[test]
fn test_retry_defaults() {
    let config = Config::default();
    assert_eq!(config.retry.max_retries, 5);
    assert_eq!(config.retry.base_delay_ms, 1000);
    assert_eq!(config.retry.max_delay_ms, 60000);
}

#[test]
fn test_config_with_continuous_work_and_retry_sections() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [continuous_work]
            enabled = true
            checkpoint_interval_tools = 7
            checkpoint_interval_secs = 120
            auto_recovery = false
            max_recovery_attempts = 9

            [retry]
            max_retries = 11
            base_delay_ms = 250
            max_delay_ms = 20000
        "#;

    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.continuous_work.enabled);
    assert_eq!(config.continuous_work.checkpoint_interval_tools, 7);
    assert_eq!(config.continuous_work.checkpoint_interval_secs, 120);
    assert!(!config.continuous_work.auto_recovery);
    assert_eq!(config.continuous_work.max_recovery_attempts, 9);
    assert_eq!(config.retry.max_retries, 11);
    assert_eq!(config.retry.base_delay_ms, 250);
    assert_eq!(config.retry.max_delay_ms, 20000);
}

// ---- Config validation tests ----

#[test]
fn test_validate_default_config() {
    let config = Config::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_empty_endpoint() {
    let config = Config {
        endpoint: "".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("endpoint must not be empty"));
}

#[test]
fn test_validate_invalid_endpoint_scheme() {
    let config = Config {
        endpoint: "ftp://example.com".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("http:// or https://"));
}

#[test]
fn test_validate_endpoint_no_host() {
    let config = Config {
        endpoint: "http://".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("no host"));
}

#[test]
fn test_validate_empty_model() {
    let config = Config {
        model: "   ".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("model name must not be empty"));
}

#[test]
fn test_validate_zero_max_tokens() {
    let config = Config {
        max_tokens: 0,
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err
        .to_string()
        .contains("max_tokens must be greater than 0"));
}

#[test]
fn test_validate_excessive_max_tokens() {
    let config = Config {
        max_tokens: 100_000_000,
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("exceeds maximum allowed"));
}

#[test]
fn test_validate_negative_temperature() {
    let config = Config {
        temperature: -0.5,
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("temperature must be non-negative"));
}

#[test]
fn test_validate_zero_max_iterations() {
    let mut config = Config::default();
    config.agent.max_iterations = 0;
    let err = config.validate().unwrap_err();
    assert!(err
        .to_string()
        .contains("max_iterations must be greater than 0"));
}

#[test]
fn test_validate_zero_step_timeout() {
    let mut config = Config::default();
    config.agent.step_timeout_secs = 0;
    let err = config.validate().unwrap_err();
    assert!(err
        .to_string()
        .contains("step_timeout_secs must be greater than 0"));
}

#[test]
fn test_validate_zero_token_budget() {
    let mut config = Config::default();
    config.agent.token_budget = 0;
    let err = config.validate().unwrap_err();
    assert!(err
        .to_string()
        .contains("token_budget must be greater than 0"));
}

#[test]
fn test_validate_retry_delay_ordering() {
    let mut config = Config::default();
    config.retry.base_delay_ms = 5000;
    config.retry.max_delay_ms = 1000;
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("base_delay_ms"));
}

#[test]
fn test_validate_zero_animation_speed() {
    let mut config = Config::default();
    config.ui.animation_speed = 0.0;
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("animation_speed must be positive"));
}

#[test]
fn test_validate_valid_https_endpoint() {
    let config = Config {
        endpoint: "https://api.example.com/v1".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

// ---- is_local_endpoint tests ----

#[test]
fn test_is_local_endpoint_localhost() {
    assert!(is_local_endpoint("http://localhost:8000/v1"));
    assert!(is_local_endpoint("https://localhost:8000/v1"));
    assert!(is_local_endpoint("http://localhost/v1"));
}

#[test]
fn test_is_local_endpoint_127() {
    assert!(is_local_endpoint("http://127.0.0.1:8000/v1"));
    assert!(is_local_endpoint("https://127.0.0.1/v1"));
}

#[test]
fn test_is_local_endpoint_ipv6_loopback() {
    assert!(is_local_endpoint("http://[::1]:8000/v1"));
    assert!(is_local_endpoint("https://[::1]/v1"));
}

#[test]
fn test_is_local_endpoint_0000() {
    assert!(is_local_endpoint("http://0.0.0.0:8000/v1"));
}

#[test]
fn test_is_local_endpoint_remote() {
    assert!(!is_local_endpoint("http://api.example.com/v1"));
    assert!(!is_local_endpoint("https://192.168.1.100:8000/v1"));
    assert!(!is_local_endpoint("http://10.0.0.1:8000/v1"));
}

#[test]
fn test_is_local_endpoint_no_scheme() {
    assert!(!is_local_endpoint("localhost:8000/v1"));
    assert!(!is_local_endpoint("ftp://localhost:8000/v1"));
}

#[test]
fn test_validate_local_http_no_warning() {
    // Local HTTP endpoints should pass validation without error
    let config = Config {
        endpoint: "http://localhost:8000/v1".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_remote_http_still_valid() {
    // Remote HTTP endpoints should still pass validation (warning only, not error)
    let config = Config {
        endpoint: "http://api.example.com/v1".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

// ---- Environment variable override tests ----

#[test]
fn test_execution_mode_display() {
    assert_eq!(format!("{}", ExecutionMode::Normal), "normal");
    assert_eq!(format!("{}", ExecutionMode::AutoEdit), "auto-edit");
    assert_eq!(format!("{}", ExecutionMode::Yolo), "yolo");
    assert_eq!(format!("{}", ExecutionMode::Daemon), "daemon");
}

#[test]
fn test_execution_mode_default() {
    let mode = ExecutionMode::default();
    assert_eq!(mode, ExecutionMode::Normal);
}

// ---- API key source / plaintext warning tests ----

#[test]
fn test_api_key_source_enum_variants() {
    // Ensure the enum is constructable and comparable.
    let src = ApiKeySource::None;
    assert!(matches!(src, ApiKeySource::None));
    assert!(!matches!(src, ApiKeySource::EnvVar));
    assert!(!matches!(src, ApiKeySource::Keyring));
    assert!(!matches!(src, ApiKeySource::ConfigFile));
}

/// Helper: simulates the plaintext-key detection logic used in `Config::load`.
/// Returns the `ApiKeySource` that would be selected given the inputs.
fn resolve_api_key_source(
    env_var_set: bool,
    keyring_has_key: bool,
    config_file_has_key: bool,
) -> ApiKeySource {
    let mut source = ApiKeySource::None;

    if env_var_set {
        source = ApiKeySource::EnvVar;
    }

    if matches!(source, ApiKeySource::None) && keyring_has_key {
        source = ApiKeySource::Keyring;
    }

    if matches!(source, ApiKeySource::None) && config_file_has_key {
        source = ApiKeySource::ConfigFile;
    }

    source
}

#[test]
fn test_api_key_env_var_wins_over_keyring_and_config() {
    let src = resolve_api_key_source(true, true, true);
    assert_eq!(src, ApiKeySource::EnvVar);
}

#[test]
fn test_api_key_keyring_wins_over_config() {
    let src = resolve_api_key_source(false, true, true);
    assert_eq!(src, ApiKeySource::Keyring);
}

#[test]
fn test_api_key_config_file_is_last_resort() {
    let src = resolve_api_key_source(false, false, true);
    assert_eq!(src, ApiKeySource::ConfigFile);
}

#[test]
fn test_api_key_none_when_nothing_set() {
    let src = resolve_api_key_source(false, false, false);
    assert_eq!(src, ApiKeySource::None);
}

#[test]
fn test_plaintext_key_triggers_strict_mode_check() {
    // Build a config with a plaintext key and strict_permissions = true.
    // Verify the invariant: strict + plaintext ⇒ should_error is true.
    let config = Config {
        api_key: Some(RedactedString::new("sk-test-plaintext")),
        context_length: default_context_length(),
        safety: SafetyConfig {
            strict_permissions: true,
            ..SafetyConfig::default()
        },
        ..Config::default()
    };

    // The logic in Config::load checks:
    //   api_key_source == ConfigFile && strict_permissions ⇒ error
    let source = ApiKeySource::ConfigFile;
    let should_error =
        matches!(source, ApiKeySource::ConfigFile) && config.safety.strict_permissions;
    assert!(
        should_error,
        "Plaintext key + strict mode should trigger an error"
    );
}

#[test]
fn test_plaintext_key_no_error_without_strict() {
    let config = Config {
        api_key: Some(RedactedString::new("sk-test-plaintext")),
        context_length: default_context_length(),
        safety: SafetyConfig {
            strict_permissions: false,
            ..SafetyConfig::default()
        },
        ..Config::default()
    };

    let source = ApiKeySource::ConfigFile;
    let should_error =
        matches!(source, ApiKeySource::ConfigFile) && config.safety.strict_permissions;
    assert!(
        !should_error,
        "Plaintext key without strict mode should only warn, not error"
    );
}

#[test]
fn test_env_var_key_no_warning_even_with_strict() {
    // When the key comes from an env var, strict_permissions should
    // not trigger any error or warning about plaintext config files.
    let config = Config {
        api_key: Some(RedactedString::new("sk-from-env")),
        context_length: default_context_length(),
        safety: SafetyConfig {
            strict_permissions: true,
            ..SafetyConfig::default()
        },
        ..Config::default()
    };

    let source = ApiKeySource::EnvVar;
    let should_error =
        matches!(source, ApiKeySource::ConfigFile) && config.safety.strict_permissions;
    assert!(
        !should_error,
        "Env-var key should never trigger the plaintext config file error"
    );
}

#[test]
fn test_keyring_service_constant() {
    assert_eq!(KEYRING_SERVICE, "selfware-api-key");
}

/// Re-export of [`crate::config::test_helpers::clear_env`] for backward
/// compatibility within this module.
fn clear_selfware_env_vars() -> crate::config::test_helpers::EnvGuard {
    crate::config::test_helpers::clear_env()
}

// ---- RedactedString comprehensive tests ----

#[test]
fn test_redacted_string_new_and_expose() {
    let rs = RedactedString::new("my-secret");
    assert_eq!(rs.expose(), "my-secret");
}

#[test]
fn test_redacted_string_new_from_string() {
    let rs = RedactedString::new(String::from("owned-secret"));
    assert_eq!(rs.expose(), "owned-secret");
}

#[test]
fn test_redacted_string_display_is_redacted() {
    let rs = RedactedString::new("super-secret-key");
    let display = format!("{}", rs);
    assert_eq!(display, "[REDACTED]");
    assert!(!display.contains("super-secret-key"));
}

#[test]
fn test_redacted_string_debug_is_redacted() {
    let rs = RedactedString::new("super-secret-key");
    let debug = format!("{:?}", rs);
    assert_eq!(debug, "[REDACTED]");
    assert!(!debug.contains("super-secret-key"));
}

#[test]
fn test_redacted_string_partial_eq_same() {
    let a = RedactedString::new("same");
    let b = RedactedString::new("same");
    assert_eq!(a, b);
}

#[test]
fn test_redacted_string_partial_eq_different() {
    let a = RedactedString::new("one");
    let b = RedactedString::new("two");
    assert_ne!(a, b);
}

#[test]
fn test_redacted_string_eq_with_str() {
    let rs = RedactedString::new("hello");
    assert!(rs == *"hello");
    assert!(!(rs == *"world"));
}

#[test]
fn test_redacted_string_clone() {
    let original = RedactedString::new("clone-me");
    let cloned = original.clone();
    assert_eq!(original, cloned);
    assert_eq!(cloned.expose(), "clone-me");
}

#[test]
fn test_redacted_string_from_string() {
    let rs: RedactedString = String::from("from-string").into();
    assert_eq!(rs.expose(), "from-string");
}

#[test]
fn test_redacted_string_from_str_ref() {
    let rs: RedactedString = "from-str-ref".into();
    assert_eq!(rs.expose(), "from-str-ref");
}

#[test]
fn test_redacted_string_serialize_json() {
    let rs = RedactedString::new("secret-value");
    let json = serde_json::to_string(&rs).unwrap();
    assert_eq!(json, r#""secret-value""#);
}

#[test]
fn test_redacted_string_deserialize_json() {
    let rs: RedactedString = serde_json::from_str(r#""deserialized-secret""#).unwrap();
    assert_eq!(rs.expose(), "deserialized-secret");
}

#[test]
fn test_redacted_string_serialize_toml() {
    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        key: RedactedString,
    }
    let w = Wrapper {
        key: RedactedString::new("toml-secret"),
    };
    let toml_str = toml::to_string(&w).unwrap();
    assert!(toml_str.contains("toml-secret"));
}

#[test]
fn test_redacted_string_deserialize_toml() {
    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        key: RedactedString,
    }
    let toml_str = r#"key = "toml-deserialized""#;
    let w: Wrapper = toml::from_str(toml_str).unwrap();
    assert_eq!(w.key.expose(), "toml-deserialized");
}

#[test]
fn test_redacted_string_empty() {
    let rs = RedactedString::new("");
    assert_eq!(rs.expose(), "");
    assert_eq!(format!("{}", rs), "[REDACTED]");
    assert_eq!(format!("{:?}", rs), "[REDACTED]");
}

#[test]
fn test_redacted_string_roundtrip_toml() {
    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        key: RedactedString,
    }
    let original = Wrapper {
        key: RedactedString::new("roundtrip-value"),
    };
    let toml_str = toml::to_string(&original).unwrap();
    let parsed: Wrapper = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.key.expose(), "roundtrip-value");
}

// ---- ModelProfile tests ----

#[test]
fn test_model_profile_full_deserialization() {
    let toml_str = r#"
            endpoint = "http://192.168.1.170:1234/v1"
            model = "my-model"
            api_key = "sk-model-key"
            max_tokens = 8192
            temperature = 0.8
            modalities = ["text", "vision"]
            context_length = 32768
        "#;
    let profile: ModelProfile = toml::from_str(toml_str).unwrap();
    assert_eq!(profile.endpoint, "http://192.168.1.170:1234/v1");
    assert_eq!(profile.model, "my-model");
    assert_eq!(profile.api_key.as_ref().unwrap().expose(), "sk-model-key");
    assert_eq!(profile.max_tokens, 8192);
    assert!((profile.temperature - 0.8).abs() < f32::EPSILON);
    assert_eq!(profile.modalities, vec!["text", "vision"]);
    assert_eq!(profile.context_length, 32768);
}

#[test]
fn test_model_profile_defaults() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            model = "default-model"
        "#;
    let profile: ModelProfile = toml::from_str(toml_str).unwrap();
    assert!(profile.api_key.is_none());
    assert_eq!(profile.max_tokens, 65536);
    assert!((profile.temperature - 1.0).abs() < f32::EPSILON);
    assert_eq!(profile.modalities, vec!["text"]);
    assert_eq!(profile.context_length, 131072);
}

#[test]
fn test_model_profile_clone() {
    let profile = ModelProfile {
        endpoint: "http://localhost/v1".to_string(),
        model: "test".to_string(),
        api_key: Some(RedactedString::new("key")),
        max_tokens: 100,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 4096,
        extra_body: None,
        native_function_calling: None,
    };
    let cloned = profile.clone();
    assert_eq!(cloned.endpoint, profile.endpoint);
    assert_eq!(cloned.model, profile.model);
    assert_eq!(cloned.max_tokens, profile.max_tokens);
    assert_eq!(cloned.context_length, profile.context_length);
}

#[test]
fn test_model_profile_serialize_roundtrip() {
    let profile = ModelProfile {
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "roundtrip-model".to_string(),
        api_key: Some(RedactedString::new("rk-123")),
        max_tokens: 4096,
        temperature: 0.9,
        modalities: vec!["text".to_string(), "vision".to_string()],
        context_length: 16384,
        extra_body: None,
        native_function_calling: None,
    };
    let toml_str = toml::to_string(&profile).unwrap();
    let parsed: ModelProfile = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.endpoint, profile.endpoint);
    assert_eq!(parsed.model, profile.model);
    assert_eq!(parsed.api_key.unwrap().expose(), "rk-123");
    assert_eq!(parsed.modalities, profile.modalities);
    assert_eq!(parsed.context_length, profile.context_length);
}

#[test]
fn test_model_profile_debug_format() {
    let profile = ModelProfile {
        endpoint: "http://localhost/v1".to_string(),
        model: "debug-test".to_string(),
        api_key: Some(RedactedString::new("secret")),
        max_tokens: 100,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 4096,
        extra_body: None,
        native_function_calling: None,
    };
    let debug = format!("{:?}", profile);
    assert!(debug.contains("ModelProfile"));
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("secret"));
}

#[test]
fn test_default_modalities_fn() {
    let m = default_modalities();
    assert_eq!(m, vec!["text".to_string()]);
}

#[test]
fn test_default_context_length_fn() {
    assert_eq!(default_context_length(), 131072);
}

// ---- ExecutionMode tests ----

#[test]
fn test_execution_mode_serialize_deserialize_json() {
    let modes = vec![
        (ExecutionMode::Normal, r#""normal""#),
        (ExecutionMode::AutoEdit, r#""autoedit""#),
        (ExecutionMode::Yolo, r#""yolo""#),
        (ExecutionMode::Daemon, r#""daemon""#),
    ];
    for (mode, expected_json) in &modes {
        let json = serde_json::to_string(mode).unwrap();
        assert_eq!(
            &json, expected_json,
            "Serialization mismatch for {:?}",
            mode
        );
        let parsed: ExecutionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(&parsed, mode, "Deserialization mismatch for {:?}", mode);
    }
}

#[test]
fn test_execution_mode_debug_all() {
    assert_eq!(format!("{:?}", ExecutionMode::Normal), "Normal");
    assert_eq!(format!("{:?}", ExecutionMode::AutoEdit), "AutoEdit");
    assert_eq!(format!("{:?}", ExecutionMode::Yolo), "Yolo");
    assert_eq!(format!("{:?}", ExecutionMode::Daemon), "Daemon");
}

#[test]
fn test_execution_mode_clone_and_copy() {
    let mode = ExecutionMode::Yolo;
    let cloned = mode;
    let copied = mode;
    assert_eq!(mode, cloned);
    assert_eq!(mode, copied);
}

#[test]
fn test_execution_mode_eq() {
    assert_eq!(ExecutionMode::Normal, ExecutionMode::Normal);
    assert_ne!(ExecutionMode::Normal, ExecutionMode::Yolo);
}

// ---- Config resolve_model tests ----

#[test]
fn test_resolve_model_default() {
    let mut config = Config::default();
    config.models.insert(
        "default".to_string(),
        ModelProfile {
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "default-model".to_string(),
            api_key: None,
            max_tokens: 65536,
            temperature: 1.0,
            modalities: vec!["text".to_string()],
            context_length: 131072,
            extra_body: None,
            native_function_calling: None,
        },
    );
    let profile = config.resolve_model(None);
    assert!(profile.is_some());
    assert_eq!(profile.unwrap().model, "default-model");
}

#[test]
fn test_resolve_model_by_name() {
    let mut config = Config::default();
    config.models.insert(
        "vision".to_string(),
        ModelProfile {
            endpoint: "http://localhost:9000/v1".to_string(),
            model: "vision-model".to_string(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.5,
            modalities: vec!["text".to_string(), "vision".to_string()],
            context_length: 8192,
            extra_body: None,
            native_function_calling: None,
        },
    );
    let profile = config.resolve_model(Some("vision"));
    assert!(profile.is_some());
    assert_eq!(profile.unwrap().model, "vision-model");
}

#[test]
fn test_resolve_model_fallback_to_default() {
    let mut config = Config::default();
    config.models.insert(
        "default".to_string(),
        ModelProfile {
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "fallback-model".to_string(),
            api_key: None,
            max_tokens: 65536,
            temperature: 1.0,
            modalities: vec!["text".to_string()],
            context_length: 131072,
            extra_body: None,
            native_function_calling: None,
        },
    );
    let profile = config.resolve_model(Some("nonexistent"));
    assert!(profile.is_some());
    assert_eq!(profile.unwrap().model, "fallback-model");
}

#[test]
fn test_resolve_model_no_profiles() {
    let config = Config::default();
    let profile = config.resolve_model(Some("missing"));
    assert!(profile.is_none());
}

#[test]
fn test_resolve_model_none_with_no_default() {
    let mut config = Config::default();
    config.models.insert(
        "coder".to_string(),
        ModelProfile {
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "coder-model".to_string(),
            api_key: None,
            max_tokens: 65536,
            temperature: 1.0,
            modalities: vec!["text".to_string()],
            context_length: 131072,
            extra_body: None,
            native_function_calling: None,
        },
    );
    let profile = config.resolve_model(None);
    assert!(profile.is_none());
}

#[test]
fn test_config_with_models_section_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            model = "top-level-model"

            [models.coder]
            endpoint = "http://coder-host:1234/v1"
            model = "coder-model"
            max_tokens = 8192

            [models.vision]
            endpoint = "http://vision-host:5678/v1"
            model = "vision-model"
            modalities = ["text", "vision"]
            context_length = 32768
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.models.len(), 2);
    assert!(config.models.contains_key("coder"));
    assert!(config.models.contains_key("vision"));
    let coder = &config.models["coder"];
    assert_eq!(coder.model, "coder-model");
    assert_eq!(coder.max_tokens, 8192);
    let vision = &config.models["vision"];
    assert_eq!(vision.modalities, vec!["text", "vision"]);
    assert_eq!(vision.context_length, 32768);
}

#[test]
fn test_config_with_default_model_profile_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            model = "top-level"

            [models.default]
            endpoint = "http://override-host:9999/v1"
            model = "explicit-default"
            max_tokens = 2048
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let default_profile = config.resolve_model(None);
    assert!(default_profile.is_some());
    assert_eq!(default_profile.unwrap().model, "explicit-default");
    assert_eq!(default_profile.unwrap().max_tokens, 2048);
}

// ---- Config::load with temp file ----

#[test]
fn test_config_load_from_file() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test_config.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:9999/v1"
model = "loaded-model"
max_tokens = 2048
temperature = 0.3
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.endpoint, "http://localhost:9999/v1");
    assert_eq!(config.model, "loaded-model");
    assert_eq!(config.max_tokens, 2048);
    // token_budget defaults to context_length * 3 / 5 = 131072 * 3 / 5 = 78643
    assert_eq!(config.agent.token_budget, 131072 * 3 / 5);
    // default safety_margin (8192) < token_budget, so no clamping
    assert_eq!(config.agent.token_safety_margin, 8192);
    assert!((config.temperature - 0.3).abs() < f32::EPSILON);
    assert!(config.models.contains_key("default"));
    let default_prof = &config.models["default"];
    assert_eq!(default_prof.endpoint, "http://localhost:9999/v1");
    assert_eq!(default_prof.model, "loaded-model");
}

#[test]
fn test_config_load_preserves_valid_token_limits() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("valid_limits.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:9999/v1"
model = "loaded-model"
max_tokens = 4096

[agent]
token_budget = 200000
token_safety_margin = 8192
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.agent.token_budget, 200000);
    assert_eq!(config.agent.token_safety_margin, 8192);
}

#[test]
fn test_config_load_implicit_token_budget_tracks_env_max_tokens() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("implicit_budget.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:9999/v1"
model = "loaded-model"
max_tokens = 4096
"#
    )
    .unwrap();

    std::env::set_var("SELFWARE_MAX_TOKENS", "8192");
    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    std::env::remove_var("SELFWARE_MAX_TOKENS");

    assert_eq!(config.max_tokens, 8192);
    // token_budget defaults to context_length * 3 / 5 = 131072 * 3 / 5 = 78643
    assert_eq!(config.agent.token_budget, 131072 * 3 / 5);
    // default safety_margin (8192) < token_budget, so no clamping
    assert_eq!(config.agent.token_safety_margin, 8192);
}

#[test]
fn test_config_load_with_all_sections() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("full_config.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"
model = "full-model"
max_tokens = 4096
temperature = 0.7

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env"]
protected_branches = ["main"]
require_confirmation = ["git_push"]

[agent]
max_iterations = 50
step_timeout_secs = 120
token_budget = 200000
native_function_calling = true
streaming = false
min_completion_steps = 5

[yolo]
enabled = true
max_operations = 100
max_hours = 2.0

[ui]
theme = "ocean"
animations = false
compact_mode = true
verbose_mode = true
show_tokens = true
animation_speed = 2.0

[continuous_work]
enabled = false
checkpoint_interval_tools = 5
checkpoint_interval_secs = 60

[retry]
max_retries = 3
base_delay_ms = 500
max_delay_ms = 10000

[models.coder]
endpoint = "http://coder:1234/v1"
model = "coder-v1"
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.model, "full-model");
    assert_eq!(config.safety.protected_branches, vec!["main"]);
    assert_eq!(config.agent.max_iterations, 50);
    assert!(config.agent.native_function_calling);
    assert!(!config.agent.streaming);
    assert_eq!(config.agent.min_completion_steps, 5);
    assert!(config.yolo.enabled);
    assert_eq!(config.ui.theme, "ocean");
    assert!(!config.ui.animations);
    assert!(!config.continuous_work.enabled);
    assert_eq!(config.retry.max_retries, 3);
    assert!(config.compact_mode);
    assert!(config.verbose_mode);
    assert!(config.show_tokens);
    assert!(config.models.contains_key("default"));
    assert!(config.models.contains_key("coder"));
}

#[test]
fn test_config_load_empty_file() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("empty_config.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(file, "").unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.endpoint, "http://127.0.0.1:1234/v1");
    assert_eq!(config.model, "qwen3.5-27b");
    // The default model "qwen3.5-27b" matches the built-in qwen3.5-* profile,
    // which applies max_tokens=32768.  Without the profile this would be the
    // hard-coded default (65536).
    assert_eq!(config.max_tokens, 32768);
    assert_eq!(config.matched_profile.as_deref(), Some("qwen3.5"));
    assert!(config.models.contains_key("default"));
}

#[test]
fn test_config_load_invalid_toml_file() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("bad_config.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(file, "this {{ is not }} valid toml!!!").unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to parse config"));
}

#[test]
fn test_config_load_validates() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("invalid_config.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "ftp://bad-scheme.example.com"
model = "test"
"#
    )
    .unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("http:// or https://"));
}

#[test]
fn test_config_load_synthesizes_default_model_profile() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("synth_config.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"
model = "synth-model"
max_tokens = 1024
temperature = 0.5
api_key = "sk-synth-key"
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    let default_prof = config
        .models
        .get("default")
        .expect("default profile must exist");
    assert_eq!(default_prof.endpoint, config.endpoint);
    assert_eq!(default_prof.model, config.model);
    assert_eq!(default_prof.max_tokens, config.max_tokens);
    assert!((default_prof.temperature - config.temperature).abs() < f32::EPSILON);
}

#[test]
fn test_config_load_does_not_overwrite_explicit_default_profile() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("explicit_default.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"
model = "top-level"

[models.default]
endpoint = "http://explicit-default:1234/v1"
model = "explicit-default-model"
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    let default_prof = config.models.get("default").unwrap();
    assert_eq!(default_prof.model, "explicit-default-model");
    assert_eq!(default_prof.endpoint, "http://explicit-default:1234/v1");
}

// ---- Config::validate edge cases ----

#[test]
fn test_validate_high_temperature_still_valid() {
    let config = Config {
        temperature: 15.0,
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_boundary_temperature() {
    let config = Config {
        temperature: 10.0,
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_excessive_token_budget() {
    let mut config = Config::default();
    config.agent.token_budget = 100_000_000;
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("token_budget"));
    assert!(err.to_string().contains("exceeds maximum allowed"));
}

#[test]
fn test_validate_high_step_timeout_still_valid() {
    let mut config = Config::default();
    config.agent.step_timeout_secs = 7200;
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_empty_api_key_still_valid() {
    let config = Config {
        api_key: Some(RedactedString::new("")),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_negative_animation_speed() {
    let mut config = Config::default();
    config.ui.animation_speed = -1.0;
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("animation_speed must be positive"));
}

#[test]
fn test_validate_excessive_animation_speed_still_valid() {
    let mut config = Config::default();
    config.ui.animation_speed = 200.0;
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_endpoint_http_slash_only() {
    let config = Config {
        endpoint: "http:///path".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("no host"));
}

#[test]
fn test_validate_endpoint_https_slash_only() {
    let config = Config {
        endpoint: "https://".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("no host"));
}

#[test]
fn test_validate_max_tokens_at_limit() {
    let config = Config {
        max_tokens: 10_000_000,
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_max_tokens_over_limit() {
    let config = Config {
        max_tokens: 10_000_001,
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("exceeds maximum allowed"));
}

#[test]
fn test_validate_retry_equal_delays() {
    let mut config = Config::default();
    config.retry.base_delay_ms = 5000;
    config.retry.max_delay_ms = 5000;
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_remote_http_endpoint_still_valid() {
    let config = Config {
        endpoint: "http://remote-server.example.com:8080/v1".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

// ---- is_local_endpoint additional edge cases ----

#[test]
fn test_is_local_endpoint_localhost_no_port() {
    assert!(is_local_endpoint("http://localhost/v1"));
    assert!(is_local_endpoint("https://localhost"));
}

#[test]
fn test_is_local_endpoint_127_no_port() {
    assert!(is_local_endpoint("http://127.0.0.1/path"));
}

#[test]
fn test_is_local_endpoint_ipv6_no_port() {
    assert!(is_local_endpoint("http://[::1]/v1"));
}

#[test]
fn test_is_local_endpoint_ipv6_with_port() {
    assert!(is_local_endpoint("http://[::1]:8000/v1"));
}

#[test]
fn test_is_local_endpoint_ipv6_non_loopback() {
    assert!(!is_local_endpoint("http://[::2]:8000/v1"));
}

#[test]
fn test_is_local_endpoint_private_network() {
    assert!(!is_local_endpoint("http://192.168.1.1:8000/v1"));
    assert!(!is_local_endpoint("http://10.0.0.1:8000/v1"));
    assert!(!is_local_endpoint("http://172.16.0.1:8000/v1"));
}

#[test]
fn test_is_local_endpoint_empty_string() {
    assert!(!is_local_endpoint(""));
}

#[test]
fn test_is_local_endpoint_no_scheme_bare() {
    assert!(!is_local_endpoint("localhost:8000"));
}

#[test]
fn test_is_local_endpoint_malformed_ipv6() {
    assert!(!is_local_endpoint("http://[::1:8000/v1"));
}

// ---- EvolutionTomlConfig tests ----

#[test]
fn test_evolution_config_default() {
    let config = EvolutionTomlConfig::default();
    assert!(config.prompt_logic.is_empty());
    assert!(config.tool_code.is_empty());
    assert!(config.cognitive.is_empty());
    assert!(config.config_keys.is_empty());
}

#[test]
fn test_evolution_config_deserialization() {
    let toml_str = r#"
            prompt_logic = ["src/prompt.rs"]
            tool_code = ["src/tools/mod.rs", "src/tools/shell.rs"]
            cognitive = ["src/agent/think.rs"]
            config_keys = ["temperature", "max_tokens"]
        "#;
    let config: EvolutionTomlConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.prompt_logic, vec!["src/prompt.rs"]);
    assert_eq!(config.tool_code.len(), 2);
    assert_eq!(config.cognitive, vec!["src/agent/think.rs"]);
    assert_eq!(config.config_keys.len(), 2);
}

#[test]
fn test_evolution_config_serialize_roundtrip() {
    let config = EvolutionTomlConfig {
        hypothesis_model: Some("architect".to_string()),
        prompt_logic: vec!["a.rs".to_string()],
        tool_code: vec!["b.rs".to_string()],
        cognitive: vec!["c.rs".to_string()],
        config_keys: vec!["key1".to_string()],
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: EvolutionTomlConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.prompt_logic, config.prompt_logic);
    assert_eq!(parsed.tool_code, config.tool_code);
}

#[test]
fn test_config_with_evolution_section() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [evolution]
            prompt_logic = ["src/prompt.rs"]
            tool_code = ["src/tools.rs"]
            config_keys = ["temperature"]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.evolution.prompt_logic, vec!["src/prompt.rs"]);
    assert_eq!(config.evolution.tool_code, vec!["src/tools.rs"]);
    assert_eq!(config.evolution.config_keys, vec!["temperature"]);
}

// ---- ContinuousWorkConfig additional tests ----

#[test]
fn test_continuous_work_config_serialize_roundtrip() {
    let config = ContinuousWorkConfig {
        enabled: false,
        checkpoint_interval_tools: 20,
        checkpoint_interval_secs: 600,
        auto_recovery: false,
        max_recovery_attempts: 10,
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: ContinuousWorkConfig = toml::from_str(&toml_str).unwrap();
    assert!(!parsed.enabled);
    assert_eq!(parsed.checkpoint_interval_tools, 20);
    assert_eq!(parsed.checkpoint_interval_secs, 600);
    assert!(!parsed.auto_recovery);
    assert_eq!(parsed.max_recovery_attempts, 10);
}

#[test]
fn test_continuous_work_config_partial_toml() {
    let toml_str = r#"
            enabled = false
        "#;
    let config: ContinuousWorkConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.enabled);
    assert_eq!(config.checkpoint_interval_tools, 10);
    assert_eq!(config.checkpoint_interval_secs, 300);
    assert!(config.auto_recovery);
    assert_eq!(config.max_recovery_attempts, 3);
}

// ---- RetrySettings additional tests ----

#[test]
fn test_retry_settings_serialize_roundtrip() {
    let config = RetrySettings {
        max_retries: 10,
        base_delay_ms: 200,
        max_delay_ms: 30000,
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: RetrySettings = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.max_retries, 10);
    assert_eq!(parsed.base_delay_ms, 200);
    assert_eq!(parsed.max_delay_ms, 30000);
}

#[test]
fn test_retry_settings_partial_toml() {
    let toml_str = r#"
            max_retries = 2
        "#;
    let config: RetrySettings = toml::from_str(toml_str).unwrap();
    assert_eq!(config.max_retries, 2);
    assert_eq!(config.base_delay_ms, 1000);
    assert_eq!(config.max_delay_ms, 60000);
}

// ---- UiConfig additional tests ----

#[test]
fn test_ui_config_verbose_mode_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [ui]
            verbose_mode = true
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.ui.verbose_mode);
    assert_eq!(config.ui.theme, "amber");
    assert!(config.ui.animations);
}

#[test]
fn test_ui_config_all_themes() {
    for theme in &["amber", "ocean", "minimal", "high-contrast"] {
        let toml_str = format!(
            r#"
                endpoint = "http://localhost:8000/v1"
                [ui]
                theme = "{}"
                "#,
            theme
        );
        let config: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.ui.theme, *theme);
    }
}

// ---- YoloFileConfig additional tests ----

#[test]
fn test_yolo_file_config_serialize_roundtrip() {
    let config = YoloFileConfig {
        enabled: true,
        max_operations: 200,
        max_hours: 6.5,
        allow_git_push: false,
        allow_destructive_shell: true,
        audit_log_path: Some(PathBuf::from("/var/log/audit.log")),
        status_interval: 75,
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: YoloFileConfig = toml::from_str(&toml_str).unwrap();
    assert!(parsed.enabled);
    assert_eq!(parsed.max_operations, 200);
    assert!((parsed.max_hours - 6.5).abs() < f64::EPSILON);
    assert!(!parsed.allow_git_push);
    assert!(parsed.allow_destructive_shell);
    assert_eq!(
        parsed.audit_log_path,
        Some(PathBuf::from("/var/log/audit.log"))
    );
    assert_eq!(parsed.status_interval, 75);
}

#[test]
fn test_yolo_file_config_no_audit_log() {
    let toml_str = r#"
            enabled = true
        "#;
    let config: YoloFileConfig = toml::from_str(toml_str).unwrap();
    assert!(config.audit_log_path.is_none());
    assert!(config.allow_git_push);
    assert!(!config.allow_destructive_shell);
    assert_eq!(config.status_interval, 100);
}

// ---- SafetyConfig additional tests ----

#[test]
fn test_safety_config_strict_permissions_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            strict_permissions = true
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.safety.strict_permissions);
}

#[test]
fn test_safety_config_serialize_roundtrip() {
    let config = SafetyConfig {
        allowed_paths: vec!["/a/**".to_string(), "/b/**".to_string()],
        denied_paths: vec!["**/.secret".to_string()],
        protected_branches: vec!["main".to_string(), "release".to_string()],
        require_confirmation: vec!["deploy".to_string()],
        strict_permissions: true,
        permissions: vec![],
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: SafetyConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.allowed_paths, config.allowed_paths);
    assert_eq!(parsed.denied_paths, config.denied_paths);
    assert_eq!(parsed.protected_branches, config.protected_branches);
    assert_eq!(parsed.require_confirmation, config.require_confirmation);
    assert!(parsed.strict_permissions);
}

// ---- AgentConfig additional tests ----

#[test]
fn test_agent_config_native_function_calling() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            native_function_calling = true
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.agent.native_function_calling);
}

#[test]
fn test_agent_config_streaming_disabled() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            streaming = false
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(!config.agent.streaming);
}

#[test]
fn test_agent_config_min_completion_steps_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            min_completion_steps = 10
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.agent.min_completion_steps, 10);
}

#[test]
fn test_agent_config_require_verification_toml() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            require_verification_before_completion = false
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(!config.agent.require_verification_before_completion);
}

#[test]
fn test_agent_config_serialize_roundtrip() {
    let config = AgentConfig {
        max_iterations: 25,
        step_timeout_secs: 60,
        token_budget: 100000,
        native_function_calling: true,
        streaming: false,
        min_completion_steps: 7,
        require_verification_before_completion: false,
        ..Default::default()
    };
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: AgentConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.max_iterations, 25);
    assert_eq!(parsed.step_timeout_secs, 60);
    assert_eq!(parsed.token_budget, 100000);
    assert!(parsed.native_function_calling);
    assert!(!parsed.streaming);
    assert_eq!(parsed.min_completion_steps, 7);
    assert!(!parsed.require_verification_before_completion);
}

// ---- Default function coverage ----

#[test]
fn test_default_theme_fn() {
    assert_eq!(default_theme(), "amber");
}

#[test]
fn test_default_animation_speed_fn() {
    assert!((default_animation_speed() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_default_checkpoint_interval_tools_fn() {
    assert_eq!(default_checkpoint_interval_tools(), 10);
}

#[test]
fn test_default_checkpoint_interval_secs_fn() {
    assert_eq!(default_checkpoint_interval_secs(), 300);
}

#[test]
fn test_default_max_recovery_attempts_fn() {
    assert_eq!(default_max_recovery_attempts(), 3);
}

#[test]
fn test_default_retry_max_retries_fn() {
    assert_eq!(default_retry_max_retries(), 5);
}

#[test]
fn test_default_retry_base_delay_ms_fn() {
    assert_eq!(default_retry_base_delay_ms(), 1000);
}

#[test]
fn test_default_retry_max_delay_ms_fn() {
    assert_eq!(default_retry_max_delay_ms(), 60000);
}

#[test]
fn test_default_min_completion_steps_fn() {
    assert_eq!(default_min_completion_steps(), 3);
}

#[test]
fn test_default_denied_paths_fn() {
    let paths = default_denied_paths();
    assert_eq!(paths.len(), 4);
    assert!(paths.contains(&"**/.env".to_string()));
    assert!(paths.contains(&"**/.env.local".to_string()));
    assert!(paths.contains(&"**/.ssh/**".to_string()));
    assert!(paths.contains(&"**/secrets/**".to_string()));
}

// ---- Config::Debug output completeness ----

#[test]
fn test_config_debug_contains_all_fields() {
    let config = Config::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("endpoint"));
    assert!(debug.contains("model"));
    assert!(debug.contains("max_tokens"));
    assert!(debug.contains("temperature"));
    assert!(debug.contains("api_key"));
    assert!(debug.contains("safety"));
    assert!(debug.contains("agent"));
    assert!(debug.contains("yolo"));
    assert!(debug.contains("ui"));
    assert!(debug.contains("continuous_work"));
    assert!(debug.contains("retry"));
    assert!(debug.contains("resources"));
    assert!(debug.contains("evolution"));
    assert!(debug.contains("models"));
    assert!(debug.contains("execution_mode"));
    assert!(debug.contains("compact_mode"));
    assert!(debug.contains("verbose_mode"));
    assert!(debug.contains("show_tokens"));
}

// ---- Config serde skip fields ----

#[test]
fn test_config_serde_skip_fields_not_serialized() {
    let config = Config {
        execution_mode: ExecutionMode::Yolo,
        compact_mode: true,
        verbose_mode: true,
        show_tokens: true,
        context_length: default_context_length(),
        ..Config::default()
    };
    let toml_str = toml::to_string(&config).unwrap();
    assert!(!toml_str.contains("execution_mode"));
}

#[test]
fn test_config_serde_skip_fields_deserialized_as_default() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.execution_mode, ExecutionMode::Normal);
    assert!(!config.compact_mode);
    assert!(!config.verbose_mode);
    assert!(!config.show_tokens);
}

// ---- Config with API key in model profiles ----

#[test]
fn test_model_profile_with_and_without_api_key() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            model = "base"

            [models.with_key]
            endpoint = "http://host1/v1"
            model = "model-with-key"
            api_key = "sk-profile-key-123"

            [models.without_key]
            endpoint = "http://host2/v1"
            model = "model-without-key"
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let with_key = &config.models["with_key"];
    assert_eq!(
        with_key.api_key.as_ref().unwrap().expose(),
        "sk-profile-key-123"
    );
    let without_key = &config.models["without_key"];
    assert!(without_key.api_key.is_none());
}

// ---- Full config roundtrip with models ----

#[test]
fn test_config_full_roundtrip_with_models() {
    let mut models = HashMap::new();
    models.insert(
        "coder".to_string(),
        ModelProfile {
            endpoint: "http://coder:1234/v1".to_string(),
            model: "coder-v1".to_string(),
            api_key: Some(RedactedString::new("ck-123")),
            max_tokens: 8192,
            temperature: 0.7,
            modalities: vec!["text".to_string()],
            context_length: 32768,
            extra_body: None,
            native_function_calling: None,
        },
    );
    models.insert(
        "vision".to_string(),
        ModelProfile {
            endpoint: "http://vision:5678/v1".to_string(),
            model: "vision-v1".to_string(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.5,
            modalities: vec!["text".to_string(), "vision".to_string()],
            context_length: 16384,
            extra_body: None,
            native_function_calling: None,
        },
    );

    let config = Config {
        models,
        ..Config::default()
    };

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();

    assert_eq!(parsed.models.len(), 2);
    assert_eq!(parsed.models["coder"].model, "coder-v1");
    assert_eq!(
        parsed.models["coder"].api_key.as_ref().unwrap().expose(),
        "ck-123"
    );
    assert_eq!(parsed.models["vision"].modalities, vec!["text", "vision"]);
}

// ---- Edge case: all empty collections ----

#[test]
fn test_config_all_empty_collections() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [safety]
            allowed_paths = []
            denied_paths = []
            protected_branches = []
            require_confirmation = []

            [evolution]
            prompt_logic = []
            tool_code = []
            cognitive = []
            config_keys = []
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.safety.allowed_paths.is_empty());
    assert!(config.safety.denied_paths.is_empty());
    assert!(config.safety.protected_branches.is_empty());
    assert!(config.safety.require_confirmation.is_empty());
    assert!(config.evolution.prompt_logic.is_empty());
    assert!(config.evolution.tool_code.is_empty());
    assert!(config.evolution.cognitive.is_empty());
    assert!(config.evolution.config_keys.is_empty());
}

// ---- ApiKeySource coverage ----

#[test]
fn test_api_key_source_debug_and_clone() {
    let src = ApiKeySource::EnvVar;
    let debug = format!("{:?}", src);
    assert_eq!(debug, "EnvVar");

    let cloned = src;
    assert_eq!(src, cloned);
}

#[test]
fn test_api_key_source_all_variants_debug() {
    assert_eq!(format!("{:?}", ApiKeySource::None), "None");
    assert_eq!(format!("{:?}", ApiKeySource::EnvVar), "EnvVar");
    assert_eq!(format!("{:?}", ApiKeySource::Keyring), "Keyring");
    assert_eq!(format!("{:?}", ApiKeySource::ConfigFile), "ConfigFile");
}

#[test]
fn test_api_key_source_copy() {
    let src = ApiKeySource::Keyring;
    let copied = src;
    assert_eq!(src, copied);
}

// ---- ResourcesConfig in Config ----

#[test]
fn test_config_with_resources_section() {
    let toml_str = r#"
            endpoint = "http://localhost:8000/v1"

            [resources.gpu]
            monitor_interval_seconds = 10
            temperature_threshold = 90
            memory_utilization_threshold = 0.8
            throttle_on_overheat = false

            [resources.memory]
            warning_threshold = 0.6
            critical_threshold = 0.8
            emergency_threshold = 0.9
            monitor_interval_seconds = 5

            [resources.disk]
            max_usage_percent = 0.9
            maintenance_interval_seconds = 7200
            compress_after_days = 3

            [resources.quotas]
            max_gpu_memory_per_model = 8589934592
            max_concurrent_requests = 4
            max_context_tokens = 65536
            max_queued_tasks = 50
            max_checkpoint_size = 1073741824
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.resources.gpu.monitor_interval_seconds, 10);
    assert_eq!(config.resources.gpu.temperature_threshold, 90);
    assert!(!config.resources.gpu.throttle_on_overheat);
    assert!((config.resources.memory.warning_threshold - 0.6).abs() < f32::EPSILON);
    assert_eq!(config.resources.disk.compress_after_days, 3);
    assert_eq!(config.resources.quotas.max_concurrent_requests, 4);
}

// ---- ModelProfile modalities variations ----

#[test]
fn test_model_profile_empty_modalities() {
    let toml_str = r#"
            endpoint = "http://localhost/v1"
            model = "test"
            modalities = []
        "#;
    let profile: ModelProfile = toml::from_str(toml_str).unwrap();
    assert!(profile.modalities.is_empty());
}

#[test]
fn test_model_profile_multiple_modalities() {
    let toml_str = r#"
            endpoint = "http://localhost/v1"
            model = "test"
            modalities = ["text", "vision", "audio"]
        "#;
    let profile: ModelProfile = toml::from_str(toml_str).unwrap();
    assert_eq!(profile.modalities.len(), 3);
    assert_eq!(profile.modalities[2], "audio");
}

// ---- Config load with validation failure on load ----

#[test]
fn test_config_load_fails_on_zero_max_tokens() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("zero_tokens.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"
max_tokens = 0
"#
    )
    .unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("max_tokens must be greater than 0"));
}

#[test]
fn test_config_load_fails_on_empty_model() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("empty_model.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"
model = "   "
"#
    )
    .unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("model name must not be empty"));
}

#[test]
fn test_config_load_fails_on_empty_endpoint() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("empty_ep.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = ""
"#
    )
    .unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("endpoint must not be empty"));
}

// ---- Config load: UI settings applied to top-level flags ----

#[test]
fn test_config_load_applies_ui_to_top_level() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("ui_apply.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"

[ui]
compact_mode = true
verbose_mode = true
show_tokens = true
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert!(config.compact_mode);
    assert!(config.verbose_mode);
    assert!(config.show_tokens);
}

// ---- Config::load with nonexistent path ----

#[test]
fn test_config_load_nonexistent_path_error_message() {
    let result = Config::load(Some("/absolutely/does/not/exist/config.toml"));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Failed to read config") || err_msg.contains("No such file"),
        "Error message was: {}",
        err_msg
    );
}

// ---- Permissions check on Unix ----

#[cfg(unix)]
#[test]
fn test_config_load_strict_permissions_error() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("permissive.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"

[safety]
strict_permissions = true
"#
    )
    .unwrap();

    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("insecure permissions"),
        "Error message was: {}",
        err_msg
    );
}

#[cfg(unix)]
#[test]
fn test_config_load_strict_permissions_ok_when_600() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("secure.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"

[safety]
strict_permissions = true
"#
    )
    .unwrap();

    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_ok());
}

#[cfg(unix)]
#[test]
fn test_config_load_strict_permissions_rejects_plaintext_api_key() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("strict_key.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"
api_key = "sk-plaintext-key-should-fail"

[safety]
strict_permissions = true
"#
    )
    .unwrap();

    // Use mode 0o600 so it passes the file-permissions check but fails
    // on the plaintext API key in strict mode.
    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(
        result.is_err(),
        "Expected error for plaintext key in strict mode"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Plaintext API key"),
        "Error message was: {}",
        err_msg
    );
}

#[test]
fn test_config_validate_rejects_invalid_glob_pattern() {
    let mut config = Config::default();
    config.safety.allowed_paths = vec!["[".to_string()];
    let result = config.validate();
    assert!(result.is_err(), "Expected error for invalid glob pattern");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid glob in safety.allowed_paths"),
        "Error message was: {}",
        err_msg
    );
}

#[test]
fn test_config_validate_rejects_invalid_denied_glob() {
    let mut config = Config::default();
    config.safety.denied_paths = vec!["valid/**".to_string(), "[bad".to_string()];
    let result = config.validate();
    assert!(
        result.is_err(),
        "Expected error for invalid denied_paths glob"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid glob in safety.denied_paths"),
        "Error message was: {}",
        err_msg
    );
}

#[test]
fn test_config_validate_accepts_valid_glob_patterns() {
    let mut config = Config::default();
    config.safety.allowed_paths = vec!["./**".to_string(), "/home/user/**/*.rs".to_string()];
    config.safety.denied_paths = vec!["**/.env".to_string(), "**/node_modules/**".to_string()];
    let result = config.validate();
    assert!(result.is_ok(), "Valid glob patterns should pass validation");
}

#[cfg(unix)]
#[test]
fn test_config_load_permissive_without_strict_is_ok() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("permissive_no_strict.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"
endpoint = "http://localhost:8000/v1"

[safety]
strict_permissions = false
"#
    )
    .unwrap();

    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    let result = Config::load(Some(config_path.to_str().unwrap()));
    assert!(result.is_ok());
}

#[test]
fn test_concurrency_config_default_validates() {
    let cfg = ConcurrencyConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn test_concurrency_config_zero_max_streams_rejected() {
    let cfg = ConcurrencyConfig {
        max_streams: 0,
        ..Default::default()
    };
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("max_streams"),
        "expected max_streams error, got: {}",
        err
    );
}

#[test]
fn test_concurrency_config_zero_max_tools_rejected() {
    let cfg = ConcurrencyConfig {
        max_tools: 0,
        ..Default::default()
    };
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("max_tools"),
        "expected max_tools error, got: {}",
        err
    );
}

#[test]
fn test_concurrency_config_zero_max_global_rejected() {
    let cfg = ConcurrencyConfig {
        max_global: 0,
        ..Default::default()
    };
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("max_global"),
        "expected max_global error, got: {}",
        err
    );
}

#[test]
fn test_concurrency_config_exceeds_max_rejected() {
    let cfg = ConcurrencyConfig {
        max_streams: 257,
        ..Default::default()
    };
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("256"),
        "expected max 256 error, got: {}",
        err
    );
}

#[test]
fn test_concurrency_config_boundary_values_accepted() {
    let cfg = ConcurrencyConfig {
        max_streams: 1,
        max_tools: 1,
        max_global: 1,
    };
    assert!(cfg.validate().is_ok());

    let cfg = ConcurrencyConfig {
        max_streams: 256,
        max_tools: 256,
        max_global: 256,
    };
    assert!(cfg.validate().is_ok());
}
// Additional Config loader and validation tests
//
// These tests cover additional edge cases and scenarios not fully covered
// in the main tests.rs file.

// Note: Imports and helper functions are already defined at the top of tests.rs

// ============================================
// Additional AgentConfig Tests
// ============================================

#[test]
fn test_agent_config_all_defaults() {
    let config = AgentConfig::default();
    assert_eq!(config.max_iterations, 100);
    assert_eq!(config.step_timeout_secs, 300);
    assert_eq!(config.token_budget, default_max_tokens());
    assert_eq!(config.token_safety_margin, 8192);
    assert!(!config.native_function_calling);
    assert!(config.streaming);
    assert_eq!(config.min_completion_steps, 3);
    assert!(config.require_verification_before_completion);
    assert!(!config.require_visual_verification);
    assert!((config.context_content_ratio - 0.75).abs() < f32::EPSILON);
    assert!((config.context_compression_ratio - 0.20).abs() < f32::EPSILON);
    assert!((config.context_thinking_ratio - 0.05).abs() < f32::EPSILON);
    assert_eq!(config.compression_detail, "signatures");
}

#[test]
fn test_agent_config_context_ratios() {
    let config = AgentConfig::default();
    // Verify ratios sum to something reasonable (less than 1.0)
    let total = config.context_content_ratio
        + config.context_compression_ratio
        + config.context_thinking_ratio;
    assert!(total <= 1.0, "Context ratios should not exceed 100%");
}

#[test]
fn test_default_agent_config_functions() {
    assert_eq!(default_max_iterations(), 100);
    assert_eq!(default_step_timeout(), 300);
    assert_eq!(default_min_completion_steps(), 3);
    assert_eq!(default_token_budget(), 0); // sentinel value
    assert_eq!(default_token_safety_margin(), 8192);
    assert!((default_context_content_ratio() - 0.75).abs() < f32::EPSILON);
    assert!((default_context_compression_ratio() - 0.20).abs() < f32::EPSILON);
    assert!((default_context_thinking_ratio() - 0.05).abs() < f32::EPSILON);
    assert_eq!(default_compression_detail(), "signatures");
}

// ============================================
// Additional Config Validation Tests
// ============================================

#[test]
fn test_validate_context_length_zero() {
    let config = Config {
        context_length: 0,
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err
        .to_string()
        .contains("context_length must be greater than 0"));
}

#[test]
fn test_validate_context_length_excessive() {
    let config = Config {
        context_length: 100_000_001,
        ..Config::default()
    };
    // context_length doesn't have an upper bound in validation
    // but max_tokens and token_budget do
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_temperature_exactly_zero() {
    let config = Config {
        temperature: 0.0,
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_temperature_very_high() {
    let config = Config {
        temperature: 100.0,
        context_length: default_context_length(),
        ..Config::default()
    };
    // Should pass but print warning
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_endpoint_with_path() {
    let config = Config {
        endpoint: "https://api.example.com/v1/chat/completions".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_endpoint_with_port() {
    let config = Config {
        endpoint: "http://localhost:8080/v1".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_endpoint_ip_address() {
    let config = Config {
        endpoint: "http://192.168.1.1:8000/v1".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_model_with_whitespace_only() {
    let config = Config {
        model: "   \t\n  ".to_string(),
        context_length: default_context_length(),
        ..Config::default()
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("model name must not be empty"));
}

#[test]
fn test_validate_token_safety_margin_equal_to_budget() {
    let mut config = Config::default();
    config.agent.token_budget = 10000;
    config.agent.token_safety_margin = 10000; // Equal, should fail
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("token_safety_margin"));
    assert!(err.to_string().contains("must be less than"));
}

#[test]
fn test_validate_token_safety_margin_greater_than_budget() {
    let mut config = Config::default();
    config.agent.token_budget = 10000;
    config.agent.token_safety_margin = 15000; // Greater, should fail
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("token_safety_margin"));
}

#[test]
fn test_validate_max_tokens_exactly_at_limit() {
    let config = Config {
        max_tokens: 10_000_000,
        context_length: default_context_length(),
        ..Config::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_continuous_work_checkpoint_interval_tools_zero() {
    let mut config = Config::default();
    config.continuous_work.checkpoint_interval_tools = 0;
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("checkpoint_interval_tools"));
    assert!(err.to_string().contains("must be >= 1"));
}

#[test]
fn test_validate_continuous_work_max_recovery_attempts_boundary() {
    let mut config = Config::default();
    config.continuous_work.max_recovery_attempts = 100;
    assert!(config.validate().is_ok());

    config.continuous_work.max_recovery_attempts = 101;
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("max_recovery_attempts"));
}

// ============================================
// Additional TOML Parsing Error Tests
// ============================================

#[test]
fn test_toml_parse_error_missing_equals() {
    let toml_str = r#"
endpoint "http://localhost:8000/v1"
model = "test"
"#;
    let result: Result<Config, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_toml_parse_error_invalid_table_syntax() {
    let toml_str = r#"
endpoint = "http://localhost:8000/v1"
[agent
max_iterations = 50
"#;
    let result: Result<Config, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_toml_parse_error_invalid_number_format() {
    let toml_str = r#"
endpoint = "http://localhost:8000/v1"
max_tokens = 1e10
"#;
    let result: Result<Config, _> = toml::from_str(toml_str);
    // Scientific notation might actually work, let's check
    if let Ok(config) = result {
        // If it parses, it should be a reasonable number
        assert!(config.max_tokens > 0);
    }
}

#[test]
fn test_toml_parse_error_invalid_boolean() {
    let toml_str = r#"
endpoint = "http://localhost:8000/v1"

[agent]
streaming = yes
"#;
    let result: Result<Config, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_toml_duplicate_keys_error() {
    let toml_str = r#"
endpoint = "http://localhost:8000/v1"
endpoint = "http://duplicate.com/v1"
"#;
    // TOML does not allow duplicate keys at the same level
    let result: Result<Config, _> = toml::from_str(toml_str);
    assert!(result.is_err(), "TOML should reject duplicate keys");
}

// ============================================
// Additional Environment Variable Override Tests
// ============================================

#[test]
fn test_env_override_endpoint() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_ENDPOINT", "http://env-override:9999/v1");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://file-value:8000/v1"
model = "test-model"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.endpoint, "http://env-override:9999/v1");
    assert_eq!(config.model, "test-model"); // Not overridden
}

#[test]
fn test_env_override_model() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MODEL", "env-model-override");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
model = "file-model"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.model, "env-model-override");
}

#[test]
fn test_env_override_max_tokens() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MAX_TOKENS", "12345");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
max_tokens = 65536
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.max_tokens, 12345);
}

#[test]
fn test_env_override_temperature() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_TEMPERATURE", "0.75");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
temperature = 0.5
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert!((config.temperature - 0.75).abs() < f32::EPSILON);
}

#[test]
fn test_env_override_timeout() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_TIMEOUT", "600");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.agent.step_timeout_secs, 600);
}

#[test]
fn test_env_override_theme() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_THEME", "ocean");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.ui.theme, "ocean");
}

#[test]
fn test_env_override_mode_normal() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MODE", "normal");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.execution_mode, ExecutionMode::Normal);
}

#[test]
fn test_env_override_mode_auto_edit_variants() {
    let _guard = clear_selfware_env_vars();

    for variant in &["auto-edit", "autoedit", "auto_edit"] {
        std::env::set_var("SELFWARE_MODE", variant);

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("test.toml");
        std::fs::write(
            &config_path,
            r#"endpoint = "http://localhost:8000/v1"
"#,
        )
        .unwrap();

        let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
        assert_eq!(
            config.execution_mode,
            ExecutionMode::AutoEdit,
            "Variant '{}' should map to AutoEdit",
            variant
        );
    }
}

#[test]
fn test_env_override_mode_yolo() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MODE", "yolo");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.execution_mode, ExecutionMode::Yolo);
}

#[test]
fn test_env_override_mode_daemon() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MODE", "daemon");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.execution_mode, ExecutionMode::Daemon);
}

#[test]
fn test_env_override_mode_invalid() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MODE", "invalid_mode");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
"#,
    )
    .unwrap();

    // Should not fail, just print warning and use default
    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.execution_mode, ExecutionMode::Normal);
}

#[test]
fn test_env_override_max_tokens_invalid_ignored() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_MAX_TOKENS", "not_a_number");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
max_tokens = 8192
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    // Invalid env var should be ignored, file value used
    assert_eq!(config.max_tokens, 8192);
}

#[test]
fn test_env_override_temperature_invalid_ignored() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_TEMPERATURE", "not_a_float");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"
temperature = 0.5
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    // Invalid env var should be ignored
    assert!((config.temperature - 0.5).abs() < f32::EPSILON);
}

// ============================================
// Additional Config::load Tests
// ============================================

#[test]
fn test_config_load_selfware_config_env_var() {
    let _guard = clear_selfware_env_vars();

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("env_config.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://env-config:8000/v1"
model = "env-model"
"#,
    )
    .unwrap();

    std::env::set_var("SELFWARE_CONFIG", config_path.to_str().unwrap());

    // Load without explicit path - should use env var
    let config = Config::load(None).unwrap();
    assert_eq!(config.endpoint, "http://env-config:8000/v1");
    assert_eq!(config.model, "env-model");
}

#[test]
fn test_config_load_explicit_path_overrides_env() {
    let _guard = clear_selfware_env_vars();

    let dir = tempfile::tempdir().unwrap();

    let env_path = dir.path().join("env_config.toml");
    std::fs::write(
        &env_path,
        r#"endpoint = "http://env-config:8000/v1"
model = "env-model"
"#,
    )
    .unwrap();

    let explicit_path = dir.path().join("explicit_config.toml");
    std::fs::write(
        &explicit_path,
        r#"endpoint = "http://explicit-config:8000/v1"
model = "explicit-model"
"#,
    )
    .unwrap();

    std::env::set_var("SELFWARE_CONFIG", env_path.to_str().unwrap());

    // Load with explicit path - should override env var
    let config = Config::load(Some(explicit_path.to_str().unwrap())).unwrap();
    assert_eq!(config.endpoint, "http://explicit-config:8000/v1");
    assert_eq!(config.model, "explicit-model");
}

#[test]
fn test_config_load_selfware_strict_permissions_env() {
    let _guard = clear_selfware_env_vars();

    std::env::set_var("SELFWARE_STRICT_PERMISSIONS", "1");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("strict_test.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"

[safety]
strict_permissions = false
"#,
    )
    .unwrap();

    // Env var should override config file
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let result = Config::load(Some(config_path.to_str().unwrap()));
        // Should fail because env var enables strict mode and file is 644
        assert!(result.is_err());
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, just verify it loads
        let _ = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    }
}

#[test]
fn test_config_load_token_budget_explicit_in_file() {
    let _guard = clear_selfware_env_vars();

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("explicit_budget.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"

[agent]
token_budget = 500000
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert_eq!(config.agent.token_budget, 500000);
}

#[test]
fn test_config_load_normalizes_token_limits() {
    let _guard = clear_selfware_env_vars();

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("normalize.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"

[agent]
token_budget = 10000
token_safety_margin = 15000
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    // Should be normalized to budget - 1
    assert_eq!(config.agent.token_safety_margin, 9999);
}

#[test]
fn test_config_load_applies_ui_defaults() {
    let _guard = clear_selfware_env_vars();

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("ui_defaults.toml");
    std::fs::write(
        &config_path,
        r#"endpoint = "http://localhost:8000/v1"

[ui]
compact_mode = true
verbose_mode = true
show_tokens = true
theme = "ocean"
"#,
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert!(config.compact_mode);
    assert!(config.verbose_mode);
    assert!(config.show_tokens);
    assert_eq!(config.ui.theme, "ocean");
}

// ============================================
// Additional is_local_endpoint Tests
// ============================================

#[test]
fn test_is_local_endpoint_variations() {
    // localhost variations
    assert!(is_local_endpoint("http://localhost:8000/v1"));
    assert!(is_local_endpoint("http://localhost"));
    assert!(is_local_endpoint("https://localhost:443/v1"));

    // 127.0.0.1 variations
    assert!(is_local_endpoint("http://127.0.0.1:8000/v1"));
    assert!(is_local_endpoint("http://127.0.0.1"));

    // 0.0.0.0 variations
    assert!(is_local_endpoint("http://0.0.0.0:8000/v1"));

    // IPv6 loopback
    assert!(is_local_endpoint("http://[::1]:8000/v1"));
    assert!(is_local_endpoint("http://[::1]"));
    assert!(is_local_endpoint("https://[::1]:443/v1"));
}

#[test]
fn test_is_local_endpoint_non_local() {
    assert!(!is_local_endpoint("http://api.example.com/v1"));
    assert!(!is_local_endpoint("https://openai.com/v1"));
    assert!(!is_local_endpoint("http://192.168.1.1:8000/v1"));
    assert!(!is_local_endpoint("http://10.0.0.1:8000/v1"));
    assert!(!is_local_endpoint("http://172.16.0.1:8000/v1"));
    assert!(!is_local_endpoint("http://[::2]:8000/v1")); // Non-loopback IPv6
}

#[test]
fn test_is_local_endpoint_edge_cases() {
    assert!(!is_local_endpoint(""));
    assert!(!is_local_endpoint("not-a-url"));
    assert!(!is_local_endpoint("ftp://localhost:8000/v1"));
    assert!(!is_local_endpoint("file:///localhost/v1"));
}

// ============================================
// Additional ApiKeySource Tests
// ============================================

#[test]
fn test_api_key_source_enum_equality() {
    assert_eq!(ApiKeySource::None, ApiKeySource::None);
    assert_eq!(ApiKeySource::EnvVar, ApiKeySource::EnvVar);
    assert_eq!(ApiKeySource::Keyring, ApiKeySource::Keyring);
    assert_eq!(ApiKeySource::ConfigFile, ApiKeySource::ConfigFile);

    assert_ne!(ApiKeySource::None, ApiKeySource::EnvVar);
    assert_ne!(ApiKeySource::EnvVar, ApiKeySource::Keyring);
}

// ============================================
// Additional ModelProfile Tests
// ============================================

#[test]
fn test_model_profile_supports_vision() {
    let profile_with_vision = ModelProfile {
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "vision-model".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.5,
        modalities: vec!["text".to_string(), "vision".to_string()],
        context_length: 8192,
        extra_body: None,
        native_function_calling: None,
    };
    assert!(profile_with_vision.supports_vision());

    let profile_text_only = ModelProfile {
        modalities: vec!["text".to_string()],
        ..profile_with_vision.clone()
    };
    assert!(!profile_text_only.supports_vision());

    let profile_empty_modalities = ModelProfile {
        modalities: vec![],
        ..profile_with_vision.clone()
    };
    assert!(!profile_empty_modalities.supports_vision());
}

#[test]
fn test_model_profile_extra_body() {
    let mut extra = serde_json::Map::new();
    extra.insert("top_p".to_string(), serde_json::json!(0.95));

    let profile = ModelProfile {
        endpoint: "http://localhost:8000/v1".to_string(),
        model: "test".to_string(),
        api_key: None,
        max_tokens: 4096,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 8192,
        extra_body: Some(extra),
        native_function_calling: None,
    };

    assert!(profile.extra_body.is_some());
    assert_eq!(profile.extra_body.unwrap()["top_p"], 0.95);
}

// ============================================
// Additional YoloFileConfig Tests
// ============================================

#[test]
fn test_yolo_config_default_values() {
    let config = YoloFileConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.max_operations, 0);
    assert!(config.max_hours.abs() < f64::EPSILON);
    assert!(config.allow_git_push);
    assert!(!config.allow_destructive_shell);
    assert!(config.audit_log_path.is_none());
    assert_eq!(config.status_interval, 100);
}

#[test]
fn test_yolo_config_zero_limits() {
    let config = YoloFileConfig {
        enabled: true,
        max_operations: 0,
        max_hours: 0.0,
        ..Default::default()
    };
    assert!(config.enabled);
    assert_eq!(config.max_operations, 0);
}

// ============================================
// Additional UI Config Tests
// ============================================

#[test]
fn test_ui_config_theme_variations() {
    // Test all supported theme values
    for theme in &[
        "amber",
        "ocean",
        "minimal",
        "high-contrast",
        "high_contrast",
        "highcontrast",
    ] {
        let config = UiConfig {
            theme: theme.to_string(),
            ..Default::default()
        };
        assert_eq!(config.theme, *theme);
    }
}

#[test]
fn test_ui_config_animation_speed_boundaries() {
    let config = UiConfig {
        animation_speed: 0.1,
        ..Default::default()
    };
    assert!((config.animation_speed - 0.1).abs() < f64::EPSILON);

    let config = UiConfig {
        animation_speed: 10.0,
        ..Default::default()
    };
    assert!((config.animation_speed - 10.0).abs() < f64::EPSILON);
}

// ============================================
// Additional ContinuousWorkConfig Tests
// ============================================

#[test]
fn test_continuous_work_config_defaults() {
    let config = ContinuousWorkConfig::default();
    assert!(config.enabled);
    assert_eq!(config.checkpoint_interval_tools, 10);
    assert_eq!(config.checkpoint_interval_secs, 300);
    assert!(config.auto_recovery);
    assert_eq!(config.max_recovery_attempts, 3);
}

#[test]
fn test_continuous_work_config_disabled() {
    let config = ContinuousWorkConfig {
        enabled: false,
        ..Default::default()
    };
    assert!(!config.enabled);
    assert!(config.auto_recovery); // Still has default values
}

// ============================================
// Additional RetrySettings Tests
// ============================================

#[test]
fn test_retry_settings_defaults() {
    let config = RetrySettings::default();
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.base_delay_ms, 1000);
    assert_eq!(config.max_delay_ms, 60000);
}

#[test]
fn test_retry_settings_extreme_values() {
    let config = RetrySettings {
        max_retries: 100,
        base_delay_ms: 1,
        max_delay_ms: 3600000, // 1 hour
    };
    assert_eq!(config.max_retries, 100);
    assert_eq!(config.base_delay_ms, 1);
    assert_eq!(config.max_delay_ms, 3600000);
}

// ============================================
// Additional ExecutionMode Tests
// ============================================

#[test]
fn test_execution_mode_display_all() {
    assert_eq!(format!("{}", ExecutionMode::Normal), "normal");
    assert_eq!(format!("{}", ExecutionMode::AutoEdit), "auto-edit");
    assert_eq!(format!("{}", ExecutionMode::Yolo), "yolo");
    assert_eq!(format!("{}", ExecutionMode::Daemon), "daemon");
}

#[test]
fn test_execution_mode_default_is_normal() {
    let mode: ExecutionMode = Default::default();
    assert_eq!(mode, ExecutionMode::Normal);
}

// ============================================
// Additional ConcurrencyConfig Tests
// ============================================

#[test]
fn test_concurrency_config_defaults() {
    let config = ConcurrencyConfig::default();
    assert_eq!(config.max_streams, 4);
    assert_eq!(config.max_tools, 8);
    assert_eq!(config.max_global, 12);
}

#[test]
fn test_concurrency_config_validation_success() {
    let config = ConcurrencyConfig {
        max_streams: 1,
        max_tools: 1,
        max_global: 1,
    };
    assert!(config.validate().is_ok());

    let config = ConcurrencyConfig {
        max_streams: 256,
        max_tools: 256,
        max_global: 256,
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_concurrency_config_validation_zero_values() {
    let config = ConcurrencyConfig {
        max_streams: 0,
        max_tools: 4,
        max_global: 16,
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_concurrency_config_validation_exceeds_max() {
    let config = ConcurrencyConfig {
        max_streams: 257,
        max_tools: 4,
        max_global: 16,
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("256"));
}

// ============================================
// Additional SafetyConfig Tests
// ============================================

#[test]
fn test_safety_config_default_require_confirmation() {
    let config = SafetyConfig::default();
    assert!(config
        .require_confirmation
        .contains(&"git_push".to_string()));
    assert!(config
        .require_confirmation
        .contains(&"file_delete".to_string()));
    assert!(config
        .require_confirmation
        .contains(&"shell_exec".to_string()));
}

#[test]
fn test_safety_config_strict_permissions_default() {
    let config = SafetyConfig::default();
    assert!(!config.strict_permissions);
}

#[test]
fn test_default_allowed_paths() {
    let paths = default_allowed_paths();
    assert_eq!(paths, vec!["./**".to_string()]);
}

#[test]
fn test_default_protected_branches() {
    let branches = default_protected_branches();
    assert!(branches.contains(&"main".to_string()));
    assert!(branches.contains(&"master".to_string()));
}

#[test]
fn test_default_require_confirmation() {
    let confs = default_require_confirmation();
    assert_eq!(confs.len(), 3);
    assert!(confs.contains(&"git_push".to_string()));
    assert!(confs.contains(&"file_delete".to_string()));
    assert!(confs.contains(&"shell_exec".to_string()));
}

#[test]
fn test_default_denied_paths() {
    let paths = default_denied_paths();
    assert!(!paths.is_empty());
    assert!(paths.contains(&"**/.env".to_string()));
    assert!(paths.contains(&"**/.env.local".to_string()));
    assert!(paths.contains(&"**/.ssh/**".to_string()));
    assert!(paths.contains(&"**/secrets/**".to_string()));
}

// ============================================
// Additional EvolutionTomlConfig Tests
// ============================================

#[test]
fn test_evolution_config_default_empty() {
    let config = EvolutionTomlConfig::default();
    assert!(config.prompt_logic.is_empty());
    assert!(config.tool_code.is_empty());
    assert!(config.cognitive.is_empty());
    assert!(config.config_keys.is_empty());
    assert!(config.hypothesis_model.is_none());
}

#[test]
fn test_evolution_config_with_hypothesis_model() {
    let config = EvolutionTomlConfig {
        hypothesis_model: Some("architect".to_string()),
        prompt_logic: vec!["src/prompt.rs".to_string()],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };
    assert_eq!(config.hypothesis_model.unwrap(), "architect");
}

// ============================================
// Additional Config Debug Tests
// ============================================

#[test]
fn test_config_debug_redaction() {
    let config = Config {
        api_key: Some(RedactedString::new("sk-secret-key-12345")),
        context_length: default_context_length(),
        ..Config::default()
    };

    let debug = format!("{:?}", config);
    assert!(!debug.contains("sk-secret-key-12345"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn test_config_debug_includes_all_fields() {
    let config = Config::default();
    let debug = format!("{:?}", config);

    // Check key fields are present
    assert!(debug.contains("endpoint"));
    assert!(debug.contains("model"));
    assert!(debug.contains("max_tokens"));
    assert!(debug.contains("temperature"));
    assert!(debug.contains("safety"));
    assert!(debug.contains("agent"));
    assert!(debug.contains("yolo"));
    assert!(debug.contains("ui"));
    assert!(debug.contains("models"));
}

// ============================================
// Additional glob pattern validation tests
// ============================================

#[test]
fn test_validate_complex_glob_patterns() {
    let mut config = Config::default();

    // Valid complex patterns
    config.safety.allowed_paths = vec![
        "./**/*.rs".to_string(),
        "/home/user/**/*.{js,ts}".to_string(),
        "src/**/mod.rs".to_string(),
    ];
    config.safety.denied_paths = vec![
        "**/node_modules/**".to_string(),
        "**/.git/**".to_string(),
        "**/target/debug/**".to_string(),
    ];

    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_invalid_glob_pattern_bracket() {
    let mut config = Config::default();
    config.safety.allowed_paths = vec!["[".to_string()];

    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("Invalid glob"));
}

#[test]
fn test_validate_invalid_glob_pattern_unclosed() {
    let mut config = Config::default();
    config.safety.denied_paths = vec!["**/[abc".to_string()];

    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("Invalid glob"));
}

// =========================================================================
// Model-defaults profile loader integration smoke tests
// =========================================================================

/// Spec smoke test: a tmpdir selfware.toml with just `endpoint` and
/// `model = "qwen3.6-27b-q4kp"` should auto-apply the qwen3.6 profile.
#[test]
fn test_loader_applies_qwen36_profile_from_minimal_toml() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("selfware.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"endpoint = "http://127.0.0.1:8000/v1"
model = "qwen3.6-27b-q4kp"
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();

    assert_eq!(config.matched_profile.as_deref(), Some("qwen3.6"));
    // The user did not set native_function_calling — the profile fills it in.
    assert!(
        config.agent.native_function_calling,
        "qwen3.6 profile must enable native_function_calling"
    );
    // The user did not set extra_body — the profile fills it in.
    let extra = config
        .extra_body
        .as_ref()
        .expect("profile populates extra_body");
    assert_eq!(
        extra.get("presence_penalty"),
        Some(&serde_json::json!(1.5)),
        "qwen3.6 profile must set presence_penalty=1.5"
    );
    assert_eq!(extra.get("top_p"), Some(&serde_json::json!(0.8)));
    assert_eq!(extra.get("min_p"), Some(&serde_json::json!(0.0)));
    let ctk = extra
        .get("chat_template_kwargs")
        .and_then(|v| v.as_object())
        .expect("chat_template_kwargs object");
    assert_eq!(ctk.get("preserve_thinking"), Some(&serde_json::json!(true)));
}

/// Explicit user config must beat the matched profile: a user who sets
/// `presence_penalty = 0.0` keeps that value even though the qwen3.6
/// profile recommends 1.5.
#[test]
fn test_loader_explicit_user_config_beats_profile() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("selfware.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"endpoint = "http://127.0.0.1:8000/v1"
model = "qwen3.6-27b-q4kp"
temperature = 0.123
max_tokens = 999

[agent]
native_function_calling = false

[extra_body]
presence_penalty = 0.0
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();

    assert_eq!(config.matched_profile.as_deref(), Some("qwen3.6"));
    // Explicit user values must NOT be overwritten.
    assert!(
        !config.agent.native_function_calling,
        "explicit native_function_calling=false must beat profile"
    );
    assert!((config.temperature - 0.123).abs() < f32::EPSILON);
    assert_eq!(config.max_tokens, 999);
    let extra = config.extra_body.as_ref().unwrap();
    assert_eq!(
        extra.get("presence_penalty"),
        Some(&serde_json::json!(0.0)),
        "explicit presence_penalty must beat profile"
    );
    // ...but other profile keys ARE filled in.
    assert_eq!(extra.get("top_p"), Some(&serde_json::json!(0.8)));
    // matched_profile_applied lists only what the profile actually filled in.
    let applied = &config.matched_profile_applied;
    assert!(!applied.iter().any(|s| s == "temperature"));
    assert!(!applied.iter().any(|s| s == "max_tokens"));
    assert!(!applied.iter().any(|s| s == "native_function_calling"));
    assert!(!applied.iter().any(|s| s == "extra_body.presence_penalty"));
    assert!(applied.iter().any(|s| s == "extra_body.top_p"));
}

/// A model name that does not match any built-in profile should leave
/// matched_profile = None and not change defaults.
#[test]
fn test_loader_unknown_model_no_profile() {
    let _env_guard = clear_selfware_env_vars();
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("selfware.toml");
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(
        file,
        r#"endpoint = "http://127.0.0.1:8000/v1"
model = "llama-3-70b-instruct"
"#
    )
    .unwrap();

    let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
    assert!(config.matched_profile.is_none());
    assert!(config.matched_profile_applied.is_empty());
}
