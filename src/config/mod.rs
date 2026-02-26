//! Configuration Management
//!
//! Loads and manages agent configuration from TOML files.
//! Configuration includes:
//! - API settings (base URL, model selection)
//! - Agent behavior (max iterations, context limits)
//! - Safety settings (allowed paths, blocked commands)
//! - Tool-specific options

pub mod typed;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    // SECURITY NOTE: `api_key` is stored as a plain `String`. The key material
    // may linger in memory after the value is dropped because Rust's allocator
    // does not guarantee zeroing. Ideally this should use a wrapper like
    // `secrecy::SecretString` that zeroes on drop. The custom `Debug` impl
    // below ensures the key is never printed in logs or debug output.
    /// API authentication key (can also be set via `SELFWARE_API_KEY` env var).
    pub api_key: Option<String>,

    #[serde(default)]
    pub safety: SafetyConfig,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub yolo: YoloFileConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub continuous_work: ContinuousWorkConfig,

    #[serde(default)]
    pub retry: RetrySettings,

    /// Runtime execution mode (set via CLI, not persisted)
    #[serde(skip)]
    pub execution_mode: ExecutionMode,

    /// Compact output mode (less visual chrome) - CLI override
    #[serde(skip)]
    pub compact_mode: bool,

    /// Verbose output mode (detailed tool output) - CLI override
    #[serde(skip)]
    pub verbose_mode: bool,

    /// Always show token usage after responses - CLI override
    #[serde(skip)]
    pub show_tokens: bool,
}

// Manual `Debug` implementation that redacts the `api_key` field to prevent
// accidental exposure of credentials in logs, error messages, or debug output.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("safety", &self.safety)
            .field("agent", &self.agent)
            .field("yolo", &self.yolo)
            .field("ui", &self.ui)
            .field("continuous_work", &self.continuous_work)
            .field("retry", &self.retry)
            .field("execution_mode", &self.execution_mode)
            .field("compact_mode", &self.compact_mode)
            .field("verbose_mode", &self.verbose_mode)
            .field("show_tokens", &self.show_tokens)
            .finish()
    }
}

/// UI configuration for themes, animations, and output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Color theme: "amber", "ocean", "minimal", "high-contrast"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Enable animations (spinners, progress bars)
    #[serde(default = "default_true")]
    pub animations: bool,
    /// Default to compact mode
    #[serde(default)]
    pub compact_mode: bool,
    /// Default to verbose mode
    #[serde(default)]
    pub verbose_mode: bool,
    /// Always show token usage
    #[serde(default)]
    pub show_tokens: bool,
    /// Animation speed multiplier (1.0 = normal, 2.0 = faster)
    #[serde(default = "default_animation_speed")]
    pub animation_speed: f64,
}

/// Continuous work configuration for long-running sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousWorkConfig {
    /// Enable periodic checkpointing policy.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Save checkpoint after this many tool calls.
    #[serde(default = "default_checkpoint_interval_tools")]
    pub checkpoint_interval_tools: usize,
    /// Save checkpoint after this many seconds.
    #[serde(default = "default_checkpoint_interval_secs")]
    pub checkpoint_interval_secs: u64,
    /// Enable automatic recovery attempts when available.
    #[serde(default = "default_true")]
    pub auto_recovery: bool,
    /// Maximum recovery attempts per failure.
    #[serde(default = "default_max_recovery_attempts")]
    pub max_recovery_attempts: u32,
}

impl Default for ContinuousWorkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            checkpoint_interval_tools: default_checkpoint_interval_tools(),
            checkpoint_interval_secs: default_checkpoint_interval_secs(),
            auto_recovery: true,
            max_recovery_attempts: default_max_recovery_attempts(),
        }
    }
}

/// Retry configuration for API/network operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrySettings {
    /// Maximum retries before failing.
    #[serde(default = "default_retry_max_retries")]
    pub max_retries: u32,
    /// Initial delay before first retry.
    #[serde(default = "default_retry_base_delay_ms")]
    pub base_delay_ms: u64,
    /// Upper bound for retry delay.
    #[serde(default = "default_retry_max_delay_ms")]
    pub max_delay_ms: u64,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            max_retries: default_retry_max_retries(),
            base_delay_ms: default_retry_base_delay_ms(),
            max_delay_ms: default_retry_max_delay_ms(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            animations: true,
            compact_mode: false,
            verbose_mode: false,
            show_tokens: false,
            animation_speed: 1.0,
        }
    }
}

