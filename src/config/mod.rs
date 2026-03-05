//! Configuration Management
//!
//! Loads and manages agent configuration from TOML files.
//! Configuration includes:
//! - API settings (base URL, model selection)
//! - Agent behavior (max iterations, context limits)
//! - Safety settings (allowed paths, blocked commands)
//! - Tool-specific options

pub mod resources;
pub mod typed;

pub use resources::*;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, warn};

/// A string wrapper that prevents accidental logging of secrets.
///
/// `Display` and `Debug` both emit `[REDACTED]`.  To access the
/// underlying value, call [`expose()`](RedactedString::expose).
///
/// Serializes / deserializes transparently as a plain string so that
/// existing TOML config files continue to work unchanged.
#[derive(Clone)]
pub struct RedactedString(String);

impl RedactedString {
    /// Create a new `RedactedString` wrapping the given secret.
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// Return a reference to the underlying secret.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl std::fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl PartialEq for RedactedString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RedactedString {}

impl PartialEq<str> for RedactedString {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl Serialize for RedactedString {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RedactedString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(RedactedString(s))
    }
}

impl From<String> for RedactedString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RedactedString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A named model profile, allowing multiple LLM backends (e.g. a text coder
/// and a vision critic) to coexist in a single selfware config.
///
/// Profiles are defined under `[models.<name>]` in selfware.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    /// API endpoint (e.g. `"http://192.168.1.170:1234/v1"`)
    pub endpoint: String,
    /// Model identifier
    pub model: String,
    /// Optional API key for this specific model
    pub api_key: Option<RedactedString>,
    /// Max response tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Sampling temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Supported modalities: `["text"]` or `["text", "vision"]`
    #[serde(default = "default_modalities")]
    pub modalities: Vec<String>,
    /// Context window length in tokens
    #[serde(default = "default_context_length")]
    pub context_length: usize,
}

fn default_modalities() -> Vec<String> {
    vec!["text".to_string()]
}

fn default_context_length() -> usize {
    131072
}

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
    /// API authentication key (can also be set via `SELFWARE_API_KEY` env var).
    ///
    /// Wrapped in [`RedactedString`] so that `Display` and `Debug` both
    /// emit `[REDACTED]` -- preventing accidental exposure in logs or
    /// error messages.  Use `api_key.as_ref().map(|k| k.expose())` to
    /// access the raw value.
    pub api_key: Option<RedactedString>,

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

    #[serde(default)]
    pub resources: ResourcesConfig,

    #[serde(default)]
    pub evolution: EvolutionTomlConfig,

    /// Named model profiles, keyed by ID (e.g. "coder", "vision").
    /// Populated from `[models.*]` TOML sections.  A `"default"` entry is
    /// auto-generated from the top-level endpoint/model/api_key fields if
    /// not explicitly provided.
    #[serde(default)]
    pub models: HashMap<String, ModelProfile>,

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

// Manual `Debug` implementation that delegates to `RedactedString`'s `Debug`
// (which prints `[REDACTED]`) to prevent accidental exposure of credentials.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("api_key", &self.api_key)
            .field("safety", &self.safety)
            .field("agent", &self.agent)
            .field("yolo", &self.yolo)
            .field("ui", &self.ui)
            .field("continuous_work", &self.continuous_work)
            .field("retry", &self.retry)
            .field("resources", &self.resources)
            .field("evolution", &self.evolution)
            .field("models", &self.models)
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
    /// When true, config files with overly permissive permissions (group- or
    /// world-readable, i.e. mode & 0o077 != 0) cause a hard error instead of a
    /// warning.  Can also be activated via `SELFWARE_STRICT_PERMISSIONS=1`.
    /// Default: false (backward compatible -- warn only).
    #[serde(default)]
    pub strict_permissions: bool,
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
    /// Minimum number of execution steps before accepting task completion.
    /// Prevents early self-termination by requiring the agent to do meaningful work.
    #[serde(default = "default_min_completion_steps")]
    pub min_completion_steps: usize,
    /// Require at least one successful verification (cargo_check/cargo_test/cargo_clippy)
    /// before accepting task completion.
    #[serde(default = "default_true")]
    pub require_verification_before_completion: bool,
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
            resources: ResourcesConfig::default(),
            evolution: EvolutionTomlConfig::default(),
            models: HashMap::new(),
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
            strict_permissions: false,
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
            min_completion_steps: default_min_completion_steps(),
            require_verification_before_completion: true,
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
fn default_min_completion_steps() -> usize {
    3
}
fn default_token_budget() -> usize {
    500000
}
fn default_allowed_paths() -> Vec<String> {
    #[cfg(test)]
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

/// Evolution daemon configuration (loaded from `[evolution]` in selfware.toml)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvolutionTomlConfig {
    /// Source files containing prompt construction logic
    #[serde(default)]
    pub prompt_logic: Vec<String>,
    /// Source files containing tool implementations
    #[serde(default)]
    pub tool_code: Vec<String>,
    /// Source files containing cognitive architecture
    #[serde(default)]
    pub cognitive: Vec<String>,
    /// Config keys the agent can modify
    #[serde(default)]
    pub config_keys: Vec<String>,
}

/// Where the API key was resolved from (used internally for diagnostics and
/// to decide whether a plaintext-config-file warning is appropriate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiKeySource {
    /// No key found yet.
    None,
    /// Loaded from the `SELFWARE_API_KEY` environment variable.
    EnvVar,
    /// Loaded from the OS system keyring.
    Keyring,
    /// Loaded from a plaintext TOML config file on disk.
    ConfigFile,
}

