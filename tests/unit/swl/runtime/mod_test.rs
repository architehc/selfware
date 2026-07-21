use super::*;

#[test]
fn test_execution_context() {
    let mut ctx = ExecutionContext::new();
    ctx.set("key".to_string(), "value".to_string());
    assert_eq!(ctx.get("key"), Some("value".to_string()));
}

#[test]
fn test_execution_status() {
    assert_eq!(ExecutionStatus::Completed, ExecutionStatus::Completed);
    assert_ne!(ExecutionStatus::Completed, ExecutionStatus::Failed);
}

#[test]
fn test_agent_telemetry_default() {
    let telemetry = AgentTelemetry::default();
    assert_eq!(telemetry.total_calls, 0);
    assert_eq!(telemetry.total_tokens_in, 0);
    assert_eq!(telemetry.total_tokens_out, 0);
    assert_eq!(telemetry.total_latency_ms, 0);
    assert_eq!(telemetry.avg_latency_ms, 0.0);
}

#[test]
fn test_workflow_telemetry_default() {
    let telemetry = WorkflowTelemetry::default();
    assert_eq!(telemetry.workflow_duration_ms, 0);
    assert!(telemetry.agent_metrics.is_empty());
    assert_eq!(telemetry.total_tokens, 0);
    assert_eq!(telemetry.total_api_calls, 0);
}

#[tokio::test]
async fn test_telemetry_methods_with_dry_run() {
    let runtime = SwlRuntime::new_dry_run();

    // Test get_execution_trace returns empty initially
    let trace = runtime.get_execution_trace().await;
    assert!(trace.is_empty());

    // Test get_telemetry_summary returns default
    let summary = runtime.get_telemetry_summary().await;
    assert_eq!(summary.total_api_calls, 0);
    assert!(summary.agent_metrics.is_empty());

    // Test get_agent_trace returns empty for non-existent agent
    let agent_trace = runtime.get_agent_trace("test_agent").await;
    assert!(agent_trace.is_empty());
}

#[tokio::test]
async fn execute_tool_call_blocks_dangerous_shell_command() {
    // Regression test: an SWL workflow step used to be able to invoke
    // shell_exec (or any other registered tool) with none of the
    // src/safety/ checks applied at all.
    let runtime = SwlRuntime::new_dry_run();
    let call = crate::api::types::ToolCall {
        id: "t1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "shell_exec".to_string(),
            arguments: r#"{"command":"rm -rf /"}"#.to_string(),
        },
    };
    let available = vec!["shell_exec".to_string()];

    let err = runtime
        .execute_tool_call(&call, &available)
        .await
        .expect_err("dangerous shell command must be blocked");
    assert!(err.to_string().contains("Dangerous command"));
}

#[tokio::test]
async fn execute_tool_call_allows_safe_shell_command() {
    let runtime = SwlRuntime::new_dry_run();
    let call = crate::api::types::ToolCall {
        id: "t2".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "shell_exec".to_string(),
            arguments: r#"{"command":"echo hello"}"#.to_string(),
        },
    };
    let available = vec!["shell_exec".to_string()];

    let result = runtime.execute_tool_call(&call, &available).await;
    assert!(result.is_ok(), "expected safe command to run: {result:?}");
}

#[tokio::test]
async fn test_clear_telemetry() {
    let runtime = SwlRuntime::new_dry_run();

    // Add a synthetic event to the trace
    {
        let mut ctx = runtime.context.lock().await;
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "test".to_string(),
            action: "test".to_string(),
            duration_ms: 100,
            tokens_in: 10,
            tokens_out: 20,
        });
    }

    // Verify trace is not empty
    let trace = runtime.get_execution_trace().await;
    assert_eq!(trace.len(), 1);

    // Clear telemetry
    runtime.clear_telemetry().await;

    // Verify trace is empty
    let trace = runtime.get_execution_trace().await;
    assert!(trace.is_empty());
}

#[tokio::test]
async fn test_telemetry_summary_aggregation() {
    let runtime = SwlRuntime::new_dry_run();

    // Add synthetic events to the trace
    {
        let mut ctx = runtime.context.lock().await;
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "agent1".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 100,
            tokens_in: 10,
            tokens_out: 20,
        });
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "agent1".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 200,
            tokens_in: 20,
            tokens_out: 30,
        });
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "agent2".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 150,
            tokens_in: 15,
            tokens_out: 25,
        });
    }

    // Get summary
    let summary = runtime.get_telemetry_summary().await;

    // Verify totals
    assert_eq!(summary.total_api_calls, 3);
    assert_eq!(summary.total_tokens, 120); // (10+20) + (20+30) + (15+25)

    // Verify agent1 metrics
    let agent1 = summary
        .agent_metrics
        .get("agent1")
        .expect("agent1 should exist");
    assert_eq!(agent1.total_calls, 2);
    assert_eq!(agent1.total_tokens_in, 30);
    assert_eq!(agent1.total_tokens_out, 50);
    assert_eq!(agent1.total_latency_ms, 300);
    assert_eq!(agent1.avg_latency_ms, 150.0);

    // Verify agent2 metrics
    let agent2 = summary
        .agent_metrics
        .get("agent2")
        .expect("agent2 should exist");
    assert_eq!(agent2.total_calls, 1);
    assert_eq!(agent2.total_tokens_in, 15);
    assert_eq!(agent2.total_tokens_out, 25);
    assert_eq!(agent2.total_latency_ms, 150);
    assert_eq!(agent2.avg_latency_ms, 150.0);
}

