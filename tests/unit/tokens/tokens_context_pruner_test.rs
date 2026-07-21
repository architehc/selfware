use super::*;

#[test]
fn test_pruning_config_default() {
    let config = PruningConfig::default();
    assert_eq!(config.target_tokens, 100_000);
    assert!(config.keep_system);
}

#[test]
fn test_pruner_needs_pruning() {
    let pruner = ContextPruner::default();
    assert!(!pruner.needs_pruning(50_000));
    assert!(pruner.needs_pruning(150_000));
}

#[test]
fn test_pruner_tokens_to_remove() {
    let pruner = ContextPruner::default();
    assert_eq!(pruner.tokens_to_remove(50_000), 0);
    assert_eq!(pruner.tokens_to_remove(150_000), 50_000);
}

#[test]
fn test_pruner_stats() {
    let pruner = ContextPruner::default();
    let stats = pruner.stats();
    assert_eq!(stats.total_operations, 0);
}

#[test]
fn test_pruning_strategy_enum() {
    assert_eq!(PruningStrategy::KeepRecent, PruningStrategy::KeepRecent);
    assert_ne!(PruningStrategy::KeepRecent, PruningStrategy::KeepEnds);
}
