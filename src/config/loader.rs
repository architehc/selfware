//! Configuration loading, normalization, and UI application.

use anyhow::{bail, Context, Result};
use tracing::warn;

/// Known top-level TOML keys that map to Config fields.
/// Any key not in this list triggers a warning during config load.
/// The full set of recognized config keys across every level (top-level fields,
/// section names, and section sub-keys). Now only referenced by the config-key
/// sanity tests; the production top-level typo check uses
/// [`TOP_LEVEL_CONFIG_KEYS`]. Kept as a reference for a future per-section
/// nested-key validation.
#[cfg(test)]
const KNOWN_CONFIG_KEYS: &[&str] = &[
    // Top-level config keys
    "endpoint",
    "model",
    "max_tokens",
    "context_length",
    "temperature",
    "api_key",
    "execution_mode",
    "safety",
    "agent",
    "yolo",
    "ui",
    "continuous_work",
    "retry",
    "resources",
    "concurrency",
    "evolution",
    "cache",
    "debug",
    "models",
    "extra_body",
    "qa",
    "mcp",
    "hooks",
    // agent sub-keys
    "max_iterations",
    "step_timeout_secs",
    "token_budget",
    "native_function_calling",
    "streaming",
    "max_retries",
    "base_delay_ms",
    "max_delay_ms",
    "post_edit_test_command",
    // safety sub-keys
    "allowed_paths",
    "denied_paths",
    "require_confirmation",
    // yolo sub-keys
    "enabled",
    "max_operations",
    "max_hours",
    "allow_git_push",
    "allow_destructive_shell",
    "audit_log_path",
    "status_interval",
    // continuous_work sub-keys
    "checkpoint_interval_tools",
    "checkpoint_interval_secs",
    "auto_recovery",
    "max_recovery_attempts",
    // concurrency sub-keys
    "max_streams",
    "max_tools",
    "max_global",
    // resources sub-keys
    "gpu",
    "memory",
    "disk",
    "quotas",
    "gpu_memory_limit_gb",
    "memory_limit_gb",
    "disk_limit_gb",
];

/// Keys valid at the TOP LEVEL of a config file: scalar top-level fields
/// plus section names. Section SUB-keys (e.g. max_iterations under [agent])
/// are intentionally absent so a misplaced sub-key at the top level warns.
const TOP_LEVEL_CONFIG_KEYS: &[&str] = &[
    "endpoint",
    "model",
    "max_tokens",
    "context_length",
    "temperature",
    "api_key",
    "execution_mode",
    "safety",
    "agent",
    "yolo",
    "ui",
    "continuous_work",
    "retry",
    "resources",
    "concurrency",
    "evolution",
    "cache",
    "debug",
    "models",
    "extra_body",
    "qa",
    "mcp",
    "hooks",
];

use std::path::PathBuf;

use super::api_key::{is_insecure_remote_endpoint, load_api_key_from_keyring, ApiKeySource};
use super::model::{default_modalities, ModelProfile, RedactedString};
use super::model_profiles::{apply_profile, match_profile, UserExplicitFields};
use super::provenance::{ConfigSource, ConfigSources};
use super::types::ExecutionMode;
use super::Config;

/// Walk a parsed TOML value and record `ConfigSource::ConfigFile(path)` for
/// every leaf key reachable from a known top-level field. Nested keys are
/// flattened with `.` (e.g. `agent.native_function_calling`,
/// `extra_body.top_p`).
fn record_toml_sources(
    sources: &mut ConfigSources,
    table: &toml::value::Table,
    path: &PathBuf,
    prefix: &str,
) {
    for (k, v) in table.iter() {
        let dotted = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{}.{}", prefix, k)
        };
        match v {
            toml::Value::Table(t) => {
                sources.set(dotted.clone(), ConfigSource::ConfigFile(path.clone()));
                record_toml_sources(sources, t, path, &dotted);
            }
            _ => {
                sources.set(dotted, ConfigSource::ConfigFile(path.clone()));
            }
        }
    }
}

impl Config {
    fn content_sets_agent_token_budget(content: &str) -> bool {
        toml::from_str::<toml::Value>(content)
            .ok()
            .and_then(|value| value.get("agent").cloned())
            .and_then(|agent| agent.as_table().cloned())
            .map(|agent| agent.contains_key("token_budget"))
            .unwrap_or(false)
    }

    /// Warn about unknown top-level TOML keys that would be silently ignored.
    fn warn_unknown_keys(content: &str) {
        if let Ok(toml::Value::Table(table)) = toml::from_str::<toml::Value>(content) {
            for key in table.keys() {
                if !TOP_LEVEL_CONFIG_KEYS.contains(&key.as_str()) {
                    warn!(
                        key = %key,
                        "Unknown config key [{}] — this section is ignored. \
                         Check for typos or remove it.",
                        key
                    );
                }
            }
        }
    }

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
        let path_was_from_env = path.is_none() && env_config_path.is_some();

