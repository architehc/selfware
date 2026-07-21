use super::*;
use crate::self_healing::{RecoveryAction, RecoveryStrategy, SelfHealingConfig, StateManager};

fn test_config() -> SelfHealingConfig {
    SelfHealingConfig {
        enabled: true,
        max_healing_attempts: 5,
        ..SelfHealingConfig::default()
    }
}

fn test_strategy(actions: Vec<RecoveryAction>) -> RecoveryStrategy {
    RecoveryStrategy {
        name: "test-strategy".to_string(),
        description: "Test strategy".to_string(),
        actions,
        success_probability: 0.9,
        estimated_duration_ms: 100,
    }
}

#[tokio::test]
async fn test_executor_disabled_config() {
    let config = SelfHealingConfig {
        enabled: false,
        ..SelfHealingConfig::default()
    };
    let executor = RecoveryExecutor::new(config);
    let strategy = test_strategy(vec![RecoveryAction::Fallback {
        target: "backup".to_string(),
    }]);

    let result = executor.execute(&strategy).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("disabled"));
}

#[tokio::test]
async fn test_executor_fallback_success() {
    let executor = RecoveryExecutor::new(test_config());
    let strategy = test_strategy(vec![RecoveryAction::Fallback {
        target: "backup".to_string(),
    }]);

    let result = executor.execute(&strategy).await;
    assert!(result.success);
    assert_eq!(result.actions_executed, vec!["fallback"]);
}

#[tokio::test]
async fn test_executor_fallback_empty_target() {
    let executor = RecoveryExecutor::new(test_config());
    let strategy = test_strategy(vec![RecoveryAction::Fallback {
        target: "".to_string(),
    }]);

    let result = executor.execute(&strategy).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("empty"));
}

#[tokio::test]
async fn test_executor_restart_empty_component() {
    let executor = RecoveryExecutor::new(test_config());
    let strategy = test_strategy(vec![RecoveryAction::Restart {
        component: "  ".to_string(),
    }]);

    let result = executor.execute(&strategy).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("empty"));
}

