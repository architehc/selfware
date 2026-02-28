use super::*;
use crate::api::types::{ToolCall, ToolFunction};
use crate::config::{Config, ExecutionMode};
use crate::errors::AgentError;
use crate::testing::mock_api::MockLlmServer;
use crate::tool_parser::parse_tool_calls;
use loop_control::{AgentLoop, AgentState};

// =========================================================================
// Test 1: Agent State Transitions
// =========================================================================

#[test]
fn test_agent_state_transitions_idle_to_planning() {
    // AgentLoop starts in Planning state (not Idle, as there's no Idle state)
    let mut loop_ctrl = AgentLoop::new(100);

    // First state should be Planning
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Planning)));

    // Transition to Executing
    loop_ctrl.set_state(AgentState::Executing { step: 0 });
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Executing { step: 0 })));
}

#[test]
fn test_agent_state_transitions_planning_to_executing() {
    let mut loop_ctrl = AgentLoop::new(100);

    // Start in Planning
    let _ = loop_ctrl.next_state();
    assert!(matches!(loop_ctrl.next_state(), Some(AgentState::Planning)));

    // Transition to Executing with step 0
    loop_ctrl.set_state(AgentState::Executing { step: 0 });
    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::Executing { step }) => assert_eq!(step, 0),
        _ => panic!("Expected Executing state with step 0"),
    }
}

#[test]
fn test_agent_state_transitions_executing_to_completed() {
    let mut loop_ctrl = AgentLoop::new(100);

    // Start execution
    loop_ctrl.set_state(AgentState::Executing { step: 0 });
    let _ = loop_ctrl.next_state();

    // Simulate task completion
    loop_ctrl.set_state(AgentState::Completed);
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Completed)));
}

#[test]
fn test_agent_state_transitions_executing_to_error_recovery() {
    let mut loop_ctrl = AgentLoop::new(100);

    // Start execution
    loop_ctrl.set_state(AgentState::Executing { step: 0 });
    let _ = loop_ctrl.next_state();

    // Simulate error
    loop_ctrl.set_state(AgentState::ErrorRecovery {
        error: "Tool execution failed".to_string(),
    });
    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::ErrorRecovery { error }) => {
            assert_eq!(error, "Tool execution failed");
        }
        _ => panic!("Expected ErrorRecovery state"),
    }
}

#[test]
fn test_agent_state_full_lifecycle() {
    let mut loop_ctrl = AgentLoop::new(100);

    // Planning -> Executing -> Error -> Recovery -> Executing -> Completed
    assert!(matches!(loop_ctrl.next_state(), Some(AgentState::Planning)));

    loop_ctrl.set_state(AgentState::Executing { step: 0 });
    assert!(matches!(
        loop_ctrl.next_state(),
        Some(AgentState::Executing { .. })
    ));

    loop_ctrl.set_state(AgentState::ErrorRecovery {
        error: "test".to_string(),
    });
    assert!(matches!(
        loop_ctrl.next_state(),
        Some(AgentState::ErrorRecovery { .. })
    ));

    loop_ctrl.set_state(AgentState::Executing { step: 1 });
    assert!(matches!(
        loop_ctrl.next_state(),
        Some(AgentState::Executing { step: 1 })
    ));

    loop_ctrl.set_state(AgentState::Completed);
    assert!(matches!(
        loop_ctrl.next_state(),
        Some(AgentState::Completed)
    ));
}

// =========================================================================
// Test 2: Tool Call Handling with Mock Data
// =========================================================================

fn create_mock_tool_call(name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: format!("call_{}", uuid::Uuid::new_v4()),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }
}

fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        agent: crate::config::AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 5,
            streaming,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn test_agent_run_task_e2e_tool_workflow_with_mock_api() {
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Task complete: read finished.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Read Cargo.toml and finish").await;
    assert!(result.is_ok(), "run_task should succeed with mock API");
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.content.contains("<tool_result>")),
        "agent should have executed at least one tool call"
    );
    assert!(
        agent
            .context_files
            .iter()
            .any(|p| p.ends_with("Cargo.toml")),
        "file_read should add Cargo.toml to context tracking"
    );
    assert!(agent.last_assistant_response.contains("Task complete"));

    server.stop().await;
}

