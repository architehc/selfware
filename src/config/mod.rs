//! Configuration Management
//!
//! Loads and manages agent configuration from TOML files.
//!
//! Submodules:
//! - [`model`]: RedactedString, ModelProfile
//! - [`agent`][]: AgentConfig (iteration limits, token budgets, etc.)
//! - [`safety`][]: SafetyConfig (allowed/denied paths, protected branches)
//! - [`types`]: ExecutionMode, UiConfig, ContinuousWorkConfig, RetrySettings,
//!   YoloFileConfig, EvolutionTomlConfig
//! - [`api_key`]: Keyring integration, endpoint validation
//! - `loader`: Config::load() and normalization
//! - `validation`: Config::validate()
//! - [`auto_config`]: Automatic configuration generation
//! - [`resources`][]: ResourcesConfig

pub mod agent;
pub mod api_key;
pub mod auto_config;
pub mod debug;
mod loader;
pub mod model;
pub mod model_profiles;
pub mod provenance;
pub mod resources;
pub mod safety;
pub mod types;
pub mod unpack;
mod validation;

pub use agent::*;
pub use api_key::{is_local_endpoint, load_api_key_from_keyring, save_api_key_to_keyring};
pub use auto_config::*;
pub use debug::DebugConfig;
pub use model::*;
pub use model_profiles::{
    apply_profile as apply_model_defaults_profile, builtin_profiles, match_profile,
    AppliedFields, ModelDefaultsProfile, UserExplicitFields,
};
pub use provenance::{ConfigSource, ConfigSources};
pub use resources::*;
pub use safety::*;
pub use types::*;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Fields in `extra_body` that are always safe (sampling / template params).
const EXTRA_BODY_ALLOWED: &[&str] = &[
    "chat_template_kwargs",
    "top_p",
    "top_k",
    "min_p",
    "presence_penalty",
    "repetition_penalty",
    "frequency_penalty",
    "seed",
];

/// Fields that could alter model behaviour in unexpected ways.
/// We allow them but emit a warning so the operator is aware.
const EXTRA_BODY_WARN: &[&str] = &["logit_bias", "stop", "response_format"];

/// Fields that would override core chat-completion request fields.
/// Allowing these would let a config silently replace the model, messages,
/// tool definitions, or streaming flag, which is a security risk.
const EXTRA_BODY_BLOCKED: &[&str] = &["model", "messages", "tools", "stream"];

/// Validate an `extra_body` map.
///
/// * Blocked keys -> hard error.
/// * Warned keys  -> `eprintln!` warning, request continues.
/// * Allowed keys -> pass silently.
/// * Unknown keys -> `eprintln!` warning (future-proof).
pub fn validate_extra_body(
    extra: &serde_json::Map<String, serde_json::Value>,
    label: &str,
) -> Result<()> {
    let allowed: HashSet<&str> = EXTRA_BODY_ALLOWED.iter().copied().collect();
    let warned: HashSet<&str> = EXTRA_BODY_WARN.iter().copied().collect();
    let blocked: HashSet<&str> = EXTRA_BODY_BLOCKED.iter().copied().collect();

    for key in extra.keys() {
        let k: &str = key.as_str();
        if blocked.contains(k) {
            bail!(
                "Config error: extra_body field '{}' in {} would override a core API request field and is not allowed",
                k,
                label,
            );
        }
        if warned.contains(k) {
            eprintln!(
                "Config warning: extra_body field '{}' in {} can alter model behaviour — use with caution",
                k,
                label,
            );
        } else if !allowed.contains(k) {
            eprintln!(
                "Config warning: extra_body field '{}' in {} is not a recognised safe field",
                k, label,
            );
        }
    }
    Ok(())
}

