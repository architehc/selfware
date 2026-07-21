use super::*;
use crate::cognitive::memory_hierarchy::{FileContext, MemoryMetrics, MemoryUsage, TokenBudget};

// ========================================================================
// Helper functions
// ========================================================================

fn make_message(role: &str, content: &str) -> Message {
    Message {
        role: role.to_string(),
        content: content.to_string().into(),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }
}

fn make_working_context(messages: Vec<Message>) -> WorkingContext {
    WorkingContext {
        messages,
        active_code: Vec::new(),
        current_task: None,
        system_prompt: "You are Selfware, an AI assistant.".to_string(),
        estimated_tokens: 0,
        usage: MemoryUsage::default(),
    }
}

fn make_episode(id: &str, importance: Importance, content: &str) -> Episode {
    Episode {
        id: id.to_string(),
        episode_type: EpisodeType::Conversation,
        content: content.to_string(),
        token_count: estimate_tokens_with_overhead(content, 100),
        importance,
        timestamp: current_timestamp_secs(),
        embedding_id: id.to_string(),
        related_episodes: Vec::new(),
        insights: Vec::new(),
        is_summarized: false,
        original_id: None,
        metadata: std::collections::HashMap::new(),
    }
}

// ========================================================================
// Existing tests
// ========================================================================

#[test]
fn test_is_self_improvement_query() {
    // Would need to create a test instance
    // For now, just test the keyword matching logic
    let keywords = ["improve", "refactor", "optimize", "enhance"];

    let query = "How do I improve the memory system?";
    let lower = query.to_lowercase();
    assert!(keywords.iter().any(|k| lower.contains(k)));
}

#[test]
fn test_context_build_options_default() {
    let options = ContextBuildOptions::default();
    assert_eq!(options.task_type, TaskType::Conversation);
    assert!(options.include_self_ref);
    assert!(!options.force_self_improvement);
}

// ========================================================================
// ContextBuildOptions tests
// ========================================================================

#[test]
fn test_context_build_options_max_tokens_reserves_for_response() {
    let options = ContextBuildOptions::default();
    assert_eq!(options.max_tokens, TOTAL_CONTEXT_TOKENS - 100_000);
    assert!(options.max_tokens < TOTAL_CONTEXT_TOKENS);
}

#[test]
fn test_context_build_options_custom() {
    let options = ContextBuildOptions {
        task_type: TaskType::SelfImprovement,
        include_self_ref: false,
        max_tokens: 500_000,
        force_self_improvement: true,
    };
    assert_eq!(options.task_type, TaskType::SelfImprovement);
    assert!(!options.include_self_ref);
    assert_eq!(options.max_tokens, 500_000);
    assert!(options.force_self_improvement);
}

// ========================================================================
// is_self_improvement_query keyword tests
// ========================================================================

#[test]
fn test_is_self_improvement_query_improve() {
    // Directly test the keyword matching logic used by the method
    let keywords = [
        "improve",
        "refactor",
        "optimize",
        "enhance",
        "upgrade",
        "self",
        "my code",
        "myself",
        "own code",
        "modify myself",
        "memory system",
        "cognitive",
        "architecture",
        "redesign",
        "fix myself",
        "better",
        "more efficient",
        "performance",
    ];

    let positive_queries = [
        "How do I improve the memory system?",
        "Refactor the codebase structure",
        "Optimize token counting performance",
        "Enhance the cognitive layer",
        "Upgrade the self model",
        "Redesign the architecture for better performance",
        "Make the code more efficient",
    ];

    for query in &positive_queries {
        let lower = query.to_lowercase();
        assert!(
            keywords.iter().any(|k| lower.contains(k)),
            "Expected '{}' to match self-improvement keywords",
            query
        );
    }
}

#[test]
fn test_is_self_improvement_query_negative() {
    let keywords = [
        "improve",
        "refactor",
        "optimize",
        "enhance",
        "upgrade",
        "self",
        "my code",
        "myself",
        "own code",
        "modify myself",
        "memory system",
        "cognitive",
        "architecture",
        "redesign",
        "fix myself",
        "better",
        "more efficient",
        "performance",
    ];

    let query = "What is the weather today?";
    let lower = query.to_lowercase();
    assert!(!keywords.iter().any(|k| lower.contains(k)));
}

// ========================================================================
// estimate_context_tokens tests
// ========================================================================

#[test]
fn test_estimate_context_tokens_empty() {
    let working = make_working_context(Vec::new());
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

    assert_eq!(tokens, 0);
}

#[test]
fn test_estimate_context_tokens_with_working_messages() {
    let msgs = vec![
        make_message("user", "Hello"),
        make_message("assistant", "Hi there!"),
    ];
    let working = make_working_context(msgs);
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

    // Each message adds estimate_tokens_with_overhead(content, 50)
    // "Hello" ~ 1 token + 50 overhead = ~51
    // "Hi there!" ~ 2 tokens + 50 overhead = ~52
    assert!(tokens > 0, "Should count working memory tokens");
}

#[test]
fn test_estimate_context_tokens_with_episodic() {
    let working = make_working_context(Vec::new());
    let episodic = vec![
        make_episode("ep-1", Importance::Normal, "An episode about something"),
        make_episode("ep-2", Importance::High, "Another episode"),
    ];
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

    let expected_episodic: usize = episodic.iter().map(|e| e.token_count).sum();
    assert_eq!(tokens, expected_episodic);
}

#[test]
fn test_estimate_context_tokens_with_semantic() {
    let working = make_working_context(Vec::new());
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: vec![FileContext {
            path: "src/main.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: "rust".to_string(),
            estimated_tokens: 10,
            relevance_score: 0.9,
        }],
        symbols: Vec::new(),
        total_tokens: 500,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

    assert_eq!(tokens, 500);
}

