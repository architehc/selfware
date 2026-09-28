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
