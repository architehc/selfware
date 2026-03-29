use super::*;
use std::path::PathBuf;

#[test]
fn test_workflow_context_creation() {
    let ctx = WorkflowContext::new("/tmp");
    assert_eq!(ctx.working_dir, PathBuf::from("/tmp"));
    assert!(ctx.variables.is_empty());
    assert_eq!(ctx.status, WorkflowStatus::Pending);
}

#[test]
fn test_workflow_context_variables() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("name", "test");
    ctx.set_var("count", 42);

    assert_eq!(
        ctx.get_var("name").and_then(|v| v.as_string()),
        Some("test".to_string())
    );
}

#[test]
fn test_workflow_context_substitute() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("name", "world");
    ctx.set_var("count", 5);

    let result = ctx.substitute("Hello ${name}, count is ${count}");
    assert_eq!(result, "Hello world, count is 5");
}

#[test]
fn test_workflow_context_substitute_dollar_syntax() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("var", "value");

    let result = ctx.substitute("Test $var here");
    assert_eq!(result, "Test value here");
}

#[test]
fn test_workflow_context_evaluate_condition() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("flag", true);

    assert!(ctx.evaluate_condition("true"));
    assert!(!ctx.evaluate_condition("false"));
    assert!(ctx.evaluate_condition("defined(flag)"));
    assert!(!ctx.evaluate_condition("defined(unknown)"));
}

#[test]
fn test_workflow_context_evaluate_equality() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("x", "hello");

    assert!(ctx.evaluate_condition("hello == hello"));
    assert!(!ctx.evaluate_condition("hello == world"));
}

#[test]
fn test_workflow_context_evaluate_step_success() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.step_results.insert(
        "step1".to_string(),
        StepResult {
            step_id: "step1".to_string(),
            status: StepStatus::Completed,
            output: None,
            error: None,
            duration_ms: 100,
            retry_count: 0,
        },
    );

    assert!(ctx.evaluate_condition("success(step1)"));
    assert!(!ctx.evaluate_condition("failed(step1)"));
}

#[test]
fn test_workflow_context_log() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.log(LogLevel::Info, "Test message", Some("step1".to_string()));

    assert_eq!(ctx.logs.len(), 1);
    assert_eq!(ctx.logs[0].message, "Test message");
}

#[tokio::test]
async fn test_condition_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Condition {
        condition: "true".to_string(),
        then_steps: vec!["step1".to_string(), "step2".to_string()],
        else_steps: Some(vec!["step3".to_string()]),
    };

    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::List(steps) = result {
        assert_eq!(steps.len(), 2);
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_workflow_context_elapsed() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.started_at = Some(std::time::Instant::now());

    std::thread::sleep(std::time::Duration::from_millis(10));

    assert!(ctx.elapsed_ms() > 0);
}

#[test]
fn test_workflow_context_elapsed_not_started() {
    let ctx = WorkflowContext::new("/tmp");
    assert_eq!(ctx.elapsed_ms(), 0);
}

#[test]
fn test_workflow_context_multiple_vars() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("a", "1");
    ctx.set_var("b", "2");
    ctx.set_var("c", "3");

    assert_eq!(ctx.variables.len(), 3);
}

#[test]
fn test_workflow_context_overwrite_var() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("x", "old");
    ctx.set_var("x", "new");

    assert_eq!(
        ctx.get_var("x").and_then(|v| v.as_string()),
        Some("new".to_string())
    );
}

#[test]
fn test_substitute_missing_var() {
    let ctx = WorkflowContext::new("/tmp");
    let result = ctx.substitute("Hello ${name}");
    // Should leave the placeholder as-is if variable doesn't exist
    assert!(result.contains("${name}"));
}

#[test]
fn test_condition_non_empty_string() {
    let ctx = WorkflowContext::new("/tmp");
    assert!(ctx.evaluate_condition("non_empty"));
    assert!(!ctx.evaluate_condition("0"));
    assert!(!ctx.evaluate_condition(""));
}

#[tokio::test]
async fn test_condition_else_branch() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Condition {
        condition: "false".into(),
        then_steps: vec!["then1".into()],
        else_steps: Some(vec!["else1".into(), "else2".into()]),
    };

    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::List(steps) = result {
        assert_eq!(steps.len(), 2);
    }
}

