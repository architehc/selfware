use super::*;
use crate::config::Config;

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
fn match_profile_picks_glm52_for_openrouter_glm_model() {
    // config.model is the OpenRouter id "z-ai/glm-5.2".
    let p = match_profile("z-ai/glm-5.2").expect("should match");
    assert_eq!(p.name, "glm-5.2");
    assert_eq!(p.temperature, Some(1.0));
    assert_eq!(p.max_tokens, Some(65536));
    assert_eq!(p.native_function_calling, Some(true));
    let eb = p.extra_body.as_object().expect("extra_body is object");
    assert_eq!(eb.get("top_p"), Some(&json!(0.95)));
    assert_eq!(
        eb.get("chat_template_kwargs")
            .and_then(|k| k.get("enable_thinking")),
        Some(&json!(true))
    );
    // dated snapshot ids still match via the trailing wildcard.
    assert_eq!(
        match_profile("z-ai/glm-5.2-20260616").map(|p| p.name),
        Some("glm-5.2")
    );
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
fn pattern_matches_provider_prefixed_id_via_last_segment() {
    // OpenRouter-style `vendor/model` ids: the anchored profile glob must
    // match the model name after the namespace prefix.
    assert!(pattern_matches_model("qwen3.6-*", "qwen/qwen3.6-27b"));
    // Multiple prefixes still resolve to the final segment.
    assert!(pattern_matches_model(
        "qwen3.6-*",
        "openrouter/qwen/qwen3.6-27b"
    ));
    // The full-id attempt keeps working for patterns that already span it.
    assert!(pattern_matches_model("*glm-5.2*", "z-ai/glm-5.2"));
    // A prefix alone must not conjure a match: the tail is not qwen3.6.
    assert!(!pattern_matches_model("qwen3.6-*", "qwen/qwen3.5-27b"));
    // Trailing slash leaves an empty tail — no match beyond the full id.
    assert!(!pattern_matches_model("qwen3.6-*", "models/qwen3.6-27b/"));
}

#[test]
fn pattern_matches_path_qualified_local_id_via_last_segment() {
    // Local sglang/vLLM ids are serving paths; a sensible glob on the
    // directory basename must match.
    assert!(pattern_matches_model(
        "qwen38-*",
        "/home/rig/models/qwen38-unc-kt"
    ));
    assert!(pattern_matches_model(
        "qwen3.6-*",
        "/home/rig/models/qwen3.6-27b"
    ));
}

#[test]
fn match_profile_picks_qwen36_for_provider_prefixed_and_path_ids() {
    assert_eq!(
        match_profile("qwen/qwen3.6-27b").map(|p| p.name),
        Some("qwen3.6")
    );
    assert_eq!(
        match_profile("/home/rig/models/qwen3.6-27b").map(|p| p.name),
        Some("qwen3.6")
    );
    // Case-insensitivity applies to the tail segment too.
    assert_eq!(
        match_profile("Qwen/Qwen3.6-27B").map(|p| p.name),
        Some("qwen3.6")
    );
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
    assert!(applied
        .extra_body_keys
        .contains(&"presence_penalty".to_string()));
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
        context_length: false,
        max_streams: false,
        max_global: false,
        max_call_secs: false,
        context_content_ratio: false,
        extra_body_keys: vec!["presence_penalty".to_string()],
        workload_max_tokens: None,
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
    assert!(applied.extra_body_keys.contains(&"top_p".to_string()));
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
        context_length: true,
        max_streams: true,
        max_global: true,
        max_call_secs: false,
        context_content_ratio: false,
        max_call_secs_scaled_for_max_tokens: None,
        extra_body_keys: vec!["a".to_string(), "b".to_string()],
        workload_fields: Vec::new(),
        workload_overrides: Vec::new(),
    };
    let s = af.render();
    assert!(s.contains("native_function_calling"));
    assert!(s.contains("temperature"));
    assert!(s.contains("context_length"));
    assert!(s.contains("concurrency.max_streams"));
    assert!(s.contains("concurrency.max_global"));
    assert!(s.contains("extra_body.a"));
    assert!(s.contains("extra_body.b"));
    assert!(!s.contains("streaming"));
}

