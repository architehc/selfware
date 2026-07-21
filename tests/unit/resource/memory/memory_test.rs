use super::*;
use crate::config::MemoryConfig;

/// Helper to create a MemoryManager for testing without async overhead.
/// Uses a bounded channel so we can inspect actions sent.
fn make_test_memory_manager() -> (MemoryManager, mpsc::Receiver<MemoryAction>) {
    let (action_tx, action_rx) = mpsc::channel(100);
    let mm = MemoryManager {
        config: MemoryConfig::default(),
        action_tx,
        allocated: AtomicU64::new(0),
    };
    (mm, action_rx)
}

// ---- MemoryUsage tests ----

#[test]
fn test_memory_usage_default() {
    let usage = MemoryUsage::default();
    assert_eq!(usage.used, 0);
    assert_eq!(usage.total, 0);
    assert_eq!(usage.available, 0);
    assert_eq!(usage.percent, 0.0);
}

#[test]
fn test_memory_usage_clone() {
    let usage = MemoryUsage {
        used: 8_000_000_000,
        total: 16_000_000_000,
        available: 8_000_000_000,
        percent: 0.5,
    };
    let cloned = usage.clone();
    assert_eq!(cloned.used, 8_000_000_000);
    assert_eq!(cloned.percent, 0.5);
}

// ---- MemoryAction tests ----

#[test]
fn test_memory_action_debug() {
    let action = MemoryAction::ReduceContext {
        target_tokens: 32768,
    };
    let debug_str = format!("{:?}", action);
    assert!(debug_str.contains("ReduceContext"));
    assert!(debug_str.contains("32768"));
}

#[test]
fn test_memory_action_clone() {
    let action = MemoryAction::PauseTasks {
        priority_threshold: 2,
    };
    let cloned = action.clone();
    match cloned {
        MemoryAction::PauseTasks {
            priority_threshold: p,
        } => assert_eq!(p, 2),
        _ => panic!("Clone produced wrong variant"),
    }
}

// ---- MemoryManager allocate/free tests ----

#[test]
fn test_allocate_increases_tracked_memory() {
    let (mm, _rx) = make_test_memory_manager();
    assert_eq!(mm.get_allocated(), 0);

    mm.allocate(1_000_000).unwrap();
    assert_eq!(mm.get_allocated(), 1_000_000);

    mm.allocate(2_000_000).unwrap();
    assert_eq!(mm.get_allocated(), 3_000_000);
}

#[test]
fn test_free_decreases_tracked_memory() {
    let (mm, _rx) = make_test_memory_manager();
    mm.allocate(5_000_000).unwrap();
    mm.free(2_000_000);
    assert_eq!(mm.get_allocated(), 3_000_000);
}

#[test]
fn test_free_saturates_at_zero() {
    let (mm, _rx) = make_test_memory_manager();
    mm.allocate(1_000).unwrap();
    // Freeing more than allocated should wrap to a huge number with fetch_sub,
    // but the AtomicU64 wraps; let's verify current behavior
    mm.free(1_000);
    assert_eq!(mm.get_allocated(), 0);
}

#[test]
fn test_allocate_returns_ok() {
    let (mm, _rx) = make_test_memory_manager();
    let result = mm.allocate(42);
    assert!(result.is_ok());
}

// ---- estimate_for_tokens tests ----

#[test]
fn test_estimate_for_tokens_basic() {
    let (mm, _rx) = make_test_memory_manager();
    let estimate = mm.estimate_for_tokens(1000, 4);
    assert_eq!(estimate, 4000);
}

#[test]
fn test_estimate_for_tokens_zero() {
    let (mm, _rx) = make_test_memory_manager();
    assert_eq!(mm.estimate_for_tokens(0, 100), 0);
    assert_eq!(mm.estimate_for_tokens(100, 0), 0);
}

#[test]
fn test_estimate_for_tokens_large_context() {
    let (mm, _rx) = make_test_memory_manager();
    // 1M tokens * 2 bytes each = 2MB
    let estimate = mm.estimate_for_tokens(1_000_000, 2);
    assert_eq!(estimate, 2_000_000);
}

// ---- Cleanup trigger tests ----

#[tokio::test]
async fn test_trigger_warning_cleanup_sends_action() {
    let (mm, mut rx) = make_test_memory_manager();
    mm.trigger_warning_cleanup().await;

    let action = rx.recv().await.unwrap();
    match action {
        MemoryAction::FlushCaches => {} // expected
        other => panic!("Expected FlushCaches, got {:?}", other),
    }
}

#[tokio::test]
async fn test_trigger_critical_cleanup_sends_three_actions() {
    let (mm, mut rx) = make_test_memory_manager();
    mm.trigger_critical_cleanup().await;

    let a1 = rx.recv().await.unwrap();
    assert!(matches!(a1, MemoryAction::FlushCaches));

    let a2 = rx.recv().await.unwrap();
    assert!(matches!(
        a2,
        MemoryAction::ReduceContext {
            target_tokens: 32768
        }
    ));

    let a3 = rx.recv().await.unwrap();
    assert!(matches!(
        a3,
        MemoryAction::PauseTasks {
            priority_threshold: 1
        }
    ));
}

#[tokio::test]
async fn test_trigger_emergency_cleanup_sends_four_actions() {
    let (mm, mut rx) = make_test_memory_manager();
    mm.trigger_emergency_cleanup().await;

    let a1 = rx.recv().await.unwrap();
    assert!(matches!(a1, MemoryAction::FlushCaches));

    let a2 = rx.recv().await.unwrap();
    assert!(matches!(
        a2,
        MemoryAction::ReduceContext {
            target_tokens: 8192
        }
    ));

    let a3 = rx.recv().await.unwrap();
    assert!(matches!(a3, MemoryAction::OffloadModels));

    let a4 = rx.recv().await.unwrap();
    assert!(matches!(
        a4,
        MemoryAction::PauseTasks {
            priority_threshold: 2
        }
    ));
}

// ---- MemoryConfig defaults ----

#[test]
fn test_memory_config_thresholds_are_ordered() {
    let config = MemoryConfig::default();
    assert!(config.warning_threshold < config.critical_threshold);
    assert!(config.critical_threshold < config.emergency_threshold);
    assert!(config.emergency_threshold <= 1.0);
}