#[test]
fn test_estimate_context_tokens_combined() {
    let msgs = vec![make_message("user", "Hello")];
    let working = make_working_context(msgs);
    let episodic = vec![make_episode("ep-1", Importance::Normal, "Episode text")];
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 200,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);

    // Should be sum of all three components
    let working_tokens = estimate_tokens_with_overhead("Hello", 50);
    let episodic_tokens = episodic[0].token_count;
    let semantic_tokens = 200;

    assert_eq!(tokens, working_tokens + episodic_tokens + semantic_tokens);
}

#[test]
fn test_estimate_context_tokens_with_self_context() {
    let working = make_working_context(Vec::new());
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };
    let self_ctx = Some(SelfImprovementContext {
        goal: "Improve memory".to_string(),
        self_model: "Model info".to_string(),
        architecture: "Arch info".to_string(),
        recent_modifications: "None".to_string(),
        relevant_code: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        suggestions: vec!["Suggestion".to_string()],
    });

    let tokens =
        CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &self_ctx);

    assert!(tokens > 0, "Self-improvement context should add tokens");
}

// ========================================================================
// LlmContext tests
// ========================================================================

#[test]
fn test_llm_context_to_prompt_empty() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 0,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("Selfware"));
    // Should not contain section headers for empty sections
    assert!(!prompt.contains("## Conversation History"));
    assert!(!prompt.contains("## Relevant Past Experiences"));
    assert!(!prompt.contains("## Relevant Code"));
}

#[test]
fn test_llm_context_to_prompt_with_messages() {
    let ctx = LlmContext {
        working: make_working_context(vec![
            make_message("user", "What is memory?"),
            make_message("assistant", "Memory is a system for storing data."),
        ]),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 100,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Conversation History"));
    assert!(prompt.contains("user: What is memory?"));
    assert!(prompt.contains("assistant: Memory is a system for storing data."));
}

#[test]
fn test_llm_context_to_prompt_with_episodic() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: vec![make_episode(
            "ep-1",
            Importance::Normal,
            "Found a bug in the parser",
        )],
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 100,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Relevant Past Experiences"));
    assert!(prompt.contains("conversation"));
}

#[test]
fn test_llm_context_to_prompt_with_semantic() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: vec![FileContext {
                path: "src/main.rs".to_string(),
                content: "fn main() { println!(\"hello\"); }".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 20,
                relevance_score: 0.95,
            }],
            symbols: Vec::new(),
            total_tokens: 100,
        },
        self_context: None,
        estimated_tokens: 100,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Relevant Code"));
    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("0.95"));
}

#[test]
fn test_llm_context_summary() {
    let ctx = LlmContext {
        working: make_working_context(vec![
            make_message("user", "Hello"),
            make_message("assistant", "Hi"),
        ]),
        episodic: vec![make_episode("e1", Importance::Normal, "ep")],
        semantic: CodeContext {
            files: vec![FileContext {
                path: "src/lib.rs".to_string(),
                content: "mod test;".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 10,
                relevance_score: 0.5,
            }],
            symbols: Vec::new(),
            total_tokens: 50,
        },
        self_context: None,
        estimated_tokens: 1000,
    };

    let summary = ctx.summary();
    assert!(summary.contains("2 working messages"));
    assert!(summary.contains("1 episodes"));
    assert!(summary.contains("1 code files"));
    assert!(summary.contains("1000 tokens"));
}

#[test]
fn test_llm_context_to_prompt_with_current_task() {
    let mut working = make_working_context(Vec::new());
    working.current_task = Some(super::super::memory_hierarchy::TaskContext {
        description: "Fix parser bug".to_string(),
        goal: "Eliminate null pointer exception".to_string(),
        progress: vec!["Identified root cause".to_string()],
        next_steps: vec!["Apply fix".to_string()],
        relevant_files: vec!["src/parser.rs".to_string()],
    });

    let ctx = LlmContext {
        working,
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 50,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Current Task"));
    assert!(prompt.contains("Fix parser bug"));
    assert!(prompt.contains("Eliminate null pointer exception"));
    assert!(prompt.contains("Identified root cause"));
}

// ========================================================================
// suggest_task_type delegation test
// ========================================================================

#[test]
fn test_suggest_task_type_delegates_correctly() {
    // This tests the same underlying function that CognitiveSystem delegates to
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("improve memory"),
        TaskType::SelfImprovement
    );
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("debug this error"),
        TaskType::Debugging
    );
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("generate a test"),
        TaskType::CodeGeneration
    );
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("analyze the code"),
        TaskType::CodeAnalysis
    );
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("learn from this"),
        TaskType::Learning
    );
    assert_eq!(
        TokenBudgetAllocator::suggest_task_type("hello world"),
        TaskType::Conversation
    );
}

// ========================================================================
// generate_id and timestamp helpers
// ========================================================================

#[test]
fn test_generate_id_format() {
    let id = generate_id();
    assert!(id.starts_with("ep-"), "ID should start with 'ep-'");
}

#[test]
fn test_generate_id_uniqueness() {
    let id1 = generate_id();
    // Small sleep to ensure different timestamp (millisecond precision)
    std::thread::sleep(std::time::Duration::from_millis(2));
    let id2 = generate_id();
    assert_ne!(id1, id2, "Generated IDs should be unique");
}

#[test]
fn test_format_timestamp() {
    let formatted = format_timestamp(0);
    assert!(formatted.contains("1970"));
}

// ========================================================================
// NEW: Comprehensive coverage tests
// ========================================================================

// --- ContextBuildOptions Debug / Clone ---

#[test]
fn test_context_build_options_debug() {
    let options = ContextBuildOptions::default();
    let debug_str = format!("{:?}", options);
    assert!(debug_str.contains("ContextBuildOptions"));
    assert!(debug_str.contains("Conversation"));
    assert!(debug_str.contains("include_self_ref"));
    assert!(debug_str.contains("force_self_improvement"));
}

