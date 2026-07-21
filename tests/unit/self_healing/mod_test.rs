use super::*;

#[test]
fn test_config_default() {
    let config = SelfHealingConfig::default();
    assert!(config.enabled);
    assert_eq!(config.max_healing_attempts, 3);
}

#[test]
fn test_error_occurrence_new() {
    let error = ErrorOccurrence::new("network", "connection failed", "api_call");
    assert_eq!(error.error_type, "network");
    assert!(!error.recovery_success);
}

#[test]
fn test_error_occurrence_with_location() {
    let error = ErrorOccurrence::new("network", "err", "ctx").with_location("main.rs:42");
    assert_eq!(error.location, Some("main.rs:42".to_string()));
}

#[test]
fn test_error_occurrence_with_recovery() {
    let error = ErrorOccurrence::new("network", "err", "ctx").with_recovery("retry", true);
    assert_eq!(error.recovery_action, Some("retry".to_string()));
    assert!(error.recovery_success);
}

#[test]
fn test_error_learner_record() {
    let learner = ErrorLearner::default();
    learner.record(ErrorOccurrence::new("network", "err", "ctx"));

    let summary = learner.summary();
    assert_eq!(summary.errors_recorded, 1);
}

#[test]
fn test_recovery_strategy_retry() {
    let strategy = RecoveryStrategy::retry();
    assert_eq!(strategy.name, "retry");
    assert!(!strategy.actions.is_empty());
}

#[test]
fn test_recovery_strategy_from_name() {
    let strategy = RecoveryStrategy::from_name("fallback");
    assert_eq!(strategy.name, "fallback");
}

#[test]
fn test_state_checkpoint_new() {
    let checkpoint = StateCheckpoint::new("test", serde_json::json!({"key": "value"}));
    assert!(checkpoint.id.starts_with("ckpt_"));
}

#[test]
fn test_state_manager_checkpoint() {
    let manager = StateManager::default();
    let id = manager.checkpoint("test", serde_json::json!({}));
    assert!(id.starts_with("ckpt_"));

    let summary = manager.summary();
    assert_eq!(summary.checkpoints_created, 1);
}

#[test]
fn test_state_manager_restore() {
    let manager = StateManager::default();
    manager.checkpoint("test", serde_json::json!({"data": 42}));

    let restored = manager.restore(None);
    assert!(restored.is_some());
}

#[test]
fn test_health_predictor_record() {
    let predictor = HealthPredictor::default();
    predictor.record("api", true, Some(100), 0);
    // Should not panic
}

#[test]
fn test_predicted_health_enum() {
    assert_eq!(PredictedHealth::Healthy, PredictedHealth::Healthy);
    assert_ne!(PredictedHealth::Healthy, PredictedHealth::Degrading);
}

#[tokio::test]
async fn test_recovery_executor_execute() {
    let executor = RecoveryExecutor::default();
    // Use zero-delay retry so the test is fast
    let strategy = RecoveryStrategy {
        name: "retry".to_string(),
        description: "test retry".to_string(),
        actions: vec![RecoveryAction::Retry {
            delay_ms: 0,
            max_attempts: 3,
        }],
        success_probability: 0.7,
        estimated_duration_ms: 0,
    };

    let result = executor.execute(&strategy).await;
    assert_eq!(result.strategy, "retry");
    assert!(result.completed_at.is_some());
    assert!(result.success);
}

#[tokio::test]
async fn test_recovery_executor_restore_requires_state_manager() {
    let executor = RecoveryExecutor::default();
    let strategy = RecoveryStrategy::restore();

    let result = executor.execute(&strategy).await;
    assert_eq!(result.strategy, "restore");
    assert!(!result.success);
    assert!(result.error.is_some());
}

#[tokio::test]
async fn test_recovery_executor_restore_with_state_manager() {
    let config = SelfHealingConfig::default();
    let executor = RecoveryExecutor::new(config.clone());
    let state = StateManager::new(config);
    state.checkpoint("before_restore", serde_json::json!({"ok": true}));

    let result = executor
        .execute_with_state(&RecoveryStrategy::restore(), &state)
        .await;
    assert!(result.success);
}

