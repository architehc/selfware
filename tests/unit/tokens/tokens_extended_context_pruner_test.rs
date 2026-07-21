use super::*;
use crate::api::types::Message;

#[test]
fn test_pruner_default() {
    let pruner = ContextPruner::default();
    assert!(!pruner.needs_pruning(50_000));
    assert_eq!(pruner.stats().total_operations, 0);
}

#[test]
fn test_pruner_reset_stats() {
    let pruner = ContextPruner::default();
    // Manually can't trigger prune without enough messages to exceed tokens,
    // but we can test reset_stats independently
    pruner.reset_stats();
    let stats = pruner.stats();
    assert_eq!(stats.total_operations, 0);
    assert_eq!(stats.tokens_removed, 0);
    assert_eq!(stats.messages_removed, 0);
    assert!((stats.cost_saved - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_prune_no_pruning_needed() {
    let config = PruningConfig {
        target_tokens: 1_000_000, // very high target
        strategy: PruningStrategy::KeepRecent,
        min_messages: 2,
        keep_system: true,
        keep_last_n: 2,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System prompt"),
        Message::user("Hello"),
        Message::assistant("Hi there"),
    ];
    let result = pruner.prune(&messages);
    assert_eq!(result.len(), 3);
}

#[test]
fn test_prune_keep_recent_strategy() {
    let config = PruningConfig {
        target_tokens: 1, // force pruning
        strategy: PruningStrategy::KeepRecent,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 1,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("You are a helpful assistant"),
        Message::user("First question"),
        Message::assistant("First answer"),
        Message::user("Second question"),
        Message::assistant("Second answer"),
    ];
    let result = pruner.prune(&messages);
    // Should keep system message + last 1 message at minimum
    assert!(result.len() >= 2);
    // System message should be first
    assert_eq!(result[0].role, "system");
    // stats should be updated
    let stats = pruner.stats();
    assert_eq!(stats.total_operations, 1);
}

#[test]
fn test_prune_keep_ends_strategy() {
    let config = PruningConfig {
        target_tokens: 1, // force pruning
        strategy: PruningStrategy::KeepEnds,
        min_messages: 2,
        keep_system: true,
        keep_last_n: 2,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System prompt"),
        Message::user("First question"),
        Message::assistant("First answer"),
        Message::user("Middle question"),
        Message::assistant("Middle answer"),
        Message::user("Last question"),
        Message::assistant("Last answer"),
    ];
    let result = pruner.prune(&messages);
    // KeepEnds: pushes first, then last N in reverse, then reverses all,
    // then ensure_single_system_first pins the system message to front.
    // Result = [System prompt, Last question, Last answer]
    assert_eq!(result.len(), 3);
    // System message must be first and unique
    assert_eq!(result[0].role, "system");
    assert_eq!(result[0].content.text(), "System prompt");
    assert_eq!(result[1].content.text(), "Last question");
    assert_eq!(result[2].content.text(), "Last answer");
    // Stats should be updated
    let stats = pruner.stats();
    assert_eq!(stats.total_operations, 1);
}

#[test]
fn test_prune_keep_ends_few_messages() {
    let config = PruningConfig {
        target_tokens: 1, // force pruning
        strategy: PruningStrategy::KeepEnds,
        min_messages: 10, // more than messages.len()
        keep_system: true,
        keep_last_n: 2,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System"),
        Message::user("Hello"),
        Message::assistant("Hi"),
    ];
    // len <= min_messages, so returns all
    let result = pruner.prune(&messages);
    assert_eq!(result.len(), 3);
}

#[test]
fn test_prune_remove_tool_results_strategy() {
    let config = PruningConfig {
        target_tokens: 1, // force pruning
        strategy: PruningStrategy::RemoveToolResults,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 1,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System"),
        Message::user("Run a command"),
        Message::assistant("Sure"),
        Message::tool("command output", "call_1"),
        Message::assistant("Done"),
    ];
    let result = pruner.prune(&messages);
    // Tool messages should be removed
    assert!(result.iter().all(|m| m.role != "tool"));
    assert_eq!(result.len(), 4);
}

#[test]
fn test_prune_remove_system_messages_strategy() {
    let config = PruningConfig {
        target_tokens: 1, // force pruning
        strategy: PruningStrategy::RemoveSystemMessages,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 1,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("First system"),
        Message::user("Hello"),
        Message::system("Second system"),
        Message::assistant("Hi"),
        Message::system("Third system"),
    ];
    let result = pruner.prune(&messages);
    // Only first system should remain
    let system_count = result.iter().filter(|m| m.role == "system").count();
    assert_eq!(system_count, 1);
    assert_eq!(result[0].role, "system");
}

#[test]
fn test_prune_fallback_strategies() {
    // ByRelevance and Summarize fall through to KeepRecent
    for strategy in [PruningStrategy::ByRelevance, PruningStrategy::Summarize] {
        let config = PruningConfig {
            target_tokens: 1,
            strategy,
            min_messages: 1,
            keep_system: false,
            keep_last_n: 1,
        };
        let pruner = ContextPruner::new(config);
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi"),
            Message::user("Bye"),
        ];
        let result = pruner.prune(&messages);
        // Should not panic, and should return some messages
        assert!(!result.is_empty());
    }
}

#[test]
fn test_prune_keep_recent_no_system() {
    let config = PruningConfig {
        target_tokens: 1,
        strategy: PruningStrategy::KeepRecent,
        min_messages: 1,
        keep_system: false,
        keep_last_n: 1,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::user("First"),
        Message::assistant("First reply"),
        Message::user("Second"),
        Message::assistant("Second reply"),
    ];
    let result = pruner.prune(&messages);
    assert!(!result.is_empty());
}

#[test]
fn test_prune_updates_stats_correctly() {
    let config = PruningConfig {
        target_tokens: 1,
        strategy: PruningStrategy::RemoveToolResults,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 1,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System"),
        Message::user("Do something"),
        Message::tool("result", "call_1"),
        Message::assistant("Done"),
    ];
    pruner.prune(&messages);
    let stats = pruner.stats();
    assert_eq!(stats.total_operations, 1);
    assert!(stats.messages_removed > 0);
}

#[test]
fn test_prune_empty_messages() {
    let config = PruningConfig {
        target_tokens: 1,
        strategy: PruningStrategy::KeepRecent,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 1,
    };
    let pruner = ContextPruner::new(config);
    let messages: Vec<Message> = vec![];
    let result = pruner.prune(&messages);
    // No pruning needed since 0 tokens < target
    assert!(result.is_empty());
}

#[test]
fn test_prune_system_message_first_and_unique_keep_recent() {
    let config = PruningConfig {
        target_tokens: 1,
        strategy: PruningStrategy::KeepRecent,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 2,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System prompt A"),
        Message::user("First question"),
        Message::assistant("First answer"),
        Message::system("Duplicate system B"),
        Message::user("Second question"),
        Message::assistant("Second answer"),
        Message::system("Duplicate system C"),
    ];
    let result = pruner.prune(&messages);
    let system_count = result.iter().filter(|m| m.role == "system").count();
    assert_eq!(
        system_count, 1,
        "KeepRecent: expected exactly 1 system message"
    );
    assert_eq!(
        result[0].role, "system",
        "KeepRecent: system message must be first"
    );
}

#[test]
fn test_prune_system_message_first_and_unique_keep_ends() {
    let config = PruningConfig {
        target_tokens: 1,
        strategy: PruningStrategy::KeepEnds,
        min_messages: 1,
        keep_system: true,
        keep_last_n: 2,
    };
    let pruner = ContextPruner::new(config);
    let messages = vec![
        Message::system("System prompt A"),
        Message::user("First question"),
        Message::assistant("First answer"),
        Message::system("Duplicate system B"),
        Message::user("Second question"),
        Message::assistant("Second answer"),
        Message::system("Duplicate system C"),
    ];
    let result = pruner.prune(&messages);
    let system_count = result.iter().filter(|m| m.role == "system").count();
    assert_eq!(
        system_count, 1,
        "KeepEnds: expected exactly 1 system message"
    );
    assert_eq!(
        result[0].role, "system",
        "KeepEnds: system message must be first"
    );
}
