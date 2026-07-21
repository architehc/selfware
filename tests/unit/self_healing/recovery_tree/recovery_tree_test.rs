use super::*;
use crate::config::Config;

#[test]
fn choose_local_endpoint_prefers_override() {
    // A non-empty override wins over the Ollama default (llama.cpp/:8080 etc.).
    assert_eq!(
        LocalEndpointFallback::choose_local_endpoint(Some("http://localhost:8080/v1")),
        "http://localhost:8080/v1"
    );
    // Blank / whitespace-only overrides are ignored.
    assert_eq!(
        LocalEndpointFallback::choose_local_endpoint(Some("   ")),
        OLLAMA_LOCAL_ENDPOINT
    );
    // No override -> Ollama default.
    assert_eq!(
        LocalEndpointFallback::choose_local_endpoint(None),
        OLLAMA_LOCAL_ENDPOINT
    );
}

// ------------------------------------------------------------------
// classify()
// ------------------------------------------------------------------

#[test]
fn test_classify_llm_unreachable() {
    assert_eq!(classify("Connection refused"), FailureKind::LlmUnreachable);
    assert_eq!(
        classify("dns resolution failed"),
        FailureKind::LlmUnreachable
    );
    assert_eq!(classify("host unreachable"), FailureKind::LlmUnreachable);
    assert_eq!(classify("tcp reset"), FailureKind::LlmUnreachable);
    assert_eq!(
        classify("timed out connecting to upstream"),
        FailureKind::LlmUnreachable
    );
}

#[test]
fn test_classify_auth_error() {
    assert_eq!(classify("401 Unauthorized"), FailureKind::AuthError);
    assert_eq!(classify("invalid api key"), FailureKind::AuthError);
    assert_eq!(classify("authentication failed"), FailureKind::AuthError);
    assert_eq!(classify("403 Forbidden"), FailureKind::AuthError);
}

#[test]
fn test_classify_rate_limited() {
    assert_eq!(classify("429 Too Many Requests"), FailureKind::RateLimited);
    assert_eq!(classify("rate limit exceeded"), FailureKind::RateLimited);
}

#[test]
fn test_classify_tool_timeout() {
    assert_eq!(
        classify("tool execution timed out"),
        FailureKind::ToolTimeout
    );
    assert_eq!(classify("operation timeout"), FailureKind::ToolTimeout);
}

#[test]
fn test_classify_context_overflow() {
    assert_eq!(
        classify("context length exceeded"),
        FailureKind::ContextOverflow
    );
    assert_eq!(
        classify("token limit reached"),
        FailureKind::ContextOverflow
    );
    assert_eq!(
        classify("maximum context window exceeded"),
        FailureKind::ContextOverflow
    );
    assert_eq!(classify("too many tokens"), FailureKind::ContextOverflow);
}

#[test]
fn test_classify_config_invalid() {
    assert_eq!(
        classify("invalid configuration: missing endpoint"),
        FailureKind::ConfigInvalid
    );
    assert_eq!(
        classify("missing field 'api_key'"),
        FailureKind::ConfigInvalid
    );
    assert_eq!(classify("config parse error"), FailureKind::ConfigInvalid);
}

#[test]
fn test_classify_disk_full() {
    assert_eq!(classify("no space left on device"), FailureKind::DiskFull);
    assert_eq!(classify("disk full"), FailureKind::DiskFull);
    assert_eq!(classify("ENOSPC"), FailureKind::DiskFull);
}

#[test]
fn test_classify_unknown() {
    assert_eq!(
        classify("something completely unexpected"),
        FailureKind::Unknown
    );
    assert_eq!(classify("wibble"), FailureKind::Unknown);
}

#[test]
fn test_classify_case_insensitive() {
    assert_eq!(classify("CONNECTION REFUSED"), FailureKind::LlmUnreachable);
    assert_eq!(classify("Unauthorized"), FailureKind::AuthError);
    assert_eq!(classify("RATE LIMIT"), FailureKind::RateLimited);
}

