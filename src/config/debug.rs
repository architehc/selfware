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
#[path = "../../tests/unit/config/debug/debug_test.rs"]
mod tests;
