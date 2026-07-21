use super::*;

#[test]
fn test_task_type_allocation_ratios() {
    for task in [
        TaskType::Conversation,
        TaskType::CodeAnalysis,
        TaskType::SelfImprovement,
        TaskType::CodeGeneration,
        TaskType::Debugging,
        TaskType::Refactoring,
        TaskType::Learning,
    ] {
        let ratios = task.allocation_ratios();
        assert!(ratios.is_valid(), "Ratios for {:?} don't sum to 100", task);
    }
}

#[test]
fn test_token_allocation_conversion() {
    let ratios = AllocationRatios {
        working: 10,
        episodic: 20,
        semantic: 60,
        reserve: 10,
    };

    let budget = ratios.to_token_allocation(1_000_000);
    assert_eq!(budget.working_memory, 100_000);
    assert_eq!(budget.episodic_memory, 200_000);
    assert_eq!(budget.semantic_memory, 600_000);
    assert_eq!(budget.response_reserve, 100_000);
}

#[test]
fn test_suggest_task_type() {
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("How do I improve the memory system?"),
        TaskType::SelfImprovement
    );

    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("Debug this error in the code"),
        TaskType::Debugging
    );

    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("Generate a new function"),
        TaskType::CodeGeneration
    );
}

#[test]
fn test_adaptation() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);

    // Simulate underutilization of working memory and overutilization of semantic
    for _ in 0..10 {
        allocator.record_usage(&MemoryUsage {
            working_tokens: 10_000,   // Way under budget
            episodic_tokens: 100_000, // Normal
            semantic_tokens: 650_000, // Over budget
            self_tokens: 0,
            total_used: 760_000,
        });
    }

    let result = allocator.adapt();

    // Should have adapted
    assert!(result.adapted);
    assert!(
        result.new_allocation.semantic_memory > result.old_allocation.semantic_memory
            || result.new_allocation.working_memory < result.old_allocation.working_memory
    );
}

// ========================================================================
// record_usage tests
// ========================================================================

#[test]
fn test_record_usage_appends_snapshot() {
    let mut allocator = TokenBudgetAllocator::new(100_000, TaskType::Conversation);
    assert_eq!(allocator.usage_history.len(), 0);

    allocator.record_usage(&MemoryUsage {
        working_tokens: 100,
        episodic_tokens: 200,
        semantic_tokens: 300,
        self_tokens: 0,
        total_used: 0,
    });
    assert_eq!(allocator.usage_history.len(), 1);

    let snap = allocator.usage_history.back().unwrap();
    assert_eq!(snap.working_used, 100);
    assert_eq!(snap.episodic_used, 200);
    assert_eq!(snap.semantic_used, 300);
    assert_eq!(snap.task_type, TaskType::Conversation);
}

#[test]
fn test_record_usage_bounds_history() {
    let mut allocator = TokenBudgetAllocator::new(100_000, TaskType::Conversation);
    // max_history defaults to 100
    for i in 0..150 {
        allocator.record_usage(&MemoryUsage {
            working_tokens: i,
            episodic_tokens: 0,
            semantic_tokens: 0,
            self_tokens: 0,
            total_used: 0,
        });
    }
    assert_eq!(allocator.usage_history.len(), 100);
    // The oldest entries should have been evicted; front should be entry #50
    assert_eq!(allocator.usage_history.front().unwrap().working_used, 50);
}

// ========================================================================
// adapt() edge-case tests
// ========================================================================

#[test]
fn test_adapt_disabled_returns_no_adaptation() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);
    allocator.set_adaptation_enabled(false);

    // Even with lots of history it should not adapt
    for _ in 0..10 {
        allocator.record_usage(&MemoryUsage {
            working_tokens: 1,
            episodic_tokens: 1,
            semantic_tokens: 999_000,
            self_tokens: 0,
            total_used: 0,
        });
    }

    let result = allocator.adapt();
    assert!(!result.adapted);
    assert_eq!(result.reason, "Adaptation disabled");
}

#[test]
fn test_adapt_insufficient_history() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);
    // Only 4 snapshots -- need at least 5
    for _ in 0..4 {
        allocator.record_usage(&MemoryUsage {
            working_tokens: 100,
            episodic_tokens: 100,
            semantic_tokens: 100,
            self_tokens: 0,
            total_used: 0,
        });
    }

    let result = allocator.adapt();
    assert!(!result.adapted);
    assert_eq!(result.reason, "Insufficient history");
}