#[test]
fn qwen38_profile_sets_preserve_thinking_false_and_sampling_defaults() {
    let p = match_profile("Qwen/Qwen3.8-Flash-Next").expect("Qwen/Qwen3.8-Flash-Next should match");
    assert_eq!(p.name, "qwen3.8");
    assert_eq!(p.native_function_calling, Some(false));
    assert_eq!(p.streaming, Some(true));
    assert_eq!(p.temperature, Some(0.7));
    assert_eq!(p.context_length, Some(163_840));
    assert_eq!(p.max_tokens, Some(24_576));
    assert_eq!(p.max_call_secs, Some(1_628));
    assert_eq!(p.max_streams, Some(8));
    assert_eq!(p.max_global, Some(16));
    let obj = p.extra_body.as_object().expect("extra_body object");
    assert_eq!(obj.get("top_p"), Some(&json!(0.95)));
    assert_eq!(obj.get("top_k"), Some(&json!(20)));
    assert_eq!(obj.get("presence_penalty"), Some(&json!(0.0)));
    assert_eq!(obj.get("repetition_penalty"), Some(&json!(1.0)));
    let ctk = obj
        .get("chat_template_kwargs")
        .and_then(|v| v.as_object())
        .expect("chat_template_kwargs object");
    assert_eq!(ctk.get("enable_thinking"), Some(&json!(true)));
    assert_eq!(ctk.get("preserve_thinking"), Some(&json!(false)));

    let p2 = match_profile("qwen38-flash-next").expect("qwen38-flash-next should match");
    assert_eq!(p2.name, "qwen38");
    assert_eq!(p2.native_function_calling, Some(false));
    assert_eq!(p2.temperature, Some(0.7));
    assert_eq!(p2.context_length, Some(163_840));
    assert_eq!(p2.max_streams, Some(8));
    assert_eq!(p2.max_global, Some(16));
    let obj2 = p2.extra_body.as_object().expect("extra_body object");
    let ctk2 = obj2
        .get("chat_template_kwargs")
        .and_then(|v| v.as_object())
        .expect("chat_template_kwargs object");
    assert_eq!(ctk2.get("preserve_thinking"), Some(&json!(false)));
}

#[test]
fn test_config_preserve_thinking_false_by_default_and_true_when_configured() {
    let cfg = Config::default();
    assert!(
        !cfg.preserve_thinking(),
        "preserve_thinking must be false by default"
    );

    let mut cfg2 = Config::default();
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "preserve_thinking": true }),
    );
    cfg2.extra_body = Some(extra);
    assert!(
        cfg2.preserve_thinking(),
        "preserve_thinking must be true when set"
    );
}

#[test]
fn test_apply_profile_sets_context_length_and_max_streams_for_qwen38() {
    let mut config = Config::default();
    let profile = match_profile("Qwen/Qwen3.8-Flash-Next").expect("should match profile");
    let user_explicit = UserExplicitFields::default();
    let applied = apply_profile(&mut config, &profile, &user_explicit);

    assert!(applied.context_length);
    assert!(applied.max_streams);
    assert!(applied.max_global);
    assert!(applied.max_call_secs);
    assert_eq!(config.context_length, 163_840);
    assert_eq!(config.max_tokens, 24_576);
    assert_eq!(config.concurrency.max_streams, 8);
    assert_eq!(config.concurrency.max_global, 16);
    assert_eq!(config.agent.max_call_secs, Some(1_628));
    assert_eq!(config.temperature, 0.7);
}

