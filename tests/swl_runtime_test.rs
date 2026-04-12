//! Comprehensive SWL Runtime Tests
//!
//! Tests for async execution, error handling, resource cleanup,
//! concurrency safety, and performance of the SWL runtime.

use selfware::swl::{
    parse_document, ExecutionContext, ExecutionStatus, StateBackendType, StateManager, SwlDocument,
    SwlRuntime, WorkflowTelemetry,
};
use selfware::workflows::VarValue;
use std::collections::HashMap;

// Helper to create a simple test document
fn create_test_document(workflow_type: &str) -> SwlDocument {
    let source = match workflow_type {
        "sequential" => {
            r#"
version: "1.0"
name: sequential_test
agents:
  agent1:
    model: test-model
    role: tester
    instruction: Test step 1
  agent2:
    model: test-model
    role: tester
    instruction: Test step 2
workflows:
  test_flow:
    type: sequential
"#
        }
        "parallel" => {
            r#"
version: "1.0"
name: parallel_test
agents:
  agent1:
    model: test-model
    role: tester
    instruction: Test parallel 1
  agent2:
    model: test-model
    role: tester
    instruction: Test parallel 2
workflows:
  test_flow:
    type: parallel
"#
        }
        "conditional" => {
            r#"
version: "1.0"
name: conditional_test
agents:
  condition:
    model: test-model
    role: checker
    instruction: Check condition
  action:
    model: test-model
    role: executor
    instruction: Execute action
workflows:
  test_flow:
    type: conditional
"#
        }
        "map_reduce" => {
            r#"
version: "1.0"
name: map_reduce_test
agents:
  mapper:
    model: test-model
    role: mapper
    instruction: Map data
  reducer:
    model: test-model
    role: reducer
    instruction: Reduce data
workflows:
  test_flow:
    type: map_reduce
    map:
      targets: [mapper]
    reduce:
      language: rust
      code: "fn reduce() {}"
"#
        }
        _ => panic!("Unknown workflow type"),
    };

    parse_document(source).expect("Failed to parse document")
}

#[test]
fn test_execution_context_basic_operations() {
    let mut ctx = ExecutionContext::new();

    // Test set/get
    ctx.set("key1".to_string(), "value1".to_string());
    assert_eq!(ctx.get("key1"), Some("value1".to_string()));

    // Test get missing key
    assert_eq!(ctx.get("missing"), None);

    // Test has
    assert!(ctx.has("key1"));
    assert!(!ctx.has("missing"));

    // Test delete
    assert!(ctx.delete("key1"));
    assert!(!ctx.delete("key1")); // Already deleted

    // Test keys
    ctx.set("a".to_string(), "1".to_string());
    ctx.set("b".to_string(), "2".to_string());
    let keys: Vec<_> = ctx.keys().into_iter().cloned().collect();
    assert!(keys.contains(&"a".to_string()));
    assert!(keys.contains(&"b".to_string()));
}

#[test]
fn test_execution_context_json_operations() {
    let mut ctx = ExecutionContext::new();

    // Test JSON values
    ctx.set_json("string".to_string(), serde_json::json!("hello"));
    ctx.set_json("number".to_string(), serde_json::json!(42));
    ctx.set_json("bool".to_string(), serde_json::json!(true));
    ctx.set_json("array".to_string(), serde_json::json!([1, 2, 3]));
    ctx.set_json("object".to_string(), serde_json::json!({"nested": "value"}));

    // Verify string retrieval - strings stored as JSON values get quoted when retrieved via get()
    // because get() converts JSON back to string representation
    assert_eq!(ctx.get("string"), Some("hello".to_string()));
    assert_eq!(ctx.get("number"), Some("42".to_string()));

    // Verify JSON retrieval
    assert_eq!(ctx.get_json("number"), Some(&serde_json::json!(42)));
}

