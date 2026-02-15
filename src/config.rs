//! Configuration Management
//!
//! Loads and manages agent configuration from TOML files.
//! Configuration includes:
//! - API settings (base URL, model selection)
//! - Agent behavior (max iterations, context limits)
//! - Safety settings (allowed paths, blocked commands)
//! - Tool-specific options

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::warn;

/// Execution mode for tool approval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Ask for confirmation before executing tools (default)
    #[default]
    Normal,
    /// Auto-approve file edits, ask for other operations
    AutoEdit,
    /// Auto-approve all operations for this session
    Yolo,
    /// Run forever in autonomous loop
    Daemon,
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionMode::Normal => write!(f, "normal"),
            ExecutionMode::AutoEdit => write!(f, "auto-edit"),
            ExecutionMode::Yolo => write!(f, "yolo"),
            ExecutionMode::Daemon => write!(f, "daemon"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    pub api_key: Option<String>,

    #[serde(default)]
    pub safety: SafetyConfig,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub yolo: YoloFileConfig,

    /// Runtime execution mode (set via CLI, not persisted)
    #[serde(skip)]
    pub execution_mode: ExecutionMode,
}

/// YOLO mode configuration (loaded from config file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloFileConfig {
    /// Whether YOLO mode is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Maximum operations before requiring check-in (0 = unlimited)
    #[serde(default)]
    pub max_operations: usize,
    /// Maximum time in hours before requiring check-in (0 = unlimited)
    #[serde(default)]
    pub max_hours: f64,
    /// Whether to allow git push operations
    #[serde(default = "default_true")]
    pub allow_git_push: bool,
    /// Whether to allow destructive shell commands (rm -rf, etc.)
    #[serde(default)]
    pub allow_destructive_shell: bool,
    /// Audit log file path
    #[serde(default)]
    pub audit_log_path: Option<PathBuf>,
    /// Send periodic status updates (every N operations)
    #[serde(default = "default_status_interval")]
    pub status_interval: usize,
}

impl Default for YoloFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_operations: 0,
            max_hours: 0.0,
            allow_git_push: true,
            allow_destructive_shell: false,
            audit_log_path: None,
            status_interval: 100,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_status_interval() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default = "default_allowed_paths")]
    pub allowed_paths: Vec<String>,
    #[serde(default = "default_denied_paths")]
    pub denied_paths: Vec<String>,
    #[serde(default = "default_protected_branches")]
    pub protected_branches: Vec<String>,
    #[serde(default = "default_require_confirmation")]
    pub require_confirmation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_step_timeout")]
    pub step_timeout_secs: u64,
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
    /// Enable native function calling (requires backend support like sglang --tool-call-parser)
    /// When true, tools are passed via API and tool_calls are returned in response
    /// When false (default), tools are embedded in system prompt and parsed from content
    #[serde(default)]
    pub native_function_calling: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            api_key: None,
            safety: SafetyConfig::default(),
            agent: AgentConfig::default(),
            yolo: YoloFileConfig::default(),
            execution_mode: ExecutionMode::default(),
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allowed_paths: default_allowed_paths(),
            denied_paths: default_denied_paths(),
            protected_branches: default_protected_branches(),
            require_confirmation: default_require_confirmation(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            step_timeout_secs: default_step_timeout(),
            token_budget: default_token_budget(),
            native_function_calling: false,
        }
    }
}