#[tokio::test]
async fn test_agent_run_task_streaming_fallback_to_non_streaming() {
    let server = MockLlmServer::builder()
        .with_response("Plan: answer directly.")
        .with_error(503, r#"{"error":"temporary stream failure"}"#)
        .with_response("Fallback completed successfully.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), true);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Respond with a short completion").await;
    assert!(
        result.is_ok(),
        "run_task should recover by falling back to non-streaming chat"
    );
    assert!(agent.last_assistant_response.contains("Fallback completed"));

    server.stop().await;
}

#[test]
fn test_tool_call_parsing_xml_format() {
    let content = r#"
        Let me read that file for you.

        <tool>
        <name>file_read</name>
        <arguments>{"path": "./src/main.rs"}</arguments>
        </tool>
        "#;

    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");

    let args = &result.tool_calls[0].arguments;
    assert_eq!(args["path"], "./src/main.rs");
}

#[test]
fn test_tool_call_parsing_multiple_tools() {
    let content = r#"
        I'll check the git status and read a file.

        <tool>
        <name>git_status</name>
        <arguments>{}</arguments>
        </tool>

        <tool>
        <name>file_read</name>
        <arguments>{"path": "Cargo.toml"}</arguments>
        </tool>
        "#;

    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 2);
    assert_eq!(result.tool_calls[0].tool_name, "git_status");
    assert_eq!(result.tool_calls[1].tool_name, "file_read");
}

#[test]
fn test_tool_call_with_complex_arguments() {
    let content = r#"
        <tool>
        <name>file_edit</name>
        <arguments>{
            "path": "./src/lib.rs",
            "old_str": "fn old_function() {\n    println!(\"old\");\n}",
            "new_str": "fn new_function() {\n    println!(\"new\");\n}"
        }</arguments>
        </tool>
        "#;

    let result = parse_tool_calls(content);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_edit");

    let args = &result.tool_calls[0].arguments;
    assert!(args["old_str"].as_str().unwrap().contains("old_function"));
    assert!(args["new_str"].as_str().unwrap().contains("new_function"));
}

#[test]
fn test_tool_call_no_tools_in_content() {
    let content = "This is just a regular response without any tool calls.";

    let result = parse_tool_calls(content);
    assert!(result.tool_calls.is_empty());
    assert!(!result.text_content.is_empty());
}

#[test]
fn test_mock_tool_call_creation() {
    let call = create_mock_tool_call("shell_exec", r#"{"command": "ls -la"}"#);
    assert_eq!(call.function.name, "shell_exec");
    assert!(call.function.arguments.contains("ls -la"));
    assert_eq!(call.call_type, "function");
    assert!(call.id.starts_with("call_"));
}

// =========================================================================
// Test 3: Error Recovery Scenarios
// =========================================================================

#[test]
fn test_error_recovery_state_preserves_error_message() {
    let mut loop_ctrl = AgentLoop::new(100);

    let error_message = "Connection timeout while calling external API";
    loop_ctrl.set_state(AgentState::ErrorRecovery {
        error: error_message.to_string(),
    });

    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::ErrorRecovery { error }) => {
            assert_eq!(error, error_message);
        }
        _ => panic!("Expected ErrorRecovery state"),
    }
}

#[test]
fn test_error_recovery_transitions_back_to_executing() {
    let mut loop_ctrl = AgentLoop::new(100);

    // Enter error recovery
    loop_ctrl.set_state(AgentState::ErrorRecovery {
        error: "some error".to_string(),
    });
    let _ = loop_ctrl.next_state();

    // Transition back to executing after recovery
    let current_step = loop_ctrl.current_step();
    loop_ctrl.set_state(AgentState::Executing { step: current_step });
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Executing { .. })));
}

#[test]
fn test_error_recovery_can_transition_to_failed() {
    let mut loop_ctrl = AgentLoop::new(100);

    // Enter error recovery
    loop_ctrl.set_state(AgentState::ErrorRecovery {
        error: "unrecoverable error".to_string(),
    });
    let _ = loop_ctrl.next_state();

    // If recovery fails, transition to Failed
    loop_ctrl.set_state(AgentState::Failed {
        reason: "Max retries exceeded".to_string(),
    });
    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::Failed { reason }) => {
            assert_eq!(reason, "Max retries exceeded");
        }
        _ => panic!("Expected Failed state"),
    }
}