// Re-export default functions used by other config submodules via `super::`.
pub fn default_context_length() -> usize {
    131072
}
pub fn default_endpoint() -> String {
    "http://127.0.0.1:1234/v1".to_string()
}
pub fn default_model() -> String {
    "qwen3.5-27b".to_string()
}
pub fn default_max_tokens() -> usize {
    65536
}
pub fn default_temperature() -> f32 {
    1.0
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Context window length in tokens (must match vLLM --max-model-len)
    #[serde(default = "default_context_length")]
    pub context_length: usize,
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
    pub concurrency: ConcurrencyConfig,

    #[serde(default)]
    pub evolution: EvolutionTomlConfig,

    /// LLM response caching configuration
    #[serde(default)]
    pub cache: crate::session::cache::LlmCacheConfig,

    /// Debug-channel configuration (request/response/gate/turn logging).
    ///
    /// Layered as: defaults → `[debug]` TOML → CLI `--debug[=channels]` →
    /// `SELFWARE_DEBUG_*` env vars (force-on only).
    #[serde(default)]
    pub debug: DebugConfig,

    /// Named model profiles, keyed by ID (e.g. "coder", "vision").
    /// Populated from `[models.*]` TOML sections.  A `"default"` entry is
    /// auto-generated from the top-level endpoint/model/api_key fields if
    /// not explicitly provided.
    #[serde(default)]
    pub models: HashMap<String, ModelProfile>,

    /// Extra fields merged into every chat-completion request body.
    ///
    /// Use this for backend-specific extensions like SGLang's
    /// `chat_template_kwargs`.
    ///
    /// ```toml
    /// [extra_body]
    /// chat_template_kwargs = { enable_thinking = false }
    /// ```
    #[serde(default)]
    pub extra_body: Option<serde_json::Map<String, serde_json::Value>>,

    /// QA framework configuration for multi-language verification.
    #[serde(default)]
    pub qa: crate::testing::qa_profiles::QaConfig,

    /// MCP (Model Context Protocol) server connections.
    ///
    /// ```toml
    /// [[mcp.servers]]
    /// name = "github"
    /// command = "npx"
    /// args = ["-y", "@modelcontextprotocol/server-github"]
    /// env = { GITHUB_TOKEN = "..." }
    /// ```
    #[serde(default)]
    pub mcp: crate::mcp::McpConfig,

    /// Event hooks that run shell commands at key lifecycle points.
    ///
    /// ```toml
    /// [[hooks]]
    /// event = "PostToolUse"
    /// match_tools = ["file_write", "file_edit"]
    /// command = "cargo fmt -- {path}"
    /// ```
    #[serde(default)]
    pub hooks: Vec<crate::hooks::HookConfig>,

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

    /// Plan mode: agent reasons and proposes tool calls without executing them.
    #[serde(skip)]
    pub plan_mode: bool,

    /// Name of the built-in [`ModelDefaultsProfile`] that matched
    /// `self.model` and was applied during config load, if any.  Set by
    /// `Config::load`; `None` for [`Config::default`] or when no rule matches.
    #[serde(skip)]
    pub matched_profile: Option<String>,

    /// Names of fields the matched profile actually filled in (i.e. fields
    /// the user did NOT set explicitly).  Empty when `matched_profile` is
    /// `None` or when the user explicitly set every relevant field.
    #[serde(skip)]
    pub matched_profile_applied: Vec<String>,

    /// Provenance map: dotted field name → where the value came from.
    /// Populated by `Config::load`. Not persisted (transient runtime metadata).
    #[serde(skip)]
    pub sources: ConfigSources,
}

impl Config {
    /// Look up where a top-level field's effective value originated.
    ///
    /// Keys use dotted notation matching the TOML schema, e.g. `"endpoint"`,
    /// `"agent.native_function_calling"`, `"extra_body.top_p"`.
    pub fn source_of(&self, key: &str) -> ConfigSource {
        self.sources
            .get(key)
            .cloned()
            .unwrap_or(ConfigSource::Default)
    }
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
            .field("concurrency", &self.concurrency)
            .field("evolution", &self.evolution)
            .field("cache", &self.cache)
            .field("debug", &self.debug)
            .field("models", &self.models)
            .field("execution_mode", &self.execution_mode)
            .field("compact_mode", &self.compact_mode)
            .field("verbose_mode", &self.verbose_mode)
            .field("show_tokens", &self.show_tokens)
            .field("extra_body", &self.extra_body)
            .field("qa", &self.qa)
            .field("mcp", &self.mcp)
            .field("hooks", &self.hooks)
            .field("plan_mode", &self.plan_mode)
            .field("matched_profile", &self.matched_profile)
            .field("matched_profile_applied", &self.matched_profile_applied)
            .field("sources", &self.sources)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            context_length: default_context_length(),
            temperature: default_temperature(),
            api_key: None,
            safety: SafetyConfig::default(),
            agent: AgentConfig::default(),
            yolo: YoloFileConfig::default(),
            ui: UiConfig::default(),
            continuous_work: ContinuousWorkConfig::default(),
            retry: RetrySettings::default(),
            resources: ResourcesConfig::default(),
            concurrency: ConcurrencyConfig::default(),
            evolution: EvolutionTomlConfig::default(),
            cache: crate::session::cache::LlmCacheConfig::default(),
            debug: DebugConfig::default(),
            models: HashMap::new(),
            extra_body: None,
            qa: crate::testing::qa_profiles::QaConfig::default(),
            mcp: crate::mcp::McpConfig::default(),
            hooks: Vec::new(),
            execution_mode: ExecutionMode::default(),
            compact_mode: false,
            verbose_mode: false,
            show_tokens: false,
            plan_mode: false,
            matched_profile: None,
            matched_profile_applied: Vec::new(),
            sources: ConfigSources::new(),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
