//! Unit tests for the config module
//!
//! Tests cover:
//! - ExecutionMode enum
//! - Config defaults
//! - SafetyConfig
//! - AgentConfig
//! - UiConfig
//! - TOML serialization/deserialization

use selfware::config::{
    AgentConfig, Config, ExecutionMode, SafetyConfig, UiConfig, YoloFileConfig,
};

// ============================================================================
// ExecutionMode Tests
// ============================================================================

mod execution_mode_tests {
    use super::*;

    #[test]
    fn test_default_is_normal() {
        let mode = ExecutionMode::default();
        assert_eq!(mode, ExecutionMode::Normal);
    }

    #[test]
    fn test_display_normal() {
        assert_eq!(format!("{}", ExecutionMode::Normal), "normal");
    }

    #[test]
    fn test_display_auto_edit() {
        assert_eq!(format!("{}", ExecutionMode::AutoEdit), "auto-edit");
    }

    #[test]
    fn test_display_yolo() {
        assert_eq!(format!("{}", ExecutionMode::Yolo), "yolo");
    }

    #[test]
    fn test_display_daemon() {
        assert_eq!(format!("{}", ExecutionMode::Daemon), "daemon");
    }

    #[test]
    fn test_execution_mode_equality() {
        assert_eq!(ExecutionMode::Normal, ExecutionMode::Normal);
        assert_ne!(ExecutionMode::Normal, ExecutionMode::Yolo);
        assert_ne!(ExecutionMode::AutoEdit, ExecutionMode::Daemon);
    }

    #[test]
    fn test_execution_mode_clone() {
        let original = ExecutionMode::Yolo;
        let cloned = original;
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_execution_mode_debug() {
        let debug = format!("{:?}", ExecutionMode::Daemon);
        assert!(debug.contains("Daemon"));
    }
}

// ============================================================================
// UiConfig Tests
// ============================================================================

mod ui_config_tests {
    use super::*;

    #[test]
    fn test_default_ui_config() {
        let config = UiConfig::default();
        assert_eq!(config.theme, "amber");
        assert!(config.animations);
        assert!(!config.compact_mode);
        assert!(!config.verbose_mode);
        assert!(!config.show_tokens);
        assert!((config.animation_speed - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_ui_config_serialization() {
        let config = UiConfig {
            theme: "ocean".to_string(),
            animations: false,
            compact_mode: true,
            verbose_mode: false,
            show_tokens: true,
            animation_speed: 2.0,
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("theme = \"ocean\""));
        assert!(toml.contains("animations = false"));
        assert!(toml.contains("compact_mode = true"));
    }

    #[test]
    fn test_ui_config_deserialization() {
        let toml = r#"
            theme = "minimal"
            animations = true
            compact_mode = false
            verbose_mode = true
            show_tokens = false
            animation_speed = 1.5
        "#;

        let config: UiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.theme, "minimal");
        assert!(config.animations);
        assert!(config.verbose_mode);
        assert!((config.animation_speed - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_ui_config_clone() {
        let original = UiConfig::default();
        let cloned = original.clone();
        assert_eq!(original.theme, cloned.theme);
        assert_eq!(original.animations, cloned.animations);
    }
}

// ============================================================================
// SafetyConfig Tests
// ============================================================================

mod safety_config_tests {
    use super::*;

    #[test]
    fn test_default_safety_config() {
        let config = SafetyConfig::default();
        // Check that default values are sensible - allowed_paths should have defaults
        assert!(!config.allowed_paths.is_empty());
    }

    #[test]
    fn test_safety_config_with_allowed_paths() {
        let toml = r#"
            allowed_paths = ["/home/user/projects", "/tmp"]
            denied_paths = ["/etc"]
        "#;

        let config: SafetyConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.allowed_paths.len(), 2);
        assert!(config
            .allowed_paths
            .contains(&"/home/user/projects".to_string()));
        assert!(config.denied_paths.contains(&"/etc".to_string()));
    }

    #[test]
    fn test_safety_config_serialization() {
        let config = SafetyConfig {
            allowed_paths: vec!["/safe/path".to_string()],
            denied_paths: vec!["/dangerous".to_string()],
            protected_branches: vec!["main".to_string()],
            require_confirmation: vec!["git push".to_string()],
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("/safe/path"));
        assert!(toml.contains("/dangerous"));
        assert!(toml.contains("main"));
    }

    #[test]
    fn test_safety_config_protected_branches() {
        let config = SafetyConfig::default();
        // Default should protect main/master
        assert!(!config.protected_branches.is_empty());
    }
}

// ============================================================================
// AgentConfig Tests
// ============================================================================

mod agent_config_tests {
    use super::*;

    #[test]
    fn test_default_agent_config() {
        let config = AgentConfig::default();
        assert!(config.max_iterations > 0);
        assert!(config.token_budget > 0);
        assert!(config.streaming);
    }

    #[test]
    fn test_agent_config_serialization() {
        let config = AgentConfig {
            max_iterations: 50,
            step_timeout_secs: 120,
            token_budget: 8000,
            native_function_calling: true,
            streaming: false,
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("max_iterations = 50"));
        assert!(toml.contains("token_budget = 8000"));
        assert!(toml.contains("streaming = false"));
    }

    #[test]
    fn test_agent_config_deserialization() {
        let toml = r#"
            max_iterations = 100
            step_timeout_secs = 60
            token_budget = 16000
            native_function_calling = false
            streaming = true
        "#;

        let config: AgentConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.token_budget, 16000);
        assert!(!config.native_function_calling);
        assert!(config.streaming);
    }

    #[test]
    fn test_agent_config_step_timeout() {
        let config = AgentConfig::default();
        assert!(config.step_timeout_secs > 0);
    }
}

// ============================================================================
// YoloFileConfig Tests
// ============================================================================

mod yolo_config_tests {
    use super::*;

    #[test]
    fn test_default_yolo_config() {
        let config = YoloFileConfig::default();
        assert!(!config.enabled);
        assert!(config.allow_git_push);
        assert!(!config.allow_destructive_shell);
    }

    #[test]
    fn test_yolo_config_serialization() {
        let config = YoloFileConfig {
            enabled: true,
            max_operations: 100,
            max_hours: 8.0,
            allow_git_push: false,
            allow_destructive_shell: false,
            audit_log_path: Some(std::path::PathBuf::from("/var/log/agent.log")),
            status_interval: 10,
        };

        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("enabled = true"));
        assert!(toml.contains("max_operations = 100"));
    }

    #[test]
    fn test_yolo_config_deserialization() {
        let toml = r#"
            enabled = true
            max_operations = 50
            max_hours = 4.0
            allow_git_push = true
            allow_destructive_shell = false
            status_interval = 5
        "#;

        let config: YoloFileConfig = toml::from_str(toml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.max_operations, 50);
        assert!((config.max_hours - 4.0).abs() < 0.001);
    }
}

// ============================================================================
// Config Integration Tests
// ============================================================================

mod config_integration_tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(!config.endpoint.is_empty());
        assert!(!config.model.is_empty());
        assert!(config.max_tokens > 0);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let toml = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml).unwrap();