#[test]
fn test_adapt_balanced_usage_no_change() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);
    // Conversation ratios: working=30%, episodic=30%, semantic=30%, reserve=10%
    // Use exactly the allocated amounts so utilization is ~1.0 across the board
    for _ in 0..10 {
        allocator.record_usage(&MemoryUsage {
            working_tokens: 300_000,
            episodic_tokens: 300_000,
            semantic_tokens: 300_000,
            self_tokens: 0,
            total_used: 0,
        });
    }

    let result = allocator.adapt();
    // Balanced usage should not trigger adaptation
    assert!(!result.adapted);
}

#[test]
fn test_adapt_semantic_to_working_transfer() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);
    // Conversation: working=300k, episodic=300k, semantic=300k
    // Make semantic underused and working overused
    for _ in 0..10 {
        allocator.record_usage(&MemoryUsage {
            working_tokens: 600_000,  // 200% of allocation -- way over
            episodic_tokens: 300_000, // right at allocation
            semantic_tokens: 50_000,  // ~17% of allocation -- way under
            self_tokens: 0,
            total_used: 950_000,
        });
    }

    let old_working = allocator.allocation.working_memory;
    let old_semantic = allocator.allocation.semantic_memory;

    let result = allocator.adapt();
    assert!(result.adapted);
    // Tokens should move from semantic to working
    assert!(allocator.allocation.working_memory > old_working);
    assert!(allocator.allocation.semantic_memory < old_semantic);
}

// ========================================================================
// force_allocation / reset tests
// ========================================================================

#[test]
fn test_force_allocation_sets_values_and_disables_adaptation() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);
    assert!(allocator.adaptation_enabled);

    let custom = TokenBudget {
        working_memory: 500_000,
        episodic_memory: 200_000,
        semantic_memory: 200_000,
        response_reserve: 100_000,
    };
    allocator.force_allocation(custom);

    assert_eq!(allocator.allocation.working_memory, 500_000);
    assert_eq!(allocator.allocation.episodic_memory, 200_000);
    assert_eq!(allocator.allocation.semantic_memory, 200_000);
    assert_eq!(allocator.allocation.response_reserve, 100_000);
    assert!(!allocator.adaptation_enabled);
}

#[test]
fn test_reset_restores_default_allocation_and_enables_adaptation() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::CodeAnalysis);

    // Force a custom allocation
    allocator.force_allocation(TokenBudget {
        working_memory: 1,
        episodic_memory: 1,
        semantic_memory: 1,
        response_reserve: 1,
    });
    assert!(!allocator.adaptation_enabled);

    allocator.reset();

    // CodeAnalysis ratios: working=15%, episodic=15%, semantic=60%, reserve=10%
    assert_eq!(allocator.allocation.working_memory, 150_000);
    assert_eq!(allocator.allocation.episodic_memory, 150_000);
    assert_eq!(allocator.allocation.semantic_memory, 600_000);
    assert_eq!(allocator.allocation.response_reserve, 100_000);
    assert!(allocator.adaptation_enabled);
}

// ========================================================================
// get_stats tests
// ========================================================================

#[test]
fn test_get_stats_empty_history() {
    let allocator = TokenBudgetAllocator::new(500_000, TaskType::Learning);
    let stats = allocator.get_stats();

    assert_eq!(stats.total_tokens, 500_000);
    assert_eq!(stats.task_type, TaskType::Learning);
    assert!(stats.adaptation_enabled);
    assert_eq!(stats.history_count, 0);
    assert_eq!(stats.avg_working_usage, 0.0);
    assert_eq!(stats.avg_episodic_usage, 0.0);
    assert_eq!(stats.avg_semantic_usage, 0.0);
}

#[test]
fn test_get_stats_with_history() {
    let mut allocator = TokenBudgetAllocator::new(500_000, TaskType::Conversation);

    allocator.record_usage(&MemoryUsage {
        working_tokens: 100,
        episodic_tokens: 200,
        semantic_tokens: 300,
        self_tokens: 0,
        total_used: 0,
    });
    allocator.record_usage(&MemoryUsage {
        working_tokens: 300,
        episodic_tokens: 400,
        semantic_tokens: 500,
        self_tokens: 0,
        total_used: 0,
    });

    let stats = allocator.get_stats();
    assert_eq!(stats.history_count, 2);
    assert!((stats.avg_working_usage - 200.0).abs() < f32::EPSILON);
    assert!((stats.avg_episodic_usage - 300.0).abs() < f32::EPSILON);
    assert!((stats.avg_semantic_usage - 400.0).abs() < f32::EPSILON);
}

// ========================================================================
// AllocationRatios::is_valid tests
// ========================================================================

#[test]
fn test_allocation_ratios_valid_sum() {
    let ratios = AllocationRatios {
        working: 25,
        episodic: 25,
        semantic: 40,
        reserve: 10,
    };
    assert!(ratios.is_valid());
}