#[test]
fn test_context_build_options_clone() {
    let original = ContextBuildOptions {
        task_type: TaskType::Debugging,
        include_self_ref: false,
        max_tokens: 42_000,
        force_self_improvement: true,
    };
    let cloned = original.clone();
    assert_eq!(cloned.task_type, TaskType::Debugging);
    assert!(!cloned.include_self_ref);
    assert_eq!(cloned.max_tokens, 42_000);
    assert!(cloned.force_self_improvement);
}

// --- LlmContext Debug / Clone ---

#[test]
fn test_llm_context_debug() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 42,
    };
    let debug_str = format!("{:?}", ctx);
    assert!(debug_str.contains("LlmContext"));
    assert!(debug_str.contains("42"));
}

#[test]
fn test_llm_context_clone() {
    let ctx = LlmContext {
        working: make_working_context(vec![make_message("user", "hi")]),
        episodic: vec![make_episode("e1", Importance::Low, "ep content")],
        semantic: CodeContext {
            files: vec![FileContext {
                path: "a.rs".to_string(),
                content: "fn a() {}".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 10,
                relevance_score: 0.8,
            }],
            symbols: Vec::new(),
            total_tokens: 77,
        },
        self_context: Some(SelfImprovementContext {
            goal: "g".to_string(),
            self_model: "m".to_string(),
            architecture: "a".to_string(),
            recent_modifications: "r".to_string(),
            relevant_code: CodeContext {
                files: Vec::new(),
                symbols: Vec::new(),
                total_tokens: 0,
            },
            suggestions: vec!["s".to_string()],
        }),
        estimated_tokens: 999,
    };
    let cloned = ctx.clone();
    assert_eq!(cloned.estimated_tokens, 999);
    assert_eq!(cloned.working.messages.len(), 1);
    assert_eq!(cloned.episodic.len(), 1);
    assert_eq!(cloned.semantic.total_tokens, 77);
    assert!(cloned.self_context.is_some());
}

// --- CognitiveSystemStats Debug / Clone ---

#[test]
fn test_cognitive_system_stats_debug() {
    let stats = CognitiveSystemStats {
        memory: MemoryStats {
            budget: Some(TokenBudget::default()),
            usage: Default::default(),
            metrics: Default::default(),
            working_entries: 5,
            episodic_entries: 10,
            semantic_files: 20,
            ..Default::default()
        },
        budget: BudgetStats {
            total_tokens: 1_000_000,
            current_allocation: TokenBudget::default(),
            task_type: TaskType::Conversation,
            adaptation_enabled: true,
            history_count: 0,
            avg_working_usage: 0.0,
            avg_episodic_usage: 0.0,
            avg_semantic_usage: 0.0,
        },
        self_model_modules: 3,
        self_model_capabilities: 7,
        recent_modifications: 2,
    };
    let debug_str = format!("{:?}", stats);
    assert!(debug_str.contains("CognitiveSystemStats"));
    assert!(debug_str.contains("self_model_modules: 3"));
    assert!(debug_str.contains("self_model_capabilities: 7"));
    assert!(debug_str.contains("recent_modifications: 2"));
}

#[test]
fn test_cognitive_system_stats_clone() {
    let stats = CognitiveSystemStats {
        memory: MemoryStats {
            budget: Some(TokenBudget::default()),
            usage: Default::default(),
            metrics: Default::default(),
            working_entries: 1,
            episodic_entries: 2,
            semantic_files: 3,
            ..Default::default()
        },
        budget: BudgetStats {
            total_tokens: 500_000,
            current_allocation: TokenBudget::default(),
            task_type: TaskType::CodeAnalysis,
            adaptation_enabled: false,
            history_count: 5,
            avg_working_usage: 0.5,
            avg_episodic_usage: 0.3,
            avg_semantic_usage: 0.2,
        },
        self_model_modules: 10,
        self_model_capabilities: 20,
        recent_modifications: 30,
    };
    let cloned = stats.clone();
    assert_eq!(cloned.self_model_modules, 10);
    assert_eq!(cloned.self_model_capabilities, 20);
    assert_eq!(cloned.recent_modifications, 30);
    assert_eq!(cloned.memory.working_entries, 1);
    assert_eq!(cloned.memory.episodic_entries, 2);
    assert_eq!(cloned.memory.semantic_files, 3);
    assert_eq!(cloned.budget.total_tokens, 500_000);
    assert_eq!(cloned.budget.task_type, TaskType::CodeAnalysis);
}

// --- format_timestamp with real timestamps ---

#[test]
fn test_format_timestamp_epoch() {
    let formatted = format_timestamp(0);
    assert_eq!(formatted, "1970-01-01 00:00");
}

#[test]
fn test_format_timestamp_known_date() {
    let formatted = format_timestamp(1704067200);
    assert!(formatted.contains("2024"));
    assert!(formatted.contains("01-01"));
    assert!(formatted.contains("00:00"));
}

#[test]
fn test_format_timestamp_midday() {
    let formatted = format_timestamp(1686831000);
    assert!(formatted.contains("2023"));
    assert!(formatted.contains("06-15"));
    assert!(formatted.contains("12:10"));
}

// --- current_timestamp_secs ---

#[test]
fn test_current_timestamp_secs_reasonable() {
    let ts = current_timestamp_secs();
    assert!(ts > 1704067200, "Timestamp should be after 2024-01-01");
    assert!(ts < 4102444800, "Timestamp should be before 2100-01-01");
}

#[test]
fn test_current_timestamp_secs_monotonic() {
    let t1 = current_timestamp_secs();
    let t2 = current_timestamp_secs();
    assert!(t2 >= t1, "Successive timestamps should be non-decreasing");
}

// --- generate_id extended ---

#[test]
fn test_generate_id_contains_numeric_part() {
    let id = generate_id();
    let numeric_part = &id[3..];
    assert!(
        numeric_part.chars().all(|c| c.is_ascii_digit()),
        "ID suffix should be numeric, got: {}",
        numeric_part
    );
}

#[test]
fn test_generate_id_not_empty() {
    let id = generate_id();
    assert!(id.len() > 3, "ID should be longer than just the prefix");
}