#[test]
fn test_confirmation_error_detection() {
    // Case 1: Wrapped in SelfwareError::Agent
    let error = crate::errors::SelfwareError::Agent(AgentError::ConfirmationRequired {
        tool_name: "shell_exec".to_string(),
    });
    let anyhow_error: anyhow::Error = error.into();
    assert!(is_confirmation_error(&anyhow_error));

    // Case 2: AgentError returned directly into anyhow (as in execution.rs non-interactive path)
    let direct_error: anyhow::Error = AgentError::ConfirmationRequired {
        tool_name: "shell_exec".to_string(),
    }
    .into();
    assert!(is_confirmation_error(&direct_error));
}

#[test]
fn test_non_confirmation_error_detection() {
    let error = anyhow::anyhow!("Some other error");
    assert!(!is_confirmation_error(&error));
}

// =========================================================================
// Test 4: Context Compression Triggers
// =========================================================================

#[test]
fn test_context_compressor_threshold_calculation() {
    let compressor = ContextCompressor::new(100000);
    // Threshold is 85% of budget
    assert!(!compressor.should_compress(&[]));

    // Create messages that exceed threshold
    let mut large_messages = vec![Message::system("System prompt")];
    for _ in 0..100 {
        large_messages.push(Message::user("x".repeat(1000)));
    }

    // With 100 messages of ~1000 chars each, this should trigger compression
    let compressor_small = ContextCompressor::new(10000);
    assert!(compressor_small.should_compress(&large_messages));
}

#[test]
fn test_context_compressor_estimate_tokens() {
    let compressor = ContextCompressor::new(100000);

    let messages = vec![
        Message::system("You are a helpful assistant"),
        Message::user("Hello, how are you?"),
        Message::assistant("I'm doing well, thank you!"),
    ];

    let estimate = compressor.estimate_tokens(&messages);
    // Should have reasonable estimate (base cost + content)
    assert!(estimate > 150); // 3 messages * ~50 base minimum
    assert!(estimate < 500); // Shouldn't be too high for short messages
}

#[test]
fn test_context_compressor_code_content_factor() {
    let compressor = ContextCompressor::new(100000);

    // Code content (with braces) uses factor 3
    let code_msg = vec![Message::user("fn main() { println!(\"hello\"); }")];

    // Plain text uses factor 4
    let text_msg = vec![Message::user("This is plain text content")];

    let code_estimate = compressor.estimate_tokens(&code_msg);
    let text_estimate = compressor.estimate_tokens(&text_msg);

    // Both should have reasonable estimates
    assert!(code_estimate > 50);
    assert!(text_estimate > 50);
}

