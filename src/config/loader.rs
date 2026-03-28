//! Configuration loading, normalization, and UI application.

use anyhow::{bail, Context, Result};
use tracing::warn;

use super::api_key::{load_api_key_from_keyring, ApiKeySource};
use super::model::{default_modalities, ModelProfile, RedactedString};
use super::types::ExecutionMode;
use super::Config;

impl Config {
    fn content_sets_agent_token_budget(content: &str) -> bool {
        toml::from_str::<toml::Value>(content)
            .ok()
            .and_then(|value| value.get("agent").cloned())
            .and_then(|agent| agent.as_table().cloned())
            .map(|agent| agent.contains_key("token_budget"))
            .unwrap_or(false)
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

        let mut loaded_from_path: Option<String> = None;
        let mut token_budget_was_explicit = false;
        let mut config = match effective_path {
            Some(p) => {
                let content = std::fs::read_to_string(p)
                    .with_context(|| format!("Failed to read config from {}", p))?;
                loaded_from_path = Some(p.to_string());
                token_budget_was_explicit = Self::content_sets_agent_token_budget(&content);
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
                        token_budget_was_explicit = Self::content_sets_agent_token_budget(&content);
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
                    bail!(
                        "Plaintext API key in config file is not allowed in strict mode. \
                         Use SELFWARE_API_KEY environment variable or system keyring via `selfware config set-key`."
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
        if !token_budget_was_explicit {
            config.agent.token_budget = config.max_tokens;
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
                },
            );
        }

        // Normalize agent token limits so derived defaults and explicit values
        // both satisfy validation.  Local models have varying context sizes —
        // defaulting to 500k was wrong because it misrepresents the actual capacity.
        config.normalize_agent_limits();

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