// --- LlmContext::to_prompt with active_code ---

#[test]
fn test_llm_context_to_prompt_with_active_code() {
    use crate::cognitive::memory_hierarchy::{ActiveCodeContext, CodeContent, CodeEdit};

    let working = WorkingContext {
        messages: Vec::new(),
        active_code: vec![
            ActiveCodeContext {
                path: "src/main.rs".to_string(),
                content: CodeContent::Full("fn main() {}".to_string()),
                last_accessed: current_timestamp_secs(),
                edit_history: Vec::new(),
            },
            ActiveCodeContext {
                path: "src/lib.rs".to_string(),
                content: CodeContent::Summary {
                    overview: "Library root".to_string(),
                    key_functions: vec!["init".to_string()],
                },
                last_accessed: current_timestamp_secs(),
                edit_history: vec![CodeEdit {
                    timestamp: current_timestamp_secs(),
                    description: "Added init fn".to_string(),
                    lines_changed: (1, 5),
                }],
            },
        ],
        current_task: None,
        system_prompt: "You are Selfware, an AI assistant.".to_string(),
        estimated_tokens: 0,
        usage: MemoryUsage::default(),
    };

    let ctx = LlmContext {
        working,
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 0,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Active Code Files"));
    assert!(prompt.contains("- src/main.rs"));
    assert!(prompt.contains("- src/lib.rs"));
}

// --- LlmContext::to_prompt with self_context ---

#[test]
fn test_llm_context_to_prompt_with_self_context() {
    let self_ctx = SelfImprovementContext {
        goal: "Optimize token counting".to_string(),
        self_model: "Current self model summary".to_string(),
        architecture: "Layered architecture".to_string(),
        recent_modifications: "Refactored memory module".to_string(),
        relevant_code: CodeContext {
            files: vec![FileContext {
                path: "src/token_count.rs".to_string(),
                content: "pub fn estimate() -> usize { 0 }".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 15,
                relevance_score: 0.88,
            }],
            symbols: Vec::new(),
            total_tokens: 50,
        },
        suggestions: vec![
            "Use SIMD for counting".to_string(),
            "Cache results".to_string(),
        ],
    };

    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: Some(self_ctx),
        estimated_tokens: 200,
    };

    let prompt = ctx.to_prompt();
    assert!(
        prompt.contains("Self-Improvement Context"),
        "Prompt should contain self-improvement section"
    );
    assert!(prompt.contains("Optimize token counting"));
    assert!(prompt.contains("Current self model summary"));
    assert!(prompt.contains("Layered architecture"));
    assert!(prompt.contains("Refactored memory module"));
    assert!(prompt.contains("Use SIMD for counting"));
    assert!(prompt.contains("Cache results"));
}

// --- LlmContext::to_prompt with task that has empty progress ---

#[test]
fn test_llm_context_to_prompt_task_no_progress() {
    let mut working = make_working_context(Vec::new());
    working.current_task = Some(super::super::memory_hierarchy::TaskContext {
        description: "New task".to_string(),
        goal: "Achieve something".to_string(),
        progress: Vec::new(),
        next_steps: Vec::new(),
        relevant_files: Vec::new(),
    });

    let ctx = LlmContext {
        working,
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 0,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Current Task"));
    assert!(prompt.contains("Description: New task"));
    assert!(prompt.contains("Goal: Achieve something"));
    assert!(
        !prompt.contains("Progress:"),
        "Should not show progress section when progress is empty"
    );
}

// --- LlmContext::to_prompt episodic content truncation ---

#[test]
fn test_llm_context_to_prompt_episodic_truncates_long_content() {
    let long_content = "x".repeat(500);
    let episode = Episode {
        id: "ep-long".to_string(),
        episode_type: EpisodeType::Success,
        content: long_content.clone(),
        token_count: 200,
        importance: Importance::High,
        timestamp: 1704067200,
        embedding_id: "ep-long".to_string(),
        related_episodes: Vec::new(),
        insights: Vec::new(),
        is_summarized: false,
        original_id: None,
        metadata: std::collections::HashMap::new(),
    };

    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: vec![episode],
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 200,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Relevant Past Experiences"));
    assert!(
        !prompt.contains(&long_content),
        "Full long content should not appear in prompt"
    );
    let truncated: String = "x".repeat(200);
    assert!(
        prompt.contains(&truncated),
        "Should contain first 200 chars of content"
    );
    assert!(prompt.contains("..."), "Should have truncation ellipsis");
    assert!(prompt.contains("[success]"));
}

// --- LlmContext::to_prompt with all episode types ---

#[test]
fn test_llm_context_to_prompt_various_episode_types() {
    let episode_types = vec![
        (EpisodeType::Conversation, "conversation"),
        (EpisodeType::ToolExecution, "tool_execution"),
        (EpisodeType::Error, "error"),
        (EpisodeType::Success, "success"),
        (EpisodeType::Action, "action"),
        (EpisodeType::Learning, "learning"),
        (EpisodeType::Thought, "thought"),
    ];

    for (ep_type, expected_str) in &episode_types {
        let episode = Episode {
            id: format!("ep-{}", expected_str),
            episode_type: *ep_type,
            content: format!("Content for {}", expected_str),
            token_count: 10,
            importance: Importance::Normal,
            timestamp: 1704067200,
            embedding_id: format!("emb-{}", expected_str),
            related_episodes: Vec::new(),
            insights: Vec::new(),
            is_summarized: false,
            original_id: None,
            metadata: std::collections::HashMap::new(),
        };

        let ctx = LlmContext {
            working: make_working_context(Vec::new()),
            episodic: vec![episode],
            semantic: CodeContext {
                files: Vec::new(),
                symbols: Vec::new(),
                total_tokens: 0,
            },
            self_context: None,
            estimated_tokens: 10,
        };

        let prompt = ctx.to_prompt();
        assert!(
            prompt.contains(&format!("[{}]", expected_str)),
            "Prompt should contain episode type [{}], got: {}",
            expected_str,
            prompt
        );
    }
}

// --- LlmContext::to_prompt with multiple semantic files ---

#[test]
fn test_llm_context_to_prompt_multiple_semantic_files() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: vec![
                FileContext {
                    path: "src/a.rs".to_string(),
                    content: "fn a() {}".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 10,
                    relevance_score: 0.99,
                },
                FileContext {
                    path: "src/b.rs".to_string(),
                    content: "fn b() {}".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 10,
                    relevance_score: 0.50,
                },
                FileContext {
                    path: "src/c.rs".to_string(),
                    content: "fn c() {}".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 10,
                    relevance_score: 0.10,
                },
            ],
            symbols: Vec::new(),
            total_tokens: 300,
        },
        self_context: None,
        estimated_tokens: 300,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("## Relevant Code"));
    assert!(prompt.contains("### src/a.rs (relevance: 0.99)"));
    assert!(prompt.contains("### src/b.rs (relevance: 0.50)"));
    assert!(prompt.contains("### src/c.rs (relevance: 0.10)"));
    assert!(prompt.contains("fn a() {}"));
    assert!(prompt.contains("fn b() {}"));
    assert!(prompt.contains("fn c() {}"));
}

