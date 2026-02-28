//! Dynamic Token Budget Allocator
//!
//! Manages token allocation across memory layers based on task type
//! and adapts based on actual usage patterns.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{debug, info};

use crate::cognitive::memory_hierarchy::{MemoryUsage, TokenBudget};

/// Task types for specialized token allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// General conversation
    Conversation,
    /// Analyzing code
    CodeAnalysis,
    /// Self-improvement tasks
    SelfImprovement,
    /// Generating new code
    CodeGeneration,
    /// Debugging issues
    Debugging,
    /// Refactoring code
    Refactoring,
    /// Learning from experiences
    Learning,
}

impl TaskType {
    /// Get default allocation ratios for this task type
    pub fn allocation_ratios(&self) -> AllocationRatios {
        match self {
            TaskType::Conversation => AllocationRatios {
                working: 30,
                episodic: 30,
                semantic: 30,
                reserve: 10,
            },
            TaskType::CodeAnalysis => AllocationRatios {
                working: 15,
                episodic: 15,
                semantic: 60,
                reserve: 10,
            },
            TaskType::SelfImprovement => AllocationRatios {
                working: 10,
                episodic: 10,
                semantic: 70,
                reserve: 10,
            },
            TaskType::CodeGeneration => AllocationRatios {
                working: 20,
                episodic: 20,
                semantic: 50,
                reserve: 10,
            },
            TaskType::Debugging => AllocationRatios {
                working: 25,
                episodic: 35,
                semantic: 30,
                reserve: 10,
            },
            TaskType::Refactoring => AllocationRatios {
                working: 15,
                episodic: 15,
                semantic: 60,
                reserve: 10,
            },
            TaskType::Learning => AllocationRatios {
                working: 20,
                episodic: 40,
                semantic: 30,
                reserve: 10,
            },
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            TaskType::Conversation => "General conversation",
            TaskType::CodeAnalysis => "Code analysis and understanding",
            TaskType::SelfImprovement => "Self-improvement and modification",
            TaskType::CodeGeneration => "Code generation",
            TaskType::Debugging => "Debugging and error resolution",
            TaskType::Refactoring => "Code refactoring",
            TaskType::Learning => "Learning from experiences",
        }
    }
}

/// Allocation ratios as percentages
#[derive(Debug, Clone, Copy)]
pub struct AllocationRatios {
    pub working: usize,
    pub episodic: usize,
    pub semantic: usize,
    pub reserve: usize,
}

impl AllocationRatios {
    /// Verify ratios sum to 100
    pub fn is_valid(&self) -> bool {
        self.working + self.episodic + self.semantic + self.reserve == 100
    }

    /// Convert to actual token counts
    pub fn to_token_allocation(&self, total: usize) -> TokenBudget {
        TokenBudget {
            working_memory: total * self.working / 100,
            episodic_memory: total * self.episodic / 100,
            semantic_memory: total * self.semantic / 100,
            response_reserve: total * self.reserve / 100,
        }
    }
}

/// Dynamic token budget allocator
pub struct TokenBudgetAllocator {
    /// Total available tokens
    total_tokens: usize,
    /// Current allocation
    allocation: TokenBudget,
    /// Current task type
    task_type: TaskType,
    /// Usage history for adaptive allocation
    usage_history: VecDeque<UsageSnapshot>,
    /// Maximum history size
    max_history: usize,
    /// Adaptation enabled
    adaptation_enabled: bool,
    /// Adaptation threshold (ratio difference to trigger adaptation)
    adaptation_threshold: f32,
}

/// Usage snapshot for tracking
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub timestamp: u64,
    pub working_used: usize,
    pub episodic_used: usize,
    pub semantic_used: usize,
    pub task_type: TaskType,
}

/// Adaptation result
#[derive(Debug, Clone)]
pub struct AdaptationResult {
    pub adapted: bool,
    pub old_allocation: TokenBudget,
    pub new_allocation: TokenBudget,
    pub reason: String,
}

impl TokenBudgetAllocator {
    /// Create new allocator with default task type
    pub fn new(total_tokens: usize, task_type: TaskType) -> Self {
        let ratios = task_type.allocation_ratios();
        let allocation = ratios.to_token_allocation(total_tokens);

        Self {
            total_tokens,
            allocation,
            task_type,
            usage_history: VecDeque::new(),
            max_history: 100,
            adaptation_enabled: true,
            adaptation_threshold: 0.3,
        }
    }

    /// Create with custom initial allocation
    pub fn with_allocation(total_tokens: usize, allocation: TokenBudget) -> Self {
        Self {
            total_tokens,
            allocation,
            task_type: TaskType::Conversation,
            usage_history: VecDeque::new(),
            max_history: 100,
            adaptation_enabled: true,
            adaptation_threshold: 0.3,
        }
    }

