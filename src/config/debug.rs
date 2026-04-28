//! Unified debug configuration.
//!
//! Replaces the older patchwork of `SELFWARE_DEBUG`, `SELFWARE_DEBUG_RAW`,
//! `SELFWARE_DEBUG_REQUEST`, `SELFWARE_DEBUG_GATE`, and `SELFWARE_DEBUG_TURNS`
//! environment-variable checks scattered through the codebase.  The primary
//! interface is now the `--debug` CLI flag and the `[debug]` section in
//! `selfware.toml`; the legacy environment variables remain as backward-
//! compatible overrides that layer on top of CLI + config.
//!
//! Precedence (highest first):
//!   1. Environment variables (force-on; never override to false)
//!   2. CLI flag `--debug[=channel,...]`
//!   3. `[debug]` section in `selfware.toml`
//!   4. Defaults (everything off)

use serde::{Deserialize, Serialize};

/// Configuration for debug output channels.
///
/// Each field controls one specific debug channel.  `all` is a convenience
/// flag that enables every channel without having to set them individually.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DebugConfig {
    /// Print every outgoing request body to stderr.
    #[serde(default)]
    pub log_requests: bool,
    /// Print every received response body to stderr.
    #[serde(default)]
    pub log_responses: bool,
    /// Print gate-decision reasoning when the completion gate is checked.
    #[serde(default)]
    pub log_gates: bool,
    /// Write per-turn JSON artifacts (raw model response per turn) to stderr.
    #[serde(default)]
    pub log_turns: bool,
    /// Convenience: enable every channel above.
    #[serde(default)]
    pub all: bool,
}

impl DebugConfig {
    /// Construct a `DebugConfig` with `all = true`.
    pub fn all() -> Self {
        Self {
            log_requests: false,
            log_responses: false,
            log_gates: false,
            log_turns: false,
            all: true,
        }
    }

    /// Returns true if any debug channel is active.
    pub fn is_active(&self) -> bool {
        self.all || self.log_requests || self.log_responses || self.log_gates || self.log_turns
    }

    pub fn should_log_requests(&self) -> bool {
        self.log_requests || self.all
    }

    pub fn should_log_responses(&self) -> bool {
        self.log_responses || self.all
    }

    pub fn should_log_gates(&self) -> bool {
        self.log_gates || self.all
    }

    pub fn should_log_turns(&self) -> bool {
        self.log_turns || self.all
    }