// --- LlmContext::to_prompt fully populated context ---

#[test]
fn test_llm_context_to_prompt_all_sections() {
    use crate::cognitive::memory_hierarchy::{ActiveCodeContext, CodeContent};

    let working = WorkingContext {
        messages: vec![make_message("user", "Tell me about memory")],
        active_code: vec![ActiveCodeContext {
            path: "src/memory.rs".to_string(),
            content: CodeContent::Full("struct Memory;".to_string()),
            last_accessed: current_timestamp_secs(),
            edit_history: Vec::new(),
        }],
        current_task: Some(super::super::memory_hierarchy::TaskContext {
            description: "Build memory system".to_string(),
            goal: "Efficient storage".to_string(),
            progress: vec!["Designed schema".to_string()],
            next_steps: vec!["Implement".to_string()],
            relevant_files: vec!["src/memory.rs".to_string()],
        }),
        system_prompt: "You are Selfware, an AI assistant.".to_string(),
        estimated_tokens: 0,
        usage: MemoryUsage::default(),
    };

    let self_ctx = SelfImprovementContext {
        goal: "Better memory".to_string(),
        self_model: "SM".to_string(),
        architecture: "Arch".to_string(),
        recent_modifications: "Mods".to_string(),
        relevant_code: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        suggestions: vec!["Do X".to_string()],
    };

    let ctx = LlmContext {
        working,
        episodic: vec![make_episode(
            "ep-all",
            Importance::Critical,
            "Critical event",
        )],
        semantic: CodeContext {
            files: vec![FileContext {
                path: "src/semantic.rs".to_string(),
                content: "fn search() {}".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 15,
                relevance_score: 0.75,
            }],
            symbols: Vec::new(),
            total_tokens: 100,
        },
        self_context: Some(self_ctx),
        estimated_tokens: 5000,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("You are Selfware"));
    assert!(prompt.contains("## Conversation History"));
    assert!(prompt.contains("user: Tell me about memory"));
    assert!(prompt.contains("## Current Task"));
    assert!(prompt.contains("Build memory system"));
    assert!(prompt.contains("Efficient storage"));
    assert!(prompt.contains("Designed schema"));
    assert!(prompt.contains("## Relevant Past Experiences"));
    assert!(prompt.contains("[conversation]"));
    assert!(prompt.contains("Critical event"));
    assert!(prompt.contains("Self-Improvement Context"));
    assert!(prompt.contains("Better memory"));
    assert!(prompt.contains("## Relevant Code"));
    assert!(prompt.contains("src/semantic.rs"));
    assert!(prompt.contains("## Active Code Files"));
    assert!(prompt.contains("- src/memory.rs"));
}

// --- LlmContext::summary edge cases ---

#[test]
fn test_llm_context_summary_empty() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 0,
    };
    let summary = ctx.summary();
    assert!(summary.contains("0 working messages"));
    assert!(summary.contains("0 episodes"));
    assert!(summary.contains("0 code files"));
    assert!(summary.contains("~0 tokens"));
}

#[test]
fn test_llm_context_summary_large_numbers() {
    let ctx = LlmContext {
        working: make_working_context(vec![
            make_message("user", "a"),
            make_message("assistant", "b"),
            make_message("user", "c"),
            make_message("assistant", "d"),
            make_message("user", "e"),
        ]),
        episodic: vec![
            make_episode("e1", Importance::Low, "ep1"),
            make_episode("e2", Importance::Normal, "ep2"),
            make_episode("e3", Importance::High, "ep3"),
        ],
        semantic: CodeContext {
            files: vec![
                FileContext {
                    path: "a.rs".to_string(),
                    content: "a".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 5,
                    relevance_score: 0.1,
                },
                FileContext {
                    path: "b.rs".to_string(),
                    content: "b".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 5,
                    relevance_score: 0.2,
                },
            ],
            symbols: Vec::new(),
            total_tokens: 10000,
        },
        self_context: None,
        estimated_tokens: 999_999,
    };
    let summary = ctx.summary();
    assert!(summary.contains("5 working messages"));
    assert!(summary.contains("3 episodes"));
    assert!(summary.contains("2 code files"));
    assert!(summary.contains("999999 tokens"));
}

// --- estimate_context_tokens: self_context with code files ---