#[test]
fn test_classify_connect_phrase_wins_over_timeout() {
    // "timed out connecting" must classify as LlmUnreachable, not ToolTimeout
    assert_eq!(
        classify("Error: timed out connecting to https://api"),
        FailureKind::LlmUnreachable
    );
}

// ------------------------------------------------------------------
// FailureSignal
// ------------------------------------------------------------------

#[test]
fn test_failure_signal_from_message() {
    let sig = FailureSignal::from_message("connection refused");
    assert_eq!(sig.kind, FailureKind::LlmUnreachable);
    assert_eq!(sig.message, "connection refused");
}

// ------------------------------------------------------------------
// LocalEndpointFallback
// ------------------------------------------------------------------

fn make_config(endpoint: &str) -> Config {
    let mut config = Config::default();
    config.endpoint = endpoint.to_string();
    config
}

#[test]
fn test_local_endpoint_fallback_remote_config_resolves() {
    let config = make_config("https://openrouter.ai/api/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    // Opted-in resolver: the ONLY way a fallback may be emitted.
    let resolver = LocalEndpointFallback::opt_in();
    let outcome = resolver.resolve(&ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Fallback {
            target,
        })) => {
            assert!(
                target.contains("localhost") || target.contains("127.0.0.1"),
                "expected a local target, got {}",
                target
            );
        }
        other => panic!("expected Resolved(Action(Fallback)), got {:?}", other),
    }
}

#[test]
fn test_local_endpoint_fallback_not_opted_in_escalates() {
    // The self-healing P2 fix: without an explicit opt-in the resolver
    // must NOT reroute the agent to localhost on a bare "unreachable".
    let config = make_config("https://openrouter.ai/api/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    let resolver = LocalEndpointFallback { enabled: false };
    match resolver.resolve(&ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate without opt-in, got {:?}", other),
    }
}

#[test]
fn test_local_endpoint_fallback_env_opted_in_table() {
    for v in ["1", "true", "TRUE", "yes", "on", " On ", "YES"] {
        assert!(
            LocalEndpointFallback::env_opted_in(Some(v)),
            "{:?} should opt in",
            v
        );
    }
    for v in ["0", "false", "no", "off", "", "2", "enabled"] {
        assert!(
            !LocalEndpointFallback::env_opted_in(Some(v)),
            "{:?} should NOT opt in",
            v
        );
    }
    assert!(!LocalEndpointFallback::env_opted_in(None));
}

#[test]
fn test_local_endpoint_fallback_from_env_respects_opt_in() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_ENDPOINT_FALLBACK"]);
    _env.set("SELFWARE_ENDPOINT_FALLBACK", "1");
    let resolver = LocalEndpointFallback::from_env();
    let config = make_config("https://openrouter.ai/api/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    match resolver.resolve(&ctx) {
        ResolutionOutcome::Resolved(_) => {}
        other => panic!("expected Resolved with env opt-in, got {:?}", other),
    }
}