#[test]
fn test_workflow_context_clone() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("test", "value");

    let cloned = ctx.clone();
    assert_eq!(ctx.working_dir, cloned.working_dir);
    assert_eq!(ctx.variables.len(), cloned.variables.len());
}

#[tokio::test]
async fn test_condition_executes_then_branch() {
    // Create a workflow with a condition step that references nested steps
    let yaml = r#"
name: test_condition
description: Test condition execution
steps:
  - id: cond
    name: Check flag
    type: condition
    if: "true"
    then:
      - log_then
    else:
      - log_else
  - id: log_then
    name: Log then branch
    type: log
    message: "then_executed"
  - id: log_else
    name: Log else branch
    type: log
    message: "else_executed"
"#;

    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("test_condition", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    // The condition step should have executed the then branch
    assert!(result.step_results.contains_key("cond"));
    assert_eq!(result.step_results["cond"].status, StepStatus::Completed);
    // Check logs for then_executed message
    let has_then_log = result
        .logs
        .iter()
        .any(|l| l.message.contains("then_executed"));
    assert!(has_then_log, "Then branch should have been executed");
}

#[tokio::test]
async fn test_condition_executes_else_branch() {
    let yaml = r#"
name: test_else
description: Test else branch
steps:
  - id: cond
    name: Check false
    type: condition
    if: "false"
    then:
      - log_then
    else:
      - log_else
  - id: log_then
    name: Log then branch
    type: log
    message: "then_executed"
  - id: log_else
    name: Log else branch
    type: log
    message: "else_executed"
"#;

    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("test_else", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    // Check logs for else_executed message
    let has_else_log = result
        .logs
        .iter()
        .any(|l| l.message.contains("else_executed"));
    assert!(has_else_log, "Else branch should have been executed");
}

#[tokio::test]
async fn test_nested_condition_in_loop() {
    let yaml = r#"
name: nested_control
description: Nested control flow
steps:
  - id: loop
    name: Outer loop
    type: loop
    for: num
    in: "1, 2, 3"
    do:
      - check_num
  - id: check_num
    name: Check number
    type: condition
    if: "true"
    then:
      - log_num
  - id: log_num
    name: Log number
    type: log
    message: "Number: ${num}"
"#;

    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("nested_control", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
}

#[test]
fn test_iteration_aware_dependency_lookup() {
    // Test that check_dependencies with current_iteration looks up dep@idx first
    let mut ctx = WorkflowContext::new("/tmp");

    // Set up step IDs (for validation)
    let all_step_ids: std::collections::HashSet<String> =
        ["step_a", "step_b"].iter().map(|s| s.to_string()).collect();

    // Simulate step_a completed in iteration 2
    ctx.step_results.insert(
        "step_a@2".to_string(),
        StepResult {
            step_id: "step_a".to_string(),
            status: StepStatus::Completed,
            output: None,
            error: None,
            duration_ms: 0,
            retry_count: 0,
        },
    );

    // step_b depends on step_a
    let step_b = WorkflowStep {
        id: "step_b".to_string(),
        name: "Step B".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".to_string(),
            level: LogLevel::Info,
        },
        depends_on: vec!["step_a".to_string()],
        required: true,
        timeout_secs: None,
        retry: RetryConfig::default(),
    };

    // Without iteration context (None), step_a is NOT found (only step_a@2 exists)
    let result_no_iter = ctx.check_dependencies(&step_b, &all_step_ids, None);
    assert!(
        result_no_iter.is_err(),
        "Without iteration context, plain 'step_a' should not be found"
    );

    // With iteration context (Some(2)), step_a@2 IS found
    let result_with_iter = ctx.check_dependencies(&step_b, &all_step_ids, Some(2));
    assert!(
        result_with_iter.is_ok(),
        "With iteration 2, step_a@2 should be found: {:?}",
        result_with_iter
    );

    // With wrong iteration context (Some(0)), step_a@0 NOT found, falls back to step_a NOT found
    let result_wrong_iter = ctx.check_dependencies(&step_b, &all_step_ids, Some(0));
    assert!(
        result_wrong_iter.is_err(),
        "With iteration 0, step_a@0 should not be found"
    );

    // Now add a global step_a result (aggregate from previous loop)
    ctx.step_results.insert(
        "step_a".to_string(),
        StepResult {
            step_id: "step_a".to_string(),
            status: StepStatus::Completed,
            output: None,
            error: None,
            duration_ms: 0,
            retry_count: 0,
        },
    );

    // With wrong iteration (Some(0)), should fall back to global step_a
    let result_fallback = ctx.check_dependencies(&step_b, &all_step_ids, Some(0));
    assert!(
        result_fallback.is_ok(),
        "With iteration 0, should fall back to global step_a: {:?}",
        result_fallback
    );
}

#[tokio::test]
async fn test_intra_loop_dependency_execution() {
    // Test that step B in a loop can depend on step A in the SAME iteration
    let yaml = r#"
name: intra_loop_dep_test
version: "1.0"
description: Test intra-loop dependencies

steps:
  - id: loop_test
    name: Loop with deps
    type: loop
    for: item
    in: "1, 2, 3"
    do:
      - step_a
      - step_b

  - id: step_a
    name: Step A
    type: log
    message: "A processing ${item}"

  - id: step_b
    name: Step B
    type: log
    message: "B depends on A for ${item}"
    depends_on:
      - step_a
"#;

    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).expect("Should parse YAML");

    let result = executor
        .execute("intra_loop_dep_test", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .expect("Workflow should execute");

    // Workflow should complete successfully
    assert_eq!(
        result.status,
        WorkflowStatus::Completed,
        "Workflow should complete. Logs: {:?}",
        result.logs
    );

    // Verify both steps completed for all 3 iterations
    assert!(
        result.step_results.contains_key("step_a@0"),
        "step_a iteration 0 should exist"
    );
    assert!(
        result.step_results.contains_key("step_a@1"),
        "step_a iteration 1 should exist"
    );
    assert!(
        result.step_results.contains_key("step_a@2"),
        "step_a iteration 2 should exist"
    );
    assert!(
        result.step_results.contains_key("step_b@0"),
        "step_b iteration 0 should exist"
    );
    assert!(
        result.step_results.contains_key("step_b@1"),
        "step_b iteration 1 should exist"
    );
    assert!(
        result.step_results.contains_key("step_b@2"),
        "step_b iteration 2 should exist"
    );

    // All iterations of step_b should have completed (not skipped due to missing dep)
    for i in 0..3 {
        let key = format!("step_b@{}", i);
        let step_result = result
            .step_results
            .get(&key)
            .unwrap_or_else(|| panic!("{} should exist", key));
        assert_eq!(
            step_result.status,
            StepStatus::Completed,
            "step_b@{} should be Completed, not {:?}",
            i,
            step_result.status
        );
    }
}

#[test]
fn test_dependency_error_unknown_is_definition_error() {
    let err = DependencyError::Unknown("missing_step".into());
    assert!(err.is_definition_error());
    let display = format!("{}", err);
    assert!(display.contains("missing_step"));
}

#[test]
fn test_dependency_error_not_executed_is_not_definition_error() {
    let err = DependencyError::NotExecuted("pending_step".into());
    assert!(!err.is_definition_error());
    let display = format!("{}", err);
    assert!(display.contains("pending_step"));
}

#[test]
fn test_dependency_error_not_satisfied_is_not_definition_error() {
    let err = DependencyError::NotSatisfied {
        dep: "failed_step".into(),
        status: StepStatus::Failed,
    };
    assert!(!err.is_definition_error());
    let display = format!("{}", err);
    assert!(display.contains("failed_step"));
    assert!(display.contains("Failed"));
}

#[test]
fn test_context_can_recurse_within_limit() {
    let ctx = WorkflowContext::new("/tmp");
    assert!(ctx.can_recurse("step1").is_ok());
}

#[test]
fn test_context_can_recurse_detects_cycle() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.executing_steps.push("step1".to_string());
    let result = ctx.can_recurse("step1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Circular reference"));
}