#[test]
fn test_estimate_context_tokens_self_context_with_code() {
    let working = make_working_context(Vec::new());
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };
    let self_ctx = Some(SelfImprovementContext {
        goal: "Improve all the things".to_string(),
        self_model: "Complex model description that is fairly long to generate some tokens"
            .to_string(),
        architecture: "Multi-layered architecture with several components".to_string(),
        recent_modifications: "Changed tokenizer, improved caching, added new vector store"
            .to_string(),
        relevant_code: CodeContext {
            files: vec![
                FileContext {
                    path: "src/tokenizer.rs".to_string(),
                    content: "pub fn tokenize(s: &str) -> Vec<Token> { vec![] }".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 20,
                    relevance_score: 0.9,
                },
                FileContext {
                    path: "src/cache.rs".to_string(),
                    content: "pub struct Cache { entries: HashMap<String, Vec<u8>> }".to_string(),
                    language: "rust".to_string(),
                    estimated_tokens: 20,
                    relevance_score: 0.7,
                },
            ],
            symbols: Vec::new(),
            total_tokens: 100,
        },
        suggestions: vec![
            "Use arena allocation".to_string(),
            "Implement batch tokenization".to_string(),
            "Add LRU eviction".to_string(),
        ],
    });

    let tokens =
        CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &self_ctx);
    assert!(
        tokens > 0,
        "Self-context with code and suggestions should produce tokens"
    );
    let expected_self_tokens = self_ctx.as_ref().unwrap().estimate_tokens();
    assert_eq!(tokens, expected_self_tokens);
}

// --- estimate_context_tokens: all four components with values ---

#[test]
fn test_estimate_context_tokens_all_four_components() {
    let msgs = vec![
        make_message("user", "What about the architecture?"),
        make_message("assistant", "It has several layers."),
    ];
    let working = make_working_context(msgs);

    let episodic = vec![
        make_episode("e1", Importance::Normal, "First episode"),
        make_episode("e2", Importance::High, "Second episode with more text"),
    ];

    let semantic = CodeContext {
        files: vec![FileContext {
            path: "src/arch.rs".to_string(),
            content: "pub struct Architecture;".to_string(),
            language: "rust".to_string(),
            estimated_tokens: 10,
            relevance_score: 0.85,
        }],
        symbols: Vec::new(),
        total_tokens: 333,
    };

    let self_ctx = Some(SelfImprovementContext {
        goal: "Improve architecture".to_string(),
        self_model: "SM".to_string(),
        architecture: "A".to_string(),
        recent_modifications: "None".to_string(),
        relevant_code: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        suggestions: vec!["Consider patterns".to_string()],
    });

    let tokens =
        CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &self_ctx);

    let expected_working: usize = [
        estimate_tokens_with_overhead("What about the architecture?", 50),
        estimate_tokens_with_overhead("It has several layers.", 50),
    ]
    .iter()
    .sum();
    let expected_episodic: usize = episodic.iter().map(|e| e.token_count).sum();
    let expected_semantic = 333;
    let expected_self = self_ctx.as_ref().unwrap().estimate_tokens();

    assert_eq!(
        tokens,
        expected_working + expected_episodic + expected_semantic + expected_self
    );
    assert!(expected_working > 0);
    assert!(expected_episodic > 0);
    assert!(expected_self > 0);
}

// --- estimate_context_tokens: None self_context contributes zero ---

#[test]
fn test_estimate_context_tokens_none_self_context_contributes_zero() {
    let working = make_working_context(Vec::new());
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 100,
    };

    let with_none = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);
    assert_eq!(with_none, 100, "Only semantic tokens should count");

    let self_ctx = Some(SelfImprovementContext {
        goal: "g".to_string(),
        self_model: "m".to_string(),
        architecture: "a".to_string(),
        recent_modifications: "r".to_string(),
        relevant_code: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        suggestions: Vec::new(),
    });

    let with_some =
        CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &self_ctx);
    assert!(
        with_some > with_none,
        "Adding self_context should increase token count"
    );
}

// --- ContextBuildOptions for each TaskType ---

#[test]
fn test_context_build_options_all_task_types() {
    let task_types = vec![
        TaskType::Conversation,
        TaskType::CodeAnalysis,
        TaskType::SelfImprovement,
        TaskType::CodeGeneration,
        TaskType::Debugging,
        TaskType::Refactoring,
        TaskType::Learning,
    ];

    for tt in task_types {
        let options = ContextBuildOptions {
            task_type: tt,
            include_self_ref: true,
            max_tokens: 100_000,
            force_self_improvement: false,
        };
        let cloned = options.clone();
        assert_eq!(cloned.task_type, tt);
        assert_eq!(cloned.max_tokens, 100_000);
        let debug = format!("{:?}", options);
        assert!(!debug.is_empty());
    }
}

// --- format_timestamp edge cases ---

#[test]
fn test_format_timestamp_current_approx() {
    let ts = current_timestamp_secs();
    let formatted = format_timestamp(ts);
    assert!(
        formatted.starts_with("202") || formatted.starts_with("203"),
        "Formatted timestamp should start with 202x or 203x: {}",
        formatted
    );
    assert_eq!(formatted.len(), 16, "Format should be 'YYYY-MM-DD HH:MM'");
    assert_eq!(formatted.chars().nth(4), Some('-'));
    assert_eq!(formatted.chars().nth(7), Some('-'));
    assert_eq!(formatted.chars().nth(10), Some(' '));
    assert_eq!(formatted.chars().nth(13), Some(':'));
}

#[test]
fn test_format_timestamp_large_value() {
    let formatted = format_timestamp(2145916800);
    assert!(formatted.contains("2037") || formatted.contains("2038"));
}

// --- LlmContext::to_prompt with episodic at exactly 200 chars ---

#[test]
fn test_llm_context_to_prompt_episodic_exactly_200_chars() {
    let exact_200 = "a".repeat(200);
    let episode = Episode {
        id: "ep-200".to_string(),
        episode_type: EpisodeType::Learning,
        content: exact_200.clone(),
        token_count: 50,
        importance: Importance::Normal,
        timestamp: 1704067200,
        embedding_id: "ep-200".to_string(),
        related_episodes: Vec::new(),
        insights: Vec::new(),
        is_summarized: false,
        original_id: None,
        metadata: std::collections::HashMap::new(),
    };

    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: vec![episode],
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 50,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains(&exact_200));
    assert!(prompt.contains("[learning]"));
}