#[test]
fn test_local_endpoint_fallback_local_config_escalates() {
    let config = make_config("http://localhost:11434/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    let resolver = LocalEndpointFallback::opt_in();
    let outcome = resolver.resolve(&ctx);
    match outcome {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate, got {:?}", other),
    }
}

#[test]
fn test_local_endpoint_fallback_non_llm_escalates() {
    let config = make_config("https://openrouter.ai/api/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "rate limit exceeded",
    };
    let resolver = LocalEndpointFallback::opt_in();
    // Non-LlmUnreachable should escalate
    match resolver.resolve(&ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate, got {:?}", other),
    }
}

#[test]
fn test_local_endpoint_fallback_offline_capable() {
    let resolver = LocalEndpointFallback::opt_in();
    assert!(resolver.offline_capable());
    assert_eq!(resolver.name(), "LocalEndpointFallback");
}

// ------------------------------------------------------------------
// RecoveryTree
// ------------------------------------------------------------------

#[test]
fn test_recovery_tree_with_defaults_escalates_llm_unreachable_without_opt_in() {
    // Default tree must NOT reroute endpoints silently (self-healing P2):
    // with SELFWARE_ENDPOINT_FALLBACK unset, LlmUnreachable escalates.
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_ENDPOINT_FALLBACK"]);
    std::env::remove_var("SELFWARE_ENDPOINT_FALLBACK");
    let tree = RecoveryTree::with_defaults();
    let config = make_config("https://openrouter.ai/api/v1");
    let signal = FailureSignal {
        kind: FailureKind::LlmUnreachable,
        message: "connection refused".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    match tree.resolve(&signal, &ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate without opt-in, got {:?}", other),
    }
}

#[test]
fn test_recovery_tree_with_opted_in_fallback_resolves_llm_unreachable() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_ENDPOINT_FALLBACK"]);
    _env.set("SELFWARE_ENDPOINT_FALLBACK", "1");
    let tree = RecoveryTree::with_defaults();
    let config = make_config("https://openrouter.ai/api/v1");
    let signal = FailureSignal {
        kind: FailureKind::LlmUnreachable,
        message: "connection refused".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    let outcome = tree.resolve(&signal, &ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Fallback {
            target,
        })) => {
            assert!(
                target.contains("localhost") || target.contains("127.0.0.1"),
                "expected a local target, got {}",
                target
            );
        }
        other => panic!("expected Resolved(Action(Fallback)), got {:?}", other),
    }
}

#[test]
fn test_recovery_tree_no_resolvers_returns_unresolvable() {
    let tree = RecoveryTree::new(); // empty
    let config = make_config("https://example.com");
    let signal = FailureSignal {
        kind: FailureKind::Deadlock,
        message: "deadlock".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    match tree.resolve(&signal, &ctx) {
        ResolutionOutcome::Unresolvable => {}
        other => panic!("expected Unresolvable, got {:?}", other),
    }
}

#[test]
fn test_recovery_tree_all_escalate_returns_escalate() {
    // LocalEndpointFallback on a local config escalates.
    let tree = RecoveryTree::with_defaults();
    let config = make_config("http://localhost:11434/v1");
    let signal = FailureSignal {
        kind: FailureKind::LlmUnreachable,
        message: "connection refused".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    match tree.resolve(&signal, &ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate, got {:?}", other),
    }
}

#[test]
fn test_recovery_tree_custom_resolver_registered() {
    struct AlwaysRetry;
    impl Resolution for AlwaysRetry {
        fn name(&self) -> &'static str {
            "AlwaysRetry"
        }
        fn offline_capable(&self) -> bool {
            true
        }
        fn resolve(&self, _ctx: &RecoveryContext) -> ResolutionOutcome {
            ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
                delay_ms: 500,
                max_attempts: 1,
            }))
        }
    }

    let mut tree = RecoveryTree::new();
    tree.register(FailureKind::Unknown, Box::new(AlwaysRetry));

    let config = make_config("https://example.com");
    let signal = FailureSignal {
        kind: FailureKind::Unknown,
        message: "weird error".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    let outcome = tree.resolve(&signal, &ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
            delay_ms,
            ..
        })) => {
            assert_eq!(delay_ms, 500);
        }
        other => panic!("expected Resolved(Action(Retry)), got {:?}", other),
    }
}

#[test]
fn test_recovery_tree_default_impl() {
    // Default::default() should equal with_defaults(): without the env
    // opt-in, LlmUnreachable escalates (no silent endpoint reroute).
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_ENDPOINT_FALLBACK"]);
    std::env::remove_var("SELFWARE_ENDPOINT_FALLBACK");
    let tree = RecoveryTree::default();
    let config = make_config("https://openrouter.ai/api/v1");
    let signal = FailureSignal {
        kind: FailureKind::LlmUnreachable,
        message: "connection refused".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    match tree.resolve(&signal, &ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate without opt-in, got {:?}", other),
    }
}

// ------------------------------------------------------------------
// RateLimitBackoff
// ------------------------------------------------------------------

#[test]
fn test_rate_limit_backoff_resolves_rate_limited() {
    let resolver = RateLimitBackoff::new(2000, 3);
    let config = make_config("https://api.example.com/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "429 Too Many Requests",
    };
    let outcome = resolver.resolve(&ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
            delay_ms,
            max_attempts,
        })) => {
            assert_eq!(delay_ms, 2000);
            assert_eq!(max_attempts, 3);
        }
        other => panic!("expected Resolved(Action(Retry)), got {:?}", other),
    }
}

