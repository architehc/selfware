//! Configuration loading, normalization, and UI application.

use anyhow::{bail, Context, Result};
use tracing::warn;

/// Keys valid at the TOP LEVEL of a config file: scalar top-level fields
/// plus section names. Section SUB-keys (e.g. max_iterations under [agent])
/// are intentionally absent so a misplaced sub-key at the top level warns.
const TOP_LEVEL_CONFIG_KEYS: &[&str] = &[
    "endpoint",
    "model",
    "max_tokens",
    "context_length",
    "context_mode",
    "context_fit_ratio",
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

use super::api_key::{
    endpoint_has_userinfo, is_insecure_remote_endpoint, is_local_endpoint, is_openrouter_endpoint,
    load_api_key_from_keyring, ApiKeySource,
};
use super::model::{default_modalities, ModelProfile, RedactedString};
use super::model_profiles::{apply_profile, match_profile, UserExplicitFields};
use super::provenance::{ConfigSource, ConfigSources};
use super::types::ExecutionMode;
use super::Config;

/// Emit a user-actionable config warning. Goes through `tracing::warn!`, and
/// additionally straight to stderr ONLY when no tracing subscriber exists.
/// `init_tracing()` installs a subscriber iff RUST_LOG / SELFWARE_LOG_LEVEL is
/// set; without this check, runs with logging enabled see every warning twice
/// (subscriber output plus the stderr fallback).
fn config_warning(message: &str) {
    warn!("{}", message);
    if std::env::var_os("RUST_LOG").is_none() && std::env::var_os("SELFWARE_LOG_LEVEL").is_none() {
        eprintln!("Config warning: {}", message);
    }
}

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

/// Whether `config_path` is an auto-discovered project-local `selfware.toml`.
/// Matches by FILE NAME so it covers both the relative `selfware.toml`
/// (auto-discovery) and an absolute `<dir>/selfware.toml` (the `-C` case),
/// while the home `~/.config/selfware/config.toml` and explicit `--config`
/// paths to other names are NOT treated as checkout-local.
fn config_is_checkout_local(config_path: &std::path::Path) -> bool {
    config_path.file_name() == Some(std::ffi::OsStr::new("selfware.toml"))
}

/// Known sub-keys of a fixed config section, derived from the section struct's
/// serde field list — the list therefore cannot drift from the schema the way
/// a hand-maintained list would (and unlike a default() serialization, it
/// keeps None-defaulted options). Returns `None` for sections
/// whose keys are dynamic (`models`, `extra_body`, `mcp`, `hooks`, `qa`) or
/// unrecognized; those get no nested-key check. None of the section structs
/// uses `skip_serializing_if` or `flatten`, so every schema key appears in
/// the serialized form.
/// Field names of a `Deserialize` struct, including `None`-defaulted options.
/// The previous approach serialized `Section::default()` and read its keys —
/// but TOML has no None representation, so every `Option` field vanished from
/// the "known keys" set and valid keys like `agent.max_wall_secs` got flagged
/// as unknown (found by the harness proposer reading TB 3.0 run traces).
/// This records the names from serde's own field list, so renamed fields and
/// options are always correct.
fn struct_field_names<T>() -> std::collections::HashSet<String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    use serde::de::{Deserializer, Error, Visitor};
    use std::cell::Cell;

    struct Collector(Cell<Option<&'static [&'static str]>>);

    impl<'de> Deserializer<'de> for &Collector {
        type Error = serde::de::value::Error;

        fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
            Err(Self::Error::custom("field collector"))
        }

        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _name: &'static str,
            fields: &'static [&'static str],
            _visitor: V,
        ) -> Result<V::Value, Self::Error> {
            self.0.set(Some(fields));
            Err(Self::Error::custom("collected"))
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map enum identifier ignored_any
        }
    }

    let collector = Collector(Cell::new(None));
    let _ = T::deserialize(&collector);
    collector
        .0
        .take()
        .unwrap_or_default()
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// The set of sub-keys a known fixed section defines, or None for sections
/// whose keys are dynamic (`models`, `extra_body`, `mcp`, `hooks`, `qa`) or
/// unrecognized; those get no nested-key check.
fn known_section_keys(section: &str) -> Option<std::collections::HashSet<String>> {
    let names = match section {
        "agent" => struct_field_names::<super::agent::AgentConfig>(),
        "safety" => struct_field_names::<super::safety::SafetyConfig>(),
        "yolo" => struct_field_names::<super::types::YoloFileConfig>(),
        "ui" => struct_field_names::<super::types::UiConfig>(),
        "continuous_work" => struct_field_names::<super::types::ContinuousWorkConfig>(),
        "retry" => struct_field_names::<super::types::RetrySettings>(),
        "resources" => struct_field_names::<super::resources::ResourcesConfig>(),
        "concurrency" => struct_field_names::<super::types::ConcurrencyConfig>(),
        "evolution" => struct_field_names::<super::types::EvolutionTomlConfig>(),
        "cache" => struct_field_names::<crate::session::cache::LlmCacheConfig>(),
        "debug" => struct_field_names::<super::debug::DebugConfig>(),
        _ => return None,
    };
    Some(names)
}