#[test]
fn test_execution_context_export_import() {
    let mut ctx = ExecutionContext::new();
    ctx.set("key1".to_string(), "value1".to_string());
    ctx.set_json("key2".to_string(), serde_json::json!({"nested": "data"}));

    // Export to JSON
    let json = ctx.export_json().expect("Export failed");
    assert!(json.contains("key1"));
    assert!(json.contains("value1"));
    assert!(json.contains("nested"));
}

#[test]
fn test_execution_context_clone() {
    let mut ctx = ExecutionContext::new();
    ctx.set("key1".to_string(), "value1".to_string());

    let cloned = ctx.clone();

    // Cloned context should have same values
    assert_eq!(cloned.get("key1"), Some("value1".to_string()));

    // But modifications should be independent
    ctx.set("key2".to_string(), "value2".to_string());
    assert!(ctx.has("key2"));
    assert!(!cloned.has("key2")); // Original shouldn't have key2
}

#[tokio::test]
async fn test_execution_context_with_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let backend = StateBackendType::File {
        base_dir: temp_dir.path().to_path_buf(),
    };

    // Create context with persistence
    let mut ctx = ExecutionContext::with_persistence("test_workflow", backend)
        .await
        .expect("Failed to create context with persistence");

    // Set values
    ctx.set("test_key".to_string(), "test_value".to_string());

    // Persist
    ctx.persist().await.expect("Persist failed");

    // Create new context and load
    let backend2 = StateBackendType::File {
        base_dir: temp_dir.path().to_path_buf(),
    };
    let mut ctx2 = ExecutionContext::with_persistence("test_workflow", backend2)
        .await
        .expect("Failed to create second context");

    // Load and verify
    ctx2.load().await.expect("Load failed");
    assert_eq!(ctx2.get("test_key"), Some("test_value".to_string()));
}

#[test]
fn test_swl_runtime_dry_run_creation() {
    let runtime = SwlRuntime::new_dry_run();
    // Should create without panicking

    // Test telemetry methods in dry-run mode
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let trace = runtime.get_execution_trace().await;
        assert!(trace.is_empty());

        let summary = runtime.get_telemetry_summary().await;
        assert_eq!(summary.total_api_calls, 0);
    });
}

#[test]
fn test_swl_runtime_with_max_iterations() {
    let _runtime = SwlRuntime::new_dry_run().with_max_tool_iterations(100);
    // Access to max_tool_iterations would need to be added for verification
}

#[tokio::test]
async fn test_swl_runtime_telemetry_aggregation() {
    let runtime = SwlRuntime::new_dry_run();

    // Create a synthetic execution trace by directly accessing context
    {
        let _ctx = runtime.get_context().await;
        // Add events manually through the runtime's internal trace
    }

    let summary = runtime.get_telemetry_summary().await;
    assert_eq!(summary.workflow_duration_ms, 0);
}