#[tokio::test]
async fn test_recovery_executor_clear_cache_clears_checkpoints() {
    let config = SelfHealingConfig::default();
    let executor = RecoveryExecutor::new(config.clone());
    let state = StateManager::new(config);
    state.checkpoint("to_clear", serde_json::json!({"x": 1}));

    let strategy = RecoveryStrategy {
        name: "clear_cache".to_string(),
        description: "clear".to_string(),
        actions: vec![RecoveryAction::ClearCache {
            scope: "all".to_string(),
        }],
        success_probability: 1.0,
        estimated_duration_ms: 1,
    };

    let result = executor.execute_with_state(&strategy, &state).await;
    assert!(result.success);
    assert!(state.restore(None).is_none());
}

#[tokio::test]
async fn test_recovery_executor_summary() {
    let executor = RecoveryExecutor::default();
    // Use zero-delay retry
    let strategy = RecoveryStrategy {
        name: "retry".to_string(),
        description: "test".to_string(),
        actions: vec![RecoveryAction::Retry {
            delay_ms: 0,
            max_attempts: 3,
        }],
        success_probability: 0.7,
        estimated_duration_ms: 0,
    };
    executor.execute(&strategy).await;

    let summary = executor.summary();
    assert_eq!(summary.executions, 1);
}

#[test]
fn test_self_healing_engine_new() {
    let engine = SelfHealingEngine::default();
    let summary = engine.summary();
    assert_eq!(summary.learner.errors_recorded, 0);
}

#[test]
fn test_self_healing_engine_checkpoint() {
    let engine = SelfHealingEngine::default();
    let id = engine.checkpoint("test", serde_json::json!({}));
    assert!(id.starts_with("ckpt_"));
}

#[test]
fn test_self_healing_engine_restore() {
    let engine = SelfHealingEngine::default();
    engine.checkpoint("test", serde_json::json!({"value": 123}));

    let state = engine.restore(None);
    assert!(state.is_some());
}

#[test]
fn test_self_healing_engine_record_health() {
    let engine = SelfHealingEngine::default();
    engine.record_health("api", true, Some(100));
    // Should not panic
}

#[test]
fn test_recovery_action_variants() {
    let retry = RecoveryAction::Retry {
        delay_ms: 1000,
        max_attempts: 3,
    };
    let restart = RecoveryAction::Restart {
        component: "svc".to_string(),
    };
    let fallback = RecoveryAction::Fallback {
        target: "backup".to_string(),
    };

    // Just test that they can be created
    match retry {
        RecoveryAction::Retry { delay_ms, .. } => assert_eq!(delay_ms, 1000),
        _ => panic!("wrong variant"),
    }
    match restart {
        RecoveryAction::Restart { component } => assert_eq!(component, "svc"),
        _ => panic!("wrong variant"),
    }
    match fallback {
        RecoveryAction::Fallback { target } => assert_eq!(target, "backup"),
        _ => panic!("wrong variant"),
    }
}

// Additional comprehensive tests

#[test]
fn test_config_serialization() {
    let config = SelfHealingConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: SelfHealingConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.enabled, deserialized.enabled);
    assert_eq!(
        config.max_healing_attempts,
        deserialized.max_healing_attempts
    );
}

#[test]
fn test_config_clone() {
    let config = SelfHealingConfig {
        enabled: false,
        max_healing_attempts: 5,
        ..Default::default()
    };
    let cloned = config.clone();
    assert_eq!(config.enabled, cloned.enabled);
    assert_eq!(config.max_healing_attempts, cloned.max_healing_attempts);
}

#[test]
fn test_error_occurrence_serialization() {
    let error = ErrorOccurrence::new("test", "message", "context")
        .with_location("file.rs:10")
        .with_recovery("retry", true);

    let json = serde_json::to_string(&error).unwrap();
    let deserialized: ErrorOccurrence = serde_json::from_str(&json).unwrap();

    assert_eq!(error.error_type, deserialized.error_type);
    assert_eq!(error.location, deserialized.location);
}

#[test]
fn test_error_occurrence_clone() {
    let error = ErrorOccurrence::new("clone_test", "msg", "ctx");
    let cloned = error.clone();
    assert_eq!(error.error_type, cloned.error_type);
}