    /// Get current allocation
    pub fn get_allocation(&self) -> &TokenBudget {
        &self.allocation
    }

    /// Get current task type
    pub fn get_task_type(&self) -> TaskType {
        self.task_type
    }

    /// Change task type and reallocate
    pub fn set_task_type(&mut self, task_type: TaskType) {
        if self.task_type != task_type {
            info!(
                "Changing task type from {:?} to {:?}",
                self.task_type, task_type
            );

            self.task_type = task_type;
            let ratios = task_type.allocation_ratios();
            self.allocation = ratios.to_token_allocation(self.total_tokens);

            debug!("New allocation: {:?}", self.allocation);
        }
    }

    /// Record usage snapshot
    pub fn record_usage(&mut self, usage: &MemoryUsage) {
        let snapshot = UsageSnapshot {
            timestamp: current_timestamp_secs(),
            working_used: usage.working_tokens,
            episodic_used: usage.episodic_tokens,
            semantic_used: usage.semantic_tokens,
            task_type: self.task_type,
        };

        self.usage_history.push_back(snapshot);

        // Keep history bounded
        if self.usage_history.len() > self.max_history {
            self.usage_history.pop_front();
        }
    }

    /// Adapt allocation based on usage history
    pub fn adapt(&mut self) -> AdaptationResult {
        if !self.adaptation_enabled {
            return AdaptationResult {
                adapted: false,
                old_allocation: self.allocation.clone(),
                new_allocation: self.allocation.clone(),
                reason: "Adaptation disabled".to_string(),
            };
        }

        if self.usage_history.len() < 5 {
            return AdaptationResult {
                adapted: false,
                old_allocation: self.allocation.clone(),
                new_allocation: self.allocation.clone(),
                reason: "Insufficient history".to_string(),
            };
        }

        // Calculate average usage for current task type
        let relevant_history: Vec<_> = self
            .usage_history
            .iter()
            .filter(|s| s.task_type == self.task_type)
            .rev()
            .take(10)
            .collect();

        if relevant_history.is_empty() {
            return AdaptationResult {
                adapted: false,
                old_allocation: self.allocation.clone(),
                new_allocation: self.allocation.clone(),
                reason: "No relevant history".to_string(),
            };
        }

        let avg_working: usize = relevant_history
            .iter()
            .map(|s| s.working_used)
            .sum::<usize>()
            / relevant_history.len();
        let avg_episodic: usize = relevant_history
            .iter()
            .map(|s| s.episodic_used)
            .sum::<usize>()
            / relevant_history.len();
        let avg_semantic: usize = relevant_history
            .iter()
            .map(|s| s.semantic_used)
            .sum::<usize>()
            / relevant_history.len();

        // Calculate utilization ratios
        let working_ratio = avg_working as f32 / self.allocation.working_memory.max(1) as f32;
        let episodic_ratio = avg_episodic as f32 / self.allocation.episodic_memory.max(1) as f32;
        let semantic_ratio = avg_semantic as f32 / self.allocation.semantic_memory.max(1) as f32;

        debug!(
            "Utilization ratios - working: {:.2}, episodic: {:.2}, semantic: {:.2}",
            working_ratio, episodic_ratio, semantic_ratio
        );

        let old_allocation = self.allocation.clone();
        let mut adapted = false;
        let mut reasons = Vec::new();

        // Reallocate from underutilized to overutilized
        // Minimum transfer to avoid thrashing
        let min_transfer = self.total_tokens / 50; // 2% of total

        // Working -> Semantic
        if working_ratio < (1.0 - self.adaptation_threshold)
            && semantic_ratio > (1.0 + self.adaptation_threshold)
        {
            let transfer = ((self.allocation.working_memory as f32 * 0.1) as usize)
                .max(min_transfer)
                .min(self.allocation.working_memory / 4);

            if transfer > 0 {
                self.allocation.working_memory -= transfer;
                self.allocation.semantic_memory += transfer;
                adapted = true;
                reasons.push(format!(
                    "Moved {} tokens from working to semantic (working {:.0}%, semantic {:.0}%)",
                    transfer,
                    working_ratio * 100.0,
                    semantic_ratio * 100.0
                ));
            }
        }

        // Episodic -> Semantic
        if episodic_ratio < (1.0 - self.adaptation_threshold)
            && semantic_ratio > (1.0 + self.adaptation_threshold)
        {
            let transfer = ((self.allocation.episodic_memory as f32 * 0.1) as usize)
                .max(min_transfer)
                .min(self.allocation.episodic_memory / 4);

            if transfer > 0 {
                self.allocation.episodic_memory -= transfer;
                self.allocation.semantic_memory += transfer;
                adapted = true;
                reasons.push(format!(
                    "Moved {} tokens from episodic to semantic (episodic {:.0}%, semantic {:.0}%)",
                    transfer,
                    episodic_ratio * 100.0,
                    semantic_ratio * 100.0
                ));
            }
        }

        // Semantic -> Working (if semantic underutilized)
        if semantic_ratio < (1.0 - self.adaptation_threshold)
            && working_ratio > (1.0 + self.adaptation_threshold)
        {
            let transfer = ((self.allocation.semantic_memory as f32 * 0.05) as usize)
                .max(min_transfer)
                .min(self.allocation.semantic_memory / 10);

            if transfer > 0 {
                self.allocation.semantic_memory -= transfer;
                self.allocation.working_memory += transfer;
                adapted = true;
                reasons.push(format!(
                    "Moved {} tokens from semantic to working (semantic {:.0}%, working {:.0}%)",
                    transfer,
                    semantic_ratio * 100.0,
                    working_ratio * 100.0
                ));
            }
        }

        if adapted {
            info!("Token budget adapted: {}", reasons.join("; "));
        }

        AdaptationResult {
            adapted,
            old_allocation,
            new_allocation: self.allocation.clone(),
            reason: reasons.join("; "),
        }
    }

