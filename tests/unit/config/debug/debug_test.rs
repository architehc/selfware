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