#[test]
fn test_context_can_recurse_exceeds_depth() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.recursion_depth = 10; // MAX_RECURSION_DEPTH
    let result = ctx.can_recurse("step1");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("recursion depth"));
}

#[test]
fn test_context_enter_and_exit_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    assert_eq!(ctx.recursion_depth, 0);
    assert!(ctx.executing_steps.is_empty());

    ctx.enter_step("step1");
    assert_eq!(ctx.recursion_depth, 1);
    assert_eq!(ctx.executing_steps, vec!["step1"]);

    ctx.enter_step("step2");
    assert_eq!(ctx.recursion_depth, 2);
    assert_eq!(ctx.executing_steps, vec!["step1", "step2"]);

    ctx.exit_step();
    assert_eq!(ctx.recursion_depth, 1);
    assert_eq!(ctx.executing_steps, vec!["step1"]);

    ctx.exit_step();
    assert_eq!(ctx.recursion_depth, 0);
    assert!(ctx.executing_steps.is_empty());
}

#[test]
fn test_context_exit_step_saturates_at_zero() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.exit_step(); // Should not underflow
    assert_eq!(ctx.recursion_depth, 0);
}

#[test]
fn test_log_eviction_at_max_entries() {
    let mut ctx = WorkflowContext::new("/tmp");
    // Add MAX_WORKFLOW_LOG_ENTRIES + 5 entries
    for i in 0..1005 {
        ctx.log(LogLevel::Info, format!("msg {}", i), None);
    }
    // Should be capped at MAX_WORKFLOW_LOG_ENTRIES (1000)
    assert_eq!(ctx.logs.len(), 1000);
    // The first entry should be msg 5 (first 5 evicted)
    assert_eq!(ctx.logs[0].message, "msg 5");
}