#[test]
fn test_rate_limit_backoff_non_rate_limited_escalates() {
    let resolver = RateLimitBackoff::new(2000, 3);
    let config = make_config("https://api.example.com/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    match resolver.resolve(&ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate, got {:?}", other),
    }
}

#[test]
fn test_rate_limit_backoff_offline_capable() {
    let resolver = RateLimitBackoff::new(2000, 3);
    assert!(resolver.offline_capable());
    assert_eq!(resolver.name(), "RateLimitBackoff");
}

#[test]
fn test_recovery_tree_with_defaults_resolves_rate_limited() {
    let tree = RecoveryTree::with_defaults();
    let config = make_config("https://api.example.com/v1");
    let signal = FailureSignal {
        kind: FailureKind::RateLimited,
        message: "429 Too Many Requests".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    let outcome = tree.resolve(&signal, &ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
            delay_ms,
            max_attempts,
        })) => {
            assert_eq!(delay_ms, 2000);
            assert_eq!(max_attempts, 3);
        }
        other => panic!("expected Resolved(Action(Retry)), got {:?}", other),
    }
}

// ------------------------------------------------------------------
// DeadlockDetector
// ------------------------------------------------------------------

#[test]
fn test_deadlock_detector_triggers_after_threshold() {
    let mut detector = DeadlockDetector::new(3);

    // First two no-progress waits → None
    assert_eq!(detector.observe(true, false), None);
    assert_eq!(detector.observe(true, false), None);
    assert_eq!(detector.current_count(), 2);

    // Third → Deadlock, counter resets
    let result = detector.observe(true, false);
    assert_eq!(result, Some(FailureKind::Deadlock));
    assert_eq!(detector.current_count(), 0);
}

#[test]
fn test_deadlock_detector_progress_resets() {
    let mut detector = DeadlockDetector::new(3);

    detector.observe(true, false);
    detector.observe(true, false);
    assert_eq!(detector.current_count(), 2);

    // Progress resets
    assert_eq!(detector.observe(true, true), None);
    assert_eq!(detector.current_count(), 0);

    // Need 3 more to trigger
    assert_eq!(detector.observe(true, false), None);
    assert_eq!(detector.observe(true, false), None);
    assert_eq!(detector.observe(true, false), Some(FailureKind::Deadlock));
}

#[test]
fn test_deadlock_detector_not_waiting_does_not_increment() {
    let mut detector = DeadlockDetector::new(2);
    // Not waiting, no progress → counter stays 0
    assert_eq!(detector.observe(false, false), None);
    assert_eq!(detector.current_count(), 0);
    assert_eq!(detector.observe(false, false), None);
    assert_eq!(detector.current_count(), 0);
}

#[test]
fn test_deadlock_detector_threshold_one() {
    let mut detector = DeadlockDetector::new(1);
    assert_eq!(detector.observe(true, false), Some(FailureKind::Deadlock));
    assert_eq!(detector.current_count(), 0);
}

// ------------------------------------------------------------------
// ContextOverflowCompress
// ------------------------------------------------------------------

#[test]
fn test_context_overflow_compress_resolves_context_overflow() {
    let resolver = ContextOverflowCompress {
        target_tokens: 8000,
    };
    let config = make_config("https://api.example.com/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "context length exceeded",
    };
    let outcome = resolver.resolve(&ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::CompressContext { target_tokens }) => {
            assert_eq!(target_tokens, 8000);
        }
        other => panic!("expected Resolved(CompressContext), got {:?}", other),
    }
}

#[test]
fn test_context_overflow_compress_non_overflow_escalates() {
    let resolver = ContextOverflowCompress {
        target_tokens: 8000,
    };
    let config = make_config("https://api.example.com/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    match resolver.resolve(&ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate, got {:?}", other),
    }
}