/// If the value for `key` originated from an untrusted, checkout-local
/// `selfware.toml`, return that file's path; otherwise `None`.
///
/// Env / CLI / keyring / profile / home-config origins (non-`ConfigFile`, or a
/// `ConfigFile` not named `selfware.toml`) and trusted files all return `None`,
/// so values from those sources are never restricted — only a value that a
/// specific untrusted project file set is caught.
fn untrusted_checkout_origin<'a>(
    sources: &'a ConfigSources,
    key: &str,
) -> Option<&'a std::path::Path> {
    match sources.get(key) {
        Some(ConfigSource::ConfigFile(p))
            if config_is_checkout_local(p) && !super::trust::is_config_trusted(p) =>
        {
            Some(p.as_path())
        }
        _ => None,
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

    /// Whether the raw TOML sets a TOP-LEVEL `context_length` key (an explicit
    /// user choice the loader must not override with a smarter default).
    fn content_sets_context_length(content: &str) -> bool {
        toml::from_str::<toml::Value>(content)
            .ok()
            .map(|value| value.get("context_length").is_some())
            .unwrap_or(false)
    }

    /// Restricted mode for untrusted project configs.
    ///
    /// A checkout-local `selfware.toml` that the user has not trusted must NOT be
    /// able to grant itself privileged execution — auto-approval of every tool,
    /// shell hooks, MCP subprocesses, a post-edit command, forced yolo — or
    /// weaken the safety defaults (paths / confirmation list). For each such
    /// privileged field whose value ORIGINATED from the untrusted file (via the
    /// provenance map), reset it to its safe default. Values from env / CLI /
    /// trusted files are preserved, because their provenance is not an untrusted
    /// checkout-local `ConfigFile`.
    ///
    /// Running is still permitted (this is restriction, not refusal);
    /// `selfware trust <path>` lifts it. Returns the offending file path and the
    /// list of reset keys for a single user-facing warning, or `None` when
    /// nothing was restricted.
    fn restrict_untrusted_project_config(
        &mut self,
        sources: &ConfigSources,
    ) -> Option<(std::path::PathBuf, Vec<String>)> {
        let mut reset: Vec<String> = Vec::new();
        let mut origin: Option<std::path::PathBuf> = None;

        // Arbitrary tool auto-approval (`tool_pattern = "*"` → silent approval).
        if let Some(p) = untrusted_checkout_origin(sources, "safety.permissions") {
            if !self.safety.permissions.is_empty() {
                self.safety.permissions = Vec::new();
                origin.get_or_insert_with(|| p.to_path_buf());
                reset.push("safety.permissions".to_string());
            }
        }
        // Shell hooks (`sh -c <command>` on tool events).
        if let Some(p) = untrusted_checkout_origin(sources, "hooks") {
            if !self.hooks.is_empty() {
                self.hooks = Vec::new();
                origin.get_or_insert_with(|| p.to_path_buf());
                reset.push("hooks".to_string());
            }
        }
        // Arbitrary MCP subprocess commands.
        if let Some(p) = untrusted_checkout_origin(sources, "mcp.servers") {
            if !self.mcp.servers.is_empty() {
                self.mcp.servers = Vec::new();
                origin.get_or_insert_with(|| p.to_path_buf());
                reset.push("mcp.servers".to_string());
            }
        }
        // Post-edit command executed as a subprocess after every edit.
        if let Some(p) = untrusted_checkout_origin(sources, "agent.post_edit_test_command") {
            if self.agent.post_edit_test_command.is_some() {
                self.agent.post_edit_test_command = None;
                origin.get_or_insert_with(|| p.to_path_buf());
                reset.push("agent.post_edit_test_command".to_string());
            }
        }
        // Forced yolo (config can turn it on / allow destructive shell).
        if let Some(p) = untrusted_checkout_origin(sources, "yolo") {
            self.yolo = super::types::YoloFileConfig::default();
            origin.get_or_insert_with(|| p.to_path_buf());
            reset.push("yolo".to_string());
        }
        // Weakening the safety defaults (confirmation list / path guardrails):
        // reset to the strong built-in defaults rather than the project's.
        let safe = super::safety::SafetyConfig::default();
        if let Some(p) = untrusted_checkout_origin(sources, "safety.require_confirmation") {
            self.safety.require_confirmation = safe.require_confirmation.clone();
            origin.get_or_insert_with(|| p.to_path_buf());
            reset.push("safety.require_confirmation".to_string());
        }
        if let Some(p) = untrusted_checkout_origin(sources, "safety.denied_paths") {
            self.safety.denied_paths = safe.denied_paths.clone();
            origin.get_or_insert_with(|| p.to_path_buf());
            reset.push("safety.denied_paths".to_string());
        }
        if let Some(p) = untrusted_checkout_origin(sources, "safety.allowed_paths") {
            self.safety.allowed_paths = safe.allowed_paths.clone();
            origin.get_or_insert_with(|| p.to_path_buf());
            reset.push("safety.allowed_paths".to_string());
        }
        // Push-to-protected-branch protection (enforced by the safety
        // checker's git_push guards): an untrusted repo could ship
        // `protected_branches = []` and silently disable push-to-main
        // protection, so restore the built-in defaults.
        if let Some(p) = untrusted_checkout_origin(sources, "safety.protected_branches") {
            self.safety.protected_branches = safe.protected_branches.clone();
            origin.get_or_insert_with(|| p.to_path_buf());
            reset.push("safety.protected_branches".to_string());
        }
        // Profile endpoints (`[models.*] endpoint`): profile-routed requests
        // (`chat_with_profile` via `resolve_model`) use the profile's
        // endpoint, so a checkout-supplied one would bypass every gate that
        // only inspects the top-level key. Reset any profile endpoint that
        // ORIGINATED from the untrusted file to the top-level endpoint (which
        // is itself gated above).
        let default_endpoint = self.endpoint.clone();
        for (name, profile) in self.models.iter_mut() {
            let key = format!("models.{name}.endpoint");
            if let Some(p) = untrusted_checkout_origin(sources, &key) {
                if profile.endpoint != default_endpoint {
                    profile.endpoint = default_endpoint.clone();
                    origin.get_or_insert_with(|| p.to_path_buf());
                    reset.push(key);
                }
            }
        }

        origin.map(|p| (p, reset))
    }

    /// Warn about unknown top-level TOML keys that would be silently ignored,
    /// and about unknown SUB-keys inside known fixed sections (a nested typo
    /// like `[agent] max_iteration = 5` is otherwise dropped by serde and the
    /// value silently stays at its default).
    fn warn_unknown_keys(content: &str) {
        if let Ok(toml::Value::Table(table)) = toml::from_str::<toml::Value>(content) {
            for (key, value) in table.iter() {
                if !TOP_LEVEL_CONFIG_KEYS.contains(&key.as_str()) {
                    config_warning(&format!(
                        "Unknown config key [{}] — this section is ignored. \
                         Check for typos or remove it.",
                        key
                    ));
                    continue;
                }
                // Nested check: within a known fixed section, flag sub-keys the
                // section struct does not define (they deserialize to nothing).
                if let toml::Value::Table(section_table) = value {
                    if let Some(known_sub_keys) = known_section_keys(key) {
                        for sub_key in section_table.keys() {
                            if !known_sub_keys.contains(sub_key.as_str()) {
                                config_warning(&format!(
                                    "Unknown config key [{}].{} — this key is ignored. \
                                     Check for typos or remove it.",
                                    key, sub_key
                                ));
                            }
                        }
                    }
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
                config_warning(&format!(
                    "Config file '{}' is accessible by other users (mode {:o}). \
                     This file may contain API keys. Consider running: chmod 600 {}",
                    path,
                    mode & 0o777,
                    path
                ));
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

        // Surface which file won the precedence search (and when a
        // checkout-local `selfware.toml` shadows the global config) — this
        // makes the cwd-shadows-global trap visible on every run instead of
        // silently loading an unexpected file. The loaded path is also
        // recorded in the provenance map under a reserved key so downstream
        // diagnostics (`doctor`, `llm-doctor`) can report it.
        if let Some(ref p) = loaded_from_path {
            let shadowed_home = if config_is_checkout_local(std::path::Path::new(p)) {
                dirs::home_dir()
                    .map(|h| h.join(".config/selfware/config.toml"))
                    .filter(|h| h.is_file())
            } else {
                None
            };
            match shadowed_home {
                Some(h) => eprintln!("config: {} (shadowing {})", p, h.display()),
                None => eprintln!("config: {}", p),
            }
            sources.set(
                "__config_path".to_string(),
                ConfigSource::ConfigFile(PathBuf::from(p)),
            );
        }

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
        // 1. Environment variable (highest priority, never persisted to disk):
        //    SELFWARE_API_KEY, then OPENROUTER_API_KEY when the configured
        //    endpoint IS openrouter.ai (de-facto-standard var name there).
        // 2. System keyring (set via `selfware config set-key`, the init
        //    wizard, or manually in the OS keyring)
        // 3. Config file (lowest priority, plaintext on disk -- warn the user)
        let mut api_key_source = ApiKeySource::None;

        if let Ok(api_key) = std::env::var("SELFWARE_API_KEY") {
            config.api_key = Some(RedactedString::new(api_key));
            api_key_source = ApiKeySource::EnvVar;
        }

        // OpenRouter convenience fallback: a user who exported the standard
        // OPENROUTER_API_KEY expects it to "just work" against the default
        // OpenRouter endpoint. Host-exact check — a lookalike host must never
        // receive this credential.
        if matches!(api_key_source, ApiKeySource::None) && is_openrouter_endpoint(&config.endpoint)
        {
            if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
                if !api_key.trim().is_empty() {
                    config.api_key = Some(RedactedString::new(api_key));
                    api_key_source = ApiKeySource::EnvVar;
                    sources.set("api_key", ConfigSource::EnvVar("OPENROUTER_API_KEY".into()));
                }
            }
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
                    config_warning(&format!(
                        "failed to read API key from system keyring: {e} (falling back to config file / env)"
                    ));
                }
            }
        }

        // If the key still comes from the plaintext config file, emit a warning.
        if matches!(api_key_source, ApiKeySource::None) && plaintext_key_in_config {
            api_key_source = ApiKeySource::ConfigFile;
            if let Some(ref cfg_path) = loaded_from_path {
                config_warning(&format!(
                    "API key loaded from plaintext config file '{}'. \
                     For production use, set the SELFWARE_API_KEY environment variable \
                     or store it in the OS keyring (`selfware config set-key`).",
                    cfg_path
                ));

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

        // Missing key against a REMOTE endpoint: the run will fail with an
        // upstream `401 No cookie auth credentials found` — say so now, with
        // the concrete fix, instead of after a wasted round-trip.
        if config.api_key.is_none() && !is_local_endpoint(&config.endpoint) {
            config_warning(&format!(
                "no API key configured for remote endpoint '{}' — requests will fail with 401. \
                 Fix: export SELFWARE_API_KEY=<key>{}, or run `selfware config set-key <key>` \
                 to store it in the OS keyring, or add `api_key = \"...\"` to your config file.",
                config.endpoint,
                if is_openrouter_endpoint(&config.endpoint) {
                    " (or OPENROUTER_API_KEY)"
                } else {
                    ""
                },
            ));
        }
        // Suppress unused-variable warning; the value is consumed by the
        // match arms above and kept around only for clarity / future use.
        let _ = api_key_source;

        // Never send a credential over plaintext HTTP to a REMOTE host: a
        // checkout-local config can choose the endpoint, so an http:// remote
        // URL with a key present is a downgrade / exfiltration risk. Local HTTP
        // is fine (traffic stays on the machine).
        if config.api_key.is_some() {
            if endpoint_has_userinfo(&config.endpoint) {
                bail!(
                    "Refusing to send the API key: endpoint '{}' embeds URL credentials \
                     (user:pass@host), a host-spoofing vector. Remove the '@' userinfo.",
                    config.endpoint
                );
            }
            if is_insecure_remote_endpoint(&config.endpoint) {
                bail!(
                    "Refusing to send the API key over plaintext HTTP to a remote endpoint '{}'. \
                     Use https:// or a local endpoint (localhost / 127.0.0.1).",
                    config.endpoint
                );
            }
        }

        // Credential-origin gate: refuse to send a GLOBALLY-exported credential
        // (SELFWARE_API_KEY) to a REMOTE endpoint that an untrusted, checkout-
        // local `selfware.toml` selected. Local endpoints are exempt (no login
        // for local models). Explicit --config / home-config endpoints are not
        // "selfware.toml" and so are unaffected. Trust a repo by adding its
        // config's canonical path to ~/.selfware/trusted_repos.
        if config.api_key.is_some()
            && matches!(api_key_source, ApiKeySource::EnvVar)
            && !is_local_endpoint(&config.endpoint)
        {
            if let Some(ConfigSource::ConfigFile(p)) = sources.get("endpoint") {
                if config_is_checkout_local(p) && !super::trust::is_config_trusted(p) {
                    let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                    bail!(
                        "Refusing to send the global SELFWARE_API_KEY to endpoint '{}' selected by \
                         the project's selfware.toml: this repository is not trusted. If you trust \
                         it, add this path to ~/.selfware/trusted_repos:\n  {}",
                        config.endpoint,
                        canon.display()
                    );
                }
            }
        }

        // Untrusted-endpoint gate: even with NO credential configured, an
        // untrusted checkout-local `selfware.toml` must not redirect the
        // agent's whole conversation (code, prompts, tool output) to an
        // attacker-controlled REMOTE endpoint. Localhost endpoints stay
        // allowed (dev servers); endpoints from env / CLI / home config are
        // operator choices and unaffected — as is a config FILE the operator
        // explicitly selected via SELFWARE_CONFIG (same trust level as
        // SELFWARE_ENDPOINT). Trusting the repo (`selfware trust`) lifts the
        // refusal.
        let config_path_from_env = matches!(
            sources.get("__config_path_source"),
            Some(ConfigSource::EnvVar(_))
        );
        if !config_path_from_env && !is_local_endpoint(&config.endpoint) {
            if let Some(ConfigSource::ConfigFile(p)) = sources.get("endpoint") {
                if config_is_checkout_local(p) && !super::trust::is_config_trusted(p) {
                    let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                    bail!(
                        "Refusing to load remote endpoint '{}' selected by the project's \
                         selfware.toml: this repository is not trusted, and the endpoint \
                         would receive the full conversation. If you trust it, run \
                         `selfware trust` or add this path to ~/.selfware/trusted_repos:\n  {}",
                        config.endpoint,
                        canon.display()
                    );
                }
            }
        }
        // Profile endpoints (`[models.*] endpoint`) route profile traffic
        // (`chat_with_profile` via `resolve_model`) to their OWN endpoint, so
        // they need the same gate — otherwise an untrusted checkout ships a
        // remote `[models.default] endpoint` and the top-level check above
        // never sees it. Localhost profile endpoints stay allowed.
        if !config_path_from_env {
            for (name, profile) in &config.models {
                if is_local_endpoint(&profile.endpoint) {
                    continue;
                }
                let key = format!("models.{name}.endpoint");
                if let Some(ConfigSource::ConfigFile(p)) = sources.get(&key) {
                    if config_is_checkout_local(p) && !super::trust::is_config_trusted(p) {
                        let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                        bail!(
                            "Refusing to load remote endpoint '{}' selected by the project's \
                             selfware.toml: this repository is not trusted, and the endpoint \
                             would receive the full conversation. If you trust it, run \
                             `selfware trust` or add this path to ~/.selfware/trusted_repos:\n  {}",
                            profile.endpoint,
                            canon.display()
                        );
                    }
                }
            }
        }

        // Restricted mode: an untrusted checkout-local `selfware.toml` may not
        // grant itself privileged execution or weaken safety. Neutralize any
        // such field before it can reach the Agent (which activates hooks / MCP /
        // permissions / yolo / post-edit at construction time).
        if let Some((path, keys)) = config.restrict_untrusted_project_config(&sources) {
            config_warning(&format!(
                "Untrusted project selfware.toml '{}': ignored privileged settings [{}]. Run \
                 `selfware trust` in this directory to enable them.",
                path.display(),
                keys.join(", ")
            ));
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

        // Conservative context window for UNRECOGNIZED models: the 1M built-in
        // default describes the shipped default model, not an arbitrary one —
        // deriving a ~630k token budget from it overflows typical local
        // contexts. When the user did not set `context_length` and no built-in
        // profile matches the model, assume a conservative 32k. Explicit
        // `context_length` (top-level or per-model) always wins, and
        // `auto-config` / `unpack` write the real value detected from /models.
        let context_length_explicit = raw_toml_content
            .as_deref()
            .map(Self::content_sets_context_length)
            .unwrap_or(false);
        config.apply_unknown_model_context_fallback(context_length_explicit);

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

    /// Apply the load-time derivations a generated config needs before it can
    /// pass [`Config::validate`]: derive `token_budget` from `context_length`
    /// when unset, then clamp the safety margin. Mirrors what `Config::load`
    /// does after parsing so wizard output validates exactly the way a file
    /// read back from disk would.
    fn apply_generated_derivations(&mut self) {
        if self.agent.token_budget == 0 {
            self.agent.token_budget = self.context_length * 3 / 5;
        }
        self.normalize_agent_limits();
    }

    /// Conservative context window for UNRECOGNIZED models when the source
    /// TOML did not set `context_length` explicitly. Single implementation of
    /// the fallback `Config::load` applies on disk reads, shared with
    /// [`Config::validate_generated_toml`] so a generated config is judged
    /// against the SAME effective context window it gets at load time.
    fn apply_unknown_model_context_fallback(&mut self, context_length_explicit: bool) {
        if !context_length_explicit && match_profile(&self.model).is_none() {
            self.context_length = super::UNKNOWN_MODEL_CONTEXT_LENGTH;
        }
    }

    /// Validate this in-memory generated config the way the loader would.
    /// Wizards (auto-config / unpack) call this before persisting so they can
    /// never emit a config `Config::load` would reject.
    pub fn validate_generated(&mut self) -> Result<()> {
        self.apply_generated_derivations();
        self.validate()
            .context("generated config failed validation — refusing to write it")
    }

    /// Parse and validate a GENERATED config body exactly the way
    /// `Config::load` would after reading it from disk. Wizards
    /// (`init`, `unpack --save`) must run this before writing so they fail
    /// loudly instead of emitting a file the loader rejects.
    pub fn validate_generated_toml(content: &str) -> Result<()> {
        let mut cfg: Config =
            toml::from_str(content).context("generated config does not match the Config schema")?;
        // Mirror the load pipeline in order: the unknown-model context
        // fallback fires BEFORE token_budget derives from context_length, and
        // the strict context-fit check holds generated output to the
        // unclamped invariant (the runtime derivation would silently clamp an
        // oversized max_tokens — a wizard must not emit a config that only
        // starts because of that clamp).
        cfg.apply_unknown_model_context_fallback(Self::content_sets_context_length(content));
        cfg.apply_generated_derivations();
        cfg.check_generated_context_fit()
            .context("generated config failed validation — refusing to write it")?;
        cfg.validate()
            .context("generated config failed validation — refusing to write it")?;
        Ok(())
    }

    /// Path of the config file this `Config` was loaded from, if any.
    /// Recorded by `Config::load` under a reserved provenance key; `None`
    /// for `Config::default()` and hand-built configs.
    pub fn loaded_config_path(&self) -> Option<&std::path::Path> {
        match self.sources.get("__config_path") {
            Some(ConfigSource::ConfigFile(p)) => Some(p.as_path()),
            _ => None,
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
#[path = "../../tests/unit/config/loader/loader_test.rs"]
mod tests;
