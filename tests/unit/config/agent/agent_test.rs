use super::*;

// ---------------------------------------------------------------------------
// Default function tests
// ---------------------------------------------------------------------------

#[test]
fn test_default_max_iterations() {
    assert_eq!(default_max_iterations(), 100);
}

#[test]
fn test_default_step_timeout() {
    assert_eq!(default_step_timeout(), 300);
}

#[test]
fn test_default_min_completion_steps() {
    assert_eq!(default_min_completion_steps(), 3);
}

#[test]
fn test_default_token_budget_is_zero_sentinel() {
    // 0 is a sentinel meaning "derive from max_tokens at load time"
    assert_eq!(default_token_budget(), 0);
}

#[test]
fn test_default_token_safety_margin() {
    assert_eq!(default_token_safety_margin(), 8_192);
}

#[test]
fn test_default_context_content_ratio() {
    let r = default_context_content_ratio();
    assert!((r - 0.75).abs() < f32::EPSILON);
    assert!(r > 0.0 && r < 1.0);
}

#[test]
fn test_default_context_compression_ratio() {
    let r = default_context_compression_ratio();
    assert!((r - 0.20).abs() < f32::EPSILON);
    assert!(r > 0.0 && r < 1.0);
}

#[test]
fn test_default_context_thinking_ratio() {
    let r = default_context_thinking_ratio();
    assert!((r - 0.05).abs() < f32::EPSILON);
    assert!(r > 0.0 && r < 1.0);
}

#[test]
fn test_default_context_ratios_sum_to_one() {
    // The three reserved fractions should sum to exactly 1.0, meaning
    // all of the token_budget is allocated across content, compression,
    // and thinking partitions.
    let total = default_context_content_ratio()
        + default_context_compression_ratio()
        + default_context_thinking_ratio();
    assert!(
        (total - 1.0_f32).abs() < f32::EPSILON,
        "content + compression + thinking ratios should sum to 1.0, got {total}"
    );
}

#[test]
fn test_default_compression_detail() {
    assert_eq!(default_compression_detail(), "signatures");
}

#[test]
fn test_default_prompt_profile() {
    assert_eq!(default_prompt_profile(), "default");
}

// ---------------------------------------------------------------------------
// ReadLoopPolicy tests
// ---------------------------------------------------------------------------

#[test]
fn test_read_loop_policy_default_is_force_mutation() {
    assert_eq!(ReadLoopPolicy::default(), ReadLoopPolicy::ForceMutation);
}

#[test]
fn test_read_loop_policy_copy_and_equality() {
    let a = ReadLoopPolicy::Nudge;
    let b = a; // relies on Copy
    assert_eq!(a, b);
    assert_ne!(a, ReadLoopPolicy::ForceMutation);
}

#[test]
fn test_read_loop_policy_serde_snake_case() {
    // ForceMutation → "force_mutation"
    let json = serde_json::to_string(&ReadLoopPolicy::ForceMutation).unwrap();
    assert_eq!(json, "\"force_mutation\"");

    let nudge_json = serde_json::to_string(&ReadLoopPolicy::Nudge).unwrap();
    assert_eq!(nudge_json, "\"nudge\"");
}