#[test]
fn qwen38_measured_defaults_yield_to_explicit_user_config() {
    // Profile defaults (context 163,840 / max_tokens 24,576 / 8 streams /
    // max_call_secs 1,628) must never override values the user set in TOML.
    let toml = r#"
model = "qwen38-flash-next"
max_tokens = 4096
context_length = 65536

[concurrency]
max_streams = 2
max_global = 5

[agent]
max_call_secs = 120
"#;
    let user_explicit = UserExplicitFields::from_toml(toml);
    assert!(user_explicit.max_call_secs);
    let mut config = Config {
        max_tokens: 4096,
        context_length: 65536,
        ..Default::default()
    };
    config.concurrency.max_streams = 2;
    config.concurrency.max_global = 5;
    config.agent.max_call_secs = Some(120);
    let profile = match_profile("qwen38-flash-next").unwrap();
    let applied = apply_profile(&mut config, &profile, &user_explicit);
    assert!(!applied.max_tokens && !applied.context_length);
    assert!(!applied.max_streams && !applied.max_global && !applied.max_call_secs);
    assert_eq!(config.max_tokens, 4096);
    assert_eq!(config.context_length, 65536);
    assert_eq!(config.concurrency.max_streams, 2);
    assert_eq!(config.concurrency.max_global, 5);
    assert_eq!(config.agent.max_call_secs, Some(120));
}

#[test]
fn profiles_without_call_cap_leave_max_call_secs_uncapped() {
    for model in ["qwen3.6-27b", "glm-5.2", "claude-sonnet-5", "gpt-6"] {
        let mut config = Config::default();
        let profile = match_profile(model).unwrap();
        let applied = apply_profile(&mut config, &profile, &UserExplicitFields::default());
        assert!(!applied.max_call_secs, "{model}");
        assert_eq!(config.agent.max_call_secs, None, "{model}");
    }
}

/// D7: the qwen38 cap is sized for its own 24,576 max_tokens; a larger
/// max_tokens scales it by the same ratio, rounded up.
#[test]
fn qwen38_max_call_secs_scales_with_max_tokens() {
    let p = match_profile("qwen38-flash-next").expect("qwen38 profile");
    assert_eq!(p.max_call_secs_for(24_576), Some((1_628, false)));
    assert_eq!(p.max_call_secs_for(8_192), Some((1_628, false)));
    // ceil(1628 * 65536 / 24576) = ceil(4341.33) = 4342
    assert_eq!(p.max_call_secs_for(65_536), Some((4_342, true)));
    // ceil(1628 * 30000 / 24576) = ceil(1987.30) = 1988
    assert_eq!(p.max_call_secs_for(30_000), Some((1_988, true)));
}

/// Rule-5 sweep: every built-in profile that sets both fields gets the same
/// sizing rule; a profile without max_call_secs implies no cap.
#[test]
fn every_profile_with_both_fields_scales_its_call_cap() {
    for p in builtin_profiles() {
        match (p.max_call_secs, p.max_tokens) {
            (Some(secs), Some(mt)) => {
                assert_eq!(p.max_call_secs_for(mt), Some((secs, false)), "{}", p.name);
                let (scaled, did) = p.max_call_secs_for(mt * 2).unwrap();
                assert!(did && scaled == secs * 2, "{}: {scaled}", p.name);
            }
            (None, _) => assert_eq!(p.max_call_secs_for(1 << 20), None, "{}", p.name),
            (Some(secs), None) => {
                assert_eq!(
                    p.max_call_secs_for(1 << 20),
                    Some((secs, false)),
                    "{}",
                    p.name
                )
            }
        }
    }
}

// ── Per-workload quotas ─────────────────────────────────────────────────────