#[test]
fn test_substitute_shell_safe_quotes_values() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("name", "hello world");

    let result = ctx.substitute_shell_safe("echo ${name}");
    assert_eq!(result, "echo 'hello world'");
}

#[test]
fn test_substitute_shell_safe_prevents_semicolon_injection() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("user", "foo; rm -rf /");

    let result = ctx.substitute_shell_safe("echo ${user}");
    assert_eq!(result, "echo 'foo; rm -rf /'");
}

#[test]
fn test_substitute_shell_safe_prevents_pipe_injection() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("input", "x | cat /etc/passwd");

    let result = ctx.substitute_shell_safe("grep ${input} file.txt");
    assert_eq!(result, "grep 'x | cat /etc/passwd' file.txt");
}

#[test]
fn test_substitute_shell_safe_prevents_command_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("val", "$(whoami)");

    let result = ctx.substitute_shell_safe("echo ${val}");
    assert_eq!(result, "echo '$(whoami)'");
}

#[test]
fn test_substitute_shell_safe_prevents_backtick_injection() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("val", "`whoami`");

    let result = ctx.substitute_shell_safe("echo ${val}");
    assert_eq!(result, "echo '`whoami`'");
}

#[test]
fn test_substitute_shell_safe_escapes_single_quotes_in_value() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("msg", "it's dangerous");

    let result = ctx.substitute_shell_safe("echo ${msg}");
    assert_eq!(result, "echo 'it'\\''s dangerous'");
}

#[test]
fn test_substitute_shell_safe_dollar_syntax() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("x", "a & b");

    let result = ctx.substitute_shell_safe("echo $x");
    assert_eq!(result, "echo 'a & b'");
}

#[test]
fn test_substitute_shell_safe_plain_values_still_quoted() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("name", "alice");

    // Even safe values get quoted — consistent behavior
    let result = ctx.substitute_shell_safe("echo ${name}");
    assert_eq!(result, "echo 'alice'");
}

#[test]
fn test_substitute_unchanged_for_non_shell() {
    // Regular substitute should remain unquoted for non-shell contexts
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("user", "foo; rm -rf /");

    let result = ctx.substitute("echo ${user}");
    assert_eq!(result, "echo foo; rm -rf /");
}

#[test]
fn test_check_dependencies_no_deps() {
    let ctx = WorkflowContext::new("/tmp");
    let all_ids: std::collections::HashSet<String> = ["s1"].iter().map(|s| s.to_string()).collect();
    let step = WorkflowStep {
        id: "s1".to_string(),
        name: "S1".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec![],
    };
    assert!(ctx.check_dependencies(&step, &all_ids, None).is_ok());
}