#[test]
fn test_read_loop_policy_serde_roundtrip() {
    for variant in [ReadLoopPolicy::Nudge, ReadLoopPolicy::ForceMutation] {
        let json = serde_json::to_string(&variant).unwrap();
        let back: ReadLoopPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn test_read_loop_policy_serde_invalid_variant() {
    let result: Result<ReadLoopPolicy, _> = serde_json::from_str("\"bogus\"");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// AgentConfig::default tests
// ---------------------------------------------------------------------------

#[test]
fn test_agent_config_default_matches_individual_defaults() {
    let cfg = AgentConfig::default();
    assert_eq!(cfg.max_iterations, default_max_iterations());
    assert_eq!(cfg.step_timeout_secs, default_step_timeout());
    assert_eq!(cfg.min_completion_steps, default_min_completion_steps());
    assert_eq!(cfg.token_safety_margin, default_token_safety_margin());
    assert!((cfg.context_content_ratio - default_context_content_ratio()).abs() < f32::EPSILON);
    assert!(
        (cfg.context_compression_ratio - default_context_compression_ratio()).abs() < f32::EPSILON
    );
    assert!((cfg.context_thinking_ratio - default_context_thinking_ratio()).abs() < f32::EPSILON);
    assert_eq!(cfg.compression_detail, default_compression_detail());
    assert_eq!(cfg.prompt_profile, default_prompt_profile());
}

#[test]
fn test_agent_config_default_specific_values() {
    let cfg = AgentConfig::default();
    // Fields that don't have a dedicated default fn but have known defaults
    assert!(!cfg.native_function_calling);
    assert!(cfg.streaming);
    assert!(cfg.require_verification_before_completion);
    assert_eq!(cfg.read_loop_policy, ReadLoopPolicy::ForceMutation);
    assert!(!cfg.require_visual_verification);
    // P0 privacy: turn artifacts (request/response/reasoning/tool-args
    // written to disk) must be OFF by default so secrets in conversation
    // text are NOT persisted unless the user explicitly opts in.
    assert!(cfg.disable_turn_artifacts);
    assert_eq!(cfg.post_edit_test_command, None);
    assert_eq!(cfg.max_budget_tokens, None);
    assert_eq!(cfg.max_wall_secs, None);
    assert_eq!(cfg.max_cost_usd, None);
}

#[test]
fn test_agent_config_default_token_budget_matches_max_tokens() {
    // In Default::default(), token_budget is set to super::default_max_tokens()
    // which is 65536, NOT default_token_budget() (which returns 0).
    let cfg = AgentConfig::default();
    assert_eq!(cfg.token_budget, 65536);
    assert_ne!(cfg.token_budget, default_token_budget());
}

// ---------------------------------------------------------------------------
// Serde round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_agent_config_serde_roundtrip() {
    let cfg = AgentConfig {
        max_iterations: 42,
        step_timeout_secs: 99,
        verify_after_edit: None,
        stream_stall_timeout_secs: None,
        token_budget: 12345,
        token_safety_margin: 4096,
        native_function_calling: true,
        streaming: false,
        min_completion_steps: 7,
        require_verification_before_completion: false,
        read_loop_policy: ReadLoopPolicy::Nudge,
        require_visual_verification: true,
        context_content_ratio: 0.5,
        context_compression_ratio: 0.3,
        context_thinking_ratio: 0.1,
        compression_detail: "names".to_string(),
        disable_turn_artifacts: true,
        prompt_profile: "swe_bench".to_string(),
        post_edit_test_command: Some("cargo test".to_string()),
        max_budget_tokens: Some(99999),
        max_wall_secs: Some(600),
        max_cost_usd: Some(1.5),
    };

    let json = serde_json::to_string(&cfg).unwrap();
    let back: AgentConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(back.max_iterations, 42);
    assert_eq!(back.step_timeout_secs, 99);
    assert_eq!(back.token_budget, 12345);
    assert_eq!(back.token_safety_margin, 4096);
    assert!(back.native_function_calling);
    assert!(!back.streaming);
    assert_eq!(back.min_completion_steps, 7);
    assert!(!back.require_verification_before_completion);
    assert_eq!(back.read_loop_policy, ReadLoopPolicy::Nudge);
    assert!(back.require_visual_verification);
    assert!((back.context_content_ratio - 0.5).abs() < f32::EPSILON);
    assert!((back.context_compression_ratio - 0.3).abs() < f32::EPSILON);
    assert!((back.context_thinking_ratio - 0.1).abs() < f32::EPSILON);
    assert_eq!(back.compression_detail, "names");
    assert!(back.disable_turn_artifacts);
    assert_eq!(back.prompt_profile, "swe_bench");
    assert_eq!(back.post_edit_test_command, Some("cargo test".to_string()));
    // Budget fields persist in config files (benchmark profiles need them —
    // found by the harness proposer: max_wall_secs under [agent] was ignored
    // on every Harbor run because the fields were serde(skip) CLI-only).
    assert_eq!(back.max_budget_tokens, Some(99999));
    assert_eq!(back.max_wall_secs, Some(600));
    assert_eq!(back.max_cost_usd, Some(1.5));
}

#[test]
fn test_agent_config_serde_skip_fields_not_in_json() {
    let cfg = AgentConfig {
        max_budget_tokens: Some(50000),
        max_wall_secs: Some(120),
        max_cost_usd: Some(0.5),
        ..AgentConfig::default()
    };
    let json = serde_json::to_string(&cfg).unwrap();
    // The budget fields are persisted now (benchmark profiles set them).
    assert!(json.contains("max_budget_tokens"));
    assert!(json.contains("max_wall_secs"));
    assert!(json.contains("max_cost_usd"));
}

#[test]
fn test_agent_config_serde_empty_json_uses_defaults() {
    let back: AgentConfig = serde_json::from_str("{}").unwrap();
    // All serde-defaulted fields should match their default functions
    assert_eq!(back.max_iterations, default_max_iterations());
    assert_eq!(back.step_timeout_secs, default_step_timeout());
    assert_eq!(back.token_budget, default_token_budget()); // 0 sentinel
    assert_eq!(back.token_safety_margin, default_token_safety_margin());
    assert!(!back.native_function_calling); // #[serde(default)] → false
    assert!(back.streaming); // default_true
    assert_eq!(back.min_completion_steps, default_min_completion_steps());
    assert!(back.require_verification_before_completion); // default_true
    assert_eq!(back.read_loop_policy, ReadLoopPolicy::default());
    assert!(!back.require_visual_verification); // #[serde(default)] → false
    assert!((back.context_content_ratio - default_context_content_ratio()).abs() < f32::EPSILON);
    assert!(
        (back.context_compression_ratio - default_context_compression_ratio()).abs() < f32::EPSILON
    );
    assert!((back.context_thinking_ratio - default_context_thinking_ratio()).abs() < f32::EPSILON);
    assert_eq!(back.compression_detail, default_compression_detail());
    assert!(back.disable_turn_artifacts); // omitted capture remains privacy-safe
    assert_eq!(back.prompt_profile, default_prompt_profile());
    assert_eq!(back.post_edit_test_command, None); // #[serde(default)] → None
    assert_eq!(back.max_budget_tokens, None);
    assert_eq!(back.max_wall_secs, None);
    assert_eq!(back.max_cost_usd, None);
}

#[test]
fn test_agent_config_serde_partial_json_overrides() {
    let json = r#"{
            "max_iterations": 50,
            "native_function_calling": true,
            "compression_detail": "full"
        }"#;
    let back: AgentConfig = serde_json::from_str(json).unwrap();
    assert_eq!(back.max_iterations, 50);
    assert!(back.native_function_calling);
    assert_eq!(back.compression_detail, "full");
    // Untouched fields should still have defaults
    assert_eq!(back.step_timeout_secs, default_step_timeout());
    assert!(back.streaming);
}

#[test]
fn test_agent_config_serde_read_loop_policy_in_json() {
    let json = r#"{"read_loop_policy": "nudge"}"#;
    let back: AgentConfig = serde_json::from_str(json).unwrap();
    assert_eq!(back.read_loop_policy, ReadLoopPolicy::Nudge);

    let json2 = r#"{"read_loop_policy": "force_mutation"}"#;
    let back2: AgentConfig = serde_json::from_str(json2).unwrap();
    assert_eq!(back2.read_loop_policy, ReadLoopPolicy::ForceMutation);
}

#[test]
fn test_agent_config_serde_post_edit_test_command() {
    let json = r#"{"post_edit_test_command": "make test"}"#;
    let back: AgentConfig = serde_json::from_str(json).unwrap();
    assert_eq!(back.post_edit_test_command, Some("make test".to_string()));

    let json_none = r#"{}"#;
    let back_none: AgentConfig = serde_json::from_str(json_none).unwrap();
    assert_eq!(back_none.post_edit_test_command, None);
}

#[test]
fn test_agent_config_serde_unknown_field_rejected() {
    // Without #[serde(deny_unknown_fields)] serde will ignore unknown fields.
    // Verify deserialization succeeds even with extra fields (current behavior).
    let json = r#"{"unknown_field": 123, "max_iterations": 10}"#;
    let result: Result<AgentConfig, _> = serde_json::from_str(json);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().max_iterations, 10);
}