// --- LlmContext::to_prompt with episodic shorter than 200 chars ---

#[test]
fn test_llm_context_to_prompt_episodic_short_content() {
    let short = "short content";
    let episode = Episode {
        id: "ep-short".to_string(),
        episode_type: EpisodeType::Error,
        content: short.to_string(),
        token_count: 5,
        importance: Importance::Low,
        timestamp: 1704067200,
        embedding_id: "ep-short".to_string(),
        related_episodes: Vec::new(),
        insights: Vec::new(),
        is_summarized: false,
        original_id: None,
        metadata: std::collections::HashMap::new(),
    };

    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: vec![episode],
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 5,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("short content"));
    assert!(prompt.contains("[error]"));
    assert!(prompt.contains("2024-01-01"));
}

// --- LlmContext::to_prompt with multiple episodes ---

#[test]
fn test_llm_context_to_prompt_multiple_episodes() {
    let episodes = vec![
        Episode {
            id: "ep-1".to_string(),
            episode_type: EpisodeType::Conversation,
            content: "First conversation".to_string(),
            token_count: 10,
            importance: Importance::Normal,
            timestamp: 1704067200,
            embedding_id: "emb-1".to_string(),
            related_episodes: Vec::new(),
            insights: Vec::new(),
            is_summarized: false,
            original_id: None,
            metadata: std::collections::HashMap::new(),
        },
        Episode {
            id: "ep-2".to_string(),
            episode_type: EpisodeType::Thought,
            content: "Made a decision about architecture".to_string(),
            token_count: 15,
            importance: Importance::High,
            timestamp: 1704153600,
            embedding_id: "emb-2".to_string(),
            related_episodes: vec!["ep-1".to_string()],
            insights: vec!["Good decision".to_string()],
            is_summarized: false,
            original_id: None,
            metadata: std::collections::HashMap::new(),
        },
    ];

    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: episodes,
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 25,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("[conversation]"));
    assert!(prompt.contains("[thought]"));
    assert!(prompt.contains("First conversation"));
    assert!(prompt.contains("Made a decision about architecture"));
}

// --- LlmContext::to_prompt with task with multiple progress items ---

#[test]
fn test_llm_context_to_prompt_task_multiple_progress() {
    let mut working = make_working_context(Vec::new());
    working.current_task = Some(super::super::memory_hierarchy::TaskContext {
        description: "Complex task".to_string(),
        goal: "Multi-step goal".to_string(),
        progress: vec![
            "Step 1 done".to_string(),
            "Step 2 done".to_string(),
            "Step 3 in progress".to_string(),
        ],
        next_steps: Vec::new(),
        relevant_files: Vec::new(),
    });

    let ctx = LlmContext {
        working,
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: None,
        estimated_tokens: 0,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("Progress:"));
    assert!(prompt.contains("- Step 1 done"));
    assert!(prompt.contains("- Step 2 done"));
    assert!(prompt.contains("- Step 3 in progress"));
}

// --- make_episode helper produces valid episodes ---

#[test]
fn test_make_episode_helper() {
    let ep = make_episode("test-ep", Importance::Critical, "Test content here");
    assert_eq!(ep.id, "test-ep");
    assert_eq!(ep.episode_type, EpisodeType::Conversation);
    assert_eq!(ep.content, "Test content here");
    assert!(ep.token_count > 0);
    assert_eq!(ep.importance, Importance::Critical);
    assert!(ep.timestamp > 0);
    assert_eq!(ep.embedding_id, "test-ep");
    assert!(ep.related_episodes.is_empty());
    assert!(ep.insights.is_empty());
    assert!(!ep.is_summarized);
    assert!(ep.original_id.is_none());
}

// --- make_message helper produces valid messages ---

#[test]
fn test_make_message_helper() {
    let msg = make_message("system", "You are a helpful assistant");
    assert_eq!(msg.role, "system");
    assert_eq!(msg.content, "You are a helpful assistant");
    assert!(msg.reasoning_content.is_none());
    assert!(msg.tool_calls.is_none());
    assert!(msg.tool_call_id.is_none());
    assert!(msg.name.is_none());
}

// --- make_working_context helper ---

#[test]
fn test_make_working_context_helper() {
    let wc = make_working_context(vec![make_message("user", "hi")]);
    assert_eq!(wc.messages.len(), 1);
    assert!(wc.active_code.is_empty());
    assert!(wc.current_task.is_none());

    let wc_empty = make_working_context(Vec::new());
    assert!(wc_empty.messages.is_empty());
}

// --- estimate_context_tokens with single large message ---

#[test]
fn test_estimate_context_tokens_large_message() {
    let large_content = "word ".repeat(10_000);
    let msgs = vec![make_message("user", &large_content)];
    let working = make_working_context(msgs);
    let episodic: Vec<Episode> = Vec::new();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);
    assert!(
        tokens > 1000,
        "Large message should produce many tokens, got {}",
        tokens
    );
}

// --- estimate_context_tokens with many episodic entries ---

#[test]
fn test_estimate_context_tokens_many_episodes() {
    let working = make_working_context(Vec::new());
    let episodic: Vec<Episode> = (0..50)
        .map(|i| make_episode(&format!("ep-{}", i), Importance::Normal, "Episode content"))
        .collect();
    let semantic = CodeContext {
        files: Vec::new(),
        symbols: Vec::new(),
        total_tokens: 0,
    };

    let tokens = CognitiveSystem::estimate_context_tokens(&working, &episodic, &semantic, &None);
    let expected: usize = episodic.iter().map(|e| e.token_count).sum();
    assert_eq!(tokens, expected);
    assert!(tokens > 0);
}

// --- CognitiveSystemStats field access ---

