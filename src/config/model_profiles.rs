//! Built-in model defaults profiles.
//!
//! Different model families need different sampling parameters and request
//! shapes to perform well.  Hard-coding these in user configs is a footgun:
//! a fresh user pointing selfware at Qwen 3.6 with default settings sees
//! 0/10 SWE-bench Pro because the model wants `presence_penalty = 1.5`,
//! `top_p = 0.8`, `min_p = 0.0`, `enable_thinking = true`, etc.
//!
//! This module ships built-in [`ModelDefaultsProfile`] rules keyed by a
//! glob pattern on the model name.  At config-load time the loader looks up
//! the first matching profile and fills in any field the user did NOT set
//! explicitly.  Explicit user config always wins over a profile.
//!
//! These profiles are intentionally **static** — they are derived from
//! `config.model` only and never make network calls.  Live capability
//! detection lives in [`crate::config::auto_config`].
//!
//! Naming note: the existing [`crate::config::ModelProfile`] type is a
//! per-named-model TOML section ("coder", "vision", ...).  The struct here
//! is a *defaults rule* keyed by model-name glob — different concept,
//! different name.

use serde_json::{json, Map, Value};

/// A built-in defaults profile for a family of models, matched by a glob
/// pattern on the model name (e.g. `"qwen3.6-*"`).
///
/// Each `Option` field is "apply only if the user did not set it";
/// the `extra_body` map is merged key-by-key (user keys win).
#[derive(Debug, Clone)]
pub struct ModelDefaultsProfile {
    /// Stable identifier for diagnostics (e.g. `"qwen3.6"`).
    pub name: &'static str,
    /// Glob pattern matched against `config.model` (case-insensitive).
    /// Supports `*` and `?` wildcards.
    pub pattern: &'static str,
    pub native_function_calling: Option<bool>,
    pub streaming: Option<bool>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    /// Extra JSON fields to merge into `config.extra_body`.
    /// Keys already present in the user's `extra_body` are preserved.
    pub extra_body: Value,
}

/// Names of fields a profile filled in for a particular config.  Returned
/// by [`apply_profile`] for diagnostic / introspection purposes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedFields {
    pub native_function_calling: bool,
    pub streaming: bool,
    pub temperature: bool,
    pub max_tokens: bool,
    /// Names of `extra_body` keys that were filled from the profile.
    pub extra_body_keys: Vec<String>,
}

impl AppliedFields {
    pub fn is_empty(&self) -> bool {
        !self.native_function_calling
            && !self.streaming
            && !self.temperature
            && !self.max_tokens
            && self.extra_body_keys.is_empty()
    }

    /// Render as a stable, sorted, comma-separated list for human output.
    pub fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.native_function_calling {
            parts.push("native_function_calling".to_string());
        }
        if self.streaming {
            parts.push("streaming".to_string());
        }
        if self.temperature {
            parts.push("temperature".to_string());
        }
        if self.max_tokens {
            parts.push("max_tokens".to_string());
        }
        for k in &self.extra_body_keys {
            parts.push(format!("extra_body.{}", k));
        }
        parts.join(", ")
    }
}

/// Built-in profile rules.  Order matters: the *first* matching pattern
/// wins.  Keep more-specific patterns before more-general ones.
pub fn builtin_profiles() -> Vec<ModelDefaultsProfile> {
    vec![
        // GLM-5.2 (z-ai) — the project's default model.  Card-recommended
        // reasoning-mode sampling: temperature=1.0, top_p=0.95, with the
        // model's thinking enabled via the chat template.  selfware parses the
        // resulting reasoning_content automatically, so no client change is
        // needed.  Provider pinning (full 1M context) stays in user config.
        ModelDefaultsProfile {
            name: "glm-5.2",
            pattern: "*glm-5.2*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: Some(1.0),
            max_tokens: Some(65536),
            extra_body: json!({
                "top_p": 0.95,
                "chat_template_kwargs": {
                    "enable_thinking": true,
                },
            }),
        },
        // Qwen 3.6 — needs the high presence_penalty / min_p kit and the
        // SGLang `preserve_thinking` template knob to produce its best
        // function-calling output.  Without these, SWE-bench Pro hovers
        // around 0/10 even on a 27B parameter checkpoint.
        ModelDefaultsProfile {
            name: "qwen3.6",
            pattern: "qwen3.6-*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: Some(0.7),
            max_tokens: Some(32768),
            extra_body: json!({
                "top_p": 0.8,
                "top_k": 20,
                "min_p": 0.0,
                "presence_penalty": 1.5,
                "chat_template_kwargs": {
                    "enable_thinking": true,
                    "preserve_thinking": true,
                },
            }),
        },
        // Qwen 3.5 — earlier generation, recommended sampling is closer to
        // a vanilla nucleus setup with no presence penalty.
        ModelDefaultsProfile {
            name: "qwen3.5",
            pattern: "qwen3.5-*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: Some(0.6),
            max_tokens: Some(32768),
            extra_body: json!({
                "top_p": 0.95,
                "top_k": 20,
                "presence_penalty": 0.0,
            }),
        },
        // Anthropic Claude — native tools + streaming work out of the box.
        // Sampling/extra_body left to user (Claude API rejects most knobs).
        ModelDefaultsProfile {
            name: "claude",
            pattern: "claude-*",
            native_function_calling: Some(true),
            streaming: Some(true),
            temperature: None,
            max_tokens: None,
            extra_body: Value::Null,
        },
        // OpenAI GPT — same story: native tools + streaming.
        ModelDefaultsProfile {
            name: "gpt",
            pattern: "gpt-*",
            native_function_calling: Some(true),
            streaming: Some(true),
            temperature: None,
            max_tokens: None,
            extra_body: Value::Null,
        },
    ]
}