#[test]
fn test_error_learner_patterns() {
    let learner = ErrorLearner::default();

    // Record multiple similar errors
    for _ in 0..5 {
        learner.record(ErrorOccurrence::new("timeout", "request timed out", "api"));
    }

    let patterns = learner.patterns();
    assert!(!patterns.is_empty());
}

#[test]
fn test_error_learner_recommend_recovery() {
    let learner = ErrorLearner::default();

    // Record errors first
    for _ in 0..5 {
        learner.record(ErrorOccurrence::new("connection", "failed", "network"));
    }

    let strategy = learner.recommend_recovery("connection", "network");
    // May or may not have a recommendation depending on pattern threshold
    if let Some(s) = strategy {
        assert!(!s.name.is_empty());
    }
}

#[test]
fn test_recovery_strategy_restore() {
    let strategy = RecoveryStrategy::restore();
    assert_eq!(strategy.name, "restore");
}

#[test]
fn test_recovery_strategy_clone() {
    let strategy = RecoveryStrategy::retry();
    let cloned = strategy.clone();
    assert_eq!(strategy.name, cloned.name);
}

#[test]
fn test_recovery_action_serialization() {
    let actions = vec![
        RecoveryAction::Retry {
            delay_ms: 1000,
            max_attempts: 3,
        },
        RecoveryAction::Restart {
            component: "api".to_string(),
        },
        RecoveryAction::Fallback {
            target: "backup".to_string(),
        },
        RecoveryAction::RestoreCheckpoint {
            checkpoint_id: None,
        },
        RecoveryAction::ClearCache {
            scope: "all".to_string(),
        },
    ];

    for action in actions {
        let json = serde_json::to_string(&action).unwrap();
        let _: RecoveryAction = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn test_state_checkpoint_with_components() {
    let checkpoint = StateCheckpoint::new("test", serde_json::json!({"data": 1}))
        .with_components(vec!["comp1".to_string(), "comp2".to_string()]);

    assert_eq!(checkpoint.components.len(), 2);
}

#[test]
fn test_state_checkpoint_clone() {
    let checkpoint = StateCheckpoint::new("test", serde_json::json!({}));
    let cloned = checkpoint.clone();
    assert_eq!(checkpoint.id, cloned.id);
}

#[test]
fn test_state_manager_restore_by_id() {
    let manager = StateManager::default();

    let id1 = manager.checkpoint("first", serde_json::json!({"v": 1}));
    let _id2 = manager.checkpoint("second", serde_json::json!({"v": 2}));

    let restored = manager.restore(Some(&id1));
    assert!(restored.is_some());
    assert_eq!(restored.unwrap().description, "first");
}

#[test]
fn test_state_manager_clear() {
    let manager = StateManager::default();

    manager.checkpoint("test", serde_json::json!({}));
    manager.clear();

    assert!(manager.restore(None).is_none());
}

#[test]
fn test_health_predictor_predict() {
    let predictor = HealthPredictor::default();

    // Record healthy data points
    for _ in 0..10 {
        predictor.record("service", true, Some(50), 0);
    }

    let prediction = predictor.predict("service");
    // Prediction might be None if not enough data
    if let Some(pred) = prediction {
        assert!(!pred.component.is_empty());
    }
}

#[test]
fn test_predicted_health_all_variants() {
    let variants = [
        PredictedHealth::Healthy,
        PredictedHealth::Degrading,
        PredictedHealth::AtRisk,
        PredictedHealth::FailureImminent,
    ];

    for variant in variants {
        let _ = format!("{:?}", variant);
        let cloned = variant;
        assert_eq!(variant, cloned);
    }
}

#[test]
fn test_recovery_executor_new() {
    let config = SelfHealingConfig::default();
    let executor = RecoveryExecutor::new(config);
    let summary = executor.summary();
    assert_eq!(summary.executions, 0);
}

#[tokio::test]
async fn test_self_healing_engine_handle_error() {
    let engine = SelfHealingEngine::default();

    // Handle an error — uses zero-delay default retry
    let error = ErrorOccurrence::new("test", "msg", "ctx");
    let result = engine.handle_error(error).await;

    // Should return a recovery execution
    assert!(result.is_some());
}

#[test]
fn test_learner_summary_clone() {
    let summary = LearnerSummary {
        errors_recorded: 10,
        patterns_detected: 5,
        recoveries_suggested: 3,
        active_patterns: 2,
    };
    let cloned = summary.clone();
    assert_eq!(summary.errors_recorded, cloned.errors_recorded);
}

#[test]
fn test_state_summary_clone() {
    let summary = StateSummary {
        checkpoints_created: 5,
        restores_performed: 2,
        total_bytes_saved: 1000,
        active_checkpoints: 3,
    };
    let cloned = summary.clone();
    assert_eq!(summary.checkpoints_created, cloned.checkpoints_created);
}

#[test]
fn test_executor_summary_clone() {
    let summary = ExecutorSummary {
        executions: 10,
        successes: 8,
        failures: 2,
        success_rate: 0.8,
    };
    let cloned = summary.clone();
    assert_eq!(summary.executions, cloned.executions);
}

#[test]
fn test_self_healing_summary_clone() {
    let engine = SelfHealingEngine::default();
    let summary = engine.summary();
    let cloned = summary.clone();
    assert_eq!(
        summary.learner.errors_recorded,
        cloned.learner.errors_recorded
    );
}

#[test]
fn test_error_learner_clear() {
    let learner = ErrorLearner::default();
    learner.record(ErrorOccurrence::new("test", "msg", "ctx"));

    learner.clear();

    let patterns = learner.patterns();
    assert!(patterns.is_empty());
}

#[test]
fn test_recovery_strategy_all_types() {
    let strategies = vec![
        RecoveryStrategy::retry(),
        RecoveryStrategy::restart(),
        RecoveryStrategy::fallback(),
        RecoveryStrategy::restore(),
        RecoveryStrategy::from_name("custom"),
    ];

    for strategy in strategies {
        assert!(!strategy.name.is_empty());
        assert!(strategy.success_probability >= 0.0);
    }
}

#[test]
fn test_health_predictor_clear() {
    let predictor = HealthPredictor::default();
    predictor.record("test", true, Some(100), 0);
    predictor.clear();
    // After clear, prediction should be None
    assert!(predictor.predict("test").is_none());
}

// ================================================================
// Error classification tests
// ================================================================

#[test]
fn test_error_class_classify_network() {
    assert_eq!(
        ErrorClass::classify("connection", "connection refused"),
        ErrorClass::Network
    );
    assert_eq!(
        ErrorClass::classify("network", "dns resolution failed"),
        ErrorClass::Network
    );
    assert_eq!(
        ErrorClass::classify("io", "connection reset by peer"),
        ErrorClass::Network
    );
}

#[test]
fn test_error_class_classify_timeout() {
    assert_eq!(
        ErrorClass::classify("timeout", "request timed out"),
        ErrorClass::Timeout
    );
    assert_eq!(
        ErrorClass::classify("api", "operation timeout after 30s"),
        ErrorClass::Timeout
    );
}

#[test]
fn test_error_class_classify_rate_limit() {
    assert_eq!(
        ErrorClass::classify("api", "rate limit exceeded"),
        ErrorClass::RateLimit
    );
    assert_eq!(
        ErrorClass::classify("http", "429 Too Many Requests"),
        ErrorClass::RateLimit
    );
}

#[test]
fn test_error_class_classify_resource() {
    assert_eq!(
        ErrorClass::classify("system", "out of memory"),
        ErrorClass::ResourceExhaustion
    );
    assert_eq!(
        ErrorClass::classify("io", "no space left on device"),
        ErrorClass::ResourceExhaustion
    );
}

#[test]
fn test_error_class_classify_parse() {
    assert_eq!(
        ErrorClass::classify("json", "invalid json: unexpected token"),
        ErrorClass::ParseError
    );
    assert_eq!(
        ErrorClass::classify("api", "failed to deserialize response"),
        ErrorClass::ParseError
    );
}

#[test]
fn test_error_class_classify_auth() {
    assert_eq!(
        ErrorClass::classify("api", "401 Unauthorized"),
        ErrorClass::AuthError
    );
    assert_eq!(
        ErrorClass::classify("auth", "invalid api key"),
        ErrorClass::AuthError
    );
}

#[test]
fn test_error_class_classify_unknown() {
    assert_eq!(
        ErrorClass::classify("misc", "something weird happened"),
        ErrorClass::Unknown
    );
}

#[test]
fn test_error_class_default_strategies() {
    // Each error class should produce a named strategy
    let classes = [
        ErrorClass::Network,
        ErrorClass::Timeout,
        ErrorClass::RateLimit,
        ErrorClass::ResourceExhaustion,
        ErrorClass::ParseError,
        ErrorClass::AuthError,
        ErrorClass::Unknown,
    ];

    for class in classes {
        let strategy = class.default_strategy();
        assert!(!strategy.name.is_empty());
        assert!(!strategy.actions.is_empty());
    }
}

#[test]
fn test_error_class_escalation_chain() {
    // Network/timeout/rate-limit should escalate to checkpoint restore
    assert!(ErrorClass::Network.escalation_strategy().is_some());
    assert!(ErrorClass::Timeout.escalation_strategy().is_some());
    assert!(ErrorClass::RateLimit.escalation_strategy().is_some());

    // Resource exhaustion escalates to full reset
    assert!(ErrorClass::ResourceExhaustion
        .escalation_strategy()
        .is_some());

    // Parse errors escalate to context compression
    assert!(ErrorClass::ParseError.escalation_strategy().is_some());

    // Auth and unknown have no further escalation
    assert!(ErrorClass::AuthError.escalation_strategy().is_none());
    assert!(ErrorClass::Unknown.escalation_strategy().is_none());
}

#[test]
fn test_error_class_serialization() {
    let class = ErrorClass::RateLimit;
    let json = serde_json::to_string(&class).unwrap();
    let deserialized: ErrorClass = serde_json::from_str(&json).unwrap();
    assert_eq!(class, deserialized);
}

// ================================================================
// classify_anyhow typed downcast tests
// ================================================================

#[test]
fn test_classify_anyhow_io_connection_refused() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
    let err: anyhow::Error = io_err.into();
    assert_eq!(ErrorClass::classify_anyhow(&err), ErrorClass::Network);
}

#[test]
fn test_classify_anyhow_io_timed_out() {
    let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    let err: anyhow::Error = io_err.into();
    assert_eq!(ErrorClass::classify_anyhow(&err), ErrorClass::Timeout);
}

#[test]
fn test_classify_anyhow_io_permission_denied() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let err: anyhow::Error = io_err.into();
    assert_eq!(ErrorClass::classify_anyhow(&err), ErrorClass::AuthError);
}