#[test]
fn test_hard_compress_preserves_structure() {
    let compressor = ContextCompressor::new(100000);

    let messages = vec![
        Message::system("system prompt"),
        Message::user("question 1"),
        Message::assistant("answer 1"),
        Message::user("question 2"),
        Message::assistant("answer 2"),
        Message::user("recent question"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // Should preserve system message
    assert_eq!(compressed[0].role, "system");

    // Should end with user message
    let last = compressed.last().unwrap();
    assert_eq!(last.role, "user");
}

// =========================================================================
// Test 5: Execution Mode and Tool Confirmation
// =========================================================================

#[test]
fn test_execution_mode_normal_needs_confirmation() {
    let config = Config {
        execution_mode: ExecutionMode::Normal,
        ..Default::default()
    };

    // In normal mode, most tools need confirmation
    // Safe tools (read-only) don't need confirmation
    let safe_tools = [
        "file_read",
        "directory_tree",
        "glob_find",
        "grep_search",
        "symbol_search",
        "git_status",
        "git_diff",
    ];

    for tool in &safe_tools {
        // Safe tools shouldn't need confirmation even in normal mode
        assert!(
            !needs_confirmation_for_tool(&config, tool),
            "{} should not need confirmation",
            tool
        );
    }

    // Dangerous tools need confirmation in normal mode
    let dangerous_tools = ["shell_exec", "file_write", "git_commit"];
    for tool in &dangerous_tools {
        assert!(
            needs_confirmation_for_tool(&config, tool),
            "{} should need confirmation",
            tool
        );
    }
}

#[test]
fn test_execution_mode_yolo_no_confirmation() {
    let config = Config {
        execution_mode: ExecutionMode::Yolo,
        ..Default::default()
    };

    // In YOLO mode, nothing needs confirmation
    let all_tools = [
        "file_read",
        "file_write",
        "shell_exec",
        "git_commit",
        "cargo_test",
    ];

    for tool in &all_tools {
        assert!(
            !needs_confirmation_for_tool(&config, tool),
            "{} should not need confirmation in YOLO mode",
            tool
        );
    }
}

#[test]
fn test_execution_mode_auto_edit_file_ops() {
    let config = Config {
        execution_mode: ExecutionMode::AutoEdit,
        ..Default::default()
    };

    // Auto-edit mode auto-approves file operations
    assert!(!needs_confirmation_for_tool(&config, "file_write"));
    assert!(!needs_confirmation_for_tool(&config, "file_edit"));

    // But still asks for other operations
    assert!(needs_confirmation_for_tool(&config, "shell_exec"));
    assert!(needs_confirmation_for_tool(&config, "git_commit"));
}

#[test]
fn test_execution_mode_cycle() {
    let mut mode = ExecutionMode::Normal;

    // Normal -> AutoEdit
    mode = cycle_mode(mode);
    assert_eq!(mode, ExecutionMode::AutoEdit);

    // AutoEdit -> Yolo
    mode = cycle_mode(mode);
    assert_eq!(mode, ExecutionMode::Yolo);

    // Yolo -> Normal
    mode = cycle_mode(mode);
    assert_eq!(mode, ExecutionMode::Normal);
}

// Helper function to check confirmation without full Agent
fn needs_confirmation_for_tool(config: &Config, tool_name: &str) -> bool {
    let safe_tools = [
        "file_read",
        "directory_tree",
        "glob_find",
        "grep_search",
        "symbol_search",
        "git_status",
        "git_diff",
    ];

    if safe_tools.contains(&tool_name) {
        return false;
    }

    if matches!(
        config.execution_mode,
        ExecutionMode::Yolo | ExecutionMode::Daemon
    ) {
        return false;
    }

    // Check config's require_confirmation list
    if config
        .safety
        .require_confirmation
        .iter()
        .any(|t| t == tool_name)
    {
        return true;
    }

    match config.execution_mode {
        ExecutionMode::Yolo | ExecutionMode::Daemon => false,
        ExecutionMode::AutoEdit => !matches!(
            tool_name,
            "file_write" | "file_edit" | "directory_tree" | "glob_find"
        ),
        ExecutionMode::Normal => !safe_tools.contains(&tool_name),
    }
}

// Helper function to cycle execution mode
fn cycle_mode(mode: ExecutionMode) -> ExecutionMode {
    match mode {
        ExecutionMode::Normal => ExecutionMode::AutoEdit,
        ExecutionMode::AutoEdit => ExecutionMode::Yolo,
        ExecutionMode::Yolo => ExecutionMode::Normal,
        ExecutionMode::Daemon => ExecutionMode::Normal,
    }
}

// =========================================================================
// Additional Edge Case Tests
// =========================================================================

#[test]
fn test_agent_error_display() {
    let error = AgentError::ConfirmationRequired {
        tool_name: "dangerous_tool".to_string(),
    };
    let display = format!("{}", error);
    assert!(display.contains("dangerous_tool"));
    assert!(display.contains("requires confirmation"));
}

#[test]
fn test_max_iterations_triggers_failure() {
    let mut loop_ctrl = AgentLoop::new(3);

    // Use up all iterations
    loop_ctrl.next_state(); // 1
    loop_ctrl.next_state(); // 2
    loop_ctrl.next_state(); // 3

    // Next should fail
    let state = loop_ctrl.next_state();
    assert!(matches!(
        state,
        Some(AgentState::Failed { reason }) if reason.contains("Max iterations")
    ));
}

#[test]
fn test_step_increment_updates_state() {
    let mut loop_ctrl = AgentLoop::new(100);

    assert_eq!(loop_ctrl.current_step(), 0);

    loop_ctrl.increment_step();
    assert_eq!(loop_ctrl.current_step(), 1);

    // State should be updated to Executing with new step
    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::Executing { step }) => assert_eq!(step, 1),
        _ => panic!("Expected Executing state with step 1"),
    }
}