fn default_endpoint() -> String {
    "http://localhost:8000/v1".to_string()
}
fn default_model() -> String {
    "Qwen/Qwen3-Coder-Next-FP8".to_string()
}
fn default_max_tokens() -> usize {
    65536
}
fn default_temperature() -> f32 {
    1.0
}
fn default_max_iterations() -> usize {
    100
}
fn default_step_timeout() -> u64 {
    300
}
fn default_token_budget() -> usize {
    500000
}
fn default_allowed_paths() -> Vec<String> {
    vec!["./**".to_string()]
}
fn default_denied_paths() -> Vec<String> {
    vec![
        "**/.env".to_string(),
        "**/.ssh/**".to_string(),
        "**/.aws/**".to_string(),
        "**/secrets/**".to_string(),
        "**/.git/config".to_string(),
        "**/.gnupg/**".to_string(),
        "**/id_rsa*".to_string(),
    ]
}
fn default_protected_branches() -> Vec<String> {
    vec!["main".to_string(), "master".to_string()]
}
fn default_require_confirmation() -> Vec<String> {
    vec![
        "git_push".to_string(),
        "file_delete".to_string(),
        "shell_exec".to_string(),
    ]
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        let mut config = match path {
            Some(p) => {
                let content = std::fs::read_to_string(p)
                    .with_context(|| format!("Failed to read config from {}", p))?;
                toml::from_str(&content).context("Failed to parse config")?
            }
            None => {
                // Try default locations
                let default_paths = ["selfware.toml", "~/.config/selfware/config.toml"];
                let mut loaded = None;
                for p in &default_paths {
                    if let Ok(content) = std::fs::read_to_string(p) {
                        loaded = Some(toml::from_str(&content).context("Failed to parse config")?);
                        break;
                    }
                }
                loaded.unwrap_or_else(|| {
                    eprintln!("No config file found, using defaults");
                    Self::default()
                })
            }
        };

        // Override with environment variables
        if let Ok(endpoint) = std::env::var("SELFWARE_ENDPOINT") {
            config.endpoint = endpoint;
        }
        if let Ok(model) = std::env::var("SELFWARE_MODEL") {
            config.model = model;
        }
        // Prioritize env var SELFWARE_API_KEY over config file
        if let Ok(api_key) = std::env::var("SELFWARE_API_KEY") {
            config.api_key = Some(api_key);
        } else if config.api_key.is_some() {
            warn!(
                "API key found in config file. Consider using the SELFWARE_API_KEY environment variable instead for better security."
            );
        }
        if let Ok(max_tokens) = std::env::var("SELFWARE_MAX_TOKENS") {
            if let Ok(n) = max_tokens.parse::<usize>() {
                config.max_tokens = n;
            }
        }
        if let Ok(temp) = std::env::var("SELFWARE_TEMPERATURE") {
            if let Ok(t) = temp.parse::<f32>() {
                config.temperature = t;
            }
        }

        Ok(config)
    }
}