#[test]
fn test_check_dependencies_unknown_dep() {
    let ctx = WorkflowContext::new("/tmp");
    let all_ids: std::collections::HashSet<String> = ["s1"].iter().map(|s| s.to_string()).collect();
    let step = WorkflowStep {
        id: "s1".to_string(),
        name: "S1".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec!["nonexistent".to_string()],
    };
    let result = ctx.check_dependencies(&step, &all_ids, None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_definition_error());
    assert!(matches!(err, DependencyError::Unknown(_)));
}

#[test]
fn test_check_dependencies_not_executed() {
    let ctx = WorkflowContext::new("/tmp");
    let all_ids: std::collections::HashSet<String> =
        ["s1", "s2"].iter().map(|s| s.to_string()).collect();
    let step = WorkflowStep {
        id: "s2".to_string(),
        name: "S2".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec!["s1".to_string()],
    };
    let result = ctx.check_dependencies(&step, &all_ids, None);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DependencyError::NotExecuted(_)
    ));
}

#[test]
fn test_check_dependencies_not_satisfied_failed() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.step_results.insert(
        "s1".to_string(),
        StepResult {
            step_id: "s1".to_string(),
            status: StepStatus::Failed,
            output: None,
            error: Some("boom".into()),
            duration_ms: 0,
            retry_count: 0,
        },
    );
    let all_ids: std::collections::HashSet<String> =
        ["s1", "s2"].iter().map(|s| s.to_string()).collect();
    let step = WorkflowStep {
        id: "s2".to_string(),
        name: "S2".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec!["s1".to_string()],
    };
    let result = ctx.check_dependencies(&step, &all_ids, None);
    assert!(result.is_err());
    match result.unwrap_err() {
        DependencyError::NotSatisfied { dep, status } => {
            assert_eq!(dep, "s1");
            assert_eq!(status, StepStatus::Failed);
        }
        other => panic!("Expected NotSatisfied, got {:?}", other),
    }
}

#[test]
fn test_check_dependencies_satisfied() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.step_results.insert(
        "s1".to_string(),
        StepResult {
            step_id: "s1".to_string(),
            status: StepStatus::Completed,
            output: None,
            error: None,
            duration_ms: 100,
            retry_count: 0,
        },
    );
    let all_ids: std::collections::HashSet<String> =
        ["s1", "s2"].iter().map(|s| s.to_string()).collect();
    let step = WorkflowStep {
        id: "s2".to_string(),
        name: "S2".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec!["s1".to_string()],
    };
    assert!(ctx.check_dependencies(&step, &all_ids, None).is_ok());
}

#[test]
fn test_check_dependencies_not_satisfied_skipped() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.step_results.insert(
        "s1".to_string(),
        StepResult {
            step_id: "s1".to_string(),
            status: StepStatus::Skipped,
            output: None,
            error: None,
            duration_ms: 0,
            retry_count: 0,
        },
    );
    let all_ids: std::collections::HashSet<String> =
        ["s1", "s2"].iter().map(|s| s.to_string()).collect();
    let step = WorkflowStep {
        id: "s2".to_string(),
        name: "S2".to_string(),
        description: String::new(),
        step_type: StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec!["s1".to_string()],
    };
    let result = ctx.check_dependencies(&step, &all_ids, None);
    assert!(result.is_err());
    match result.unwrap_err() {
        DependencyError::NotSatisfied { status, .. } => {
            assert_eq!(status, StepStatus::Skipped);
        }
        other => panic!("Expected NotSatisfied, got {:?}", other),
    }
}

#[test]
fn test_dependency_error_display_unknown() {
    let err = DependencyError::Unknown("xyz".into());
    let msg = format!("{}", err);
    assert_eq!(msg, "Unknown dependency: 'xyz'");
}

#[test]
fn test_dependency_error_display_not_executed() {
    let err = DependencyError::NotExecuted("abc".into());
    let msg = format!("{}", err);
    assert_eq!(msg, "Dependency 'abc' not yet executed");
}

#[test]
fn test_dependency_error_display_not_satisfied() {
    let err = DependencyError::NotSatisfied {
        dep: "step_x".into(),
        status: StepStatus::Skipped,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("step_x"));
    assert!(msg.contains("Skipped"));
}

#[tokio::test]
async fn test_condition_step_no_else_false() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::Condition {
        condition: "false".into(),
        then_steps: vec!["step1".into()],
        else_steps: None,
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    // No else steps, should return empty list
    if let VarValue::List(steps) = result {
        assert!(steps.is_empty());
    } else {
        panic!("Expected empty list");
    }
}