#[test]
fn test_classify_anyhow_serde_json_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("not valid json").unwrap_err();
    let err: anyhow::Error = json_err.into();
    assert_eq!(ErrorClass::classify_anyhow(&err), ErrorClass::ParseError);
}

#[test]
fn test_classify_anyhow_fallback_to_string() {
    // An anyhow error with no typed inner error falls back to string matching
    let err = anyhow::anyhow!("rate limit exceeded");
    assert_eq!(ErrorClass::classify_anyhow(&err), ErrorClass::RateLimit);
}

#[test]
fn test_classify_anyhow_unknown_fallback() {
    let err = anyhow::anyhow!("something completely unrecognizable");
    assert_eq!(ErrorClass::classify_anyhow(&err), ErrorClass::Unknown);
}

// ================================================================
// Retry with exponential backoff tests
// ================================================================

#[tokio::test]
async fn test_retry_with_zero_delay() {
    let config = SelfHealingConfig::default();
    let executor = RecoveryExecutor::new(config.clone());
    let state = StateManager::new(config);

    let strategy = RecoveryStrategy {
        name: "fast_retry".to_string(),
        description: "test".to_string(),
        actions: vec![RecoveryAction::Retry {
            delay_ms: 0,
            max_attempts: 3,
        }],
        success_probability: 1.0,
        estimated_duration_ms: 0,
    };

    // First call succeeds
    let result = executor
        .execute_for_pattern(&strategy, &state, "test_pattern")
        .await;
    assert!(result.success);
    assert_eq!(executor.retry_attempt_count("test_pattern"), 1);

    // Second call succeeds (attempt 2)
    let result = executor
        .execute_for_pattern(&strategy, &state, "test_pattern")
        .await;
    assert!(result.success);
    assert_eq!(executor.retry_attempt_count("test_pattern"), 2);

    // Third call succeeds (attempt 3)
    let result = executor
        .execute_for_pattern(&strategy, &state, "test_pattern")
        .await;
    assert!(result.success);
    assert_eq!(executor.retry_attempt_count("test_pattern"), 3);

    // Fourth call exhausts max_attempts
    let result = executor
        .execute_for_pattern(&strategy, &state, "test_pattern")
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Max retry attempts"));
}

