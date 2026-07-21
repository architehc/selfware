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
        extra_body_keys: vec!["a".to_string(), "b".to_string()],
    };
    let s = af.render();
    assert!(s.contains("native_function_calling"));
    assert!(s.contains("temperature"));
    assert!(s.contains("extra_body.a"));
    assert!(s.contains("extra_body.b"));
    assert!(!s.contains("streaming"));
}