fn default_theme() -> String {
    "amber".to_string()
}
fn default_animation_speed() -> f64 {
    1.0
}
fn default_checkpoint_interval_tools() -> usize {
    10
}
fn default_checkpoint_interval_secs() -> u64 {
    300
}
fn default_max_recovery_attempts() -> u32 {
    3
}
fn default_retry_max_retries() -> u32 {
    5
}
fn default_retry_base_delay_ms() -> u64 {
    1000
}
fn default_retry_max_delay_ms() -> u64 {
    60000
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

/// Safety guardrails: allowed/denied paths, protected branches, and confirmation rules.
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

/// Agent behavior settings: iteration limits, timeouts, token budgets, and calling mode.
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
    /// Enable streaming responses for real-time output
    /// When true, LLM responses are displayed as they arrive
    #[serde(default = "default_true")]
    pub streaming: bool,
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
            ui: UiConfig::default(),
            continuous_work: ContinuousWorkConfig::default(),
            retry: RetrySettings::default(),
            execution_mode: ExecutionMode::default(),
            compact_mode: false,
            verbose_mode: false,
            show_tokens: false,
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
            streaming: true,
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
    if std::env::var("SELFWARE_TEST_MODE").is_ok() {
        return vec!["/**".to_string()];
    }
    vec!["./**".to_string()]
}
fn default_denied_paths() -> Vec<String> {
    vec![
        "**/.env".to_string(),
        "**/.env.local".to_string(),
        "**/.ssh/**".to_string(),
        "**/secrets/**".to_string(),
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

/// Check whether an endpoint URL points to a local address.
/// Local addresses include localhost, 127.0.0.1, [::1], and 0.0.0.0.
/// These are safe to use over plain HTTP since traffic stays on the machine.
fn is_local_endpoint(endpoint: &str) -> bool {
    // Extract host portion from the URL (after scheme, before port/path)
    let after_scheme = if endpoint.starts_with("https://") {
        &endpoint[8..]
    } else if endpoint.starts_with("http://") {
        &endpoint[7..]
    } else {
        return false;
    };

    // Handle bracketed IPv6 addresses like [::1]:8000/v1
    if after_scheme.starts_with('[') {
        // Extract the bracketed host (e.g., "[::1]")
        if let Some(bracket_end) = after_scheme.find(']') {
            let bracketed_host = &after_scheme[..=bracket_end];
            return bracketed_host == "[::1]";
        }
        return false;
    }

    // Get host (before port or path) for non-IPv6
    let host = after_scheme
        .split(':')
        .next()
        .unwrap_or(after_scheme)
        .split('/')
        .next()
        .unwrap_or(after_scheme);

    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0")
}

impl Config {
    /// On Unix, check whether a config file has overly permissive permissions
    /// (group- or world-readable). Since the config may contain API keys, we
    /// warn the user to tighten permissions.
    #[cfg(unix)]
    fn check_config_file_permissions(path: &str) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode();
            if mode & 0o077 != 0 {
                eprintln!(
                    "WARNING: Config file '{}' is accessible by other users (mode {:o}). \
                     This file may contain API keys. Consider running: chmod 600 {}",
                    path,
                    mode & 0o777,
                    path
                );
            }
        }
    }

    pub fn load(path: Option<&str>) -> Result<Self> {
        // SELFWARE_CONFIG env var overrides the config file path when no explicit
        // path is provided via CLI.
        let env_config_path = std::env::var("SELFWARE_CONFIG").ok();
        let effective_path: Option<&str> = path.or(env_config_path.as_deref());

        let mut loaded_from_path: Option<String> = None;
        let mut config = match effective_path {
            Some(p) => {
                let content = std::fs::read_to_string(p)
                    .with_context(|| format!("Failed to read config from {}", p))?;
                loaded_from_path = Some(p.to_string());
                toml::from_str(&content).context("Failed to parse config")?
            }
            None => {
                // Try default locations - expand ~ to actual home directory
                let home_config = dirs::home_dir()
                    .map(|h| h.join(".config/selfware/config.toml"))
                    .and_then(|p| p.to_str().map(String::from));

                let mut default_paths: Vec<&str> = vec!["selfware.toml"];
                let home_config_str: String;
                if let Some(ref hc) = home_config {
                    home_config_str = hc.clone();
                    default_paths.push(&home_config_str);
                }

                let mut loaded = None;
                for p in &default_paths {
                    if let Ok(content) = std::fs::read_to_string(p) {
                        loaded_from_path = Some(p.to_string());
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

        // On Unix, warn if the config file is world-readable since it may
        // contain API keys.
        #[cfg(unix)]
        if let Some(ref cfg_path) = loaded_from_path {
            Self::check_config_file_permissions(cfg_path);
        }
        // Suppress unused-variable warning on non-Unix platforms
        let _ = &loaded_from_path;

        // Override with environment variables
        if let Ok(endpoint) = std::env::var("SELFWARE_ENDPOINT") {
            config.endpoint = endpoint;
        }
        if let Ok(model) = std::env::var("SELFWARE_MODEL") {
            config.model = model;
        }
        if let Ok(api_key) = std::env::var("SELFWARE_API_KEY") {
            config.api_key = Some(api_key);
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
        if let Ok(timeout) = std::env::var("SELFWARE_TIMEOUT") {
            if let Ok(t) = timeout.parse::<u64>() {
                config.agent.step_timeout_secs = t;
            }
        }
        if let Ok(theme) = std::env::var("SELFWARE_THEME") {
            config.ui.theme = theme;
        }
        if let Ok(log_level) = std::env::var("SELFWARE_LOG_LEVEL") {
            match log_level.to_lowercase().as_str() {
                "trace" | "debug" | "info" | "warn" | "error" => {
                    // Store in agent config for downstream tracing initialization.
                    // The actual tracing subscriber is configured by the caller
                    // using this value, but we validate it here so invalid values
                    // are caught early.
                }
                other => {
                    eprintln!(
                        "Config warning: SELFWARE_LOG_LEVEL '{}' is not a valid level \
                         (expected trace, debug, info, warn, or error)",
                        other
                    );
                }
            }
        }
        if let Ok(mode) = std::env::var("SELFWARE_MODE") {
            match mode.to_lowercase().as_str() {
                "normal" => config.execution_mode = ExecutionMode::Normal,
                "auto-edit" | "autoedit" | "auto_edit" => {
                    config.execution_mode = ExecutionMode::AutoEdit;
                }
                "yolo" => config.execution_mode = ExecutionMode::Yolo,
                "daemon" => config.execution_mode = ExecutionMode::Daemon,
                other => {
                    eprintln!(
                        "Config warning: SELFWARE_MODE '{}' is not a valid mode \
                         (expected normal, auto-edit, yolo, or daemon)",
                        other
                    );
                }
            }
        }

        // Apply UI defaults from config (CLI flags will override later)
        config.compact_mode = config.ui.compact_mode;
        config.verbose_mode = config.ui.verbose_mode;
        config.show_tokens = config.ui.show_tokens;

        // Validate the loaded configuration
        config.validate()?;

        Ok(config)
    }

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
            if key.is_empty() {
                eprintln!("Config warning: api_key is set but empty");
            }
        }

        Ok(())
    }

    /// Apply UI settings to the global theme and output systems
    ///
    /// This should be called after loading config and before starting the agent.
    /// CLI flags can override the config file settings before calling this.
    pub fn apply_ui_settings(&self) {
        use crate::ui::theme::{set_theme, ThemeId};

        // Set theme from config
        let theme_id = match self.ui.theme.to_lowercase().as_str() {
            "ocean" => ThemeId::Ocean,
            "minimal" => ThemeId::Minimal,
            "high-contrast" | "highcontrast" | "high_contrast" => ThemeId::HighContrast,
            _ => ThemeId::Amber, // Default
        };
        set_theme(theme_id);

        // Initialize output module with current settings
        crate::output::init(self.compact_mode, self.verbose_mode, self.show_tokens);
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
        // When no config file exists in the specific path, it should return an error
        // Or wait, if we want to test default config values, just use Config::default()
        let config = Config::default();
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
    fn test_config_debug_redacts_api_key() {
        let config = Config {
            api_key: Some("sk-super-secret-key-12345".to_string()),
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
                streaming: true,
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
            execution_mode: ExecutionMode::default(),
            compact_mode: false,
            verbose_mode: false,
            show_tokens: false,
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
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("endpoint must not be empty"));
    }

    #[test]
    fn test_validate_invalid_endpoint_scheme() {
        let config = Config {
            endpoint: "ftp://example.com".to_string(),
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("http:// or https://"));
    }

    #[test]
    fn test_validate_endpoint_no_host() {
        let config = Config {
            endpoint: "http://".to_string(),
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("no host"));
    }

    #[test]
    fn test_validate_empty_model() {
        let config = Config {
            model: "   ".to_string(),
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("model name must not be empty"));
    }

    #[test]
    fn test_validate_zero_max_tokens() {
        let config = Config {
            max_tokens: 0,
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
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("exceeds maximum allowed"));
    }

    #[test]
    fn test_validate_negative_temperature() {
        let config = Config {
            temperature: -0.5,
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
            ..Config::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_remote_http_still_valid() {
        // Remote HTTP endpoints should still pass validation (warning only, not error)
        let config = Config {
            endpoint: "http://api.example.com/v1".to_string(),
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
}