#[test]
fn test_allocation_ratios_under_sum() {
    let ratios = AllocationRatios {
        working: 10,
        episodic: 10,
        semantic: 10,
        reserve: 10,
    };
    assert!(!ratios.is_valid());
}

#[test]
fn test_allocation_ratios_over_sum() {
    let ratios = AllocationRatios {
        working: 50,
        episodic: 50,
        semantic: 50,
        reserve: 10,
    };
    assert!(!ratios.is_valid());
}

#[test]
fn test_allocation_ratios_zero_sum() {
    let ratios = AllocationRatios {
        working: 0,
        episodic: 0,
        semantic: 0,
        reserve: 0,
    };
    assert!(!ratios.is_valid());
}

// ========================================================================
// Constructor variants and set_task_type tests
// ========================================================================

#[test]
fn test_new_allocator_uses_task_ratios() {
    let allocator = TokenBudgetAllocator::new(1_000_000, TaskType::SelfImprovement);
    // SelfImprovement ratios: working=10%, episodic=10%, semantic=70%, reserve=10%
    assert_eq!(allocator.allocation.working_memory, 100_000);
    assert_eq!(allocator.allocation.episodic_memory, 100_000);
    assert_eq!(allocator.allocation.semantic_memory, 700_000);
    assert_eq!(allocator.allocation.response_reserve, 100_000);
    assert_eq!(allocator.task_type, TaskType::SelfImprovement);
}

#[test]
fn test_with_allocation_uses_custom_budget() {
    let custom = TokenBudget {
        working_memory: 1,
        episodic_memory: 2,
        semantic_memory: 3,
        response_reserve: 4,
    };
    let allocator = TokenBudgetAllocator::with_allocation(10, custom);

    assert_eq!(allocator.allocation.working_memory, 1);
    assert_eq!(allocator.allocation.episodic_memory, 2);
    assert_eq!(allocator.allocation.semantic_memory, 3);
    assert_eq!(allocator.allocation.response_reserve, 4);
    // Default task type is Conversation
    assert_eq!(allocator.task_type, TaskType::Conversation);
    assert_eq!(allocator.total_tokens, 10);
}

#[test]
fn test_set_task_type_changes_allocation() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);
    assert_eq!(allocator.task_type, TaskType::Conversation);
    // Conversation: working=30%, episodic=30%, semantic=30%
    assert_eq!(allocator.allocation.working_memory, 300_000);

    allocator.set_task_type(TaskType::CodeAnalysis);
    assert_eq!(allocator.task_type, TaskType::CodeAnalysis);
    // CodeAnalysis: working=15%, episodic=15%, semantic=60%
    assert_eq!(allocator.allocation.working_memory, 150_000);
    assert_eq!(allocator.allocation.semantic_memory, 600_000);
}

#[test]
fn test_set_task_type_same_type_is_noop() {
    let mut allocator = TokenBudgetAllocator::new(1_000_000, TaskType::Conversation);

    // Force a custom allocation
    allocator.force_allocation(TokenBudget {
        working_memory: 999,
        episodic_memory: 999,
        semantic_memory: 999,
        response_reserve: 999,
    });

    // Setting the same task type should not reallocate
    allocator.set_task_type(TaskType::Conversation);
    assert_eq!(allocator.allocation.working_memory, 999);
}

#[test]
fn test_task_type_descriptions_are_nonempty() {
    for task in [
        TaskType::Conversation,
        TaskType::CodeAnalysis,
        TaskType::SelfImprovement,
        TaskType::CodeGeneration,
        TaskType::Debugging,
        TaskType::Refactoring,
        TaskType::Learning,
    ] {
        assert!(
            !task.description().is_empty(),
            "{:?} has empty description",
            task
        );
    }
}

#[test]
fn test_set_adaptation_threshold_clamps() {
    let mut allocator = TokenBudgetAllocator::new(100_000, TaskType::Conversation);

    allocator.set_adaptation_threshold(0.0);
    assert!((allocator.adaptation_threshold - 0.1).abs() < f32::EPSILON);

    allocator.set_adaptation_threshold(1.0);
    assert!((allocator.adaptation_threshold - 0.5).abs() < f32::EPSILON);

    allocator.set_adaptation_threshold(0.25);
    assert!((allocator.adaptation_threshold - 0.25).abs() < f32::EPSILON);
}

#[test]
fn test_to_token_allocation_zero_total() {
    let ratios = AllocationRatios {
        working: 25,
        episodic: 25,
        semantic: 40,
        reserve: 10,
    };
    let budget = ratios.to_token_allocation(0);
    assert_eq!(budget.working_memory, 0);
    assert_eq!(budget.episodic_memory, 0);
    assert_eq!(budget.semantic_memory, 0);
    assert_eq!(budget.response_reserve, 0);
}
