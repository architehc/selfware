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
/// by [`apply_matched_profile`] for diagnostic / introspection purposes.
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
        let dest = config
            .extra_body
            .get_or_insert_with(Map::new);
        for (k, v) in profile_extra {
            if !user_explicit.extra_body_keys.iter().any(|uk| uk == k)
                && !dest.contains_key(k)
            {
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
mod tests {
    use super::*;

    #[test]
    fn glob_qwen36_matches_only_36() {
        assert!(glob_matches("qwen3.6-*", "qwen3.6-27b-q4kp"));
        assert!(glob_matches("qwen3.6-*", "qwen3.6-32b"));
        // Must NOT match 3.5 — the dot in the pattern is a literal.
        assert!(!glob_matches("qwen3.6-*", "qwen3.5-27b"));
        // Different family entirely.
        assert!(!glob_matches("qwen3.6-*", "claude-3-opus"));
    }

    #[test]
    fn glob_is_case_insensitive() {
        assert!(glob_matches("qwen3.6-*", "Qwen3.6-27B-Q4KP"));
        assert!(glob_matches("CLAUDE-*", "claude-3-7-sonnet"));
    }

    #[test]
    fn glob_handles_question_mark() {
        assert!(glob_matches("gpt-?", "gpt-4"));
        assert!(!glob_matches("gpt-?", "gpt-44"));
    }

    #[test]
    fn match_profile_picks_qwen36_for_qwen36_model() {
        let p = match_profile("qwen3.6-27b-q4kp").expect("should match");
        assert_eq!(p.name, "qwen3.6");
        assert_eq!(p.temperature, Some(0.7));
        assert_eq!(p.native_function_calling, Some(true));
    }

    #[test]
    fn match_profile_picks_qwen35_for_qwen35_model() {
        let p = match_profile("qwen3.5-27b").expect("should match");
        assert_eq!(p.name, "qwen3.5");
        assert_eq!(p.temperature, Some(0.6));
    }

    #[test]
    fn match_profile_picks_claude_for_claude_model() {
        let p = match_profile("claude-3-7-sonnet").expect("should match");
        assert_eq!(p.name, "claude");
        assert_eq!(p.streaming, Some(true));
    }

    #[test]
    fn match_profile_picks_gpt_for_gpt_model() {
        let p = match_profile("gpt-4o-mini").expect("should match");
        assert_eq!(p.name, "gpt");
    }

    #[test]
    fn match_profile_returns_none_for_unknown_model() {
        assert!(match_profile("llama-3-70b").is_none());
        assert!(match_profile("mistral-large").is_none());
    }

    #[test]
    fn qwen36_profile_carries_required_extra_body_keys() {
        let p = match_profile("qwen3.6-27b").expect("should match");
        let obj = p.extra_body.as_object().expect("object");
        assert_eq!(obj.get("presence_penalty"), Some(&json!(1.5)));
        assert_eq!(obj.get("top_p"), Some(&json!(0.8)));
        assert_eq!(obj.get("min_p"), Some(&json!(0.0)));
        let ctk = obj
            .get("chat_template_kwargs")
            .and_then(|v| v.as_object())
            .expect("chat_template_kwargs object");
        assert_eq!(ctk.get("enable_thinking"), Some(&json!(true)));
        assert_eq!(ctk.get("preserve_thinking"), Some(&json!(true)));
    }

    #[test]
    fn user_explicit_fields_detects_top_level_fields() {
        let toml_text = r#"
model = "qwen3.6-27b"
temperature = 0.42
max_tokens = 1234
[agent]
native_function_calling = false
streaming = false
[extra_body]
presence_penalty = 0.0
top_p = 0.5
"#;
        let u = UserExplicitFields::from_toml(toml_text);
        assert!(u.temperature);
        assert!(u.max_tokens);
        assert!(u.native_function_calling);
        assert!(u.streaming);
        let mut keys = u.extra_body_keys.clone();
        keys.sort();
        assert_eq!(keys, vec!["presence_penalty", "top_p"]);
    }

    #[test]
    fn user_explicit_fields_empty_for_minimal_toml() {
        let toml_text = r#"
endpoint = "http://localhost:1234/v1"
model = "qwen3.6-27b"
"#;
        let u = UserExplicitFields::from_toml(toml_text);
        assert!(!u.temperature);
        assert!(!u.max_tokens);
        assert!(!u.native_function_calling);
        assert!(!u.streaming);
        assert!(u.extra_body_keys.is_empty());
    }

    #[test]
    fn apply_profile_fills_missing_fields() {
        let mut config = crate::config::Config::default();
        // Mark agent as "user did not touch any of these" by setting them
        // to non-default sentinels we can detect.
        config.agent.native_function_calling = false;
        config.temperature = 1.0;
        config.max_tokens = 65536;
        config.extra_body = None;

        let profile = match_profile("qwen3.6-27b").unwrap();
        let user_explicit = UserExplicitFields::default();
        let applied = apply_profile(&mut config, &profile, &user_explicit);

        assert!(config.agent.native_function_calling);
        assert!((config.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, 32768);
        let extra = config.extra_body.as_ref().expect("extra_body filled");
        assert_eq!(extra.get("presence_penalty"), Some(&json!(1.5)));
        assert!(applied.native_function_calling);
        assert!(applied.temperature);
        assert!(applied.max_tokens);
        assert!(applied.extra_body_keys.contains(&"presence_penalty".to_string()));
    }

    #[test]
    fn apply_profile_respects_explicit_user_config() {
        let mut config = crate::config::Config::default();
        config.agent.native_function_calling = false;
        config.temperature = 0.123;
        config.max_tokens = 999;
        let mut user_extra = Map::new();
        user_extra.insert("presence_penalty".to_string(), json!(0.25));
        config.extra_body = Some(user_extra);

        let profile = match_profile("qwen3.6-27b").unwrap();
        let user_explicit = UserExplicitFields {
            native_function_calling: true,
            streaming: false,
            temperature: true,
            max_tokens: true,
            extra_body_keys: vec!["presence_penalty".to_string()],
        };
        let applied = apply_profile(&mut config, &profile, &user_explicit);

        // Explicit values must NOT be overwritten.
        assert!(!config.agent.native_function_calling);
        assert!((config.temperature - 0.123).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, 999);
        let extra = config.extra_body.as_ref().unwrap();
        assert_eq!(extra.get("presence_penalty"), Some(&json!(0.25)));
        // ...but other profile keys WERE filled in.
        assert_eq!(extra.get("top_p"), Some(&json!(0.8)));
        assert!(!applied.native_function_calling);
        assert!(!applied.temperature);
        assert!(!applied.max_tokens);
        assert!(applied
            .extra_body_keys
            .contains(&"top_p".to_string()));
        assert!(!applied
            .extra_body_keys
            .contains(&"presence_penalty".to_string()));
    }

    #[test]
    fn applied_fields_render_is_stable() {
        let af = AppliedFields {
            native_function_calling: true,
            streaming: false,
            temperature: true,
            max_tokens: false,
            extra_body_keys: vec!["a".to_string(), "b".to_string()],
        };
        let s = af.render();
        assert!(s.contains("native_function_calling"));
        assert!(s.contains("temperature"));
        assert!(s.contains("extra_body.a"));
        assert!(s.contains("extra_body.b"));
        assert!(!s.contains("streaming"));
    }
}