// ---------------------------------------------------------------------------
// Clone / Debug tests
// ---------------------------------------------------------------------------

#[test]
fn test_agent_config_clone_is_equal() {
    let cfg = AgentConfig::default();
    let cloned = cfg.clone();
    assert_eq!(cfg.max_iterations, cloned.max_iterations);
    assert_eq!(cfg.step_timeout_secs, cloned.step_timeout_secs);
    assert_eq!(cfg.token_budget, cloned.token_budget);
    assert_eq!(cfg.read_loop_policy, cloned.read_loop_policy);
    assert_eq!(cfg.compression_detail, cloned.compression_detail);
    assert_eq!(cfg.prompt_profile, cloned.prompt_profile);
}

#[test]
fn test_agent_config_debug_format() {
    let cfg = AgentConfig::default();
    let debug_str = format!("{cfg:?}");
    assert!(debug_str.contains("AgentConfig"));
    assert!(debug_str.contains("max_iterations"));
}

#[test]
fn test_read_loop_policy_debug_format() {
    let debug = format!("{:?}", ReadLoopPolicy::Nudge);
    assert_eq!(debug, "Nudge");
    let debug2 = format!("{:?}", ReadLoopPolicy::ForceMutation);
    assert_eq!(debug2, "ForceMutation");
}

// ---------------------------------------------------------------------------
// Edge case / behavioral tests
// ---------------------------------------------------------------------------

#[test]
fn test_agent_config_with_zero_ratios() {
    // Ratios of 0.0 are valid (if unusual). Ensure serde handles them.
    let json = r#"{
            "context_content_ratio": 0.0,
            "context_compression_ratio": 0.0,
            "context_thinking_ratio": 0.0
        }"#;
    let back: AgentConfig = serde_json::from_str(json).unwrap();
    assert_eq!(back.context_content_ratio, 0.0);
    assert_eq!(back.context_compression_ratio, 0.0);
    assert_eq!(back.context_thinking_ratio, 0.0);
}

#[test]
fn test_agent_config_all_bool_fields_toggle() {
    let json = r#"{
            "native_function_calling": true,
            "streaming": false,
            "require_verification_before_completion": false,
            "require_visual_verification": true,
            "disable_turn_artifacts": true
        }"#;
    let back: AgentConfig = serde_json::from_str(json).unwrap();
    assert!(back.native_function_calling);
    assert!(!back.streaming);
    assert!(!back.require_verification_before_completion);
    assert!(back.require_visual_verification);
    assert!(back.disable_turn_artifacts);
}

#[test]
fn test_agent_config_clone_independence() {
    let mut cfg = AgentConfig::default();
    let cloned = cfg.clone();
    cfg.max_iterations = 1;
    cfg.compression_detail = "changed".to_string();
    // Clone should be independent
    assert_eq!(cloned.max_iterations, default_max_iterations());
    assert_eq!(cloned.compression_detail, default_compression_detail());
}