/// Match `model` against `pattern`.  Glob semantics: `*` matches any run
/// of characters (including empty), `?` matches exactly one.  Comparison
/// is case-insensitive so e.g. `Qwen3.6-27B` matches `qwen3.6-*`.
pub fn glob_matches(pattern: &str, model: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();
    glob_matches_inner(pattern.as_bytes(), model.as_bytes())
}

fn glob_matches_inner(pat: &[u8], s: &[u8]) -> bool {
    // Iterative backtracking matcher — small, no allocations, no deps.
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star_i, mut star_j) = (None::<usize>, 0usize);
    while j < s.len() {
        if i < pat.len() && (pat[i] == b'?' || pat[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < pat.len() && pat[i] == b'*' {
            star_i = Some(i);
            star_j = j;
            i += 1;
        } else if let Some(si) = star_i {
            i = si + 1;
            star_j += 1;
            j = star_j;
        } else {
            return false;
        }
    }
    while i < pat.len() && pat[i] == b'*' {
        i += 1;
    }
    i == pat.len()
}

/// Find the first built-in profile whose pattern matches `model`.
pub fn match_profile(model: &str) -> Option<ModelDefaultsProfile> {
    builtin_profiles()
        .into_iter()
        .find(|p| glob_matches(p.pattern, model))
}

/// Apply `profile`'s defaults to `config` for any field the user did not
/// set explicitly.  `user_explicit` describes which fields were present
/// in the user's TOML; profile values do NOT override those.
///
/// Returns the set of fields that were actually filled in by the profile
/// so callers can surface this in `selfware autoconfig` output.
pub fn apply_profile(
    config: &mut crate::config::Config,
    profile: &ModelDefaultsProfile,
    user_explicit: &UserExplicitFields,
) -> AppliedFields {
    let mut applied = AppliedFields::default();

    if !user_explicit.native_function_calling {
        if let Some(v) = profile.native_function_calling {
            config.agent.native_function_calling = v;
            applied.native_function_calling = true;
        }
    }
    if !user_explicit.streaming {
        if let Some(v) = profile.streaming {
            config.agent.streaming = v;
            applied.streaming = true;
        }
    }
    if !user_explicit.temperature {
        if let Some(v) = profile.temperature {
            config.temperature = v;
            applied.temperature = true;
        }
    }
    if !user_explicit.max_tokens {
        if let Some(v) = profile.max_tokens {
            config.max_tokens = v;
            applied.max_tokens = true;
        }
    }

    // Merge extra_body — only keys NOT already present in the user's map.
    if let Value::Object(profile_extra) = &profile.extra_body {
        let dest = config.extra_body.get_or_insert_with(Map::new);
        for (k, v) in profile_extra {
            if !user_explicit.extra_body_keys.iter().any(|uk| uk == k) && !dest.contains_key(k) {
                dest.insert(k.clone(), v.clone());
                applied.extra_body_keys.push(k.clone());
            }
        }
        applied.extra_body_keys.sort();
    }

    applied
}

/// Set of fields the user explicitly set in their TOML config.  Built by
/// the loader before profile application.
#[derive(Debug, Default, Clone)]
pub struct UserExplicitFields {
    pub native_function_calling: bool,
    pub streaming: bool,
    pub temperature: bool,
    pub max_tokens: bool,
    pub extra_body_keys: Vec<String>,
}

impl UserExplicitFields {
    /// Build from raw TOML content (what was on disk before defaults).
    /// Unknown / unparseable content yields an empty set so that profiles
    /// still apply.
    pub fn from_toml(content: &str) -> Self {
        let mut out = Self::default();
        let table = match toml::from_str::<toml::Value>(content) {
            Ok(toml::Value::Table(t)) => t,
            _ => return out,
        };
        if table.contains_key("temperature") {
            out.temperature = true;
        }
        if table.contains_key("max_tokens") {
            out.max_tokens = true;
        }
        if let Some(toml::Value::Table(agent)) = table.get("agent") {
            if agent.contains_key("native_function_calling") {
                out.native_function_calling = true;
            }
            if agent.contains_key("streaming") {
                out.streaming = true;
            }
        }
        if let Some(toml::Value::Table(extra)) = table.get("extra_body") {
            out.extra_body_keys = extra.keys().cloned().collect();
        }
        out
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/model_profiles/model_profiles_test.rs"]
mod tests;
