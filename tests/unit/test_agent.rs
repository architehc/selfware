//! Unit tests for the agent module
//!
//! Tests cover:
//! - AgentLoop state machine
//! - Planner prompt generation
//! - ContextCompressor functionality
//! - AgentError types

use selfware::agent::context::ContextCompressor;
use selfware::agent::loop_control::{AgentLoop, AgentState};
use selfware::agent::planning::Planner;

// ============================================================================
// AgentLoop Tests
// ============================================================================

mod agent_loop_tests {
    use super::*;

    #[test]
    fn test_new_creates_planning_state() {
        let agent_loop = AgentLoop::new(100);
        let mut agent_loop = agent_loop;
        let state = agent_loop.next_state();
        assert!(matches!(state, Some(AgentState::Planning)));
    }

    #[test]
    fn test_iteration_tracking() {
        let mut agent_loop = AgentLoop::new(5);

        // Each call to next_state increments iteration
        for i in 0..5 {
            let state = agent_loop.next_state();
            assert!(state.is_some(), "Iteration {} should succeed", i);
        }

        // 6th call should fail (exceeds max_iterations)
        let state = agent_loop.next_state();
        assert!(matches!(
            state,
            Some(AgentState::Failed { reason }) if reason.contains("Max iterations")
        ));
    }

    #[test]
    fn test_state_transitions() {
        let mut agent_loop = AgentLoop::new(100);

        // Start in Planning
        let state = agent_loop.next_state();
        assert!(matches!(state, Some(AgentState::Planning)));

        // Transition to Executing
        agent_loop.set_state(AgentState::Executing { step: 1 });
        let state = agent_loop.next_state();
        assert!(matches!(state, Some(AgentState::Executing { step: 1 })));

        // Transition to Completed
        agent_loop.set_state(AgentState::Completed);
        let state = agent_loop.next_state();
        assert!(matches!(state, Some(AgentState::Completed)));
    }

    #[test]
    fn test_error_recovery_state() {
        let mut agent_loop = AgentLoop::new(100);

        agent_loop.set_state(AgentState::ErrorRecovery {
            error: "Connection failed".to_string(),
        });

        let state = agent_loop.next_state();
        match state {
            Some(AgentState::ErrorRecovery { error }) => {
                assert_eq!(error, "Connection failed");
            }
            _ => panic!("Expected ErrorRecovery state"),
        }
    }

    #[test]
    fn test_increment_step() {
        let mut agent_loop = AgentLoop::new(100);

        assert_eq!(agent_loop.current_step(), 0);

        agent_loop.increment_step();
        assert_eq!(agent_loop.current_step(), 1);

        agent_loop.increment_step();
        assert_eq!(agent_loop.current_step(), 2);

        agent_loop.increment_step();
        assert_eq!(agent_loop.current_step(), 3);
    }

    #[test]
    fn test_increment_step_updates_state_to_executing() {
        let mut agent_loop = AgentLoop::new(100);

        // Initially in Planning state
        agent_loop.increment_step();

        // After increment, should be in Executing state
        let state = agent_loop.next_state();
        assert!(matches!(state, Some(AgentState::Executing { step: 1 })));
    }

    #[test]
    fn test_zero_max_iterations() {
        let mut agent_loop = AgentLoop::new(0);

        // First call should fail immediately
        let state = agent_loop.next_state();
        assert!(matches!(
            state,
            Some(AgentState::Failed { reason }) if reason.contains("Max iterations")
        ));
    }

    #[test]
    fn test_large_max_iterations() {
        let mut agent_loop = AgentLoop::new(1_000_000);

        // Should handle large iteration counts
        for _ in 0..1000 {
            let state = agent_loop.next_state();
            assert!(state.is_some());
            assert!(!matches!(state, Some(AgentState::Failed { .. })));
        }
    }
}

// ============================================================================
// Planner Tests
// ============================================================================

mod planner_tests {
    use super::*;

    #[test]
    fn test_create_plan_structure() {
        let plan = Planner::create_plan("Fix bug in login", "User auth module");

        assert!(plan.contains("<task>"));
        assert!(plan.contains("</task>"));
        assert!(plan.contains("<context>"));
        assert!(plan.contains("</context>"));
    }