#[test]
fn test_context_overflow_compress_offline_capable() {
    let resolver = ContextOverflowCompress {
        target_tokens: 8000,
    };
    assert!(resolver.offline_capable());
    assert_eq!(resolver.name(), "ContextOverflowCompress");
}

#[test]
fn test_recovery_tree_with_defaults_resolves_context_overflow() {
    let tree = RecoveryTree::with_defaults();
    let config = make_config("https://api.example.com/v1");
    let signal = FailureSignal {
        kind: FailureKind::ContextOverflow,
        message: "context length exceeded".to_string(),
    };
    let ctx = RecoveryContext {
        config: &config,
        message: &signal.message,
    };
    let outcome = tree.resolve(&signal, &ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::CompressContext { target_tokens }) => {
            assert_eq!(target_tokens, 8_000);
        }
        other => panic!("expected Resolved(CompressContext), got {:?}", other),
    }
}

// ------------------------------------------------------------------
// CredentialReload
// ------------------------------------------------------------------

#[test]
fn test_credential_reload_offline_capable() {
    let resolver = CredentialReload;
    assert!(resolver.offline_capable());
    assert_eq!(resolver.name(), "CredentialReload");
}

#[test]
fn test_credential_reload_non_auth_escalates() {
    let resolver = CredentialReload;
    let config = make_config("https://api.example.com/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "connection refused",
    };
    match resolver.resolve(&ctx) {
        ResolutionOutcome::Escalate => {}
        other => panic!("expected Escalate, got {:?}", other),
    }
}

/// Test the credential-availability helper deterministically without
/// relying on global env state.  We check that when the env var is set
/// the helper returns Some, and when unset it falls back to keyring
/// (which may or may not have a key — either way it must not panic).
#[test]
fn test_credential_reload_find_available_key_with_env() {
    // Save and restore the env var to avoid cross-test flakiness.
    let saved = std::env::var("SELFWARE_API_KEY").ok();
    // Set a unique value that won't collide with keyring entries.
    std::env::set_var("SELFWARE_API_KEY", "test-credential-reload-finder-key-xyz");
    let key = CredentialReload::find_available_key("https://example.test/v1");
    assert!(
        key.is_some(),
        "find_available_key should return Some when SELFWARE_API_KEY is set"
    );
    assert_eq!(key.unwrap(), "test-credential-reload-finder-key-xyz");
    // Restore.
    match saved {
        Some(v) => std::env::set_var("SELFWARE_API_KEY", v),
        None => std::env::remove_var("SELFWARE_API_KEY"),
    }
}

/// When no env var is set, the helper falls through to the keyring.
/// It should return None (or Some if a real keyring entry exists) but
/// never panic.  We just assert it completes.
#[test]
fn test_credential_reload_find_available_key_without_env() {
    let saved = std::env::var("SELFWARE_API_KEY").ok();
    std::env::remove_var("SELFWARE_API_KEY");
    // This call may hit the OS keyring — that's fine, it must not panic.
    let _ = CredentialReload::find_available_key("https://example.test/v1");
    if let Some(v) = saved {
        std::env::set_var("SELFWARE_API_KEY", v)
    }
}

/// When a key source is present, the resolver produces ReloadCredentials.
#[test]
fn test_credential_reload_resolves_when_key_present() {
    let saved = std::env::var("SELFWARE_API_KEY").ok();
    std::env::set_var(
        "SELFWARE_API_KEY",
        "test-credential-reload-resolves-key-abc",
    );
    let resolver = CredentialReload;
    let config = make_config("https://api.example.com/v1");
    let ctx = RecoveryContext {
        config: &config,
        message: "401 Unauthorized",
    };
    let outcome = resolver.resolve(&ctx);
    match outcome {
        ResolutionOutcome::Resolved(RecoveryDirective::ReloadCredentials) => {}
        other => panic!("expected Resolved(ReloadCredentials), got {:?}", other),
    }
    match saved {
        Some(v) => std::env::set_var("SELFWARE_API_KEY", v),
        None => std::env::remove_var("SELFWARE_API_KEY"),
    }
}