#[tokio::test]
async fn test_retry_state_reset() {
    let executor = RecoveryExecutor::default();
    let strategy = RecoveryStrategy {
        name: "retry".to_string(),
        description: "test".to_string(),
        actions: vec![RecoveryAction::Retry {
            delay_ms: 0,
            max_attempts: 2,
        }],
        success_probability: 1.0,
        estimated_duration_ms: 0,
    };

    executor
        .execute_for_pattern(&strategy, &StateManager::default(), "reset_test")
        .await;
    assert_eq!(executor.retry_attempt_count("reset_test"), 1);

    // Reset clears the count
    executor.reset_retry_state("reset_test");
    assert_eq!(executor.retry_attempt_count("reset_test"), 0);

    // Can retry again from scratch
    executor
        .execute_for_pattern(&strategy, &StateManager::default(), "reset_test")
        .await;
    assert_eq!(executor.retry_attempt_count("reset_test"), 1);
}

#[tokio::test]
async fn test_retry_zero_max_attempts_fails() {
    let executor = RecoveryExecutor::default();
    let strategy = RecoveryStrategy {
        name: "bad_retry".to_string(),
        description: "test".to_string(),
        actions: vec![RecoveryAction::Retry {
            delay_ms: 0,
            max_attempts: 0,
        }],
        success_probability: 0.0,
        estimated_duration_ms: 0,
    };

    let result = executor.execute(&strategy).await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap()
        .contains("max_attempts must be greater than 0"));
}