        assert_eq!(config.endpoint, parsed.endpoint);
        assert_eq!(config.model, parsed.model);
        assert_eq!(config.max_tokens, parsed.max_tokens);
    }

    #[test]
    fn test_config_with_all_sections() {
        let toml = r#"
            endpoint = "http://localhost:8080"
            model = "test-model"
            max_tokens = 4096
            temperature = 0.7

            [safety]
            allowed_paths = ["/home"]
            denied_paths = ["/etc"]
            protected_branches = ["main"]
            require_confirmation = ["git push"]

            [agent]
            max_iterations = 25
            step_timeout_secs = 60
            token_budget = 8000
            native_function_calling = true
            streaming = true

            [ui]
            theme = "ocean"
            animations = true
            compact_mode = false
            verbose_mode = false
            show_tokens = true
            animation_speed = 1.0

            [yolo]
            enabled = false
            max_operations = 0
            max_hours = 0.0
            allow_git_push = true
            allow_destructive_shell = false
            status_interval = 20
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.endpoint, "http://localhost:8080");
        assert_eq!(config.model, "test-model");
        assert_eq!(config.safety.allowed_paths[0], "/home");
        assert_eq!(config.agent.max_iterations, 25);
        assert_eq!(config.ui.theme, "ocean");
    }

    #[test]
    fn test_config_partial_toml() {
        // Only specify some fields, rest should use defaults
        let toml = r#"
            model = "custom-model"
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.model, "custom-model");
        // Defaults should be applied
        assert!(!config.endpoint.is_empty());
        assert!(config.max_tokens > 0);
    }

    #[test]
    fn test_config_execution_mode_not_serialized() {
        let config = Config {
            execution_mode: ExecutionMode::Yolo,
            ..Default::default()
        };

        let toml = toml::to_string(&config).unwrap();
        // execution_mode should be skipped during serialization
        assert!(!toml.contains("execution_mode"));
    }

    #[test]
    fn test_config_execution_mode_not_in_toml() {
        let config = Config {
            execution_mode: ExecutionMode::Yolo,
            ..Default::default()
        };

        let toml = toml::to_string(&config).unwrap();
        // execution_mode is marked with #[serde(skip)] so should not appear
        assert!(!toml.contains("execution_mode"));
    }
}