        let mut loaded_from_path: Option<String> = None;
        let mut token_budget_was_explicit = false;
        let mut sources = ConfigSources::new();
        // Raw TOML content as read from disk. Captured so we can later compute
        // which fields the user set explicitly and so model-defaults profiles
        // do not overwrite them.
        let mut raw_toml_content: Option<String> = None;
        let mut config = match effective_path {
            Some(p) => {
                let content = std::fs::read_to_string(p)
                    .with_context(|| format!("Failed to read config from {}", p))?;
                loaded_from_path = Some(p.to_string());
                token_budget_was_explicit = Self::content_sets_agent_token_budget(&content);
                Self::warn_unknown_keys(&content);
                let cfg: Config = toml::from_str(&content).with_context(|| {
                    format!("Failed to parse config file: {}. Check TOML syntax.", p)
                })?;
                if let Ok(toml::Value::Table(table)) = toml::from_str::<toml::Value>(&content) {
                    record_toml_sources(&mut sources, &table, &PathBuf::from(p), "");
                }
                if path_was_from_env {
                    sources.set(
                        "__config_path_source".to_string(),
                        ConfigSource::EnvVar("SELFWARE_CONFIG".to_string()),
                    );
                }
                raw_toml_content = Some(content);
                cfg
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
                        token_budget_was_explicit = Self::content_sets_agent_token_budget(&content);
                        Self::warn_unknown_keys(&content);
                        let cfg: Config = toml::from_str(&content).with_context(|| {
                            format!("Failed to parse config file: {}. Check TOML syntax.", p)
                        })?;
                        if let Ok(toml::Value::Table(table)) =
                            toml::from_str::<toml::Value>(&content)
                        {
                            record_toml_sources(&mut sources, &table, &PathBuf::from(*p), "");
                        }
                        raw_toml_content = Some(content);
                        loaded = Some(cfg);
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
            sources.set("endpoint", ConfigSource::EnvVar("SELFWARE_ENDPOINT".into()));
        }
        if let Ok(model) = std::env::var("SELFWARE_MODEL") {
            config.model = model;
            sources.set("model", ConfigSource::EnvVar("SELFWARE_MODEL".into()));
        }

        // --- API key resolution hierarchy ---
        // 1. Environment variable (highest priority, never persisted to disk)
        // 2. System keyring (set via SELFWARE_API_KEY or manually in OS keyring)
        // 3. Config file (lowest priority, plaintext on disk -- warn the user)
        let mut api_key_source = ApiKeySource::None;

        if let Ok(api_key) = std::env::var("SELFWARE_API_KEY") {
            config.api_key = Some(RedactedString::new(api_key));
            api_key_source = ApiKeySource::EnvVar;
        }

        // Try the system keyring if no env var was set.
        if matches!(api_key_source, ApiKeySource::None) {
            match load_api_key_from_keyring(&config.endpoint) {
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
                     or store it in the OS keyring."
                );

                // In strict mode, plaintext keys on disk are not tolerated.
                let env_strict = std::env::var("SELFWARE_STRICT_PERMISSIONS")
                    .map(|v| v == "1")
                    .unwrap_or(false);
                if config.safety.strict_permissions || env_strict {
                    bail!(
                        "Plaintext API key in config file is not allowed in strict mode. \
                         Use SELFWARE_API_KEY environment variable or store the key in the OS keyring."
                    );
                }
            }
        }
        // Suppress unused-variable warning; the value is consumed by the
        // match arms above and kept around only for clarity / future use.
        let _ = api_key_source;

        // Never send a credential over plaintext HTTP to a REMOTE host: a
        // checkout-local config can choose the endpoint, so an http:// remote
        // URL with a key present is a downgrade / exfiltration risk. Local HTTP
        // is fine (traffic stays on the machine).
        if config.api_key.is_some() && is_insecure_remote_endpoint(&config.endpoint) {
            bail!(
                "Refusing to send the API key over plaintext HTTP to a remote endpoint '{}'. \
                 Use https:// or a local endpoint (localhost / 127.0.0.1).",
                config.endpoint
            );
        }

        if let Ok(max_tokens) = std::env::var("SELFWARE_MAX_TOKENS") {
            if let Ok(n) = max_tokens.parse::<usize>() {
                config.max_tokens = n;
                sources.set(
                    "max_tokens",
                    ConfigSource::EnvVar("SELFWARE_MAX_TOKENS".into()),
                );
            }
        }
        if !token_budget_was_explicit {
            // Default token_budget to 60% of context_length — this is the usable
            // conversation budget for the ContextMap L1/L2/L3 file tracking.
            // The old default (max_tokens = output budget, often 16K) was far too
            // small and caused aggressive context eviction.
            config.agent.token_budget = config.context_length * 3 / 5;
        }
        if let Ok(temp) = std::env::var("SELFWARE_TEMPERATURE") {
            if let Ok(t) = temp.parse::<f32>() {
                config.temperature = t;
                sources.set(
                    "temperature",
                    ConfigSource::EnvVar("SELFWARE_TEMPERATURE".into()),
                );
            }
        }
        if let Ok(timeout) = std::env::var("SELFWARE_TIMEOUT") {
            if let Ok(t) = timeout.parse::<u64>() {
                config.agent.step_timeout_secs = t;
                sources.set(
                    "agent.step_timeout_secs",
                    ConfigSource::EnvVar("SELFWARE_TIMEOUT".into()),
                );
            }
        }
        if let Ok(cmd) = std::env::var("SELFWARE_POST_EDIT_TEST_COMMAND") {
            if !cmd.trim().is_empty() {
                config.agent.post_edit_test_command = Some(cmd);
                sources.set(
                    "agent.post_edit_test_command",
                    ConfigSource::EnvVar("SELFWARE_POST_EDIT_TEST_COMMAND".into()),
                );
            }
        }
        if let Ok(theme) = std::env::var("SELFWARE_THEME") {
            config.ui.theme = theme;
            sources.set("ui.theme", ConfigSource::EnvVar("SELFWARE_THEME".into()));
        }
        // SELFWARE_LOG_LEVEL is consumed by telemetry::init_tracing() as a
        // fallback when RUST_LOG is not set. No validation needed here — the
        // tracing EnvFilter handles invalid values gracefully.
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