// ================================================================
// Escalation tests
// ================================================================

#[tokio::test]
async fn test_engine_handles_network_error_with_classification() {
    let engine = SelfHealingEngine::default();

    let error = ErrorOccurrence::new("network", "connection refused", "api_call");
    let result = engine.handle_error(error).await;

    assert!(result.is_some());
    let execution = result.unwrap();
    // Should use network_retry strategy (classified from error)
    assert!(execution.success);
}

#[tokio::test]
async fn test_engine_escalates_on_retry_exhaustion() {
    let config = SelfHealingConfig {
        enabled: true,
        max_healing_attempts: 3,
        ..Default::default()
    };
    let engine = SelfHealingEngine::new(config);

    // Create a checkpoint so escalation (restore) can succeed
    engine.checkpoint("safe_state", serde_json::json!({"ok": true}));

    // Exhaust retries for the same pattern by sending multiple errors
    for i in 0..5 {
        let error = ErrorOccurrence::new("timeout", "request timed out", "api");
        let result = engine.handle_error(error).await;
        assert!(
            result.is_some(),
            "handle_error should always return Some when enabled (iteration {})",
            i
        );
    }
}

#[tokio::test]
async fn test_engine_reset_retry_after_success() {
    let engine = SelfHealingEngine::default();

    // Record an error to start retry tracking
    let error = ErrorOccurrence::new("network", "connection refused", "api");
    engine.handle_error(error).await;

    // After a successful operation, reset the retry state
    engine.reset_retry("network", "api");

    // The next failure should start fresh
    assert_eq!(engine.executor().retry_attempt_count("network:api"), 0);
}

// ================================================================
// Multi-action strategy tests
// ================================================================