    /// Force a specific allocation
    pub fn force_allocation(&mut self, allocation: TokenBudget) {
        info!("Forcing token allocation: {:?}", allocation);
        self.allocation = allocation;
        self.adaptation_enabled = false; // Disable auto-adaptation
    }

    /// Enable/disable adaptation
    pub fn set_adaptation_enabled(&mut self, enabled: bool) {
        self.adaptation_enabled = enabled;
        debug!(
            "Adaptation {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Set adaptation threshold
    pub fn set_adaptation_threshold(&mut self, threshold: f32) {
        self.adaptation_threshold = threshold.clamp(0.1, 0.5);
    }

    /// Get usage statistics
    pub fn get_stats(&self) -> BudgetStats {
        let recent_usage: Vec<_> = self.usage_history.iter().rev().take(10).collect();

        BudgetStats {
            total_tokens: self.total_tokens,
            current_allocation: self.allocation.clone(),
            task_type: self.task_type,
            adaptation_enabled: self.adaptation_enabled,
            history_count: self.usage_history.len(),
            avg_working_usage: if recent_usage.is_empty() {
                0.0
            } else {
                recent_usage.iter().map(|s| s.working_used).sum::<usize>() as f32
                    / recent_usage.len() as f32
            },
            avg_episodic_usage: if recent_usage.is_empty() {
                0.0
            } else {
                recent_usage.iter().map(|s| s.episodic_used).sum::<usize>() as f32
                    / recent_usage.len() as f32
            },
            avg_semantic_usage: if recent_usage.is_empty() {
                0.0
            } else {
                recent_usage.iter().map(|s| s.semantic_used).sum::<usize>() as f32
                    / recent_usage.len() as f32
            },
        }
    }

    /// Reset to default allocation for current task type
    pub fn reset(&mut self) {
        let ratios = self.task_type.allocation_ratios();
        self.allocation = ratios.to_token_allocation(self.total_tokens);
        self.adaptation_enabled = true;
        info!("Token budget reset to defaults for {:?}", self.task_type);
    }

    /// Suggest optimal task type based on query
    pub fn suggest_task_type(query: &str) -> TaskType {
        let query_lower = query.to_lowercase();

        if query_lower.contains("improve")
            || query_lower.contains("refactor")
            || query_lower.contains("optimize")
            || query_lower.contains("enhance")
        {
            TaskType::SelfImprovement
        } else if query_lower.contains("debug")
            || query_lower.contains("fix")
            || query_lower.contains("error")
            || query_lower.contains("bug")
        {
            TaskType::Debugging
        } else if query_lower.contains("generate")
            || query_lower.contains("create")
            || query_lower.contains("write")
        {
            TaskType::CodeGeneration
        } else if query_lower.contains("analyze")
            || query_lower.contains("understand")
            || query_lower.contains("review")
        {
            TaskType::CodeAnalysis
        } else if query_lower.contains("learn")
            || query_lower.contains("study")
            || query_lower.contains("remember")
        {
            TaskType::Learning
        } else {
            TaskType::Conversation
        }
    }
}

/// Budget statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetStats {
    pub total_tokens: usize,
    pub current_allocation: TokenBudget,
    pub task_type: TaskType,
    pub adaptation_enabled: bool,
    pub history_count: usize,
    pub avg_working_usage: f32,
    pub avg_episodic_usage: f32,
    pub avg_semantic_usage: f32,
}

fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
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
        });
        allocator.record_usage(&MemoryUsage {
            working_tokens: 300,
            episodic_tokens: 400,
            semantic_tokens: 500,
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
}