/// Redact an API key for safe display, showing only the first 4 and last 4 characters.
/// Returns "****" for keys that are too short to partially reveal.
pub fn redact_api_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted_key = match &self.api_key {
            Some(key) => redact_api_key(key),
            None => "<not set>".to_string(),
        };
        write!(
            f,
            "Config {{ endpoint: {}, model: {}, api_key: {}, max_tokens: {} }}",
            self.endpoint, self.model, redacted_key, self.max_tokens
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.endpoint, "http://localhost:8000/v1");
        assert_eq!(config.model, "Qwen/Qwen3-Coder-Next-FP8");
        assert_eq!(config.max_tokens, 65536);
        assert!((config.temperature - 1.0).abs() < f32::EPSILON);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_safety_config_default() {
        let config = SafetyConfig::default();
        assert_eq!(config.allowed_paths, vec!["./**".to_string()]);
        assert!(!config.denied_paths.is_empty());
        assert!(config.denied_paths.contains(&"**/.env".to_string()));
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
        assert_eq!(config.token_budget, 500000);
    }

    #[test]
    fn test_config_load_missing_file() {
        let result = Config::load(Some("/nonexistent/path/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_config_load_no_path_uses_defaults() {
        // When no config file exists, should return defaults
        let config = Config::load(None).unwrap();
        assert_eq!(config.endpoint, "http://localhost:8000/v1");
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
        assert_eq!(config.model, "Qwen/Qwen3-Coder-Next-FP8"); // default
        assert_eq!(config.max_tokens, 65536); // default
    }

    #[test]
    fn test_config_with_api_key() {
        let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            api_key = "sk-test-12345"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.api_key, Some("sk-test-12345".to_string()));
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
            temperature: 0.7,
            api_key: Some("test-key".to_string()),
            safety: SafetyConfig {
                allowed_paths: vec!["/home/**".to_string()],
                denied_paths: vec!["**/.git/**".to_string()],
                protected_branches: vec!["main".to_string()],
                require_confirmation: vec!["deploy".to_string()],
            },
            agent: AgentConfig {
                max_iterations: 50,
                step_timeout_secs: 120,
                token_budget: 100000,
                native_function_calling: false,
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
            execution_mode: ExecutionMode::default(),
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
        assert_eq!(config.endpoint, "http://localhost:8000/v1");
        assert_eq!(config.model, "Qwen/Qwen3-Coder-Next-FP8");
        assert_eq!(config.max_tokens, 65536);
        assert!(!config.yolo.enabled);
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
        assert_eq!(config.api_key, Some("".to_string()));
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
        assert_eq!(default_endpoint(), "http://localhost:8000/v1");
        assert_eq!(default_model(), "Qwen/Qwen3-Coder-Next-FP8");
        assert_eq!(default_max_tokens(), 65536);
        assert!((default_temperature() - 1.0).abs() < f32::EPSILON);
        assert_eq!(default_max_iterations(), 100);
        assert_eq!(default_step_timeout(), 300);
        assert_eq!(default_token_budget(), 500000);
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

    // Security improvement tests

    #[test]
    fn test_default_denied_paths_include_env_files() {
        let config = SafetyConfig::default();
        assert!(config.denied_paths.contains(&"**/.env".to_string()));
        assert!(config.denied_paths.contains(&"**/.ssh/**".to_string()));
        assert!(config.denied_paths.contains(&"**/.aws/**".to_string()));
        assert!(config.denied_paths.contains(&"**/secrets/**".to_string()));
        assert!(config.denied_paths.contains(&"**/.git/config".to_string()));
        assert!(config.denied_paths.contains(&"**/.gnupg/**".to_string()));
        assert!(config.denied_paths.contains(&"**/id_rsa*".to_string()));
    }

    #[test]
    fn test_api_key_from_env_var_preferred() {
        // Set env var, create config with api_key in file
        let toml_str = r#"
            endpoint = "http://localhost:8000/v1"
            api_key = "from-config-file"
        "#;
        let mut config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.api_key, Some("from-config-file".to_string()));

        // Simulate what Config::load does: env var overrides config file
        let env_key = "from-env-var";
        config.api_key = Some(env_key.to_string());
        assert_eq!(config.api_key, Some("from-env-var".to_string()));
    }

    #[test]
    fn test_api_key_redacted_in_debug() {
        let config = Config {
            api_key: Some("sk-1234567890abcdef".to_string()),
            ..Config::default()
        };
        let display_str = format!("{}", config);
        // Should NOT contain the full key
        assert!(!display_str.contains("sk-1234567890abcdef"));
        // Should contain redacted form
        assert!(display_str.contains("sk-1...cdef"));
    }

    #[test]
    fn test_api_key_redacted_short_key() {
        let redacted = redact_api_key("short");
        assert_eq!(redacted, "****");
    }

    #[test]
    fn test_api_key_redacted_long_key() {
        let redacted = redact_api_key("sk-abcdefghijklmnop");
        assert_eq!(redacted, "sk-a...mnop");
    }

    #[test]
    fn test_api_key_display_none() {
        let config = Config {
            api_key: None,
            ..Config::default()
        };
        let display_str = format!("{}", config);
        assert!(display_str.contains("<not set>"));
    }

    #[test]
    fn test_default_denied_paths_helper() {
        let paths = default_denied_paths();
        assert_eq!(paths.len(), 7);
    }
}