/// With nothing set by the user, the qwen38 profile's measured table fills
/// every `[workloads]` field it defines, and names them as applied.
#[test]
fn qwen38_workload_table_fills_unset_workloads() {
    let mut config = Config::default();
    let profile = match_profile("qwen38-flash-next").unwrap();
    let applied = apply_profile(&mut config, &profile, &UserExplicitFields::default());
    assert_eq!(config.workloads, QWEN38_WORKLOAD_QUOTAS);
    assert!(applied.workload_overrides.is_empty());
    assert!(applied
        .workload_fields
        .contains(&"workloads.planning.enable_thinking".to_string()));
    assert_eq!(config.workloads.planning.enable_thinking, Some(false));
    assert_eq!(config.workloads.mechanical.enable_thinking, Some(true));
    assert_eq!(config.workloads.synthesis.enable_thinking, Some(true));
    assert_eq!(config.workloads.synthesis.max_tokens, Some(16_384));
}

/// A user `extra_body` `enable_thinking` pin holds for EVERY turn: the
/// profile's per-turn toggle is not applied, and that is reported.
/// Per-turn max_tokens still applies (the pin says nothing about it).
#[test]
fn extra_body_thinking_pin_overrides_profile_per_turn_thinking() {
    let mut config = Config::default();
    let mut extra = Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({"enable_thinking": true}),
    );
    config.extra_body = Some(extra);
    let profile = match_profile("qwen38-flash-next").unwrap();
    let user_explicit = UserExplicitFields {
        extra_body_keys: vec!["chat_template_kwargs".to_string()],
        ..Default::default()
    };
    let applied = apply_profile(&mut config, &profile, &user_explicit);
    for kind in TurnWorkload::ALL {
        assert_eq!(config.workloads.get(kind).enable_thinking, None, "{kind}");
        assert_eq!(
            config.workloads.get(kind).max_tokens,
            QWEN38_WORKLOAD_QUOTAS.get(kind).max_tokens,
            "{kind}"
        );
    }
    assert!(applied
        .workload_overrides
        .iter()
        .any(|n| n.contains("enable_thinking")));
}

/// An explicit top-level max_tokens holds for every turn; an explicit
/// `[workloads.<kind>]` value wins over both the profile and the pins.
#[test]
fn explicit_settings_win_over_the_profile_workload_table() {
    let toml = r#"
model = "qwen38-flash-next"
max_tokens = 4096

[extra_body.chat_template_kwargs]
enable_thinking = true

[workloads.mechanical]
enable_thinking = false
max_tokens = 2048
"#;
    let user_explicit = UserExplicitFields::from_toml(toml);
    assert_eq!(user_explicit.workload_max_tokens, Some(2048));
    let mut config: Config = toml::from_str(toml).unwrap();
    let profile = match_profile("qwen38-flash-next").unwrap();
    let applied = apply_profile(&mut config, &profile, &user_explicit);
    assert_eq!(
        config.workloads.mechanical,
        WorkloadQuota {
            enable_thinking: Some(false),
            max_tokens: Some(2048),
        }
    );
    assert_eq!(config.workloads.synthesis, WorkloadQuota::default());
    assert!(
        applied.workload_fields.is_empty(),
        "{:?}",
        applied.workload_fields
    );
    assert!(applied
        .workload_overrides
        .iter()
        .any(|n| n.contains("enable_thinking")));
}

/// The per-call wall-time cap is sized for the largest completion any turn
/// may ask for: an explicit workload max_tokens above the profile's scales
/// it like a raised top-level max_tokens.
#[test]
fn workload_max_tokens_above_profile_scales_the_call_cap() {
    let toml = r#"
model = "qwen38-flash-next"

[workloads.synthesis]
max_tokens = 49152
"#;
    let user_explicit = UserExplicitFields::from_toml(toml);
    let mut config: Config = toml::from_str(toml).unwrap();
    let profile = match_profile("qwen38-flash-next").unwrap();
    let applied = apply_profile(&mut config, &profile, &user_explicit);
    // ceil(1628 * 49152 / 24576) = 3256
    assert_eq!(config.agent.max_call_secs, Some(3_256));
    assert_eq!(applied.max_call_secs_scaled_for_max_tokens, Some(49_152));
}