#[tokio::test]
async fn test_executor_restart_with_state() {
    let executor = RecoveryExecutor::new(test_config());
    let state_mgr = StateManager::new(SelfHealingConfig::default());
    // Create a checkpoint so restore can find it
    state_mgr.checkpoint("test", serde_json::json!({"key": "value"}));

    let strategy = test_strategy(vec![RecoveryAction::Restart {
        component: "agent".to_string(),
    }]);

    let result = executor.execute_with_state(&strategy, &state_mgr).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_executor_reset_state() {
    let executor = RecoveryExecutor::new(test_config());
    let state_mgr = StateManager::new(SelfHealingConfig::default());

    let strategy = test_strategy(vec![RecoveryAction::ResetState {
        scope: "all".to_string(),
    }]);

    let result = executor.execute_with_state(&strategy, &state_mgr).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_executor_custom_action() {
    let executor = RecoveryExecutor::new(test_config());
    let strategy = test_strategy(vec![RecoveryAction::Custom {
        name: "compress_context".to_string(),
        params: HashMap::new(),
    }]);

    let result = executor.execute(&strategy).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_executor_custom_action_empty_name() {
    let executor = RecoveryExecutor::new(test_config());
    let strategy = test_strategy(vec![RecoveryAction::Custom {
        name: "".to_string(),
        params: HashMap::new(),
    }]);

    let result = executor.execute(&strategy).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("empty"));
}

#[tokio::test]
async fn test_executor_custom_unknown_action() {
    let executor = RecoveryExecutor::new(test_config());
    let strategy = test_strategy(vec![RecoveryAction::Custom {
        name: "totally_unknown".to_string(),
        params: HashMap::new(),
    }]);

    // Unknown custom actions are treated as no-op signals (success)
    let result = executor.execute(&strategy).await;
    assert!(result.success);
}

#[tokio::test]
async fn test_executor_max_healing_attempts_abort() {
    let config = SelfHealingConfig {
        enabled: true,
        max_healing_attempts: 1,
        ..SelfHealingConfig::default()
    };
    let executor = RecoveryExecutor::new(config);

    // Strategy with 3 actions but max_healing_attempts=1
    let strategy = test_strategy(vec![
        RecoveryAction::Fallback {
            target: "a".to_string(),
        },
        RecoveryAction::Fallback {
            target: "b".to_string(),
        },
        RecoveryAction::Fallback {
            target: "c".to_string(),
        },
    ]);

    let result = executor.execute(&strategy).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("exceeded max"));
}

#[tokio::test]
async fn test_executor_success_rate() {
    let executor = RecoveryExecutor::new(test_config());

    // Execute a successful strategy
    let good_strategy = test_strategy(vec![RecoveryAction::Fallback {
        target: "ok".to_string(),
    }]);
    executor.execute(&good_strategy).await;

    // Execute a failing strategy
    let bad_strategy = test_strategy(vec![RecoveryAction::Fallback {
        target: "".to_string(),
    }]);
    executor.execute(&bad_strategy).await;

    assert!((executor.success_rate() - 0.5).abs() < 0.01);
}

#[tokio::test]
async fn test_executor_history() {
    let executor = RecoveryExecutor::new(test_config());
    assert_eq!(executor.history().len(), 0);

    let strategy = test_strategy(vec![RecoveryAction::Fallback {
        target: "ok".to_string(),
    }]);
    executor.execute(&strategy).await;
    assert_eq!(executor.history().len(), 1);
}

#[tokio::test]
async fn test_execute_for_pattern_and_retry_count() {
    let config = SelfHealingConfig {
        enabled: true,
        max_healing_attempts: 5,
        ..SelfHealingConfig::default()
    };
    let executor = RecoveryExecutor::new(config);
    let state_mgr = StateManager::new(SelfHealingConfig::default());

    let strategy = test_strategy(vec![RecoveryAction::Retry {
        delay_ms: 0, // No actual delay in tests
        max_attempts: 3,
    }]);

    assert_eq!(executor.retry_attempt_count("test-pattern"), 0);

    executor
        .execute_for_pattern(&strategy, &state_mgr, "test-pattern")
        .await;
    assert_eq!(executor.retry_attempt_count("test-pattern"), 1);

    executor
        .execute_for_pattern(&strategy, &state_mgr, "test-pattern")
        .await;
    assert_eq!(executor.retry_attempt_count("test-pattern"), 2);
}

#[tokio::test]
async fn test_reset_retry_state() {
    let executor = RecoveryExecutor::new(test_config());
    let state_mgr = StateManager::new(SelfHealingConfig::default());

    let strategy = test_strategy(vec![RecoveryAction::Retry {
        delay_ms: 0,
        max_attempts: 5,
    }]);

    executor
        .execute_for_pattern(&strategy, &state_mgr, "pattern-a")
        .await;
    assert_eq!(executor.retry_attempt_count("pattern-a"), 1);

    executor.reset_retry_state("pattern-a");
    assert_eq!(executor.retry_attempt_count("pattern-a"), 0);
}

#[test]
fn test_executor_summary() {
    let executor = RecoveryExecutor::new(test_config());
    let summary = executor.summary();
    assert_eq!(summary.executions, 0);
    assert_eq!(summary.successes, 0);
    assert_eq!(summary.failures, 0);
}

#[test]
fn test_executor_default() {
    let executor = RecoveryExecutor::default();
    assert_eq!(executor.history().len(), 0);
    assert_eq!(executor.success_rate(), 0.0);
}

#[test]
fn test_action_name_all_variants() {
    assert_eq!(
        action_name(&RecoveryAction::Retry {
            delay_ms: 0,
            max_attempts: 1
        }),
        "retry"
    );
    assert_eq!(
        action_name(&RecoveryAction::Restart {
            component: "x".to_string()
        }),
        "restart"
    );
    assert_eq!(
        action_name(&RecoveryAction::Fallback {
            target: "x".to_string()
        }),
        "fallback"
    );
    assert_eq!(
        action_name(&RecoveryAction::RestoreCheckpoint {
            checkpoint_id: None
        }),
        "restore"
    );
    assert_eq!(
        action_name(&RecoveryAction::ClearCache {
            scope: "all".to_string()
        }),
        "clear_cache"
    );
    assert_eq!(
        action_name(&RecoveryAction::ResetState {
            scope: "all".to_string()
        }),
        "reset_state"
    );
    assert_eq!(
        action_name(&RecoveryAction::Custom {
            name: "my_action".to_string(),
            params: HashMap::new(),
        }),
        "my_action"
    );
}