#[test]
fn test_cognitive_system_stats_fields() {
    let stats = CognitiveSystemStats {
        memory: MemoryStats {
            budget: Some(TokenBudget::for_conversation()),
            usage: Some(MemoryUsage {
                working_tokens: 100,
                episodic_tokens: 200,
                semantic_tokens: 300,
                self_tokens: 0,
                total_used: 600,
            }),
            metrics: Some(MemoryMetrics {
                cache_hits: 10,
                cache_misses: 5,
                evictions: 2,
                compressions: 1,
                avg_retrieval_time_ms: 3.5,
                last_updated: 1704067200,
            }),
            working_entries: 15,
            episodic_entries: 25,
            semantic_files: 50,
            ..Default::default()
        },
        budget: BudgetStats {
            total_tokens: 1_000_000,
            current_allocation: TokenBudget::for_self_improvement(),
            task_type: TaskType::SelfImprovement,
            adaptation_enabled: true,
            history_count: 10,
            avg_working_usage: 0.3,
            avg_episodic_usage: 0.5,
            avg_semantic_usage: 0.7,
        },
        self_model_modules: 12,
        self_model_capabilities: 8,
        recent_modifications: 4,
    };

    assert_eq!(stats.memory.working_entries, 15);
    assert_eq!(stats.memory.episodic_entries, 25);
    assert_eq!(stats.memory.semantic_files, 50);
    assert_eq!(stats.memory.usage.as_ref().unwrap().working_tokens, 100);
    assert_eq!(stats.memory.usage.as_ref().unwrap().episodic_tokens, 200);
    assert_eq!(stats.memory.usage.as_ref().unwrap().semantic_tokens, 300);
    assert_eq!(stats.memory.metrics.as_ref().unwrap().cache_hits, 10);
    assert_eq!(stats.budget.total_tokens, 1_000_000);
    assert_eq!(stats.budget.task_type, TaskType::SelfImprovement);
    assert!(stats.budget.adaptation_enabled);
    assert_eq!(stats.self_model_modules, 12);
    assert_eq!(stats.self_model_capabilities, 8);
    assert_eq!(stats.recent_modifications, 4);
}

// --- Prompt ordering: system context comes first ---

#[test]
fn test_llm_context_to_prompt_section_ordering() {
    let ctx = LlmContext {
        working: make_working_context(vec![make_message("user", "hi")]),
        episodic: vec![make_episode("ep-1", Importance::Normal, "ep")],
        semantic: CodeContext {
            files: vec![FileContext {
                path: "x.rs".to_string(),
                content: "x".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 5,
                relevance_score: 0.5,
            }],
            symbols: Vec::new(),
            total_tokens: 10,
        },
        self_context: None,
        estimated_tokens: 100,
    };

    let prompt = ctx.to_prompt();
    let selfware_pos = prompt
        .find("You are Selfware")
        .expect("Should contain system context");
    let history_pos = prompt
        .find("## Conversation History")
        .expect("Should contain conversation");
    let episodic_pos = prompt
        .find("## Relevant Past Experiences")
        .expect("Should contain episodic");
    let code_pos = prompt
        .find("## Relevant Code")
        .expect("Should contain code");

    assert!(
        selfware_pos < history_pos,
        "System context should come before conversation"
    );
    assert!(
        history_pos < episodic_pos,
        "Conversation should come before episodic"
    );
    assert!(episodic_pos < code_pos, "Episodic should come before code");
}

// --- LlmContext::to_prompt with semantic file content containing special chars ---

#[test]
fn test_llm_context_to_prompt_semantic_special_chars() {
    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: vec![FileContext {
                path: "src/special.rs".to_string(),
                content: "fn main() {\n    let x = a & b | c;\n}".to_string(),
                language: "rust".to_string(),
                estimated_tokens: 20,
                relevance_score: 0.42,
            }],
            symbols: Vec::new(),
            total_tokens: 30,
        },
        self_context: None,
        estimated_tokens: 30,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("src/special.rs"));
    assert!(prompt.contains("0.42"));
    assert!(prompt.contains("a & b | c"));
}

// --- Context build options: boundary values for max_tokens ---

#[test]
fn test_context_build_options_zero_max_tokens() {
    let options = ContextBuildOptions {
        task_type: TaskType::Conversation,
        include_self_ref: false,
        max_tokens: 0,
        force_self_improvement: false,
    };
    assert_eq!(options.max_tokens, 0);
}

#[test]
fn test_context_build_options_very_large_max_tokens() {
    let options = ContextBuildOptions {
        task_type: TaskType::Conversation,
        include_self_ref: true,
        max_tokens: usize::MAX,
        force_self_improvement: false,
    };
    assert_eq!(options.max_tokens, usize::MAX);
}

// --- generate_id format with timestamp parsing ---

#[test]
fn test_generate_id_timestamp_is_millis() {
    let id = generate_id();
    let millis_str = &id[3..];
    let millis: u128 = millis_str.parse().expect("ID suffix should parse as u128");
    let min_millis: u128 = 1_704_067_200_000;
    assert!(
        millis > min_millis,
        "Timestamp millis {} should be after 2024-01-01",
        millis
    );
}

// --- LlmContext with empty self_context suggestions ---

#[test]
fn test_llm_context_to_prompt_self_context_empty_suggestions() {
    let self_ctx = SelfImprovementContext {
        goal: "Minimal goal".to_string(),
        self_model: "Minimal model".to_string(),
        architecture: "Minimal arch".to_string(),
        recent_modifications: "None".to_string(),
        relevant_code: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        suggestions: Vec::new(),
    };

    let ctx = LlmContext {
        working: make_working_context(Vec::new()),
        episodic: Vec::new(),
        semantic: CodeContext {
            files: Vec::new(),
            symbols: Vec::new(),
            total_tokens: 0,
        },
        self_context: Some(self_ctx),
        estimated_tokens: 50,
    };

    let prompt = ctx.to_prompt();
    assert!(prompt.contains("Self-Improvement Context"));
    assert!(prompt.contains("Minimal goal"));
    assert!(prompt.contains("Suggestions to Consider"));
}