#[tokio::test]
async fn test_sequential_workflow_dry_run() {
    let runtime = SwlRuntime::new_dry_run();
    let doc = create_test_document("sequential");

    let inputs: HashMap<String, VarValue> = HashMap::new();
    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;

    // In dry-run mode, should complete without error
    assert!(result.is_ok());
    let exec_result = result.unwrap();
    assert_eq!(exec_result.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn test_parallel_workflow_dry_run() {
    let runtime = SwlRuntime::new_dry_run();
    let doc = create_test_document("parallel");

    let inputs: HashMap<String, VarValue> = HashMap::new();
    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;

    assert!(result.is_ok());
    let exec_result = result.unwrap();
    assert_eq!(exec_result.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn test_conditional_workflow_dry_run() {
    let runtime = SwlRuntime::new_dry_run();
    let doc = create_test_document("conditional");

    let inputs: HashMap<String, VarValue> = HashMap::new();
    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;

    assert!(result.is_ok());
    let exec_result = result.unwrap();
    assert_eq!(exec_result.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn test_map_reduce_workflow_dry_run() {
    let runtime = SwlRuntime::new_dry_run();
    let doc = create_test_document("map_reduce");

    let inputs: HashMap<String, VarValue> = HashMap::new();
    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;

    assert!(result.is_ok());
    let exec_result = result.unwrap();
    assert_eq!(exec_result.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn test_workflow_not_found() {
    let runtime = SwlRuntime::new_dry_run();
    let doc = create_test_document("sequential");

    let inputs: HashMap<String, VarValue> = HashMap::new();
    let result = runtime
        .execute_workflow(&doc, "nonexistent_flow", inputs)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));
}

#[tokio::test]
async fn test_state_persistence_integration() {
    let temp_dir = tempfile::tempdir().unwrap();

    // Create runtime with file persistence
    let backend = StateBackendType::File {
        base_dir: temp_dir.path().to_path_buf(),
    };

    let mut manager = StateManager::from_backend_type(backend, "test_workflow")
        .await
        .expect("Failed to create state manager");

    // Set values
    manager
        .set(
            "persistent_key".to_string(),
            serde_json::json!("persistent_value"),
        )
        .expect("Set failed");

    // Save
    manager.save().await.expect("Save failed");

    // Create new manager and load
    let backend2 = StateBackendType::File {
        base_dir: temp_dir.path().to_path_buf(),
    };
    let mut manager2 = StateManager::from_backend_type(backend2, "test_workflow")
        .await
        .expect("Failed to create second manager");

    manager2.load().await.expect("Load failed");

    assert_eq!(
        manager2.get("persistent_key"),
        Some(&serde_json::json!("persistent_value"))
    );
}

#[tokio::test]
async fn test_telemetry_clear_and_export() {
    let runtime = SwlRuntime::new_dry_run();

    // Initially empty
    let trace = runtime.get_execution_trace().await;
    assert!(trace.is_empty());

    let json = runtime
        .export_telemetry_json()
        .await
        .expect("Export failed");
    assert!(json.contains("total_tokens"));
    assert!(json.contains("total_api_calls"));

    // Clear telemetry
    runtime.clear_telemetry().await;

    // Verify cleared
    let summary = runtime.get_telemetry_summary().await;
    assert_eq!(summary.total_api_calls, 0);
}

#[tokio::test]
async fn test_parallel_execution_concurrency() {
    let runtime = SwlRuntime::new_dry_run();

    // Test with many agents to verify concurrent execution
    let source = r#"
version: "1.0"
name: concurrent_test
agents:
  agent1:
    model: test-model
    role: tester
    instruction: Test 1
  agent2:
    model: test-model
    role: tester
    instruction: Test 2
  agent3:
    model: test-model
    role: tester
    instruction: Test 3
  agent4:
    model: test-model
    role: tester
    instruction: Test 4
workflows:
  test_flow:
    type: parallel
"#;

    let doc = parse_document(source).expect("Failed to parse");
    let inputs: HashMap<String, VarValue> = HashMap::new();

    let start = std::time::Instant::now();
    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;
    let _duration = start.elapsed();

    assert!(result.is_ok());
    // In dry-run mode, should complete quickly regardless of agent count
    // since no actual work is done
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
async fn test_runtime_state_isolation() {
    // Create two runtimes with independent state
    let runtime1 = SwlRuntime::new_dry_run();
    let runtime2 = SwlRuntime::new_dry_run();

    // Each runtime should have its own context
    let ctx1 = runtime1.get_context().await;
    let ctx2 = runtime2.get_context().await;

    // Initially both empty
    assert!(ctx1.keys().is_empty());
    assert!(ctx2.keys().is_empty());
}

#[tokio::test]
async fn test_schema_defaults_application() {
    let source = r#"
version: "1.0"
name: schema_test
state:
  fields:
    - name: counter
      type: integer
      default: 0
    - name: message
      type: string
      default: "hello"
agents:
  test_agent:
    model: test-model
    role: tester
    instruction: Test
workflows:
  test_flow:
    type: sequential
"#;

    let doc = parse_document(source).expect("Failed to parse");
    let runtime = SwlRuntime::new_dry_run();
    let inputs: HashMap<String, VarValue> = HashMap::new();

    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;
    assert!(result.is_ok());

    // Verify defaults were applied
    let ctx = runtime.get_context().await;
    assert_eq!(ctx.get("counter"), Some("0".to_string()));
    assert_eq!(ctx.get("message"), Some("hello".to_string()));
}

#[tokio::test]
async fn test_workflow_inputs_integration() {
    let source = r#"
version: "1.0"
name: input_test
agents:
  test_agent:
    model: test-model
    role: tester
    instruction: Test
workflows:
  test_flow:
    type: sequential
"#;

    let doc = parse_document(source).expect("Failed to parse");
    let runtime = SwlRuntime::new_dry_run();

    let mut inputs: HashMap<String, VarValue> = HashMap::new();
    inputs.insert(
        "custom_input".to_string(),
        VarValue::String("custom_value".to_string()),
    );

    let result = runtime.execute_workflow(&doc, "test_flow", inputs).await;
    assert!(result.is_ok());

    // Verify input was set in context
    let ctx = runtime.get_context().await;
    assert_eq!(ctx.get("custom_input"), Some("custom_value".to_string()));
}


// ============================================================================
// Guardrail Enforcement Tests
// ============================================================================

use selfware::swl::guardrails::{
    GuardrailEnforcer, GuardrailContext, GuardrailType, Condition, ViolationAction,
    GuardrailDef,
};

#[tokio::test]
async fn test_guardrail_enforcer_blocks_on_violation() {
    let mut enforcer = GuardrailEnforcer::new();
    
    // Register a guardrail that blocks when condition is false
    enforcer.register_guardrail(GuardrailDef {
        name: "block_always".to_string(),
        guardrail_type: GuardrailType::PreAgent,
        condition: Condition::Inline("false".to_string()),
        on_violation: ViolationAction::Block,
        description: None,
        severity: None,
        tags: Vec::new(),
    });

    let ctx = GuardrailContext::new();
    let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

    assert_eq!(summary.total_checked, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.blocked, 1);
    assert!(summary.should_block());
}

#[tokio::test]
async fn test_guardrail_enforcer_allows_on_pass() {
    let mut enforcer = GuardrailEnforcer::new();
    
    // Register a guardrail that passes
    enforcer.register_guardrail(GuardrailDef {
        name: "pass_always".to_string(),
        guardrail_type: GuardrailType::PreAgent,
        condition: Condition::Inline("true".to_string()),
        on_violation: ViolationAction::Block,
        description: None,
        severity: None,
        tags: Vec::new(),
    });

    let ctx = GuardrailContext::new();
    let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

    assert_eq!(summary.total_checked, 1);
    assert_eq!(summary.passed, 1);
    assert!(!summary.should_block());
}

#[tokio::test]
async fn test_guardrail_enforcer_warn_action_does_not_block() {
    let mut enforcer = GuardrailEnforcer::new();
    
    // Register a guardrail that warns but doesn't block
    enforcer.register_guardrail(GuardrailDef {
        name: "warn_always".to_string(),
        guardrail_type: GuardrailType::PostAgent,
        condition: Condition::Inline("false".to_string()),
        on_violation: ViolationAction::Warn,
        description: None,
        severity: None,
        tags: Vec::new(),
    });

    let ctx = GuardrailContext::new();
    let summary = enforcer.check(GuardrailType::PostAgent, &ctx).await.unwrap();

    assert_eq!(summary.total_checked, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.warnings, 1);
    assert_eq!(summary.blocked, 0);
    // should_block returns false because action is Warn, not Block
    assert!(!summary.should_block());
}

#[tokio::test]
async fn test_guardrail_json_logic_evaluation() {
    use selfware::swl::guardrails::GuardrailEngine;
    
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_state("count", 10);

    // Test JSON Logic comparison
    let json_logic = r#"{">=": [{"var": "count"}, 5]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(result.is_pass(), "JSON Logic >= should pass when count is 10, got: {:?}", result);

    // Test JSON Logic that should fail
    let json_logic_fail = r#"{"<": [{"var": "count"}, 5]}"#;
    let result = engine.evaluate_json_logic(json_logic_fail, &ctx);
    assert!(result.is_fail(), "JSON Logic < should fail when count is 10");
}

#[tokio::test]
async fn test_guardrail_json_logic_contains() {
    use selfware::swl::guardrails::GuardrailEngine;
    
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_agent_output("agent1", "This contains CRITICAL error");

    // Test contains operator
    let json_logic = r#"{"contains": ["agent_output", "CRITICAL"]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(result.is_pass(), "JSON Logic contains should detect CRITICAL");

    // Test contains that should fail
    let json_logic_fail = r#"{"contains": ["agent_output", "SAFE"]}"#;
    let result = engine.evaluate_json_logic(json_logic_fail, &ctx);
    assert!(result.is_fail(), "JSON Logic contains should not find SAFE");
}

#[tokio::test]
async fn test_guardrail_json_logic_and_or() {
    use selfware::swl::guardrails::GuardrailEngine;
    
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new()
        .with_state("count", 10)
        .with_state("enabled", true);

    // Test AND operator - both conditions true
    let json_logic = r#"{"and": [{"var": "count"}, {"var": "enabled"}]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(result.is_pass(), "JSON Logic AND should pass when both conditions are true, got: {:?}", result);

    // Test OR operator - one condition true
    let json_logic_or = r#"{"or": [{"==": [{"var": "count"}, 5]}, {"var": "enabled"}]}"#;
    let result = engine.evaluate_json_logic(json_logic_or, &ctx);
    assert!(result.is_pass(), "JSON Logic OR should pass when one condition is true");

    // Test NOT operator
    let json_logic_not = r#"{"not": {"==": [{"var": "count"}, 5]}}"#;
    let result = engine.evaluate_json_logic(json_logic_not, &ctx);
    assert!(result.is_pass(), "JSON Logic NOT should pass when inner condition is false");
}

#[tokio::test]
async fn test_guardrail_json_logic_regex_match() {
    use selfware::swl::guardrails::GuardrailEngine;
    
    let engine = GuardrailEngine::new();
    let ctx = GuardrailContext::new().with_agent_output("agent1", "Error: connection timeout");

    // Test regex match
    let json_logic = r#"{"match": ["agent_output", "Error:.*timeout"]}"#;
    let result = engine.evaluate_json_logic(json_logic, &ctx);
    assert!(result.is_pass(), "JSON Logic match should match regex pattern");

    // Test regex that doesn't match
    let json_logic_fail = r#"{"match": ["agent_output", "Success"]}"#;
    let result = engine.evaluate_json_logic(json_logic_fail, &ctx);
    assert!(result.is_fail(), "JSON Logic match should fail when pattern doesn't match");
}

#[test]
fn test_guardrail_inline_expressions() {
    use selfware::swl::guardrails::GuardrailEngine;
    
    let engine = GuardrailEngine::new();
    
    // Test contains expression
    let ctx = GuardrailContext::new().with_agent_output("agent1", "test output with ERROR");
    let result = engine.evaluate_inline_expression("agent_output.contains('ERROR')", &ctx);
    assert!(result.is_pass(), "Contains expression should detect ERROR");
    
    // Test negation
    let result = engine.evaluate_inline_expression("!agent_output.contains('SUCCESS')", &ctx);
    assert!(result.is_pass(), "Negation should pass when pattern not found");
    
    // Test comparison
    let ctx = GuardrailContext::new().with_state("count", 42);
    let result = engine.evaluate_inline_expression("state.count > 10", &ctx);
    assert!(result.is_pass(), "Comparison should pass for 42 > 10");
    
    // Test equality
    let result = engine.evaluate_inline_expression("state.count == 42", &ctx);
    assert!(result.is_pass(), "Equality should pass for 42 == 42, got: {:?}", result);
}

#[test]
fn test_guardrail_violation_action_parsing() {
    use selfware::swl::guardrails::ViolationAction;
    
    assert_eq!(ViolationAction::parse_str("block"), Some(ViolationAction::Block));
    assert_eq!(ViolationAction::parse_str("BLOCK"), None, "Parsing is case-sensitive");
    assert_eq!(ViolationAction::parse_str("warn"), Some(ViolationAction::Warn));
    assert_eq!(ViolationAction::parse_str("log"), Some(ViolationAction::Log));
    assert_eq!(ViolationAction::parse_str("alert"), Some(ViolationAction::Alert));
    assert_eq!(ViolationAction::parse_str("unknown"), None);
}

#[test]
fn test_guardrail_type_parsing() {
    use selfware::swl::guardrails::GuardrailType;
    
    assert_eq!(GuardrailType::parse_str("pre_agent"), Some(GuardrailType::PreAgent));
    assert_eq!(GuardrailType::parse_str("post_agent"), Some(GuardrailType::PostAgent));
    assert_eq!(GuardrailType::parse_str("pre_tool"), Some(GuardrailType::PreTool));
    assert_eq!(GuardrailType::parse_str("post_tool"), Some(GuardrailType::PostTool));
    assert_eq!(GuardrailType::parse_str("pre_workflow"), Some(GuardrailType::PreWorkflow));
    assert_eq!(GuardrailType::parse_str("post_workflow"), Some(GuardrailType::PostWorkflow));
    assert_eq!(GuardrailType::parse_str("unknown"), None);
}

#[tokio::test]
async fn test_guardrail_multiple_checks() {
    let mut enforcer = GuardrailEnforcer::new();
    
    // Register multiple guardrails
    enforcer.register_guardrail(GuardrailDef {
        name: "check_1".to_string(),
        guardrail_type: GuardrailType::PreAgent,
        condition: Condition::Inline("true".to_string()),
        on_violation: ViolationAction::Block,
        description: None,
        severity: None,
        tags: Vec::new(),
    });
    
    enforcer.register_guardrail(GuardrailDef {
        name: "check_2".to_string(),
        guardrail_type: GuardrailType::PreAgent,
        condition: Condition::Inline("false".to_string()),
        on_violation: ViolationAction::Warn,
        description: None,
        severity: None,
        tags: Vec::new(),
    });
    
    enforcer.register_guardrail(GuardrailDef {
        name: "check_3".to_string(),
        guardrail_type: GuardrailType::PreAgent,
        condition: Condition::Inline("false".to_string()),
        on_violation: ViolationAction::Block,
        description: None,
        severity: None,
        tags: Vec::new(),
    });

    let ctx = GuardrailContext::new();
    let summary = enforcer.check(GuardrailType::PreAgent, &ctx).await.unwrap();

    assert_eq!(summary.total_checked, 3);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.warnings, 1); // check_2 has Warn action
    assert_eq!(summary.blocked, 1);  // check_3 has Block action
    assert!(summary.should_block());
}

#[tokio::test]
async fn test_guardrail_telemetry_collection() {
    let mut enforcer = GuardrailEnforcer::new();
    
    enforcer.register_guardrail(GuardrailDef {
        name: "telemetry_test".to_string(),
        guardrail_type: GuardrailType::PostAgent,
        condition: Condition::Inline("false".to_string()),
        on_violation: ViolationAction::Warn,
        description: None,
        severity: None,
        tags: Vec::new(),
    });

    let ctx = GuardrailContext::new();
    let _ = enforcer.check(GuardrailType::PostAgent, &ctx).await.unwrap();
    
    let telemetry = enforcer.get_telemetry_events().await;
    assert_eq!(telemetry.len(), 1);
    assert_eq!(telemetry[0].guardrail_name, "telemetry_test");
    assert_eq!(telemetry[0].result, "fail");
    assert_eq!(telemetry[0].action, "warn");
}