#[tokio::test]
async fn test_resource_exhaustion_strategy_clears_then_retries() {
    let config = SelfHealingConfig::default();
    let executor = RecoveryExecutor::new(config.clone());
    let state = StateManager::new(config);
    state.checkpoint("data", serde_json::json!({"big": "object"}));

    let strategy = ErrorClass::ResourceExhaustion.default_strategy();
    let result = executor.execute_with_state(&strategy, &state).await;

    assert!(result.success);
    assert_eq!(result.actions_executed.len(), 2);
    assert_eq!(result.actions_executed[0], "clear_cache");
    assert_eq!(result.actions_executed[1], "retry");

    // Cache should have been cleared
    assert!(state.restore(None).is_none());
}

#[tokio::test]
async fn test_restart_restores_checkpoint() {
    let config = SelfHealingConfig::default();
    let executor = RecoveryExecutor::new(config.clone());
    let state = StateManager::new(config);
    state.checkpoint("before_restart", serde_json::json!({"step": 5}));

    let strategy = RecoveryStrategy::restart();
    let result = executor.execute_with_state(&strategy, &state).await;

    assert!(result.success);
    assert!(result.actions_executed.contains(&"restart".to_string()));
}

#[tokio::test]
async fn test_custom_action_compress_context() {
    let executor = RecoveryExecutor::default();
    let strategy = RecoveryStrategy {
        name: "compress".to_string(),
        description: "test".to_string(),
        actions: vec![RecoveryAction::Custom {
            name: "compress_context".to_string(),
            params: HashMap::new(),
        }],
        success_probability: 1.0,
        estimated_duration_ms: 0,
    };

    let result = executor.execute(&strategy).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_custom_action_switch_parsing_mode() {
    let executor = RecoveryExecutor::default();
    let mut params = HashMap::new();
    params.insert("mode".to_string(), "xml".to_string());

    let strategy = RecoveryStrategy {
        name: "switch".to_string(),
        description: "test".to_string(),
        actions: vec![RecoveryAction::Custom {
            name: "switch_parsing_mode".to_string(),
            params,
        }],
        success_probability: 1.0,
        estimated_duration_ms: 0,
    };

    let result = executor.execute(&strategy).await;
    assert!(result.success);
}

#[test]
fn test_uuid_v4_format() {
    let id = uuid_v4();
    // uuid v4 format: 8-4-4-4-12 hex chars with dashes
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
}

// ---- Persistence tests ----

#[test]
fn test_health_predictor_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("predictor.json");

    let predictor = HealthPredictor::new();
    for i in 0..10 {
        predictor.record("svc_a", i % 3 != 0, Some(100 + i * 10), 0);
    }
    predictor.save_history(&path).unwrap();

    let predictor2 = HealthPredictor::new();
    predictor2.load_history(&path).unwrap();

    let h1 = predictor.history.read();
    let h2 = predictor2.history.read();
    assert_eq!(
        h1.get("svc_a").map(|v| v.len()),
        h2.get("svc_a").map(|v| v.len())
    );
}

#[test]
fn test_error_learner_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("learner.json");

    let config = SelfHealingConfig {
        pattern_threshold: 2,
        ..Default::default()
    };
    let learner = ErrorLearner::new(config);
    // Record enough errors to create a pattern
    for _ in 0..3 {
        learner.record(ErrorOccurrence::new("net", "timeout", "api"));
    }
    learner.record_recovery("net:api", "retry", true);

    learner.save_patterns(&path).unwrap();

    let learner2 = ErrorLearner::default();
    learner2.load_patterns(&path).unwrap();

    let p1 = learner.patterns.read();
    let p2 = learner2.patterns.read();
    assert_eq!(p1.len(), p2.len());
}

#[test]
fn test_engine_save_load_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    let engine = SelfHealingEngine::default();
    for i in 0..10 {
        engine.record_health("comp_a", i % 2 == 0, Some(50));
    }

    engine.save_state(&state_dir).unwrap();
    assert!(state_dir.join("health_predictor.json").exists());
    assert!(state_dir.join("error_learner.json").exists());

    let engine2 = SelfHealingEngine::default();
    engine2.load_state(&state_dir).unwrap();
}

#[test]
fn test_engine_load_state_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let engine = SelfHealingEngine::default();
    // Should succeed even with no files present
    engine.load_state(dir.path()).unwrap();
}