    /// Parse a comma-separated channel list (e.g. "requests,responses,turns")
    /// into a `DebugConfig`.  Unknown channels emit a warning and are ignored.
    /// An empty string or `"all"` enables every channel.
    pub fn from_channel_list(spec: &str) -> Self {
        let mut cfg = Self::default();
        let trimmed = spec.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
            cfg.all = true;
            return cfg;
        }
        for raw in trimmed.split(',') {
            let token = raw.trim().to_ascii_lowercase();
            match token.as_str() {
                "" => {}
                "all" => cfg.all = true,
                "requests" | "request" | "req" => cfg.log_requests = true,
                "responses" | "response" | "resp" | "raw" => cfg.log_responses = true,
                "gates" | "gate" => cfg.log_gates = true,
                "turns" | "turn" => cfg.log_turns = true,
                other => {
                    eprintln!(
                        "Config warning: unknown --debug channel '{}' (expected: requests, responses, gates, turns, all)",
                        other
                    );
                }
            }
        }
        cfg
    }

    /// Layer environment-variable overrides on top of an existing config.
    /// Env vars only force channels ON — they never turn anything off.  This
    /// preserves backward compatibility with the older `SELFWARE_DEBUG_*`
    /// variables while letting CLI/config disable channels by simply not
    /// setting the corresponding env var.
    pub fn apply_env_overrides(&mut self) {
        let truthy = |name: &str| {
            std::env::var(name)
                .map(|v| {
                    let v = v.trim().to_ascii_lowercase();
                    !v.is_empty() && v != "0" && v != "false" && v != "no" && v != "off"
                })
                .unwrap_or(false)
        };

        if truthy("SELFWARE_DEBUG") {
            self.all = true;
        }
        if truthy("SELFWARE_DEBUG_REQUEST") {
            self.log_requests = true;
        }
        if truthy("SELFWARE_DEBUG_RAW") {
            self.log_responses = true;
        }
        if truthy("SELFWARE_DEBUG_GATE") {
            self.log_gates = true;
        }
        if truthy("SELFWARE_DEBUG_TURNS") {
            self.log_turns = true;
        }
    }

    /// Merge a CLI-derived `DebugConfig` on top of `self` (which is typically
    /// loaded from `[debug]` in selfware.toml).  CLI fields that are `true`
    /// override the config; CLI fields that are `false` leave the config
    /// value untouched (so you can still enable channels in TOML and only
    /// add more from the CLI).
    pub fn merge_cli(&mut self, cli: &DebugConfig) {
        if cli.all {
            self.all = true;
        }
        if cli.log_requests {
            self.log_requests = true;
        }
        if cli.log_responses {
            self.log_responses = true;
        }
        if cli.log_gates {
            self.log_gates = true;
        }
        if cli.log_turns {
            self.log_turns = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize all env-mutating tests in this module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_debug_env() -> std::sync::MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for v in &[
            "SELFWARE_DEBUG",
            "SELFWARE_DEBUG_REQUEST",
            "SELFWARE_DEBUG_RAW",
            "SELFWARE_DEBUG_GATE",
            "SELFWARE_DEBUG_TURNS",
        ] {
            std::env::remove_var(v);
        }
        g
    }

    #[test]
    fn default_is_inactive() {
        let cfg = DebugConfig::default();
        assert!(!cfg.is_active());
        assert!(!cfg.should_log_requests());
        assert!(!cfg.should_log_responses());
        assert!(!cfg.should_log_gates());
        assert!(!cfg.should_log_turns());
    }

    #[test]
    fn all_enables_every_channel() {
        let cfg = DebugConfig::all();
        assert!(cfg.is_active());
        assert!(cfg.should_log_requests());
        assert!(cfg.should_log_responses());
        assert!(cfg.should_log_gates());
        assert!(cfg.should_log_turns());
    }

    #[test]
    fn channel_list_parses_known_tokens() {
        let cfg = DebugConfig::from_channel_list("requests,responses,turns");
        assert!(cfg.log_requests);
        assert!(cfg.log_responses);
        assert!(cfg.log_turns);
        assert!(!cfg.log_gates);
        assert!(!cfg.all);
    }

    #[test]
    fn channel_list_aliases() {
        let cfg = DebugConfig::from_channel_list("req,raw,gate,turn");
        assert!(cfg.log_requests);
        assert!(cfg.log_responses);
        assert!(cfg.log_gates);
        assert!(cfg.log_turns);
    }

    #[test]
    fn channel_list_all_keyword() {
        let cfg = DebugConfig::from_channel_list("all");
        assert!(cfg.all);
        assert!(cfg.is_active());
    }

    #[test]
    fn empty_channel_list_means_all() {
        // `--debug` with no value is parsed as empty string by clap.
        let cfg = DebugConfig::from_channel_list("");
        assert!(cfg.all);
    }

    #[test]
    fn unknown_channels_are_ignored() {
        let cfg = DebugConfig::from_channel_list("requests,bogus");
        assert!(cfg.log_requests);
        assert!(!cfg.log_responses);
        assert!(!cfg.all);
    }

    #[test]
    fn env_overrides_force_on() {
        let _g = clear_debug_env();
        let mut cfg = DebugConfig::default();
        std::env::set_var("SELFWARE_DEBUG_REQUEST", "1");
        std::env::set_var("SELFWARE_DEBUG_GATE", "1");
        cfg.apply_env_overrides();
        assert!(cfg.log_requests);
        assert!(cfg.log_gates);
        assert!(!cfg.log_responses);
        assert!(!cfg.all);
        std::env::remove_var("SELFWARE_DEBUG_REQUEST");
        std::env::remove_var("SELFWARE_DEBUG_GATE");
    }

    #[test]
    fn env_debug_top_level_sets_all() {
        let _g = clear_debug_env();
        let mut cfg = DebugConfig::default();
        std::env::set_var("SELFWARE_DEBUG", "1");
        cfg.apply_env_overrides();
        assert!(cfg.all);
        assert!(cfg.is_active());
        std::env::remove_var("SELFWARE_DEBUG");
    }

    #[test]
    fn env_falsy_values_do_not_enable() {
        let _g = clear_debug_env();
        let mut cfg = DebugConfig::default();
        std::env::set_var("SELFWARE_DEBUG", "0");
        std::env::set_var("SELFWARE_DEBUG_REQUEST", "false");
        cfg.apply_env_overrides();
        assert!(!cfg.all);
        assert!(!cfg.log_requests);
        std::env::remove_var("SELFWARE_DEBUG");
        std::env::remove_var("SELFWARE_DEBUG_REQUEST");
    }

    #[test]
    fn precedence_cli_over_toml() {
        // toml only enables responses
        let mut effective = DebugConfig {
            log_responses: true,
            ..Default::default()
        };
        // CLI adds requests and turns
        let cli = DebugConfig {
            log_requests: true,
            log_turns: true,
            ..Default::default()
        };
        effective.merge_cli(&cli);
        assert!(effective.log_requests, "CLI should add requests");
        assert!(effective.log_responses, "TOML should be preserved");
        assert!(effective.log_turns, "CLI should add turns");
        assert!(!effective.log_gates);
    }

    #[test]
    fn precedence_full_chain_default_then_toml_then_cli_then_env() {
        let _g = clear_debug_env();
        // Step 1: default (all off)
        let mut effective = DebugConfig::default();
        assert!(!effective.is_active());

        // Step 2: load TOML — enables responses
        effective = DebugConfig {
            log_responses: true,
            ..Default::default()
        };

        // Step 3: CLI flag --debug=requests
        let cli = DebugConfig::from_channel_list("requests");
        effective.merge_cli(&cli);
        assert!(effective.log_requests);
        assert!(effective.log_responses);

        // Step 4: env var SELFWARE_DEBUG_GATE=1 forces gates on
        std::env::set_var("SELFWARE_DEBUG_GATE", "1");
        effective.apply_env_overrides();
        assert!(effective.log_requests);
        assert!(effective.log_responses);
        assert!(effective.log_gates);
        assert!(!effective.log_turns);
        std::env::remove_var("SELFWARE_DEBUG_GATE");
    }

    #[test]
    fn env_cannot_disable_cli_or_toml() {
        let _g = clear_debug_env();
        let mut effective = DebugConfig {
            log_requests: true,
            ..Default::default()
        };
        // No env var set — env apply must not flip log_requests off.
        effective.apply_env_overrides();
        assert!(effective.log_requests);
    }
}