#[tokio::test]
async fn test_condition_step_variable_condition() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("check", "true");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::Condition {
        condition: "${check}".into(),
        then_steps: vec!["step_a".into()],
        else_steps: None,
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::List(steps) = result {
        assert_eq!(steps.len(), 1);
    } else {
        panic!("Expected list with 1 element");
    }
}

#[test]
fn test_evaluate_condition_step_failed() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.step_results.insert(
        "s1".to_string(),
        StepResult {
            step_id: "s1".to_string(),
            status: StepStatus::Failed,
            output: None,
            error: Some("err".into()),
            duration_ms: 0,
            retry_count: 0,
        },
    );
    assert!(ctx.evaluate_condition("failed(s1)"));
    assert!(!ctx.evaluate_condition("success(s1)"));
}

#[test]
fn test_evaluate_condition_missing_step() {
    let ctx = WorkflowContext::new("/tmp");
    assert!(!ctx.evaluate_condition("success(no_such_step)"));
    assert!(!ctx.evaluate_condition("failed(no_such_step)"));
}

#[test]
fn test_evaluate_condition_equality_with_vars() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("lang", "rust");
    assert!(ctx.evaluate_condition("${lang} == rust"));
    assert!(!ctx.evaluate_condition("${lang} == python"));
}

#[test]
fn test_evaluate_condition_defined_existing_var() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("myvar", "something");
    assert!(ctx.evaluate_condition("defined(myvar)"));
    assert!(!ctx.evaluate_condition("defined(other)"));
}