    #[test]
    fn test_create_plan_includes_task() {
        let plan = Planner::create_plan("Implement feature X", "");
        assert!(plan.contains("Implement feature X"));
    }

    #[test]
    fn test_create_plan_includes_context() {
        let plan = Planner::create_plan("", "Important context information");
        assert!(plan.contains("Important context information"));
    }

    #[test]
    fn test_create_plan_instructions() {
        let plan = Planner::create_plan("task", "context");
        assert!(plan.contains("step-by-step"));
        assert!(plan.contains("plan"));
    }

    #[test]
    fn test_create_plan_multiline_task() {
        let task = "Step 1: Do this\nStep 2: Do that\nStep 3: Finish";
        let plan = Planner::create_plan(task, "");

        assert!(plan.contains("Step 1"));
        assert!(plan.contains("Step 2"));
        assert!(plan.contains("Step 3"));
    }

    #[test]
    fn test_create_plan_special_characters() {
        let task = "Handle <xml> & \"quotes\" and 'apostrophes'";
        let plan = Planner::create_plan(task, "");

        assert!(plan.contains("<xml>"));
        assert!(plan.contains("&"));
        assert!(plan.contains("\"quotes\""));
    }

    #[test]
    fn test_analyze_prompt_includes_path() {
        let prompt = Planner::analyze_prompt("/home/user/project");

        assert!(prompt.contains("/home/user/project"));
    }

    #[test]
    fn test_analyze_prompt_includes_analysis_topics() {
        let prompt = Planner::analyze_prompt("./src");

        assert!(prompt.contains("Directory structure"));
        assert!(prompt.contains("Key files"));
        assert!(prompt.contains("Dependencies"));
        assert!(prompt.contains("Architecture"));
        assert!(prompt.contains("Entry points"));
    }

    #[test]
    fn test_review_prompt_includes_file_and_content() {
        let prompt = Planner::review_prompt("main.rs", "fn main() { println!(\"Hello\"); }");

        assert!(prompt.contains("main.rs"));
        assert!(prompt.contains("fn main()"));
        assert!(prompt.contains("println!"));
    }

    #[test]
    fn test_review_prompt_includes_review_topics() {
        let prompt = Planner::review_prompt("test.rs", "code");

        assert!(prompt.contains("bugs"));
        assert!(prompt.contains("quality"));
        assert!(prompt.contains("Security"));
        assert!(prompt.contains("Performance"));
        assert!(prompt.contains("Documentation"));
    }

    #[test]
    fn test_review_prompt_with_large_content() {
        let large_content = "fn test() {\n".repeat(1000);
        let prompt = Planner::review_prompt("large.rs", &large_content);

        assert!(prompt.contains("large.rs"));
        assert!(prompt.contains("fn test()"));
    }
}

// ============================================================================
// ContextCompressor Tests
// ============================================================================

mod context_compressor_tests {
    use super::*;
    use selfware::api::types::Message;

    fn create_message(content: &str) -> Message {
        Message::user(content.to_string())
    }

    fn create_messages(count: usize, content_size: usize) -> Vec<Message> {
        let content = "x".repeat(content_size);
        (0..count).map(|_| create_message(&content)).collect()
    }

    #[test]
    fn test_new_creates_compressor() {
        let compressor = ContextCompressor::new(10000);
        // Just verify it creates without panic
        let _ = compressor;
    }

    #[test]
    fn test_should_compress_empty_messages() {
        let compressor = ContextCompressor::new(10000);
        let messages: Vec<Message> = vec![];
        // Empty messages should not need compression
        assert!(!compressor.should_compress(&messages));
    }

    #[test]
    fn test_should_compress_small_messages() {
        let compressor = ContextCompressor::new(10000);
        let messages = vec![create_message("Hello"), create_message("World")];
        // Small messages should not need compression
        assert!(!compressor.should_compress(&messages));
    }