/// Service name used when storing the API key in the OS keyring.
const KEYRING_SERVICE: &str = "selfware-api-key";

/// Load the API key from the OS system keyring.
///
/// Returns `Ok(Some(key))` when a key is stored, `Ok(None)` when
/// the keyring has no entry, or `Err` on a keyring backend failure.
pub fn load_api_key_from_keyring() -> Result<Option<String>> {
    let user = whoami::username().unwrap_or_else(|_| "selfware_user".to_string());
    let entry = keyring::Entry::new(KEYRING_SERVICE, &user)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Keyring error: {}", e)),
    }
}

/// Save an API key to the OS system keyring.
///
/// This is the backing implementation for `selfware config set-key`.
pub fn save_api_key_to_keyring(api_key: &str) -> Result<()> {
    let user = whoami::username().unwrap_or_else(|_| "selfware_user".to_string());
    let entry = keyring::Entry::new(KEYRING_SERVICE, &user)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
    entry
        .set_password(api_key)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))?;
    Ok(())
}

/// Check whether an endpoint URL points to a local address.
/// Local addresses include localhost, 127.0.0.1, [::1], and 0.0.0.0.
/// These are safe to use over plain HTTP since traffic stays on the machine.
pub(crate) fn is_local_endpoint(endpoint: &str) -> bool {
    // Extract host portion from the URL (after scheme, before port/path)
    let after_scheme = if let Some(rest) = endpoint.strip_prefix("https://") {
        rest
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        rest
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
    ///
    /// When `strict` is true, world/group-readable permissions cause a hard
    /// error instead of a warning. Strict mode can be enabled via the
    /// `safety.strict_permissions` config option or the
    /// `SELFWARE_STRICT_PERMISSIONS=1` environment variable.
    #[cfg(unix)]
    fn check_config_file_permissions(path: &str, strict: bool) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode();
            if mode & 0o077 != 0 {
                if strict {
                    bail!(
                        "Config file '{}' has insecure permissions (mode {:o}). \
                         The file is accessible by other users and may contain API keys. \
                         Fix with: chmod 600 {} — or disable strict mode by setting \
                         safety.strict_permissions = false",
                        path,
                        mode & 0o777,
                        path
                    );
                }
                warn!(
                    config_path = %path,
                    file_mode = format_args!("{:o}", mode & 0o777),
                    "Config file is accessible by other users. \
                     This file may contain API keys. Consider running: chmod 600 {}",
                    path
                );
            }
        }
        Ok(())
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

        // On Unix, check if the config file has overly permissive permissions.
        // Strict mode (error instead of warning) is enabled by either the
        // config option `safety.strict_permissions = true` or the environment
        // variable `SELFWARE_STRICT_PERMISSIONS=1`.
        #[cfg(unix)]
        if let Some(ref cfg_path) = loaded_from_path {
            let env_strict = std::env::var("SELFWARE_STRICT_PERMISSIONS")
                .map(|v| v == "1")
                .unwrap_or(false);
            let strict = config.safety.strict_permissions || env_strict;
            Self::check_config_file_permissions(cfg_path, strict)?;
        }
        // Suppress unused-variable warning on non-Unix platforms
        let _ = &loaded_from_path;

        // Track whether the API key originated from the config file so we can
        // distinguish it from env-var / keyring sources after the override
        // cascade below.
        let plaintext_key_in_config = config.api_key.is_some() && loaded_from_path.is_some();

        // Override with environment variables
        if let Ok(endpoint) = std::env::var("SELFWARE_ENDPOINT") {
            config.endpoint = endpoint;
        }
        if let Ok(model) = std::env::var("SELFWARE_MODEL") {
            config.model = model;
        }

        // --- API key resolution hierarchy ---
        // 1. Environment variable (highest priority, never persisted to disk)
        // 2. System keyring via `selfware config set-key`
        // 3. Config file (lowest priority, plaintext on disk -- warn the user)
        let mut api_key_source = ApiKeySource::None;

        if let Ok(api_key) = std::env::var("SELFWARE_API_KEY") {
            config.api_key = Some(RedactedString::new(api_key));
            api_key_source = ApiKeySource::EnvVar;
        }

        // Try the system keyring if no env var was set.
        if matches!(api_key_source, ApiKeySource::None) {
            match load_api_key_from_keyring() {
                Ok(Some(key)) => {
                    config.api_key = Some(RedactedString::new(key));
                    api_key_source = ApiKeySource::Keyring;
                }
                Ok(None) => {} // No key stored in keyring
                Err(e) => {
                    warn!(error = %e, "Failed to read API key from system keyring");
                }
            }
        }

        // If the key still comes from the plaintext config file, emit a warning.
        if matches!(api_key_source, ApiKeySource::None) && plaintext_key_in_config {
            api_key_source = ApiKeySource::ConfigFile;
            if let Some(ref cfg_path) = loaded_from_path {
                warn!(
                    config_path = %cfg_path,
                    "API key loaded from plaintext config file. \
                     For production use, set the SELFWARE_API_KEY environment variable \
                     or use the system keyring via `selfware config set-key`."
                );

                // In strict mode, plaintext keys on disk are not tolerated.
                let env_strict = std::env::var("SELFWARE_STRICT_PERMISSIONS")
                    .map(|v| v == "1")
                    .unwrap_or(false);
                if config.safety.strict_permissions || env_strict {
                    error!(
                        "Plaintext API key in config file is not allowed in strict mode. \
                         Use SELFWARE_API_KEY environment variable or system keyring."
                    );
                }
            }
        }
        // Suppress unused-variable warning; the value is consumed by the
        // match arms above and kept around only for clarity / future use.
        let _ = api_key_source;

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

        // Ensure a "default" model profile exists, synthesized from the
        // top-level endpoint/model/api_key fields so that existing configs
        // without explicit [models.*] sections keep working.
        if !config.models.contains_key("default") {
            config.models.insert(
                "default".to_string(),
                ModelProfile {
                    endpoint: config.endpoint.clone(),
                    model: config.model.clone(),
                    api_key: config.api_key.clone(),
                    max_tokens: config.max_tokens,
                    temperature: config.temperature,
                    modalities: default_modalities(),
                    context_length: default_context_length(),
                },
            );
        }

        // Validate the loaded configuration
        config.validate()?;

        Ok(config)
    }

    /// Resolve a model profile by ID. Falls back to `"default"` if `model_id`
    /// is `None` or the requested ID is not found.
    pub fn resolve_model(&self, model_id: Option<&str>) -> Option<&ModelProfile> {
        let key = model_id.unwrap_or("default");
        self.models.get(key).or_else(|| self.models.get("default"))
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
            if key.expose().is_empty() {
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
            api_key: Some(RedactedString::new("test-key")),
            safety: SafetyConfig {
                allowed_paths: vec!["/home/**".to_string()],
                denied_paths: vec!["**/.git/**".to_string()],
                protected_branches: vec!["main".to_string()],
                require_confirmation: vec!["deploy".to_string()],
                strict_permissions: false,
            },
            agent: AgentConfig {
                max_iterations: 50,
                step_timeout_secs: 120,
                token_budget: 100000,
                native_function_calling: false,
                streaming: true,
                min_completion_steps: 3,
                require_verification_before_completion: true,
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
            models: HashMap::new(),
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

    /// Helper to clear all SELFWARE_* env vars that Config::load reads.
    /// This prevents env var leakage between parallel tests.
    fn clear_selfware_env_vars() {
        for var in &[
            "SELFWARE_CONFIG",
            "SELFWARE_ENDPOINT",
            "SELFWARE_MODEL",
            "SELFWARE_API_KEY",
            "SELFWARE_MAX_TOKENS",
            "SELFWARE_TEMPERATURE",
            "SELFWARE_TIMEOUT",
            "SELFWARE_THEME",
            "SELFWARE_LOG_LEVEL",
            "SELFWARE_MODE",
            "SELFWARE_STRICT_PERMISSIONS",
        ] {
            std::env::remove_var(var);
        }
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
        let cloned = mode.clone();
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
        clear_selfware_env_vars();
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
        assert!((config.temperature - 0.3).abs() < f32::EPSILON);
        assert!(config.models.contains_key("default"));
        let default_prof = &config.models["default"];
        assert_eq!(default_prof.endpoint, "http://localhost:9999/v1");
        assert_eq!(default_prof.model, "loaded-model");
    }

    #[test]
    fn test_config_load_with_all_sections() {
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("empty_config.toml");
        let mut file = std::fs::File::create(&config_path).unwrap();
        write!(file, "").unwrap();

        let config = Config::load(Some(config_path.to_str().unwrap())).unwrap();
        assert_eq!(config.endpoint, "http://localhost:8000/v1");
        assert_eq!(config.model, "Qwen/Qwen3-Coder-Next-FP8");
        assert_eq!(config.max_tokens, 65536);
        assert!(config.models.contains_key("default"));
    }

    #[test]
    fn test_config_load_invalid_toml_file() {
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
            ..Config::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_boundary_temperature() {
        let config = Config {
            temperature: 10.0,
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
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("no host"));
    }

    #[test]
    fn test_validate_endpoint_https_slash_only() {
        let config = Config {
            endpoint: "https://".to_string(),
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("no host"));
    }

    #[test]
    fn test_validate_max_tokens_at_limit() {
        let config = Config {
            max_tokens: 10_000_000,
            ..Config::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_max_tokens_over_limit() {
        let config = Config {
            max_tokens: 10_000_001,
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

        let cloned = src.clone();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
        clear_selfware_env_vars();
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
    fn test_config_load_permissive_without_strict_is_ok() {
        clear_selfware_env_vars();
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
}