#[tokio::test]
async fn test_execute_dependency_failure_required_step() {
    let wf = Workflow {
        name: "dep_fail".into(),
        description: "Required step dep not met".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "s1".into(),
                name: "Fail".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent".into(),
                    args: HashMap::new(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s2".into(),
                name: "Depends on s1".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "should not run".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec!["s1".into()],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("dep_fail", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    // s1 fails (required), workflow aborts
    assert_eq!(result.status, WorkflowStatus::Failed);
}

#[tokio::test]
async fn test_execute_dependency_failure_optional_step() {
    let wf = Workflow {
        name: "dep_skip".into(),
        description: "Optional step dep not met".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "s1".into(),
                name: "Fail".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent".into(),
                    args: HashMap::new(),
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s2".into(),
                name: "Optional depends on s1".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "skipped".into(),
                    level: LogLevel::Info,
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec!["s1".into()],
            },
            WorkflowStep {
                id: "s3".into(),
                name: "No dep".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "runs fine".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("dep_skip", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(result.step_results["s1"].status, StepStatus::Failed);
    assert_eq!(result.step_results["s2"].status, StepStatus::Skipped);
    assert_eq!(result.step_results["s3"].status, StepStatus::Completed);
}

#[tokio::test]
async fn test_execute_unknown_dependency_fatal() {
    let yaml = r#"
name: unknown_dep
description: Step with unknown dep
steps:
  - id: s1
    name: Bad dep
    type: log
    message: "test"
    depends_on:
      - nonexistent_step
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("unknown_dep", HashMap::new(), PathBuf::from("/tmp"))
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("invalid dependency"));
}

#[tokio::test]
async fn test_condition_unknown_step_reference() {
    let yaml = r#"
name: cond_unknown
description: Condition references unknown step
steps:
  - id: cond
    name: Cond
    type: condition
    if: "true"
    then:
      - does_not_exist
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("cond_unknown", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    // The step error should indicate unknown step
    assert_eq!(result.status, WorkflowStatus::Failed);
}

#[test]
fn test_substitute_no_vars() {
    let ctx = WorkflowContext::new("/tmp");
    let result = ctx.substitute("no vars here");
    assert_eq!(result, "no vars here");
}

#[test]
fn test_substitute_mixed_syntax() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("a", "AAA");
    ctx.set_var("b", "BBB");
    let result = ctx.substitute("${a} and $b");
    assert_eq!(result, "AAA and BBB");
}

#[test]
fn test_substitute_shell_safe_empty_value() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("empty", "");
    let result = ctx.substitute_shell_safe("echo ${empty}");
    assert_eq!(result, "echo ''");
}

#[test]
fn test_substitute_shell_safe_multiple_vars() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("a", "hello world");
    ctx.set_var("b", "foo;bar");
    let result = ctx.substitute_shell_safe("echo ${a} ${b}");
    assert_eq!(result, "echo 'hello world' 'foo;bar'");
}

#[test]
fn test_shell_quote_empty() {
    let result = WorkflowContext::shell_quote("");
    assert_eq!(result, "''");
}

#[test]
fn test_shell_quote_multiple_single_quotes() {
    let result = WorkflowContext::shell_quote("it's a 'test'");
    assert_eq!(result, "'it'\\''s a '\\''test'\\'''");
}

#[tokio::test]
async fn test_condition_live_nested_with_dependency() {
    let wf = Workflow {
        name: "cond_dep".into(),
        description: "Condition with nested dep".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "setup".into(),
                name: "Setup".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "ready".into(),
                    value: "yes".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "cond".into(),
                name: "Check".into(),
                description: String::new(),
                step_type: StepType::Condition {
                    condition: "true".into(),
                    then_steps: vec!["inner_step".into()],
                    else_steps: None,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "inner_step".into(),
                name: "Inner".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "Ready: ${ready}".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec!["setup".into()],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("cond_dep", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(
        result.step_results["inner_step"].status,
        StepStatus::Completed
    );
}

#[tokio::test]
async fn test_condition_live_required_nested_fails() {
    let wf = Workflow {
        name: "cond_req_fail".into(),
        description: "Required nested step fails in condition".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "cond".into(),
                name: "Cond".into(),
                description: String::new(),
                step_type: StepType::Condition {
                    condition: "true".into(),
                    then_steps: vec!["fail_step".into()],
                    else_steps: None,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "fail_step".into(),
                name: "Fail".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent".into(),
                    args: HashMap::new(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("cond_req_fail", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
}

#[tokio::test]
async fn test_loop_step_without_workflow_context() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::Loop {
        variable: "x".into(),
        items: "1, 2, 3".into(),
        do_steps: vec!["inner".into()],
    };
    // Without workflow context, loop just sets variables and returns Null
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    // The last loop variable value should be set
    assert_eq!(
        ctx.get_var("x").and_then(|v| v.as_string()),
        Some("3".to_string())
    );
    // Result should be Null since no workflow_steps to execute
    assert!(matches!(result, VarValue::Null));
}

#[tokio::test]
async fn test_condition_optional_step_dep_skipped() {
    let wf = Workflow {
        name: "cond_opt_dep".into(),
        description: "Optional step in condition branch with unmet dep".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "cond".into(),
                name: "Cond".into(),
                description: String::new(),
                step_type: StepType::Condition {
                    condition: "true".into(),
                    then_steps: vec!["opt_step".into()],
                    else_steps: None,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "opt_step".into(),
                name: "Optional".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "test".into(),
                    level: LogLevel::Info,
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec!["unfinished_step".into()],
            },
            WorkflowStep {
                id: "unfinished_step".into(),
                name: "Unfinished".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent".into(),
                    args: HashMap::new(),
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("cond_opt_dep", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    // The optional step should have been skipped due to unmet dep
    assert!(result.is_success());
    assert_eq!(result.step_results["opt_step"].status, StepStatus::Skipped);
}

#[test]
fn test_substitute_repeated_var() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("x", "val");
    let result = ctx.substitute("${x} and ${x} again");
    assert_eq!(result, "val and val again");
}

#[test]
fn test_substitute_list_value_not_substituted() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.variables.insert(
        "list_var".into(),
        VarValue::List(vec![VarValue::String("a".into())]),
    );
    // List has no as_string(), so placeholder stays
    let result = ctx.substitute("value: ${list_var}");
    assert_eq!(result, "value: ${list_var}");
}

#[test]
fn test_substitute_map_value_not_substituted() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.variables
        .insert("map_var".into(), VarValue::Map(HashMap::new()));
    let result = ctx.substitute("value: ${map_var}");
    assert_eq!(result, "value: ${map_var}");
}

#[test]
fn test_workflow_context_initial_state() {
    let ctx = WorkflowContext::new("/tmp/project");
    assert_eq!(ctx.current_step, 0);
    assert_eq!(ctx.recursion_depth, 0);
    assert!(ctx.executing_steps.is_empty());
    assert!(ctx.control_flow_managed_steps.is_empty());
    assert!(ctx.workflow_call_stack.is_empty());
    assert!(ctx.started_at.is_none());
    assert!(ctx.step_results.is_empty());
    assert!(ctx.logs.is_empty());
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_step_uses_shell_safe_substitution() {
    // Verify that variables with special chars are safely quoted
    let yaml = r#"
name: shell_safe
description: Test shell-safe quoting
steps:
  - id: s1
    name: Echo
    type: shell
    command: "echo ${msg}"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let mut inputs = HashMap::new();
    inputs.insert("msg".into(), VarValue::String("hello; world".into()));
    let result = executor
        .execute("shell_safe", inputs, PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // The output should contain the literal string, not execute the injection
    let step = &result.step_results["s1"];
    if let Some(VarValue::String(out)) = &step.output {
        assert!(out.contains("hello; world") || out.contains("hello"));
    }
}

#[test]
fn test_dependency_error_clone() {
    let err = DependencyError::Unknown("s1".into());
    let cloned = err.clone();
    assert!(matches!(cloned, DependencyError::Unknown(ref s) if s == "s1"));

    let err2 = DependencyError::NotExecuted("s2".into());
    let cloned2 = err2.clone();
    assert!(matches!(cloned2, DependencyError::NotExecuted(ref s) if s == "s2"));

    let err3 = DependencyError::NotSatisfied {
        dep: "s3".into(),
        status: StepStatus::Failed,
    };
    let cloned3 = err3.clone();
    assert!(
        matches!(cloned3, DependencyError::NotSatisfied { ref dep, status } if dep == "s3" && status == StepStatus::Failed)
    );
}

#[test]
fn test_dependency_error_debug() {
    let err = DependencyError::Unknown("x".into());
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("Unknown"));
}

#[tokio::test]
async fn test_condition_with_step_success_check() {
    let wf = Workflow {
        name: "cond_success_check".into(),
        description: "Condition checks step success".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "setup".into(),
                name: "Setup".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "val".into(),
                    value: "ok".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "cond".into(),
                name: "Check".into(),
                description: String::new(),
                step_type: StepType::Condition {
                    condition: "success(setup)".into(),
                    then_steps: vec!["log_ok".into()],
                    else_steps: Some(vec!["log_fail".into()]),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "log_ok".into(),
                name: "Log ok".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "setup succeeded".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "log_fail".into(),
                name: "Log fail".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "setup failed".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("cond_success_check", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // The then branch should have been taken
    let has_ok_log = result
        .logs
        .iter()
        .any(|l| l.message.contains("setup succeeded"));
    assert!(
        has_ok_log,
        "Expected then-branch log. Logs: {:?}",
        result.logs
    );
}

#[tokio::test]
async fn test_condition_with_step_failed_check() {
    let wf = Workflow {
        name: "cond_failed_check".into(),
        description: "Condition checks step failure".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "maybe_fail".into(),
                name: "Maybe fail".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent".into(),
                    args: HashMap::new(),
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "cond".into(),
                name: "Check".into(),
                description: String::new(),
                step_type: StepType::Condition {
                    condition: "failed(maybe_fail)".into(),
                    then_steps: vec!["log_failed".into()],
                    else_steps: Some(vec!["log_ok".into()]),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "log_failed".into(),
                name: "Log failed".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "step failed as expected".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "log_ok".into(),
                name: "Log ok".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "step was ok".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
        ],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("cond_failed_check", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    let has_failed_log = result
        .logs
        .iter()
        .any(|l| l.message.contains("step failed as expected"));
    assert!(
        has_failed_log,
        "Expected then-branch log for failed(). Logs: {:?}",
        result.logs
    );
}

#[tokio::test]
async fn test_condition_no_else_false_live() {
    let yaml = r#"
name: cond_no_else
description: Condition false with no else
steps:
  - id: cond
    name: Check
    type: condition
    if: "false"
    then:
      - inner
  - id: inner
    name: Inner
    type: log
    message: "should not run"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("cond_no_else", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // inner should NOT have been executed
    assert!(
        !result.step_results.contains_key("inner")
            || result.step_results["inner"].status != StepStatus::Completed
    );
}