    #[test]
    fn test_should_compress_large_messages() {
        let compressor = ContextCompressor::new(1000);
        // Create messages that exceed threshold (85% of 1000 = 850 tokens)
        // Each message gets ~50 base tokens + chars/4
        // So 20 messages with 200 chars each = 20 * (50 + 50) = 2000 tokens
        let messages = create_messages(20, 200);
        assert!(compressor.should_compress(&messages));
    }

    #[test]
    fn test_estimate_tokens_empty() {
        let compressor = ContextCompressor::new(10000);
        let messages: Vec<Message> = vec![];
        assert_eq!(compressor.estimate_tokens(&messages), 0);
    }

    #[test]
    fn test_estimate_tokens_single_message() {
        let compressor = ContextCompressor::new(10000);
        let messages = vec![create_message("Hello world")];
        let tokens = compressor.estimate_tokens(&messages);
        // Should have base overhead + content tokens
        assert!(tokens > 0);
        assert!(tokens < 100);
    }

    #[test]
    fn test_estimate_tokens_code_content() {
        let compressor = ContextCompressor::new(10000);
        let code = r#"fn main() { let x = 42; println!("{}", x); }"#;
        let messages = vec![create_message(code)];
        let tokens = compressor.estimate_tokens(&messages);
        // Code with braces/semicolons uses factor of 3 instead of 4
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_multiple_messages() {
        let compressor = ContextCompressor::new(10000);
        let messages = vec![
            create_message("First message"),
            create_message("Second message"),
            create_message("Third message"),
        ];
        let tokens = compressor.estimate_tokens(&messages);
        // Should be roughly 3x single message
        let single = compressor.estimate_tokens(&messages[0..1]);
        assert!(tokens > single);
        assert!(tokens < single * 5); // Not exactly 3x due to overhead
    }

    #[test]
    fn test_estimate_tokens_large_content() {
        let compressor = ContextCompressor::new(10000);
        let large_content = "a".repeat(10000);
        let messages = vec![create_message(&large_content)];
        let tokens = compressor.estimate_tokens(&messages);
        // 10000 chars / 4 + 50 overhead = ~2550 tokens
        assert!(tokens > 2000);
        assert!(tokens < 5000);
    }

    #[test]
    fn test_compression_threshold_calculation() {
        // Threshold is 85% of budget
        let compressor = ContextCompressor::new(10000);

        // Create messages just under threshold
        let small = create_messages(5, 100);
        assert!(!compressor.should_compress(&small));

        // Create messages over threshold
        let large = create_messages(100, 500);
        assert!(compressor.should_compress(&large));
    }
}

// ============================================================================
// AgentState Enum Tests
// ============================================================================

mod agent_state_tests {
    use super::*;

    #[test]
    fn test_state_debug_output() {
        let planning = AgentState::Planning;
        assert!(format!("{:?}", planning).contains("Planning"));

        let executing = AgentState::Executing { step: 5 };
        let debug = format!("{:?}", executing);
        assert!(debug.contains("Executing"));
        assert!(debug.contains("5"));

        let error = AgentState::ErrorRecovery {
            error: "test error".to_string(),
        };
        let debug = format!("{:?}", error);
        assert!(debug.contains("ErrorRecovery"));
        assert!(debug.contains("test error"));
    }

    #[test]
    fn test_state_clone() {
        let original = AgentState::Failed {
            reason: "original reason".to_string(),
        };
        let cloned = original.clone();

        match (original, cloned) {
            (AgentState::Failed { reason: r1 }, AgentState::Failed { reason: r2 }) => {
                assert_eq!(r1, r2);
            }
            _ => panic!("Clone failed"),
        }
    }

    #[test]
    fn test_all_state_variants() {
        let states = vec![
            AgentState::Planning,
            AgentState::Executing { step: 0 },
            AgentState::Executing { step: 100 },
            AgentState::ErrorRecovery {
                error: String::new(),
            },
            AgentState::Completed,
            AgentState::Failed {
                reason: String::new(),
            },
        ];

        // All states should be clonable and debuggable
        for state in states {
            let _ = state.clone();
            let _ = format!("{:?}", state);
        }
    }
}