/// Rule-5 sweep: no built-in workload table asks for more completion than
/// its profile's max_tokens, so the profile's max_call_secs (sized for that
/// max_tokens) covers every turn kind.
#[test]
fn builtin_workload_tables_stay_within_profile_max_tokens() {
    for p in builtin_profiles() {
        let Some(table) = p.workload_quotas else {
            continue;
        };
        if let (Some(worst), Some(pm)) = (table.max_max_tokens(), p.max_tokens) {
            assert!(worst <= pm, "{}: {worst} > {pm}", p.name);
        }
        assert!(
            !p.measured.is_empty(),
            "{}: table without measurements",
            p.name
        );
    }
}

/// qwen38's compaction point is per endpoint (0.80, from measured per-turn
/// prompt growth); an explicit `[agent] context_content_ratio` still wins.
#[test]
fn qwen38_compaction_ratio_applies_unless_set_explicitly() {
    let mut config = Config::default();
    let profile = match_profile("qwen38-flash-next").unwrap();
    let applied = apply_profile(&mut config, &profile, &UserExplicitFields::default());
    assert!(applied.context_content_ratio);
    // Derived from the measured headroom: 1 − 21,221 / 106,496 = 0.8007.
    assert_eq!(config.agent.context_growth_p99_tokens, Some(21_221));
    assert!((config.effective_context_content_ratio() - 0.8007).abs() < 1e-4);
    assert!((config.agent.context_content_ratio - 0.8007).abs() < 1e-4);
    assert!(applied.render().contains("agent.context_content_ratio"));

    let toml = "model = \"qwen38-flash-next\"\n[agent]\ncontext_content_ratio = 0.6\n";
    let user_explicit = UserExplicitFields::from_toml(toml);
    let mut config: Config = toml::from_str(toml).unwrap();
    let applied = apply_profile(&mut config, &profile, &user_explicit);
    assert!(!applied.context_content_ratio);
    assert!((config.agent.context_content_ratio - 0.6).abs() < f32::EPSILON);
    assert_eq!(config.agent.context_growth_p99_tokens, None);
    assert!((config.effective_context_content_ratio() - 0.6).abs() < f32::EPSILON);
    // Other profiles keep the global default.
    let mut config = Config::default();
    apply_profile(
        &mut config,
        &match_profile("qwen3.6-27b").unwrap(),
        &UserExplicitFields::default(),
    );
    assert!((config.agent.context_content_ratio - 0.75).abs() < f32::EPSILON);
}

/// Review 2026-09-27: the 0.80 derived for a 106,496-token history budget
/// was applied whatever budget the session actually had. The measured
/// headroom is kept and the ratio follows the session's budget.
#[test]
fn qwen38_compaction_ratio_follows_the_session_budget() {
    let profile = match_profile("qwen38-flash-next").unwrap();
    let toml = "model = \"qwen38-flash-next\"\nmax_tokens = 8192\n";
    let user_explicit = UserExplicitFields::from_toml(toml);
    let mut config: Config = toml::from_str(toml).unwrap();
    apply_profile(&mut config, &profile, &user_explicit);
    // 163,840 − 8,192 − 32,768 = 122,880 → 1 − 21,221 / 122,880 = 0.8273.
    let (budget, _) = config.derive_context_budget().unwrap();
    assert_eq!(budget, 122_880);
    let ratio = config.effective_context_content_ratio();
    assert!((ratio - 0.8273).abs() < 1e-4, "{ratio}");
    // The threshold sits exactly one p99 turn below the budget.
    let threshold = (budget as f64 * ratio as f64).round() as usize;
    assert!((budget - threshold).abs_diff(21_221) < 16, "{threshold}");
    // A later `--profile`-style max_tokens change moves it again.
    config.max_tokens = 40_000;
    let (budget, _) = config.derive_context_budget().unwrap();
    let ratio = config.effective_context_content_ratio();
    assert!(((1.0 - 21_221.0 / budget as f64) as f32 - ratio).abs() < 1e-5);
}