#[tokio::test]
async fn test_get_agent_trace() {
    let runtime = SwlRuntime::new_dry_run();

    // Add synthetic events
    {
        let mut ctx = runtime.context.lock().await;
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "agent1".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 100,
            tokens_in: 10,
            tokens_out: 20,
        });
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "agent2".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 200,
            tokens_in: 20,
            tokens_out: 30,
        });
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "agent1".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 150,
            tokens_in: 15,
            tokens_out: 25,
        });
    }

    // Get agent1 trace
    let agent1_trace = runtime.get_agent_trace("agent1").await;
    assert_eq!(agent1_trace.len(), 2);
    assert_eq!(agent1_trace[0].duration_ms, 100);
    assert_eq!(agent1_trace[1].duration_ms, 150);

    // Get agent2 trace
    let agent2_trace = runtime.get_agent_trace("agent2").await;
    assert_eq!(agent2_trace.len(), 1);
    assert_eq!(agent2_trace[0].duration_ms, 200);
}

#[tokio::test]
async fn test_export_telemetry_json() {
    let runtime = SwlRuntime::new_dry_run();

    // Add a synthetic event
    {
        let mut ctx = runtime.context.lock().await;
        ctx.trace.push(ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "test_agent".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 100,
            tokens_in: 10,
            tokens_out: 20,
        });
    }

    // Export to JSON
    let json = runtime.export_telemetry_json().await;
    assert!(json.is_ok());

    let json_str = json.unwrap();
    assert!(json_str.contains("test_agent"));
    assert!(json_str.contains("total_tokens"));
    assert!(json_str.contains("total_api_calls"));
}

#[test]
fn test_execution_event_creation() {
    let event = ExecutionEvent {
        timestamp: std::time::Instant::now(),
        agent: "test_agent".to_string(),
        action: "llm_call".to_string(),
        duration_ms: 150,
        tokens_in: 50,
        tokens_out: 100,
    };

    assert_eq!(event.agent, "test_agent");
    assert_eq!(event.action, "llm_call");
    assert_eq!(event.duration_ms, 150);
    assert_eq!(event.tokens_in, 50);
    assert_eq!(event.tokens_out, 100);
}

#[test]
fn test_get_agent_tools() {
    let runtime = SwlRuntime::new_dry_run();

    // Agent with specific tools
    let agent = AgentDefinition {
        model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
        role: None,
        instruction: None,
        tools: vec!["file_read".to_string(), "file_write".to_string()],
        output_key: None,
        sub_agents: vec![],
    };

    let tools = runtime.get_agent_tools(&agent);
    assert_eq!(tools.len(), 2);
    assert!(tools.contains(&"file_read".to_string()));
    assert!(tools.contains(&"file_write".to_string()));
}

#[test]
fn test_get_agent_tools_empty() {
    let runtime = SwlRuntime::new_dry_run();

    // Agent with no tools
    let agent = AgentDefinition {
        model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
        role: None,
        instruction: None,
        tools: vec![],
        output_key: None,
        sub_agents: vec![],
    };

    let tools = runtime.get_agent_tools(&agent);
    assert!(tools.is_empty());
}

#[test]
fn test_build_system_prompt_with_tools() {
    let runtime = SwlRuntime::new_dry_run();

    let agent = AgentDefinition {
        model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
        role: Some("You are a coding assistant.".to_string()),
        instruction: None,
        tools: vec!["file_read".to_string()],
        output_key: None,
        sub_agents: vec![],
    };

    let tools = vec!["file_read".to_string()];
    let prompt = runtime.build_system_prompt(&agent, &tools);

    assert!(prompt.contains("You are a coding assistant."));
    assert!(prompt.contains("file_read"));
    assert!(prompt.contains("<tool>"));
    assert!(prompt.contains("</tool>"));
}

#[test]
fn test_build_system_prompt_no_tools() {
    let runtime = SwlRuntime::new_dry_run();

    let agent = AgentDefinition {
        model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
        role: Some("You are a helpful assistant.".to_string()),
        instruction: None,
        tools: vec![],
        output_key: None,
        sub_agents: vec![],
    };

    let tools: Vec<String> = vec![];
    let prompt = runtime.build_system_prompt(&agent, &tools);

    assert_eq!(prompt, "You are a helpful assistant.");
}

#[test]
fn test_build_tool_definitions() {
    let runtime = SwlRuntime::new_dry_run();

    let tool_names = vec!["file_read".to_string(), "shell_exec".to_string()];
    let definitions = runtime.build_tool_definitions(&tool_names);

    assert_eq!(definitions.len(), 2);

    let names: Vec<&str> = definitions
        .iter()
        .map(|d| d.function.name.as_str())
        .collect();
    assert!(names.contains(&"file_read"));
    assert!(names.contains(&"shell_exec"));
}

#[test]
fn test_build_tool_definitions_empty() {
    let runtime = SwlRuntime::new_dry_run();

    let tool_names: Vec<String> = vec![];
    let definitions = runtime.build_tool_definitions(&tool_names);

    assert!(definitions.is_empty());
}

#[test]
fn test_max_tool_iterations_config() {
    let runtime = SwlRuntime::new_dry_run().with_max_tool_iterations(100);

    assert_eq!(runtime.max_tool_iterations, 100);
}
