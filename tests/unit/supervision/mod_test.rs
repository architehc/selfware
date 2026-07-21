use super::*;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_backoff_fixed_duration() {
    let strategy = BackoffStrategy::Fixed { seconds: 5 };
    assert_eq!(strategy.duration(0), Duration::from_secs(5));
    assert_eq!(strategy.duration(1), Duration::from_secs(5));
    assert_eq!(strategy.duration(10), Duration::from_secs(5));
}

#[test]
fn test_backoff_exponential_duration() {
    let strategy = BackoffStrategy::Exponential {
        base_seconds: 1,
        max_seconds: 60,
    };
    assert_eq!(strategy.duration(0), Duration::from_secs(1)); // 1 << 0 = 1
    assert_eq!(strategy.duration(1), Duration::from_secs(2)); // 1 << 1 = 2
    assert_eq!(strategy.duration(2), Duration::from_secs(4)); // 1 << 2 = 4
    assert_eq!(strategy.duration(3), Duration::from_secs(8)); // 1 << 3 = 8
}

#[test]
fn test_backoff_exponential_respects_max() {
    let strategy = BackoffStrategy::Exponential {
        base_seconds: 1,
        max_seconds: 10,
    };
    assert_eq!(strategy.duration(5), Duration::from_secs(10)); // 1 << 5 = 32, capped at 10
    assert_eq!(strategy.duration(20), Duration::from_secs(10));
}

#[test]
fn test_backoff_exponential_overflow_uses_max() {
    let strategy = BackoffStrategy::Exponential {
        base_seconds: 1,
        max_seconds: 100,
    };
    // Shifting by 64+ bits should overflow, falling back to max_seconds
    assert_eq!(strategy.duration(64), Duration::from_secs(100));
}

#[test]
fn test_supervisor_builder_defaults() {
    let supervisor = Supervisor::builder().build();
    assert!(matches!(
        supervisor.strategy,
        SupervisionStrategy::OneForOne
    ));
    assert_eq!(supervisor.restart_policy.max_restarts, 5);
    assert_eq!(supervisor.restart_policy.max_seconds, 60);
    assert!(supervisor.children.is_empty());
}

#[test]
fn test_supervisor_builder_with_strategy() {
    let supervisor = Supervisor::builder()
        .with_strategy(SupervisionStrategy::OneForAll)
        .build();
    assert!(matches!(
        supervisor.strategy,
        SupervisionStrategy::OneForAll
    ));
}

#[test]
fn test_supervisor_builder_with_restart_policy() {
    let policy = RestartPolicy {
        max_restarts: 10,
        max_seconds: 120,
        backoff_strategy: BackoffStrategy::Fixed { seconds: 3 },
    };
    let supervisor = Supervisor::builder().with_restart_policy(policy).build();
    assert_eq!(supervisor.restart_policy.max_restarts, 10);
    assert_eq!(supervisor.restart_policy.max_seconds, 120);
}

/// Test component factory for unit tests
struct TestComponent;
struct TestFactory;

impl ComponentFactory for TestFactory {
    fn create(&self) -> Arc<dyn Send + Sync> {
        Arc::new(TestComponent)
    }

    fn stop(&self, _component: &Arc<dyn Send + Sync>) {
        // Nothing to stop in test
    }
}

#[test]
fn test_supervisor_builder_add_child() {
    let factory: Arc<dyn ComponentFactory> = Arc::new(TestFactory);

    let supervisor = Supervisor::builder()
        .add_child("worker-1", factory.clone())
        .add_child("worker-2", factory)
        .build();

    assert_eq!(supervisor.children.len(), 2);
    assert_eq!(supervisor.children[0].id, "worker-1");
    assert_eq!(supervisor.children[1].id, "worker-2");
    assert_eq!(supervisor.children[0].restart_type, RestartType::Permanent);
    assert!(supervisor.children[0].factory.is_some());
}

#[test]
fn test_child_spec_defaults() {
    let spec = ChildSpec {
        id: "test".into(),
        restart_type: RestartType::Permanent,
        max_restarts: None,
        factory: None,
    };
    assert_eq!(spec.id, "test");
    assert_eq!(spec.restart_type, RestartType::Permanent);
    assert!(spec.max_restarts.is_none());
    assert!(spec.factory.is_none());
}

#[test]
fn test_restart_type_variants() {
    assert_ne!(RestartType::Permanent, RestartType::Transient);
    assert_ne!(RestartType::Transient, RestartType::Temporary);
    assert_eq!(RestartType::Permanent, RestartType::Permanent);
}

#[test]
fn test_exit_reason_variants() {
    assert_eq!(ExitReason::Normal, ExitReason::Normal);
    assert_ne!(ExitReason::Normal, ExitReason::Error);
    assert_ne!(ExitReason::Error, ExitReason::Killed);
    assert_ne!(ExitReason::Killed, ExitReason::Timeout);
}

#[tokio::test]
async fn test_supervisor_start_returns_handle() {
    let supervisor = Supervisor::builder().build();
    let handle = supervisor.start().await;

    assert!(handle.is_ok());
    let handle = handle.unwrap();

    // Verify we can send events through the handle
    let result = handle
        .tx
        .send(ChildEvent::Heartbeat {
            child_id: "test".into(),
        })
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_supervisor_handle_send_crash_event() {
    let supervisor = Supervisor::builder().build();
    let handle = supervisor.start().await.unwrap();

    let result = handle
        .tx
        .send(ChildEvent::Crashed {
            child_id: "worker-1".into(),
            error: "out of memory".into(),
        })
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_supervisor_handle_send_exit_event() {
    let supervisor = Supervisor::builder().build();
    let handle = supervisor.start().await.unwrap();

    let result = handle
        .tx
        .send(ChildEvent::Exited {
            child_id: "worker-1".into(),
            reason: ExitReason::Normal,
        })
        .await;
    assert!(result.is_ok());
}

#[test]
fn test_restart_policy_serialization() {
    let policy = RestartPolicy {
        max_restarts: 3,
        max_seconds: 30,
        backoff_strategy: BackoffStrategy::Fixed { seconds: 2 },
    };
    let json = serde_json::to_string(&policy).unwrap();
    let deserialized: RestartPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.max_restarts, 3);
    assert_eq!(deserialized.max_seconds, 30);
}

#[test]
fn test_supervision_strategy_serialization() {
    let strategies = vec![
        SupervisionStrategy::OneForOne,
        SupervisionStrategy::OneForAll,
        SupervisionStrategy::RestForOne,
    ];
    for strategy in strategies {
        let json = serde_json::to_string(&strategy).unwrap();
        let _: SupervisionStrategy = serde_json::from_str(&json).unwrap();
    }
}