#[test]
fn test_tool_call_with_invalid_json_uses_fallback() {
    let content = r#"
        <tool>
        <name>file_read</name>
        <arguments>this is not valid json</arguments>
        </tool>
        "#;

    let result = parse_tool_calls(content);
    // Parser uses fallback - wraps invalid JSON in {"input": "..."}
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].tool_name, "file_read");
    // The fallback wraps plain text in {"input": "..."}
    assert!(result.tool_calls[0].arguments.get("input").is_some());
}

#[test]
fn test_agent_state_clone() {
    let state = AgentState::Executing { step: 5 };
    let cloned = state.clone();

    match cloned {
        AgentState::Executing { step } => assert_eq!(step, 5),
        _ => panic!("Clone should preserve state type and data"),
    }
}

#[test]
fn test_agent_state_debug() {
    let state = AgentState::ErrorRecovery {
        error: "test error".to_string(),
    };
    let debug_str = format!("{:?}", state);

    assert!(debug_str.contains("ErrorRecovery"));
    assert!(debug_str.contains("test error"));
}

#[test]
fn test_infer_task_type() {
    assert_eq!(
        Agent::infer_task_type("Please review this PR"),
        "code_review"
    );
    assert_eq!(Agent::infer_task_type("Fix this bug"), "bug_fix");
    assert_eq!(Agent::infer_task_type("Write tests for module"), "testing");
}

#[test]
fn test_classify_error_type() {
    assert_eq!(Agent::classify_error_type("request timed out"), "timeout");
    assert_eq!(
        Agent::classify_error_type("permission denied"),
        "permission"
    );
    assert_eq!(
        Agent::classify_error_type("Invalid JSON in response"),
        "parsing"
    );
}

#[test]
fn test_outcome_quality_mapping() {
    assert_eq!(Agent::outcome_quality(Outcome::Success), 1.0);
    assert_eq!(Agent::outcome_quality(Outcome::Partial), 0.65);
    assert_eq!(Agent::outcome_quality(Outcome::Failure), 0.0);
    assert_eq!(Agent::outcome_quality(Outcome::Abandoned), 0.2);
}

// =========================================================================
// trim_message_history tests
// =========================================================================

/// Helper that mirrors `Agent::trim_message_history` logic so we can
/// verify the algorithm without constructing a full Agent instance.
fn trim_messages(messages: &mut Vec<Message>, max_tokens: usize) {
    let total: usize = messages
        .iter()
        .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
        .sum();
    if total <= max_tokens {
        return;
    }

    let token_counts: Vec<usize> = messages
        .iter()
        .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
        .collect();

    let mut remaining = total;
    let mut keep = vec![true; messages.len()];
    for (i, tokens) in token_counts.iter().enumerate() {
        if remaining <= max_tokens {
            break;
        }
        if messages[i].role != "system" {
            keep[i] = false;
            remaining -= tokens;
        }
    }

    let mut idx = 0;
    messages.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

#[test]
fn test_trim_message_history_no_trim_needed() {
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("hi"),
        Message::assistant("hello"),
    ];
    let before_len = msgs.len();
    trim_messages(&mut msgs, 100_000);
    assert_eq!(msgs.len(), before_len);
}

#[test]
fn test_trim_message_history_removes_oldest_non_system() {
    // Use long messages so the total clearly exceeds a small budget.
    let long = "x".repeat(500);
    let mut msgs = vec![
        Message::system("system prompt"),
        Message::user(&long),
        Message::assistant(&long),
        Message::user(&long),
        Message::assistant(&long),
    ];

    // Budget of 20 tokens forces almost everything to be trimmed.
    trim_messages(&mut msgs, 20);

    // System message must survive.
    assert_eq!(msgs[0].role, "system");
    // At least some non-system messages should have been removed.
    assert!(msgs.len() < 5);
}

#[test]
fn test_trim_message_history_preserves_system_only() {
    let mut msgs = vec![
        Message::system("system prompt"),
        Message::user("big message ".repeat(5000)),
    ];

    // Very tiny budget: should remove the user message but keep system
    trim_messages(&mut msgs, 30);

    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, "system");
}