        // Apply built-in model-defaults profile, if any pattern matches the
        // configured model name.  Profiles fill in *only* fields the user did
        // not set explicitly via TOML or env vars — explicit user config wins.
        // This pass is purely static: it never touches the network, only inspects
        // `config.model`.  We do this BEFORE synthesizing the "default" model
        // profile below so the synthesized profile inherits any tweaks (e.g.
        // Qwen 3.6's required `presence_penalty`).
        let mut user_explicit = match raw_toml_content.as_deref() {
            Some(content) => UserExplicitFields::from_toml(content),
            None => UserExplicitFields::default(),
        };
        // Bug fix: env-var overrides should also count as "explicit user
        // intent" — without this, `SELFWARE_TEMPERATURE=0.2` was silently
        // overwritten by the profile's default.  Consult the provenance map.
        if matches!(sources.get("temperature"), Some(ConfigSource::EnvVar(_))) {
            user_explicit.temperature = true;
        }
        if matches!(sources.get("max_tokens"), Some(ConfigSource::EnvVar(_))) {
            user_explicit.max_tokens = true;
        }
        if matches!(
            sources.get("agent.native_function_calling"),
            Some(ConfigSource::EnvVar(_))
        ) {
            user_explicit.native_function_calling = true;
        }
        if matches!(
            sources.get("agent.streaming"),
            Some(ConfigSource::EnvVar(_))
        ) {
            user_explicit.streaming = true;
        }
        if let Some(profile) = match_profile(&config.model) {
            let profile_name = profile.name.to_string();
            let applied = apply_profile(&mut config, &profile, &user_explicit);
            config.matched_profile = Some(profile_name.clone());
            if !applied.is_empty() {
                let mut fields: Vec<String> = Vec::new();
                if applied.native_function_calling {
                    fields.push("native_function_calling".to_string());
                }
                if applied.streaming {
                    fields.push("streaming".to_string());
                }
                if applied.temperature {
                    fields.push("temperature".to_string());
                }
                if applied.max_tokens {
                    fields.push("max_tokens".to_string());
                }
                for k in &applied.extra_body_keys {
                    fields.push(format!("extra_body.{}", k));
                }
                // Record provenance for every field the profile filled in so
                // `selfware config show` reports them as `[profile: <name>]`
                // instead of the misleading `[default]`.
                for f in &fields {
                    let dotted = match f.as_str() {
                        "native_function_calling" => "agent.native_function_calling".to_string(),
                        "streaming" => "agent.streaming".to_string(),
                        other => other.to_string(),
                    };
                    sources.set(dotted, ConfigSource::Profile(profile_name.clone()));
                }
                config.matched_profile_applied = fields;
            }
        }

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
                    context_length: config.context_length,
                    extra_body: config.extra_body.clone(),
                    native_function_calling: None,
                },
            );
        }

        // Normalize agent token limits so derived defaults and explicit values
        // both satisfy validation.  Local models have varying context sizes —
        // defaulting to 500k was wrong because it misrepresents the actual capacity.
        config.normalize_agent_limits();

        // Attach the provenance map.
        config.sources = sources;

        // Layer SELFWARE_DEBUG_* env-var force-ons on top of TOML / defaults.
        // CLI flags merge later in `cli::run` and re-apply env overrides so the
        // env vars always win.
        config.debug.apply_env_overrides();

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

    /// Normalize agent token limits so load-time defaults always satisfy the
    /// validation invariant `token_safety_margin < token_budget`.
    fn normalize_agent_limits(&mut self) {
        if self.agent.token_budget == 0 {
            self.agent.token_budget = self.max_tokens;
        }

        if self.agent.token_safety_margin >= self.agent.token_budget {
            let clamped_margin = self.agent.token_budget.saturating_sub(1);
            if self.agent.token_safety_margin != clamped_margin {
                warn!(
                    token_budget = self.agent.token_budget,
                    token_safety_margin = self.agent.token_safety_margin,
                    normalized_token_safety_margin = clamped_margin,
                    "Config normalization: clamping token_safety_margin to stay below token_budget"
                );
            }
            self.agent.token_safety_margin = clamped_margin;
        }
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
#[allow(clippy::field_reassign_with_default)] // test config builders: default-then-tweak is clearer
mod tests {
    use super::*;
    use crate::config::test_helpers::clear_env;
    use crate::config::{default_context_length, ExecutionMode};
    use std::io::Write;
    use std::path::PathBuf;

    // =========================================================================
    // Helper: write a TOML string to a temp file and return its path
    // =========================================================================
    fn write_temp_config(content: &str, filename: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(filename);
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "{content}").unwrap();
        (dir, path)
    }

    // =========================================================================
    // content_sets_agent_token_budget
    // =========================================================================

    #[test]
    fn test_content_sets_agent_token_budget_with_explicit_budget() {
        let content = r#"
            [agent]
            token_budget = 50000
        "#;
        assert!(Config::content_sets_agent_token_budget(content));
    }

    #[test]
    fn test_content_sets_agent_token_budget_without_agent_section() {
        let content = r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
        "#;
        assert!(!Config::content_sets_agent_token_budget(content));
    }

    #[test]
    fn test_content_sets_agent_token_budget_agent_section_without_budget() {
        let content = r#"
            [agent]
            max_iterations = 50
            step_timeout_secs = 120
        "#;
        assert!(!Config::content_sets_agent_token_budget(content));
    }

    #[test]
    fn test_content_sets_agent_token_budget_empty_content() {
        assert!(!Config::content_sets_agent_token_budget(""));
    }

    #[test]
    fn test_content_sets_agent_token_budget_invalid_toml_returns_false() {
        assert!(!Config::content_sets_agent_token_budget(
            "this is not toml {{{"
        ));
    }

    // =========================================================================
    // warn_unknown_keys  (verify it doesn't panic on various inputs)
    // =========================================================================

    #[test]
    fn test_warn_unknown_keys_all_known() {
        // Should not panic; all keys are in KNOWN_CONFIG_KEYS
        let content = r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            max_tokens = 4096
            temperature = 0.7
        "#;
        Config::warn_unknown_keys(content);
    }

    #[test]
    fn test_warn_unknown_keys_with_unknown() {
        // Unknown keys trigger a warning but should not panic
        let content = r#"
            endpoint = "http://localhost:8000/v1"
            unknown_key = "value"
            another_unknown = 42
        "#;
        Config::warn_unknown_keys(content);
    }

    #[test]
    fn test_warn_unknown_keys_empty_content() {
        Config::warn_unknown_keys("");
    }

    #[test]
    fn test_warn_unknown_keys_invalid_toml() {
        // Invalid TOML should be silently ignored (returns early)
        Config::warn_unknown_keys("this is not valid toml {{{");
    }

    // =========================================================================
    // record_toml_sources
    // =========================================================================

    #[test]
    fn test_record_toml_sources_flat_keys() {
        let content = r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            max_tokens = 4096
        "#;
        let table = toml::from_str::<toml::Value>(content).unwrap();
        let path = PathBuf::from("/tmp/test.toml");
        let mut sources = ConfigSources::new();
        record_toml_sources(&mut sources, table.as_table().unwrap(), &path, "");

        assert!(matches!(
            sources.get("endpoint"),
            Some(ConfigSource::ConfigFile(_))
        ));
        assert!(matches!(
            sources.get("model"),
            Some(ConfigSource::ConfigFile(_))
        ));
        assert!(matches!(
            sources.get("max_tokens"),
            Some(ConfigSource::ConfigFile(_))
        ));
    }

    #[test]
    fn test_record_toml_sources_nested_keys() {
        let content = r#"
            [agent]
            max_iterations = 50
            step_timeout_secs = 120

            [safety]
            allowed_paths = ["/safe"]
        "#;
        let table = toml::from_str::<toml::Value>(content).unwrap();
        let path = PathBuf::from("/tmp/nested.toml");
        let mut sources = ConfigSources::new();
        record_toml_sources(&mut sources, table.as_table().unwrap(), &path, "");

        // Nested table itself
        assert!(matches!(
            sources.get("agent"),
            Some(ConfigSource::ConfigFile(_))
        ));
        // Nested leaf
        assert!(matches!(
            sources.get("agent.max_iterations"),
            Some(ConfigSource::ConfigFile(_))
        ));
        assert!(matches!(
            sources.get("agent.step_timeout_secs"),
            Some(ConfigSource::ConfigFile(_))
        ));
        assert!(matches!(
            sources.get("safety.allowed_paths"),
            Some(ConfigSource::ConfigFile(_))
        ));
    }

    #[test]
    fn test_record_toml_sources_empty_table() {
        let mut sources = ConfigSources::new();
        let empty_table = toml::value::Table::new();
        let path = PathBuf::from("/tmp/empty.toml");
        record_toml_sources(&mut sources, &empty_table, &path, "");
        assert_eq!(sources.len(), 0);
    }

    #[test]
    fn test_record_toml_sources_records_correct_path() {
        let content = r#"model = "test""#;
        let table = toml::from_str::<toml::Value>(content).unwrap();
        let path = PathBuf::from("/custom/path/config.toml");
        let mut sources = ConfigSources::new();
        record_toml_sources(&mut sources, table.as_table().unwrap(), &path, "");

        match sources.get("model") {
            Some(ConfigSource::ConfigFile(p)) => {
                assert_eq!(p, &PathBuf::from("/custom/path/config.toml"));
            }
            _ => panic!("expected ConfigFile source"),
        }
    }

    // =========================================================================
    // normalize_agent_limits
    // =========================================================================

    #[test]
    fn test_normalize_agent_limits_zero_budget_defaults_to_max_tokens() {
        let mut config = Config::default();
        config.max_tokens = 50000;
        config.agent.token_budget = 0;
        config.agent.token_safety_margin = 100;
        config.normalize_agent_limits();
        assert_eq!(
            config.agent.token_budget, 50000,
            "zero token_budget should default to max_tokens"
        );
    }

    #[test]
    fn test_normalize_agent_limits_nonzero_budget_preserved() {
        let mut config = Config::default();
        config.agent.token_budget = 30000;
        config.agent.token_safety_margin = 100;
        config.normalize_agent_limits();
        assert_eq!(
            config.agent.token_budget, 30000,
            "nonzero token_budget should be preserved"
        );
    }

    #[test]
    fn test_normalize_agent_limits_clamps_safety_margin() {
        let mut config = Config::default();
        config.agent.token_budget = 10000;
        config.agent.token_safety_margin = 15000; // >= budget
        config.normalize_agent_limits();
        assert_eq!(
            config.agent.token_safety_margin, 9999,
            "safety margin should be clamped to budget - 1"
        );
    }

    #[test]
    fn test_normalize_agent_limits_equal_margin_clamped() {
        let mut config = Config::default();
        config.agent.token_budget = 10000;
        config.agent.token_safety_margin = 10000; // == budget
        config.normalize_agent_limits();
        assert_eq!(
            config.agent.token_safety_margin, 9999,
            "safety margin equal to budget should be clamped"
        );
    }

    #[test]
    fn test_normalize_agent_limits_valid_margin_preserved() {
        let mut config = Config::default();
        config.agent.token_budget = 50000;
        config.agent.token_safety_margin = 8192;
        config.normalize_agent_limits();
        assert_eq!(
            config.agent.token_safety_margin, 8192,
            "valid safety margin should be preserved"
        );
    }

    #[test]
    fn test_normalize_agent_limits_budget_one_margin_clamped_to_zero() {
        let mut config = Config::default();
        config.agent.token_budget = 1;
        config.agent.token_safety_margin = 1;
        config.normalize_agent_limits();
        assert_eq!(
            config.agent.token_safety_margin, 0,
            "budget=1, margin=1 → clamp to 0 (saturating_sub)"
        );
    }

    // =========================================================================
    // Config::load — file-based tests
    // =========================================================================

    #[test]
    fn test_load_missing_file_returns_error() {
        let _guard = clear_env();
        let result = Config::load(Some("/nonexistent/path/that/does/not/exist.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to read config"));
    }

    #[test]
    fn test_load_valid_file_basic_fields() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://my-endpoint:8000/v1"
            model = "my-model"
            max_tokens = 8192
            temperature = 0.3
            "#,
            "basic.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.endpoint, "http://my-endpoint:8000/v1");
        assert_eq!(config.model, "my-model");
        assert_eq!(config.max_tokens, 8192);
        assert!((config.temperature - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_load_empty_file_uses_defaults() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config("", "empty.toml");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.endpoint, "https://openrouter.ai/api/v1");
        assert_eq!(config.model, "z-ai/glm-5.2");
        assert_eq!(config.max_tokens, 65536);
    }

    #[test]
    fn test_load_invalid_toml_returns_error() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config("this is {{{ not toml !!!", "bad.toml");
        let result = Config::load(Some(path.to_str().unwrap()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse config"));
    }

    #[test]
    fn test_load_synthesizes_default_model_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "synth-model"
            max_tokens = 1024
            "#,
            "synth.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        let default = config
            .models
            .get("default")
            .expect("default profile should exist");
        assert_eq!(default.endpoint, "http://localhost:8000/v1");
        assert_eq!(default.model, "synth-model");
        assert_eq!(default.max_tokens, 1024);
    }

    #[test]
    fn test_load_does_not_overwrite_explicit_default_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://top-level:8000/v1"
            model = "top-model"

            [models.default]
            endpoint = "http://custom:9000/v1"
            model = "custom-model"
            "#,
            "explicit_default.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        let default = config.models.get("default").unwrap();
        assert_eq!(default.endpoint, "http://custom:9000/v1");
        assert_eq!(default.model, "custom-model");
        // Top-level fields should still be the top-level values
        assert_eq!(config.endpoint, "http://top-level:8000/v1");
        assert_eq!(config.model, "top-model");
    }

    #[test]
    fn test_load_implicit_token_budget_from_context_length() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            max_tokens = 4096
            context_length = 100000
            "#,
            "implicit_budget.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        // token_budget defaults to 60% of context_length when not explicit
        assert_eq!(
            config.agent.token_budget,
            100000 * 3 / 5,
            "implicit token_budget should be 60% of context_length"
        );
    }

    #[test]
    fn test_load_explicit_token_budget_preserved() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            context_length = 200000

            [agent]
            token_budget = 50000
            "#,
            "explicit_budget.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.agent.token_budget, 50000);
    }

    #[test]
    fn test_load_ui_settings_applied_to_top_level() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"

            [ui]
            compact_mode = true
            verbose_mode = true
            show_tokens = true
            "#,
            "ui.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!(
            config.compact_mode,
            "compact_mode should be applied from ui"
        );
        assert!(
            config.verbose_mode,
            "verbose_mode should be applied from ui"
        );
        assert!(config.show_tokens, "show_tokens should be applied from ui");
    }

    #[test]
    fn test_load_provenance_config_file_source() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "provenance-test"
            "#,
            "prov.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        match config.source_of("endpoint") {
            ConfigSource::ConfigFile(p) => {
                assert_eq!(p, path);
            }
            other => panic!("expected ConfigFile, got {:?}", other),
        }
        match config.source_of("model") {
            ConfigSource::ConfigFile(p) => {
                assert_eq!(p, path);
            }
            other => panic!("expected ConfigFile, got {:?}", other),
        }
    }

    #[test]
    fn test_load_provenance_nested_source() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"

            [agent]
            max_iterations = 42
            "#,
            "nested_prov.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        match config.source_of("agent.max_iterations") {
            ConfigSource::ConfigFile(p) => assert_eq!(p, path),
            other => panic!("expected ConfigFile, got {:?}", other),
        }
    }

    #[test]
    fn test_load_provenance_default_for_unset() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "default_prov.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        // A key that was never set in any file/env should be Default
        assert_eq!(config.source_of("nonexistent_key"), ConfigSource::Default);
    }

    // =========================================================================
    // Config::load — env var override tests
    // =========================================================================

    #[test]
    fn test_load_env_endpoint_override() {
        let _guard = clear_env();
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://file-value:8000/v1""#, "env_ep.toml");
        std::env::set_var("SELFWARE_ENDPOINT", "http://env-override:9999/v1");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.endpoint, "http://env-override:9999/v1");
        assert!(matches!(
            config.source_of("endpoint"),
            ConfigSource::EnvVar(_)
        ));
    }

    #[test]
    fn test_load_env_model_override() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(r#"model = "file-model""#, "env_model.toml");
        std::env::set_var("SELFWARE_MODEL", "env-model");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.model, "env-model");
    }

    #[test]
    fn test_load_env_max_tokens_override() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(r#"max_tokens = 4096"#, "env_mt.toml");
        std::env::set_var("SELFWARE_MAX_TOKENS", "99999");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.max_tokens, 99999);
    }

    #[test]
    fn test_load_env_max_tokens_invalid_ignored() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(r#"max_tokens = 4096"#, "env_mt_bad.toml");
        std::env::set_var("SELFWARE_MAX_TOKENS", "not_a_number");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.max_tokens, 4096, "invalid env var should be ignored");
    }

    #[test]
    fn test_load_env_temperature_override() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(r#"temperature = 0.5"#, "env_temp.toml");
        std::env::set_var("SELFWARE_TEMPERATURE", "0.123");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!((config.temperature - 0.123).abs() < f32::EPSILON);
    }

    #[test]
    fn test_load_env_temperature_invalid_ignored() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(r#"temperature = 0.5"#, "env_temp_bad.toml");
        std::env::set_var("SELFWARE_TEMPERATURE", "not_a_float");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!((config.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_load_env_timeout_override() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "env_timeout.toml",
        );
        std::env::set_var("SELFWARE_TIMEOUT", "600");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.agent.step_timeout_secs, 600);
    }

    #[test]
    fn test_load_env_theme_override() {
        let _guard = clear_env();
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://localhost:8000/v1""#, "env_theme.toml");
        std::env::set_var("SELFWARE_THEME", "ocean");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.ui.theme, "ocean");
    }

    #[test]
    fn test_load_env_api_key_override() {
        let _guard = clear_env();
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://localhost:8000/v1""#, "env_key.toml");
        std::env::set_var("SELFWARE_API_KEY", "sk-env-key-12345");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(
            config.api_key.as_ref().unwrap().expose(),
            "sk-env-key-12345"
        );
    }

    #[test]
    fn test_load_env_api_key_overrides_config_file_key() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            api_key = "sk-file-key"
            "#,
            "env_key_priority.toml",
        );
        std::env::set_var("SELFWARE_API_KEY", "sk-env-key-wins");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.api_key.as_ref().unwrap().expose(), "sk-env-key-wins");
    }

    #[test]
    fn test_load_env_mode_normal() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "mode_normal.toml",
        );
        std::env::set_var("SELFWARE_MODE", "normal");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.execution_mode, ExecutionMode::Normal);
    }

    #[test]
    fn test_load_env_mode_yolo() {
        let _guard = clear_env();
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://localhost:8000/v1""#, "mode_yolo.toml");
        std::env::set_var("SELFWARE_MODE", "yolo");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.execution_mode, ExecutionMode::Yolo);
    }

    #[test]
    fn test_load_env_mode_daemon() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "mode_daemon.toml",
        );
        std::env::set_var("SELFWARE_MODE", "daemon");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.execution_mode, ExecutionMode::Daemon);
    }

    #[test]
    fn test_load_env_mode_auto_edit_variants() {
        let _guard = clear_env();
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://localhost:8000/v1""#, "mode_auto.toml");
        for variant in &["auto-edit", "autoedit", "auto_edit"] {
            std::env::set_var("SELFWARE_MODE", variant);
            let config = Config::load(Some(path.to_str().unwrap())).unwrap();
            assert_eq!(
                config.execution_mode,
                ExecutionMode::AutoEdit,
                "SELFWARE_MODE={variant} should map to AutoEdit"
            );
        }
    }

    #[test]
    fn test_load_env_mode_invalid_falls_back_to_normal() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "mode_invalid.toml",
        );
        std::env::set_var("SELFWARE_MODE", "totally_invalid");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        // Invalid mode prints a warning but doesn't change execution_mode
        // (defaults to Normal)
        assert_eq!(config.execution_mode, ExecutionMode::Normal);
    }

    #[test]
    fn test_load_env_post_edit_test_command() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "env_post_edit.toml",
        );
        std::env::set_var("SELFWARE_POST_EDIT_TEST_COMMAND", "cargo test --all");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(
            config.agent.post_edit_test_command.as_deref(),
            Some("cargo test --all")
        );
    }

    #[test]
    fn test_load_env_post_edit_test_command_empty_ignored() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "env_post_edit_empty.toml",
        );
        std::env::set_var("SELFWARE_POST_EDIT_TEST_COMMAND", "   ");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(
            config.agent.post_edit_test_command, None,
            "whitespace-only command should be ignored"
        );
    }

    // =========================================================================
    // Config::load — SELFWARE_CONFIG env var
    // =========================================================================

    #[test]
    fn test_load_selfware_config_env_var() {
        let _guard = clear_env();
        let (dir, path) = write_temp_config(
            r#"
            endpoint = "http://env-config-host:8000/v1"
            model = "env-config-model"
            "#,
            "via_env.toml",
        );
        std::env::set_var("SELFWARE_CONFIG", path.to_str().unwrap());
        let config = Config::load(None).unwrap();
        assert_eq!(config.endpoint, "http://env-config-host:8000/v1");
        assert_eq!(config.model, "env-config-model");
        drop(dir);
    }

    #[test]
    fn test_load_explicit_path_overrides_selfware_config_env() {
        let _guard = clear_env();
        let (_dir_env, env_path) =
            write_temp_config(r#"endpoint = "http://env-host:8000/v1""#, "env_host.toml");
        let (_dir_explicit, explicit_path) = write_temp_config(
            r#"endpoint = "http://explicit-host:8000/v1""#,
            "explicit_host.toml",
        );
        std::env::set_var("SELFWARE_CONFIG", env_path.to_str().unwrap());
        let config = Config::load(Some(explicit_path.to_str().unwrap())).unwrap();
        assert_eq!(
            config.endpoint, "http://explicit-host:8000/v1",
            "explicit path should take priority over SELFWARE_CONFIG"
        );
    }

    // =========================================================================
    // Config::load — validation failures propagated
    // =========================================================================

    #[test]
    fn test_load_fails_on_empty_endpoint() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(r#"endpoint = """#, "empty_ep.toml");
        let result = Config::load(Some(path.to_str().unwrap()));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_fails_on_empty_model() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "   "
            "#,
            "empty_model.toml",
        );
        let result = Config::load(Some(path.to_str().unwrap()));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_fails_on_invalid_endpoint_scheme() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "ftp://bad-scheme.example.com"
            model = "test"
            "#,
            "bad_scheme.toml",
        );
        let result = Config::load(Some(path.to_str().unwrap()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("http:// or https://"));
    }

    #[test]
    fn test_load_fails_on_zero_max_tokens() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            max_tokens = 0
            "#,
            "zero_mt.toml",
        );
        let result = Config::load(Some(path.to_str().unwrap()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max_tokens"));
    }

    // =========================================================================
    // Config::resolve_model
    // =========================================================================

    #[test]
    fn test_resolve_model_none_returns_default() {
        let config = Config::default();
        // Default config has no models populated, so resolve_model returns None
        let profile = config.resolve_model(None);
        assert!(
            profile.is_none(),
            "Config::default has no models, so resolve_model(None) should return None"
        );
    }

    #[test]
    fn test_resolve_model_existing_id() {
        let mut config = Config::default();
        config.models.insert(
            "coder".to_string(),
            ModelProfile {
                endpoint: "http://coder:8000/v1".into(),
                model: "coder-model".into(),
                api_key: None,
                max_tokens: 4096,
                temperature: 0.2,
                modalities: default_modalities(),
                context_length: default_context_length(),
                extra_body: None,
                native_function_calling: None,
            },
        );
        let profile = config.resolve_model(Some("coder"));
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().model, "coder-model");
    }

    #[test]
    fn test_resolve_model_nonexistent_falls_back_to_default() {
        let mut config = Config::default();
        config.models.insert(
            "default".to_string(),
            ModelProfile {
                endpoint: "http://default:8000/v1".into(),
                model: "default-model".into(),
                api_key: None,
                max_tokens: 8192,
                temperature: 0.5,
                modalities: default_modalities(),
                context_length: default_context_length(),
                extra_body: None,
                native_function_calling: None,
            },
        );
        let profile = config.resolve_model(Some("nonexistent"));
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().model, "default-model");
    }

    #[test]
    fn test_resolve_model_none_falls_back_to_default() {
        let mut config = Config::default();
        config.models.insert(
            "default".to_string(),
            ModelProfile {
                endpoint: "http://fallback:8000/v1".into(),
                model: "fallback-model".into(),
                api_key: None,
                max_tokens: 2048,
                temperature: 0.1,
                modalities: default_modalities(),
                context_length: default_context_length(),
                extra_body: None,
                native_function_calling: None,
            },
        );
        let profile = config.resolve_model(None);
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().endpoint, "http://fallback:8000/v1");
    }

    #[test]
    fn test_resolve_model_no_default_no_id_returns_none() {
        let config = Config::default();
        // No models at all
        assert!(config.resolve_model(None).is_none());
    }

    #[test]
    fn test_resolve_model_no_default_with_id_returns_none() {
        let config = Config::default();
        assert!(config.resolve_model(Some("anything")).is_none());
    }

    #[test]
    fn test_resolve_model_after_load_has_default() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "loaded-model"
            "#,
            "resolve.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        let profile = config.resolve_model(None).unwrap();
        assert_eq!(profile.endpoint, "http://localhost:8000/v1");
        assert_eq!(profile.model, "loaded-model");
    }

    #[test]
    fn test_resolve_model_explicit_profile_after_load() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "base"

            [models.coder]
            endpoint = "http://coder:9000/v1"
            model = "coder-special"
            "#,
            "resolve_explicit.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        let coder = config.resolve_model(Some("coder")).unwrap();
        assert_eq!(coder.endpoint, "http://coder:9000/v1");
        assert_eq!(coder.model, "coder-special");

        // Unknown id falls back to default
        let fallback = config.resolve_model(Some("unknown")).unwrap();
        assert_eq!(fallback.endpoint, "http://localhost:8000/v1");
    }

    // =========================================================================
    // Config::apply_ui_settings
    // =========================================================================

    // Note: apply_ui_settings mutates a global atomic theme. These tests
    // may race when run in parallel, so we verify the *mapping* logic by
    // calling set_theme directly with the same match arms, rather than
    // asserting the global state after apply_ui_settings.

    #[test]
    fn test_apply_ui_settings_does_not_panic_default() {
        let config = Config::default();
        config.apply_ui_settings();
    }

    #[test]
    fn test_apply_ui_settings_does_not_panic_ocean() {
        let mut config = Config::default();
        config.ui.theme = "ocean".to_string();
        config.apply_ui_settings();
    }

    #[test]
    fn test_apply_ui_settings_does_not_panic_minimal() {
        let mut config = Config::default();
        config.ui.theme = "minimal".to_string();
        config.apply_ui_settings();
    }

    #[test]
    fn test_apply_ui_settings_does_not_panic_high_contrast() {
        let mut config = Config::default();
        config.ui.theme = "high-contrast".to_string();
        config.apply_ui_settings();
    }

    #[test]
    fn test_apply_ui_settings_does_not_panic_unknown_theme() {
        let mut config = Config::default();
        config.ui.theme = "nonexistent-theme".to_string();
        config.apply_ui_settings();
    }

    #[test]
    fn test_apply_ui_settings_does_not_panic_case_insensitive() {
        let mut config = Config::default();
        config.ui.theme = "OCEAN".to_string();
        config.apply_ui_settings();
    }

    #[test]
    fn test_theme_mapping_logic() {
        // Test the theme name → ThemeId mapping directly (same logic as
        // apply_ui_settings) without relying on global state ordering.
        use crate::ui::theme::{current_theme_id, set_theme, ThemeId};

        fn map_theme(name: &str) -> ThemeId {
            match name.to_lowercase().as_str() {
                "ocean" => ThemeId::Ocean,
                "minimal" => ThemeId::Minimal,
                "high-contrast" | "highcontrast" | "high_contrast" => ThemeId::HighContrast,
                _ => ThemeId::Amber,
            }
        }

        // Test each mapping
        let test_cases = [
            ("ocean", ThemeId::Ocean),
            ("minimal", ThemeId::Minimal),
            ("high-contrast", ThemeId::HighContrast),
            ("highcontrast", ThemeId::HighContrast),
            ("high_contrast", ThemeId::HighContrast),
            ("amber", ThemeId::Amber),
            ("unknown", ThemeId::Amber),
            ("OCEAN", ThemeId::Ocean), // case-insensitive
            ("High-Contrast", ThemeId::HighContrast),
            ("", ThemeId::Amber), // empty string defaults to Amber
        ];

        for (name, expected) in &test_cases {
            let theme_id = map_theme(name);
            assert_eq!(
                theme_id, *expected,
                "theme name '{name}' should map to {expected:?}"
            );
            // Also verify set_theme/current_theme_id round-trip
            set_theme(theme_id);
            assert_eq!(
                current_theme_id(),
                theme_id,
                "set_theme/current_theme_id round-trip failed for {name}"
            );
        }
    }

    // =========================================================================
    // Config::load — matched profile application
    // =========================================================================

    #[test]
    fn test_load_glm52_model_matches_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "z-ai/glm-5.2"
            "#,
            "glm52.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.matched_profile.as_deref(), Some("glm-5.2"));
        // GLM-5.2 profile sets native_function_calling = true
        assert!(config.agent.native_function_calling);
    }

    #[test]
    fn test_load_custom_model_no_profile_match() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "some-custom-model"
            "#,
            "no_profile.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.matched_profile, None);
        assert!(config.matched_profile_applied.is_empty());
    }

    #[test]
    fn test_load_explicit_native_fc_not_overwritten_by_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "z-ai/glm-5.2"

            [agent]
            native_function_calling = false
            "#,
            "explicit_fc.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        // User explicitly set native_function_calling = false, profile should not override
        assert!(
            !config.agent.native_function_calling,
            "explicit user setting should win over profile"
        );
    }

    #[test]
    fn test_load_explicit_temperature_not_overwritten_by_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "z-ai/glm-5.2"
            temperature = 0.1
            "#,
            "explicit_temp.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!(
            (config.temperature - 0.1).abs() < f32::EPSILON,
            "explicit temperature should win over profile default"
        );
    }

    // =========================================================================
    // Config::load — extra_body handling
    // =========================================================================

    #[test]
    fn test_load_extra_body_from_config() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"

            [extra_body]
            top_p = 0.9
            seed = 42
            "#,
            "extra_body.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        let extra = config.extra_body.expect("extra_body should be loaded");
        assert_eq!(extra["top_p"], serde_json::json!(0.9));
        assert_eq!(extra["seed"], serde_json::json!(42));
    }

    #[test]
    fn test_load_no_extra_body_is_none() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            "#,
            "no_extra.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!(config.extra_body.is_none());
    }

    // =========================================================================
    // Config::load — full config with all sections
    // =========================================================================

    #[test]
    fn test_load_full_config_all_sections() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "full-model"
            max_tokens = 4096
            context_length = 131072
            temperature = 0.4

            [agent]
            max_iterations = 50
            step_timeout_secs = 600

            [safety]
            allowed_paths = ["/safe/**"]
            denied_paths = ["/danger/**"]

            [ui]
            theme = "ocean"
            compact_mode = true

            [concurrency]
            max_streams = 8
            max_tools = 16
            max_global = 32
            "#,
            "full.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();

        // Top-level
        assert_eq!(config.endpoint, "http://localhost:8000/v1");
        assert_eq!(config.model, "full-model");
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.context_length, 131072);
        assert!((config.temperature - 0.4).abs() < f32::EPSILON);

        // Agent
        assert_eq!(config.agent.max_iterations, 50);
        assert_eq!(config.agent.step_timeout_secs, 600);

        // Safety
        assert_eq!(config.safety.allowed_paths, vec!["/safe/**"]);
        assert_eq!(config.safety.denied_paths, vec!["/danger/**"]);

        // UI
        assert_eq!(config.ui.theme, "ocean");
        assert!(config.ui.compact_mode);
        assert!(
            config.compact_mode,
            "compact_mode should be applied to top-level"
        );

        // Concurrency
        assert_eq!(config.concurrency.max_streams, 8);
        assert_eq!(config.concurrency.max_tools, 16);
        assert_eq!(config.concurrency.max_global, 32);

        // Default model profile synthesized
        assert!(config.models.contains_key("default"));
    }

    // =========================================================================
    // Config::load — token budget normalization on load
    // =========================================================================

    #[test]
    fn test_load_normalizes_safety_margin_if_too_large() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            max_tokens = 1000

            [agent]
            token_budget = 500
            token_safety_margin = 1000
            "#,
            "normalize.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        // safety_margin (1000) >= token_budget (500) → clamped to 499
        assert_eq!(
            config.agent.token_safety_margin, 499,
            "load should normalize safety_margin to stay below token_budget"
        );
        assert_eq!(config.agent.token_budget, 500);
    }

    #[test]
    fn test_load_preserves_valid_safety_margin() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            max_tokens = 50000

            [agent]
            token_budget = 40000
            token_safety_margin = 8192
            "#,
            "valid_margin.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.agent.token_budget, 40000);
        assert_eq!(config.agent.token_safety_margin, 8192);
    }

    // =========================================================================
    // Unix-only: check_config_file_permissions
    // =========================================================================

    #[cfg(unix)]
    #[test]
    fn test_check_config_file_permissions_secure_ok() {
        let _guard = clear_env();
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://localhost:8000/v1""#, "secure.toml");
        // Set permissions to 600 (owner only)
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let result = Config::check_config_file_permissions(path.to_str().unwrap(), false);
        assert!(result.is_ok(), "secure permissions should not error");

        // Also test in strict mode
        let result_strict = Config::check_config_file_permissions(path.to_str().unwrap(), true);
        assert!(
            result_strict.is_ok(),
            "secure permissions should not error even in strict mode"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_check_config_file_permissions_insecure_warns_non_strict() {
        let _guard = clear_env();
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) =
            write_temp_config(r#"endpoint = "http://localhost:8000/v1""#, "insecure.toml");
        // Set permissions to 644 (world-readable)
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        // Non-strict mode: should warn but NOT error
        let result = Config::check_config_file_permissions(path.to_str().unwrap(), false);
        assert!(
            result.is_ok(),
            "non-strict mode should only warn, not error"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_check_config_file_permissions_insecure_errors_strict() {
        let _guard = clear_env();
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) = write_temp_config(
            r#"endpoint = "http://localhost:8000/v1""#,
            "strict_fail.toml",
        );
        // Set permissions to 644 (world-readable)
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let result = Config::check_config_file_permissions(path.to_str().unwrap(), true);
        assert!(
            result.is_err(),
            "strict mode should error on insecure permissions"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("insecure permissions"));
    }

    #[cfg(unix)]
    #[test]
    fn test_check_config_file_permissions_nonexistent_path_ok() {
        let _guard = clear_env();
        // Nonexistent path: metadata fails, function returns Ok(())
        let result = Config::check_config_file_permissions("/nonexistent/path.toml", false);
        assert!(result.is_ok(), "nonexistent path should not cause an error");
        let result_strict = Config::check_config_file_permissions("/nonexistent/path.toml", true);
        assert!(result_strict.is_ok());
    }

    // =========================================================================
    // KNOWN_CONFIG_KEYS sanity checks
    // =========================================================================

    #[test]
    fn test_known_config_keys_contains_core_fields() {
        // Verify that core top-level keys are recognized
        assert!(KNOWN_CONFIG_KEYS.contains(&"endpoint"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"model"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"max_tokens"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"context_length"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"temperature"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"api_key"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"safety"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"agent"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"ui"));
    }

    #[test]
    fn test_known_config_keys_contains_nested_sections() {
        // Verify that nested sub-keys are recognized
        assert!(KNOWN_CONFIG_KEYS.contains(&"max_iterations"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"step_timeout_secs"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"token_budget"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"native_function_calling"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"streaming"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"allowed_paths"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"denied_paths"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"require_confirmation"));
    }

    #[test]
    fn test_known_config_keys_contains_models_and_extra_body() {
        assert!(KNOWN_CONFIG_KEYS.contains(&"models"));
        assert!(KNOWN_CONFIG_KEYS.contains(&"extra_body"));
    }

    #[test]
    fn test_known_config_keys_does_not_contain_typo() {
        // Common typo should NOT be in the list
        assert!(!KNOWN_CONFIG_KEYS.contains(&"endpont"));
        assert!(!KNOWN_CONFIG_KEYS.contains(&"max_token"));
        assert!(!KNOWN_CONFIG_KEYS.contains(&"temprature"));
    }

    #[test]
    fn top_level_keys_exclude_section_subkeys() {
        // Section names + scalar top-level fields are top-level valid.
        assert!(TOP_LEVEL_CONFIG_KEYS.contains(&"endpoint"));
        assert!(TOP_LEVEL_CONFIG_KEYS.contains(&"agent"));
        assert!(TOP_LEVEL_CONFIG_KEYS.contains(&"safety"));
        // Sub-keys must NOT be valid at the top level (misplacing them warns).
        assert!(!TOP_LEVEL_CONFIG_KEYS.contains(&"max_iterations"));
        assert!(!TOP_LEVEL_CONFIG_KEYS.contains(&"step_timeout_secs"));
        assert!(!TOP_LEVEL_CONFIG_KEYS.contains(&"streaming"));
        assert!(!TOP_LEVEL_CONFIG_KEYS.contains(&"allowed_paths"));
    }

    // =========================================================================
    // Config::load — model profile with extra sections
    // =========================================================================

    #[test]
    fn test_load_multiple_model_profiles() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "base"

            [models.coder]
            endpoint = "http://coder:9000/v1"
            model = "coder-model"

            [models.vision]
            endpoint = "http://vision:9001/v1"
            model = "vision-model"
            modalities = ["text", "vision"]
            "#,
            "multi_profile.toml",
        );
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!(config.models.contains_key("default"));
        assert!(config.models.contains_key("coder"));
        assert!(config.models.contains_key("vision"));

        let coder = config.resolve_model(Some("coder")).unwrap();
        assert_eq!(coder.model, "coder-model");

        let vision = config.resolve_model(Some("vision")).unwrap();
        assert_eq!(vision.model, "vision-model");
        assert!(vision.supports_vision());
    }

    // =========================================================================
    // Config::load — env max_tokens affects implicit token_budget
    // =========================================================================

    #[test]
    fn test_load_env_max_tokens_affects_implicit_token_budget() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "test"
            context_length = 100000
            "#,
            "env_budget.toml",
        );
        // SELFWARE_MAX_TOKENS affects max_tokens, and since token_budget is
        // implicit (not set in [agent]), it's derived from context_length
        // (60% of context_length), NOT from max_tokens.
        // So changing max_tokens via env should not change token_budget here.
        std::env::set_var("SELFWARE_MAX_TOKENS", "99999");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(config.max_tokens, 99999);
        // token_budget = context_length * 3 / 5 (not affected by max_tokens)
        assert_eq!(config.agent.token_budget, 100000 * 3 / 5);
    }

    // =========================================================================
    // Config::load — env temperature treated as explicit for profile
    // =========================================================================

    #[test]
    fn test_load_env_temperature_counts_as_explicit_for_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "z-ai/glm-5.2"
            "#,
            "env_temp_profile.toml",
        );
        // GLM-5.2 profile sets temperature=1.0. Set env to 0.5 — env should win.
        std::env::set_var("SELFWARE_TEMPERATURE", "0.5");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert!(
            (config.temperature - 0.5).abs() < f32::EPSILON,
            "env temperature should override profile default"
        );
    }

    #[test]
    fn test_load_env_max_tokens_counts_as_explicit_for_profile() {
        let _guard = clear_env();
        let (_dir, path) = write_temp_config(
            r#"
            endpoint = "http://localhost:8000/v1"
            model = "z-ai/glm-5.2"
            "#,
            "env_mt_profile.toml",
        );
        // GLM-5.2 profile sets max_tokens=65536. Set env to 32768 — env should win.
        std::env::set_var("SELFWARE_MAX_TOKENS", "32768");
        let config = Config::load(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(
            config.max_tokens, 32768,
            "env max_tokens should override profile default"
        );
    }
}
