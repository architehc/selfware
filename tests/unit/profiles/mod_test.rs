use super::*;

#[test]
fn test_profile_manager() {
    let manager = ProfileManager::new();
    assert!(manager.get("architect").is_some());
    assert!(manager.get("batch-16").is_some());
}

#[test]
fn apply_profile_applies_all_mappable_overrides() {
    let manager = ProfileManager::new();
    let mut config = crate::config::Config::default();
    manager.apply_profile(&mut config, "architect").unwrap();

    // Previously only these two were applied.
    assert_eq!(config.max_tokens, 8192);
    assert!((config.temperature - 0.7).abs() < f32::EPSILON);
    // These were silently dropped before the fix.
    assert_eq!(config.agent.max_iterations, 100);
    assert_eq!(config.agent.step_timeout_secs, 900);
    assert!(!config.agent.streaming);
    assert!(config.agent.native_function_calling);
    assert_eq!(config.concurrency.max_streams, 4);
}

#[test]
fn apply_profile_unknown_name_errors() {
    let manager = ProfileManager::new();
    let mut config = crate::config::Config::default();
    assert!(manager
        .apply_profile(&mut config, "no-such-profile")
        .is_err());
}

#[test]
fn test_list_profiles() {
    let manager = ProfileManager::new();
    let profiles = manager.list();
    assert!(!profiles.is_empty());
}

/// Review 2026-09-27: `--profile` max_tokens is applied after the model
/// profile filled the per-turn caps, which then silently overrode it on
/// planning / synthesis turns.
#[test]
fn profile_max_tokens_is_obeyed_over_model_profile_turn_caps() {
    use crate::config::{ConfigSource, TurnWorkload};
    let manager = ProfileManager::new();
    let mut config = crate::config::Config::default();
    config.workloads.get_mut(TurnWorkload::Synthesis).max_tokens = Some(16_384);
    config.sources.set(
        "workloads.synthesis.max_tokens",
        ConfigSource::Profile("glm-*".to_string()),
    );
    // A cap the user wrote in [workloads.planning] stays.
    config.workloads.get_mut(TurnWorkload::Planning).max_tokens = Some(12_288);
    manager.apply_profile(&mut config, "architect").unwrap();
    assert_eq!(config.max_tokens, 8192);
    assert_eq!(
        config.workloads.get(TurnWorkload::Synthesis).max_tokens,
        None
    );
    assert_eq!(
        config.workloads.get(TurnWorkload::Planning).max_tokens,
        Some(12_288)
    );
    let notes = config.workload_overrides.join("\n");
    assert!(notes.contains("synthesis 16384"), "{notes}");
    assert!(notes.contains("planning 12288"), "{notes}");
}

#[test]
fn apply_profile_overwrites_existing_enable_thinking_and_clears_model_profile_workloads() {
    use crate::config::{ConfigSource, TurnWorkload};
    let manager = ProfileManager::new();
    let mut config = crate::config::Config::default();

    // Simulate a model profile (e.g. deepseek-v4 / qwen38) that pre-populated
    // enable_thinking = true in extra_body.chat_template_kwargs and workloads.
    let mut kwargs = serde_json::Map::new();
    kwargs.insert("enable_thinking".to_string(), serde_json::Value::Bool(true));
    kwargs.insert(
        "preserve_thinking".to_string(),
        serde_json::Value::Bool(false),
    );
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::Value::Object(kwargs),
    );
    config.extra_body = Some(extra);
    config.sources.set(
        "extra_body.chat_template_kwargs",
        ConfigSource::Profile("deepseek-v4".to_string()),
    );
    config
        .workloads
        .get_mut(TurnWorkload::Synthesis)
        .enable_thinking = Some(true);
    config.sources.set(
        "workloads.synthesis.enable_thinking",
        ConfigSource::Profile("deepseek-v4".to_string()),
    );
    // User-explicit workload setting must stay.
    config
        .workloads
        .get_mut(TurnWorkload::Planning)
        .enable_thinking = Some(true);

    // `quick` sets enable_thinking = Some(false)
    manager.apply_profile(&mut config, "quick").unwrap();

    let kw = config
        .extra_body
        .as_ref()
        .and_then(|m| m.get("chat_template_kwargs"))
        .and_then(|v| v.as_object())
        .expect("chat_template_kwargs must exist");
    assert_eq!(
        kw.get("enable_thinking"),
        Some(&serde_json::Value::Bool(false)),
        "quick profile must overwrite existing enable_thinking=true to false"
    );
    assert_eq!(
        kw.get("preserve_thinking"),
        Some(&serde_json::Value::Bool(false)),
        "sibling chat_template_kwargs keys must be preserved"
    );
    assert_eq!(
        config
            .workloads
            .get(TurnWorkload::Synthesis)
            .enable_thinking,
        None,
        "model-profile-sourced workload enable_thinking must be cleared"
    );
    assert_eq!(
        config.workloads.get(TurnWorkload::Planning).enable_thinking,
        Some(true),
        "user-explicit workload enable_thinking must be kept"
    );
}

#[test]
fn apply_profile_records_cli_provenance_preserved_across_model_override() {
    use crate::config::ConfigSource;
    let manager = ProfileManager::new();
    let mut config = crate::config::Config::default();

    manager.apply_profile(&mut config, "quick").unwrap();
    for key in [
        "max_tokens",
        "temperature",
        "agent.max_iterations",
        "agent.step_timeout_secs",
        "agent.streaming",
        "agent.native_function_calling",
        "extra_body.chat_template_kwargs",
        "concurrency.max_streams",
    ] {
        assert_eq!(
            config.sources.get(key),
            Some(&ConfigSource::CliArg("--profile quick".to_string())),
            "expected --profile quick provenance on {key}"
        );
    }

    // A subsequent CLI --model override must preserve the explicit --profile fields.
    config.apply_cli_model_override("qwen38-flash-next".to_string());
    assert_eq!(config.max_tokens, 2048);
    assert!((config.temperature - 0.5).abs() < f32::EPSILON);
    assert_eq!(config.concurrency.max_streams, 32);
    let kw = config
        .extra_body
        .as_ref()
        .and_then(|m| m.get("chat_template_kwargs"))
        .and_then(|v| v.as_object())
        .expect("chat_template_kwargs preserved");
    assert_eq!(
        kw.get("enable_thinking"),
        Some(&serde_json::Value::Bool(false))
    );
}
