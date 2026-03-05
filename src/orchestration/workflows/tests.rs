use super::*;
use std::path::PathBuf;

#[test]
fn test_workflow_status_default() {
    assert_eq!(WorkflowStatus::default(), WorkflowStatus::Pending);
}

#[test]
fn test_step_status_default() {
    assert_eq!(StepStatus::default(), StepStatus::Pending);
}

#[test]
fn test_var_value_conversions() {
    let s: VarValue = "hello".into();
    assert_eq!(s.as_string(), Some("hello".to_string()));

    let b: VarValue = true.into();
    assert_eq!(b.as_bool(), Some(true));

    let n: VarValue = 42.into();
    assert_eq!(n.as_string(), Some("42".to_string()));
}

#[test]
fn test_var_value_as_bool() {
    assert_eq!(VarValue::Boolean(true).as_bool(), Some(true));
    assert_eq!(VarValue::Boolean(false).as_bool(), Some(false));
    assert_eq!(VarValue::String("hello".into()).as_bool(), Some(true));
    assert_eq!(VarValue::String("".into()).as_bool(), Some(false));
    assert_eq!(VarValue::Number(1.0).as_bool(), Some(true));
    assert_eq!(VarValue::Number(0.0).as_bool(), Some(false));
    assert_eq!(VarValue::Null.as_bool(), Some(false));
}

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

#[test]
fn test_workflow_executor_creation() {
    let executor = WorkflowExecutor::new();
    assert!(executor.list().is_empty());
}

#[test]
fn test_workflow_executor_register() {
    let mut executor = WorkflowExecutor::new();
    executor.register(WorkflowTemplates::tdd());

    assert!(executor.get("tdd").is_some());
    assert_eq!(executor.list().len(), 1);
}

#[test]
fn test_workflow_executor_list_by_category() {
    let mut executor = WorkflowExecutor::new();
    executor.register(WorkflowTemplates::tdd());
    executor.register(WorkflowTemplates::debug());
    executor.register(WorkflowTemplates::review());

    let dev_workflows = executor.list_by_category("development");
    assert_eq!(dev_workflows.len(), 1);

    let debug_workflows = executor.list_by_category("debugging");
    assert_eq!(debug_workflows.len(), 1);
}

#[test]
fn test_workflow_templates_tdd() {
    let workflow = WorkflowTemplates::tdd();
    assert_eq!(workflow.name, "tdd");
    assert!(!workflow.steps.is_empty());
    assert!(workflow.tags.contains(&"tdd".to_string()));
}

#[test]
fn test_workflow_templates_debug() {
    let workflow = WorkflowTemplates::debug();
    assert_eq!(workflow.name, "debug");
    assert_eq!(workflow.category, "debugging");
}

#[test]
fn test_workflow_templates_review() {
    let workflow = WorkflowTemplates::review();
    assert_eq!(workflow.name, "review");
    assert!(workflow.inputs.iter().any(|i| i.name == "files"));
}

#[test]
fn test_workflow_templates_refactor() {
    let workflow = WorkflowTemplates::refactor();
    assert_eq!(workflow.name, "refactor");
    assert!(workflow.steps.iter().any(|s| s.id == "run_tests_before"));
    assert!(workflow.steps.iter().any(|s| s.id == "run_tests_after"));
}

#[tokio::test]
async fn test_workflow_execution_missing_input() {
    let mut executor = WorkflowExecutor::new();
    executor.register(WorkflowTemplates::tdd());

    let result = executor
        .execute("tdd", HashMap::new(), PathBuf::from("/tmp"))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_workflow_execution_with_inputs() {
    let mut executor = WorkflowExecutor::new();
    executor.register(WorkflowTemplates::tdd());

    let mut inputs = HashMap::new();
    inputs.insert(
        "feature".to_string(),
        VarValue::String("test feature".into()),
    );

    let result = executor
        .execute("tdd", inputs, PathBuf::from("/tmp"))
        .await
        .unwrap();

    // Workflow should run (may not complete successfully due to simulated execution)
    assert!(!result.step_results.is_empty());
}

#[test]
fn test_workflow_result_helpers() {
    let result = WorkflowResult {
        workflow_name: "test".to_string(),
        status: WorkflowStatus::Completed,
        outputs: HashMap::from([("out".to_string(), VarValue::String("value".into()))]),
        step_results: HashMap::new(),
        logs: VecDeque::new(),
        duration_ms: 1000,
    };

    assert!(result.is_success());
    assert!(result.get_output("out").is_some());
    assert!(result.failed_steps().is_empty());
}

#[test]
fn test_step_result() {
    let result = StepResult {
        step_id: "test".to_string(),
        status: StepStatus::Completed,
        output: Some(VarValue::String("output".into())),
        error: None,
        duration_ms: 100,
        retry_count: 0,
    };

    assert_eq!(result.status, StepStatus::Completed);
    assert!(result.error.is_none());
}

#[test]
fn test_retry_config_default() {
    let config = RetryConfig::default();
    assert_eq!(config.max_attempts, 0);
    assert_eq!(config.delay_secs, 0);
    assert!(!config.exponential);
}

#[test]
fn test_workflow_step_required_default() {
    // This tests the default_true function
    let step = WorkflowStep {
        id: "test".to_string(),
        name: "Test".to_string(),
        description: "".to_string(),
        step_type: StepType::Log {
            message: "test".to_string(),
            level: LogLevel::Info,
        },
        required: default_true(),
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec![],
    };

    assert!(step.required);
}

#[test]
fn test_log_level_default() {
    assert!(matches!(LogLevel::default(), LogLevel::Info));
}

#[test]
fn test_workflow_yaml_parsing() {
    let yaml = r#"
name: test_workflow
description: A test workflow
version: "1.0.0"
category: test
inputs:
  - name: input1
    description: First input
    required: true
steps:
  - id: step1
    name: First step
    type: log
    message: "Hello ${input1}"
tags:
  - test
"#;

    let mut executor = WorkflowExecutor::new();
    let result = executor.load_yaml(yaml);

    assert!(result.is_ok());
    assert!(executor.get("test_workflow").is_some());
}

#[tokio::test]
async fn test_set_var_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::SetVar {
        name: "result".to_string(),
        value: "hello".to_string(),
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(
        ctx.get_var("result").and_then(|v| v.as_string()),
        Some("hello".to_string())
    );
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

#[tokio::test]
async fn test_shell_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Shell {
        command: "echo hello".to_string(),
        working_dir: Some("/tmp".to_string()),
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
    assert!(!ctx.logs.is_empty());
}

#[tokio::test]
async fn test_input_step_with_default() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Input {
        prompt: "Enter name".to_string(),
        variable: "name".to_string(),
        default: Some("default_name".to_string()),
    };

    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    assert_eq!(result.as_string(), Some("default_name".to_string()));
}

#[tokio::test]
async fn test_input_step_without_default() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Input {
        prompt: "Enter name".to_string(),
        variable: "name".to_string(),
        default: None,
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_err()); // No default and not interactive
}

// Additional comprehensive tests

#[test]
fn test_workflow_status_all_variants() {
    let statuses = [
        WorkflowStatus::Pending,
        WorkflowStatus::Running,
        WorkflowStatus::Completed,
        WorkflowStatus::Failed,
        WorkflowStatus::Paused,
        WorkflowStatus::Cancelled,
    ];

    for status in statuses {
        let _ = format!("{:?}", status);
    }
}

#[test]
fn test_step_status_all_variants() {
    let statuses = [
        StepStatus::Pending,
        StepStatus::Running,
        StepStatus::Completed,
        StepStatus::Failed,
        StepStatus::Skipped,
    ];

    for status in statuses {
        let _ = format!("{:?}", status);
    }
}

#[test]
fn test_var_value_list() {
    let list = VarValue::List(vec![
        VarValue::String("a".into()),
        VarValue::Number(1.0),
        VarValue::Boolean(true),
    ]);

    if let VarValue::List(items) = list {
        assert_eq!(items.len(), 3);
    }
}

#[test]
fn test_var_value_map() {
    let mut map = HashMap::new();
    map.insert("key".into(), VarValue::String("value".into()));

    let var = VarValue::Map(map);
    if let VarValue::Map(m) = var {
        assert!(m.contains_key("key"));
    }
}

#[test]
fn test_var_value_null() {
    let null = VarValue::Null;
    assert_eq!(null.as_bool(), Some(false));
    assert_eq!(null.as_string(), None);
}

#[test]
fn test_var_value_from_string_owned() {
    let var: VarValue = String::from("test").into();
    assert_eq!(var.as_string(), Some("test".to_string()));
}

#[test]
fn test_var_value_clone() {
    let original = VarValue::String("test".into());
    let cloned = original.clone();
    assert_eq!(original.as_string(), cloned.as_string());
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

#[test]
fn test_log_entry() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.log(LogLevel::Debug, "Debug msg", None);
    ctx.log(LogLevel::Warn, "Warning", Some("step1".into()));
    ctx.log(LogLevel::Error, "Error", Some("step2".into()));

    assert_eq!(ctx.logs.len(), 3);
}

#[test]
fn test_log_level_variants() {
    let levels = [
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ];

    for level in levels {
        let _ = format!("{:?}", level);
    }
}

#[test]
fn test_workflow_step_clone() {
    let step = WorkflowStep {
        id: "step1".into(),
        name: "Test Step".into(),
        description: "Desc".into(),
        step_type: StepType::Log {
            message: "msg".into(),
            level: LogLevel::Info,
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: Some(60),
        depends_on: vec!["step0".into()],
    };

    let cloned = step.clone();
    assert_eq!(step.id, cloned.id);
}

#[test]
fn test_retry_config_with_values() {
    let config = RetryConfig {
        max_attempts: 3,
        delay_secs: 5,
        exponential: true,
    };

    assert_eq!(config.max_attempts, 3);
    assert!(config.exponential);
}

#[test]
fn test_workflow_input_clone() {
    let input = WorkflowInput {
        name: "param1".into(),
        description: "A parameter".into(),
        required: true,
        default: Some(VarValue::String("default".into())),
        param_type: "string".into(),
    };

    let cloned = input.clone();
    assert_eq!(input.name, cloned.name);
}

#[test]
fn test_workflow_output_clone() {
    let output = WorkflowOutput {
        name: "result".into(),
        description: "The result".into(),
        from: "result_var".into(),
    };

    let cloned = output.clone();
    assert_eq!(output.name, cloned.name);
}

#[test]
fn test_workflow_clone() {
    let workflow = WorkflowTemplates::tdd();
    let cloned = workflow.clone();
    assert_eq!(workflow.name, cloned.name);
    assert_eq!(workflow.steps.len(), cloned.steps.len());
}

#[test]
fn test_step_result_clone() {
    let result = StepResult {
        step_id: "step1".into(),
        status: StepStatus::Completed,
        output: Some(VarValue::String("output".into())),
        error: None,
        duration_ms: 100,
        retry_count: 0,
    };

    let cloned = result.clone();
    assert_eq!(result.step_id, cloned.step_id);
}

#[test]
fn test_workflow_result_is_success() {
    let result = WorkflowResult {
        workflow_name: "test".into(),
        status: WorkflowStatus::Completed,
        outputs: HashMap::new(),
        step_results: HashMap::new(),
        logs: VecDeque::new(),
        duration_ms: 1000,
    };

    assert!(result.is_success());
}

#[test]
fn test_workflow_result_is_not_success() {
    let result = WorkflowResult {
        workflow_name: "test".into(),
        status: WorkflowStatus::Failed,
        outputs: HashMap::new(),
        step_results: HashMap::new(),
        logs: VecDeque::new(),
        duration_ms: 1000,
    };

    assert!(!result.is_success());
}

#[test]
fn test_workflow_result_get_output() {
    let mut outputs = HashMap::new();
    outputs.insert("key".into(), VarValue::String("value".into()));

    let result = WorkflowResult {
        workflow_name: "test".into(),
        status: WorkflowStatus::Completed,
        outputs,
        step_results: HashMap::new(),
        logs: VecDeque::new(),
        duration_ms: 0,
    };

    assert!(result.get_output("key").is_some());
    assert!(result.get_output("missing").is_none());
}

#[test]
fn test_workflow_result_failed_steps() {
    let mut step_results = HashMap::new();
    step_results.insert(
        "step1".into(),
        StepResult {
            step_id: "step1".into(),
            status: StepStatus::Completed,
            output: None,
            error: None,
            duration_ms: 100,
            retry_count: 0,
        },
    );
    step_results.insert(
        "step2".into(),
        StepResult {
            step_id: "step2".into(),
            status: StepStatus::Failed,
            output: None,
            error: Some("Error".into()),
            duration_ms: 50,
            retry_count: 1,
        },
    );

    let result = WorkflowResult {
        workflow_name: "test".into(),
        status: WorkflowStatus::Failed,
        outputs: HashMap::new(),
        step_results,
        logs: VecDeque::new(),
        duration_ms: 150,
    };

    let failed = result.failed_steps();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].step_id, "step2");
}

#[test]
fn test_executor_default() {
    let executor = WorkflowExecutor::default();
    assert!(executor.list().is_empty());
}

#[test]
fn test_executor_load_invalid_yaml() {
    let mut executor = WorkflowExecutor::new();
    let result = executor.load_yaml("invalid yaml: [[[");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tool_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("file", "test.rs");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Tool {
        name: "file_read".into(),
        args: HashMap::from([("path".into(), "${file}".into())]),
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_llm_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Llm {
        prompt: "Explain this code".into(),
        context: vec!["file1.rs".into(), "file2.rs".into()],
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_loop_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Loop {
        variable: "item".into(),
        items: "a, b, c".into(),
        do_steps: vec!["process".into()],
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pause_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::Pause {
        message: "Press enter to continue".into(),
    };

    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sub_workflow_step() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new_dry_run();

    let step_type = StepType::SubWorkflow {
        workflow_name: "sub_wf".into(),
        inputs: HashMap::from([("param".into(), "value".into())]),
    };

    // In dry-run, sub-workflow returns placeholder even if not registered
    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_ok());
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
fn test_workflow_serialization() {
    let workflow = WorkflowTemplates::tdd();
    let json = serde_json::to_string(&workflow).unwrap();
    assert!(json.contains("tdd"));
}

#[test]
fn test_step_type_serialization() {
    let step_type = StepType::Log {
        message: "test".into(),
        level: LogLevel::Info,
    };
    let json = serde_json::to_string(&step_type).unwrap();
    assert!(json.contains("log"));
}

#[test]
fn test_workflow_context_clone() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("test", "value");

    let cloned = ctx.clone();
    assert_eq!(ctx.working_dir, cloned.working_dir);
    assert_eq!(ctx.variables.len(), cloned.variables.len());
}

#[test]
fn test_step_status_equality() {
    assert_eq!(StepStatus::Pending, StepStatus::Pending);
    assert_ne!(StepStatus::Pending, StepStatus::Running);
}

#[test]
fn test_workflow_status_equality() {
    assert_eq!(WorkflowStatus::Running, WorkflowStatus::Running);
    assert_ne!(WorkflowStatus::Running, WorkflowStatus::Completed);
}

#[test]
fn test_log_entry_clone() {
    let entry = LogEntry {
        timestamp: 12345,
        level: LogLevel::Info,
        message: "Test".into(),
        step_id: Some("step1".into()),
    };

    let cloned = entry.clone();
    assert_eq!(entry.timestamp, cloned.timestamp);
    assert_eq!(entry.message, cloned.message);
}

#[test]
fn test_var_value_default() {
    let var = VarValue::default();
    assert!(matches!(var, VarValue::Null));
}

#[test]
fn test_workflow_version_default() {
    let version = default_version();
    assert_eq!(version, "1.0.0");
}

#[test]
fn test_workflow_string_type_default() {
    let type_str = default_string_type();
    assert_eq!(type_str, "string");
}

// =========================================================================
// Control-flow execution tests
// =========================================================================

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
async fn test_loop_executes_nested_steps() {
    let yaml = r#"
name: test_loop
description: Test loop execution
steps:
  - id: loop
    name: Iterate items
    type: loop
    for: item
    in: "a, b, c"
    do:
      - log_item
  - id: log_item
    name: Log item
    type: log
    message: "Processing: ${item}"
"#;

    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("test_loop", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    // Loop should have completed
    assert_eq!(result.step_results["loop"].status, StepStatus::Completed);
    // Check that items were iterated (logged)
    let loop_logs: Vec<_> = result
        .logs
        .iter()
        .filter(|l| l.message.contains("Loop"))
        .collect();
    assert!(!loop_logs.is_empty());
}

#[tokio::test]
async fn test_sub_workflow_execution() {
    let parent_yaml = r#"
name: parent
description: Parent workflow
steps:
  - id: call_child
    name: Call child
    type: sub_workflow
    workflow: child
"#;

    let child_yaml = r#"
name: child
description: Child workflow
steps:
  - id: greet
    name: Greet
    type: log
    message: "Hello from child"
"#;

    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(parent_yaml).unwrap();
    executor.load_yaml(child_yaml).unwrap();

    let result = executor
        .execute("parent", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(
        result.step_results["call_child"].status,
        StepStatus::Completed
    );
    // Verify child workflow logs are captured
    let has_child_log = result.logs.iter().any(|l| l.message.contains("child"));
    assert!(has_child_log, "Child workflow should have been executed");
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

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_step_live_execution() {
    // Test that shell steps execute for real (not dry-run)
    let yaml = r#"
name: shell_test
description: Test shell execution
steps:
  - id: echo
    name: Echo test
    type: shell
    command: "echo 'hello world'"
"#;

    let mut executor = WorkflowExecutor::new(); // Live mode
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("shell_test", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    let step_result = &result.step_results["echo"];
    assert_eq!(step_result.status, StepStatus::Completed);
    // Verify output contains expected text
    if let Some(VarValue::String(output)) = &step_result.output {
        assert!(output.contains("hello world"));
    }
}

#[tokio::test]
async fn test_subworkflow_direct_cycle_detection() {
    // Workflow A calls itself -> direct cycle
    let yaml_a = r#"
name: workflow_a
description: Self-referencing workflow
steps:
  - id: call_self
    name: Call self
    type: sub_workflow
    workflow: workflow_a
    required: true
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml_a).unwrap();

    let result = executor
        .execute("workflow_a", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .expect("Workflow execution should return Ok with Failed status");

    // Cycle is detected and workflow fails
    assert_eq!(result.status, WorkflowStatus::Failed);

    // Check that the step error contains cycle detection message
    let step_result = result
        .step_results
        .get("call_self")
        .expect("Step result should exist");
    assert_eq!(step_result.status, StepStatus::Failed);
    let error = step_result.error.as_ref().expect("Error should exist");
    assert!(
        error.contains("cycle") || error.contains("call stack"),
        "Expected cycle detection error, got: {}",
        error
    );
}

#[tokio::test]
async fn test_subworkflow_indirect_cycle_detection() {
    // A -> B -> C -> A (indirect cycle)
    let yaml_a = r#"
name: workflow_a
description: Workflow A
steps:
  - id: call_b
    name: Call B
    type: sub_workflow
    workflow: workflow_b
    required: true
"#;
    let yaml_b = r#"
name: workflow_b
description: Workflow B
steps:
  - id: call_c
    name: Call C
    type: sub_workflow
    workflow: workflow_c
    required: true
"#;
    let yaml_c = r#"
name: workflow_c
description: Workflow C - cycles back to A
steps:
  - id: call_a
    name: Call A
    type: sub_workflow
    workflow: workflow_a
    required: true
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml_a).unwrap();
    executor.load_yaml(yaml_b).unwrap();
    executor.load_yaml(yaml_c).unwrap();

    let result = executor
        .execute("workflow_a", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .expect("Workflow execution should return Ok with Failed status");

    // Cycle is detected and workflow fails
    assert_eq!(result.status, WorkflowStatus::Failed);

    // Check logs for cycle detection
    let has_cycle_error = result
        .logs
        .iter()
        .any(|log| log.message.contains("cycle") || log.message.contains("call stack"));
    assert!(
        has_cycle_error,
        "Expected cycle detection in logs, got: {:?}",
        result.logs
    );
}

#[tokio::test]
async fn test_subworkflow_depth_limit() {
    // Create a chain of workflows that exceeds depth limit (10)
    let mut executor = WorkflowExecutor::new();

    for i in 0..12 {
        let next = if i < 11 {
            format!(
                r#"
  - id: call_next
    name: Call next
    type: sub_workflow
    workflow: workflow_{}
    required: true"#,
                i + 1
            )
        } else {
            String::new()
        };

        let yaml = format!(
            r#"
name: workflow_{}
description: Workflow {}
steps:{}
  - id: step_{}
    name: Step {}
    type: log
    message: "At depth {}"
"#,
            i, i, next, i, i, i
        );
        executor.load_yaml(&yaml).unwrap();
    }

    let result = executor
        .execute("workflow_0", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .expect("Workflow execution should return Ok with Failed status");

    // Depth limit exceeded and workflow fails
    assert_eq!(result.status, WorkflowStatus::Failed);

    // Check logs for depth limit error
    let has_depth_error = result
        .logs
        .iter()
        .any(|log| log.message.contains("depth") || log.message.contains("exceeded"));
    assert!(
        has_depth_error,
        "Expected depth limit error in logs, got: {:?}",
        result.logs
    );
}

#[test]
fn test_loop_iteration_result_format() {
    // Verify iteration results use step_id@iteration format
    let mut ctx = WorkflowContext::new("/tmp");

    // Simulate what loop execution does
    let step_id = "my_step";
    for i in 0..3 {
        let iter_key = format!("{}@{}", step_id, i);
        ctx.step_results.insert(
            iter_key.clone(),
            StepResult {
                step_id: step_id.to_string(),
                status: StepStatus::Completed,
                output: Some(VarValue::Number(i as f64)),
                error: None,
                duration_ms: 100,
                retry_count: 0,
            },
        );
    }

    // Verify per-iteration results are accessible
    assert!(ctx.step_results.contains_key("my_step@0"));
    assert!(ctx.step_results.contains_key("my_step@1"));
    assert!(ctx.step_results.contains_key("my_step@2"));
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

// =========================================================================
// Workflow template comprehensive tests
// =========================================================================

#[test]
fn test_tdd_template_metadata() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.name, "tdd");
    assert_eq!(wf.description, "Test-Driven Development workflow");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "development");
}

#[test]
fn test_tdd_template_inputs() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.inputs.len(), 2);

    let feature_input = &wf.inputs[0];
    assert_eq!(feature_input.name, "feature");
    assert!(feature_input.required);
    assert!(feature_input.default.is_none());

    let test_file_input = &wf.inputs[1];
    assert_eq!(test_file_input.name, "test_file");
    assert!(!test_file_input.required);
    assert!(test_file_input.default.is_some());
    assert_eq!(
        test_file_input.default.as_ref().unwrap().as_string(),
        Some("tests/test_feature.rs".to_string())
    );
}

#[test]
fn test_tdd_template_outputs() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.outputs.len(), 1);
    assert_eq!(wf.outputs[0].name, "test_passed");
    assert_eq!(wf.outputs[0].from, "tests_passed");
}

#[test]
fn test_tdd_template_step_count_and_ids() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.steps.len(), 5);
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "write_test",
            "run_test_red",
            "implement",
            "run_test_green",
            "refactor"
        ]
    );
}

#[test]
fn test_tdd_template_dependency_chain() {
    let wf = WorkflowTemplates::tdd();
    // First step has no dependencies
    assert!(wf.steps[0].depends_on.is_empty());
    // Each subsequent step depends on the previous one
    assert_eq!(wf.steps[1].depends_on, vec!["write_test"]);
    assert_eq!(wf.steps[2].depends_on, vec!["run_test_red"]);
    assert_eq!(wf.steps[3].depends_on, vec!["implement"]);
    assert_eq!(wf.steps[4].depends_on, vec!["run_test_green"]);
}

#[test]
fn test_tdd_template_step_required_flags() {
    let wf = WorkflowTemplates::tdd();
    // write_test: required
    assert!(wf.steps[0].required);
    // run_test_red: not required (expected to fail in red phase)
    assert!(!wf.steps[1].required);
    // implement: required
    assert!(wf.steps[2].required);
    // run_test_green: required
    assert!(wf.steps[3].required);
    // refactor: not required
    assert!(!wf.steps[4].required);
}

#[test]
fn test_tdd_template_green_step_retry_config() {
    let wf = WorkflowTemplates::tdd();
    let green_step = &wf.steps[3];
    assert_eq!(green_step.id, "run_test_green");
    assert_eq!(green_step.retry.max_attempts, 3);
    assert_eq!(green_step.retry.delay_secs, 5);
    assert!(!green_step.retry.exponential);
}

#[test]
fn test_tdd_template_tags() {
    let wf = WorkflowTemplates::tdd();
    assert_eq!(wf.tags.len(), 3);
    assert!(wf.tags.contains(&"tdd".to_string()));
    assert!(wf.tags.contains(&"testing".to_string()));
    assert!(wf.tags.contains(&"development".to_string()));
}

#[test]
fn test_debug_template_metadata() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.name, "debug");
    assert_eq!(wf.description, "Debugging workflow");
    assert_eq!(wf.version, "1.0.0");
    assert_eq!(wf.author, "Selfware");
    assert_eq!(wf.category, "debugging");
}

#[test]
fn test_debug_template_single_required_input() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.inputs.len(), 1);
    assert_eq!(wf.inputs[0].name, "issue");
    assert!(wf.inputs[0].required);
    assert!(wf.inputs[0].default.is_none());
}

#[test]
fn test_debug_template_no_outputs() {
    let wf = WorkflowTemplates::debug();
    assert!(wf.outputs.is_empty());
}

#[test]
fn test_debug_template_step_chain() {
    let wf = WorkflowTemplates::debug();
    assert_eq!(wf.steps.len(), 4);
    let ids: Vec<&str> = wf.steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["reproduce", "analyze", "fix", "verify"]);

    assert!(wf.steps[0].depends_on.is_empty());
    assert_eq!(wf.steps[1].depends_on, vec!["reproduce"]);
    assert_eq!(wf.steps[2].depends_on, vec!["analyze"]);
    assert_eq!(wf.steps[3].depends_on, vec!["fix"]);
}

#[test]
fn test_debug_template_all_steps_required() {
    let wf = WorkflowTemplates::debug();
    for step in &wf.steps {
        assert!(step.required, "Debug step '{}' should be required", step.id);
    }
}

#[test]
fn test_review_template_parallel_initial_steps() {
    let wf = WorkflowTemplates::review();
    // check_style, review_logic, and check_security have no dependencies (can run in parallel)
    assert!(wf.steps[0].depends_on.is_empty()); // check_style
    assert!(wf.steps[1].depends_on.is_empty()); // review_logic
    assert!(wf.steps[2].depends_on.is_empty()); // check_security

    // summarize depends on all three
    let summarize = &wf.steps[3];
    assert_eq!(summarize.id, "summarize");
    assert_eq!(summarize.depends_on.len(), 3);
    assert!(summarize.depends_on.contains(&"check_style".to_string()));
    assert!(summarize.depends_on.contains(&"review_logic".to_string()));
    assert!(summarize.depends_on.contains(&"check_security".to_string()));
}

#[test]
fn test_review_template_metadata() {
    let wf = WorkflowTemplates::review();
    assert_eq!(wf.name, "review");
    assert_eq!(wf.category, "review");
    assert_eq!(wf.tags, vec!["review", "code-quality"]);
}

#[test]
fn test_refactor_template_bookend_test_steps() {
    let wf = WorkflowTemplates::refactor();
    // First and last steps are shell test commands
    let first = &wf.steps[0];
    assert_eq!(first.id, "run_tests_before");
    assert!(matches!(first.step_type, StepType::Shell { .. }));

    let last = &wf.steps[3];
    assert_eq!(last.id, "run_tests_after");
    assert!(matches!(last.step_type, StepType::Shell { .. }));
}

#[test]
fn test_refactor_template_two_required_inputs() {
    let wf = WorkflowTemplates::refactor();
    assert_eq!(wf.inputs.len(), 2);
    assert_eq!(wf.inputs[0].name, "target");
    assert!(wf.inputs[0].required);
    assert_eq!(wf.inputs[1].name, "goal");
    assert!(wf.inputs[1].required);
}

#[test]
fn test_refactor_template_after_step_retry() {
    let wf = WorkflowTemplates::refactor();
    let after_step = &wf.steps[3];
    assert_eq!(after_step.id, "run_tests_after");
    assert_eq!(after_step.retry.max_attempts, 2);
    assert_eq!(after_step.retry.delay_secs, 5);
    assert!(!after_step.retry.exponential);
}

#[test]
fn test_all_templates_have_version_1_0_0() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert_eq!(
            wf.version, "1.0.0",
            "Template '{}' should have version 1.0.0",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_author_is_selfware() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert_eq!(
            wf.author, "Selfware",
            "Template '{}' should have author Selfware",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_have_unique_step_ids() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        let mut ids = std::collections::HashSet::new();
        for step in &wf.steps {
            assert!(
                ids.insert(step.id.clone()),
                "Duplicate step id '{}' in template '{}'",
                step.id,
                wf.name
            );
        }
    }
}

#[test]
fn test_all_templates_dependencies_reference_valid_steps() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        let step_ids: std::collections::HashSet<String> =
            wf.steps.iter().map(|s| s.id.clone()).collect();
        for step in &wf.steps {
            for dep in &step.depends_on {
                assert!(
                    step_ids.contains(dep),
                    "Step '{}' in template '{}' depends on unknown step '{}'",
                    step.id,
                    wf.name,
                    dep
                );
            }
        }
    }
}

#[test]
fn test_all_templates_have_at_least_one_step() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(
            !wf.steps.is_empty(),
            "Template '{}' should have at least one step",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_have_non_empty_tags() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        assert!(
            !wf.tags.is_empty(),
            "Template '{}' should have at least one tag",
            wf.name
        );
    }
}

#[test]
fn test_all_templates_steps_have_timeouts() {
    let templates: Vec<Workflow> = vec![
        WorkflowTemplates::tdd(),
        WorkflowTemplates::debug(),
        WorkflowTemplates::review(),
        WorkflowTemplates::refactor(),
    ];
    for wf in &templates {
        for step in &wf.steps {
            assert!(
                step.timeout_secs.is_some(),
                "Step '{}' in template '{}' should have a timeout",
                step.id,
                wf.name
            );
        }
    }
}

#[test]
fn test_tdd_template_variable_substitution_in_prompts() {
    let wf = WorkflowTemplates::tdd();
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("feature", "user login");
    ctx.set_var("test_file", "tests/login_test.rs");

    // write_test step uses ${feature} placeholder
    if let StepType::Llm {
        ref prompt,
        ref context,
    } = wf.steps[0].step_type
    {
        let substituted = ctx.substitute(prompt);
        assert_eq!(substituted, "Write a failing test for: user login");
        let ctx_sub = ctx.substitute(&context[0]);
        assert_eq!(ctx_sub, "tests/login_test.rs");
    } else {
        panic!("Expected LLM step type for write_test");
    }
}

// =========================================================================
// DependencyError tests
// =========================================================================

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

// =========================================================================
// WorkflowContext recursion and cycle detection tests
// =========================================================================

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

// =========================================================================
// Log eviction tests
// =========================================================================

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

// =========================================================================
// Shell-safe substitution tests
// =========================================================================

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

// =========================================================================
// Additional coverage tests
// =========================================================================

// --- VarValue edge cases ---

#[test]
fn test_var_value_as_string_returns_none_for_list() {
    let list = VarValue::List(vec![VarValue::String("a".into())]);
    assert_eq!(list.as_string(), None);
}

#[test]
fn test_var_value_as_string_returns_none_for_map() {
    let mut m = HashMap::new();
    m.insert("k".to_string(), VarValue::String("v".into()));
    let map = VarValue::Map(m);
    assert_eq!(map.as_string(), None);
}

#[test]
fn test_var_value_as_bool_returns_none_for_list() {
    let list = VarValue::List(vec![]);
    assert_eq!(list.as_bool(), None);
}

#[test]
fn test_var_value_as_bool_returns_none_for_map() {
    let map = VarValue::Map(HashMap::new());
    assert_eq!(map.as_bool(), None);
}

#[test]
fn test_var_value_from_i32() {
    let var: VarValue = 7.into();
    assert_eq!(var.as_string(), Some("7".to_string()));
    // i32 converts to f64
    if let VarValue::Number(n) = var {
        assert!((n - 7.0).abs() < f64::EPSILON);
    } else {
        panic!("Expected Number variant");
    }
}

#[test]
fn test_var_value_from_bool_false() {
    let var: VarValue = false.into();
    assert_eq!(var.as_bool(), Some(false));
    assert_eq!(var.as_string(), Some("false".to_string()));
}

#[test]
fn test_var_value_number_as_bool_negative() {
    let var = VarValue::Number(-1.0);
    assert_eq!(var.as_bool(), Some(true));
}

// --- WorkflowContext::check_dependencies ---

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

// --- DependencyError Display ---

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

// --- Workflow execution: missing workflow ---

#[tokio::test]
async fn test_execute_missing_workflow() {
    let executor = WorkflowExecutor::new();
    let result = executor
        .execute("nonexistent", HashMap::new(), PathBuf::from("/tmp"))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// --- Workflow execution: required input missing ---

#[tokio::test]
async fn test_execute_required_input_missing() {
    let yaml = r#"
name: need_input
description: Needs a required input
inputs:
  - name: required_param
    required: true
steps:
  - id: s1
    name: S1
    type: log
    message: "hello"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("need_input", HashMap::new(), PathBuf::from("/tmp"))
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Missing required input"));
}

// --- Workflow execution: default input used ---

#[tokio::test]
async fn test_execute_default_input_used() {
    let yaml = r#"
name: default_input
description: Uses default input
inputs:
  - name: greeting
    required: false
    default: "hello"
steps:
  - id: s1
    name: S1
    type: log
    message: "${greeting}"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("default_input", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
}

// --- Workflow execution: required step failure aborts workflow ---

#[tokio::test]
async fn test_execute_required_step_failure_aborts() {
    // Build workflow programmatically to avoid YAML name collision
    let wf = Workflow {
        name: "fail_workflow".into(),
        description: "A required step fails".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![WorkflowStep {
            id: "fail_step".into(),
            name: "Fail".into(),
            description: String::new(),
            step_type: StepType::Tool {
                name: "nonexistent_tool".into(),
                args: HashMap::new(),
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: None,
            depends_on: vec![],
        }],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(wf);
    let result = executor
        .execute("fail_workflow", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
    assert_eq!(result.step_results["fail_step"].status, StepStatus::Failed);
}

// --- Workflow execution: optional step failure continues ---

#[tokio::test]
async fn test_execute_optional_step_failure_continues() {
    let wf = Workflow {
        name: "optional_fail".into(),
        description: "An optional step fails, workflow continues".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "fail_step".into(),
                name: "Fail optionally".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent_tool".into(),
                    args: HashMap::new(),
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "success_step".into(),
                name: "Should succeed".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "still running".into(),
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
        .execute("optional_fail", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(result.step_results["fail_step"].status, StepStatus::Failed);
    assert_eq!(
        result.step_results["success_step"].status,
        StepStatus::Completed
    );
}

// --- Tool step: live mode without handler ---

#[tokio::test]
async fn test_tool_step_live_no_handler() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new(); // live mode, no handler
    let step_type = StepType::Tool {
        name: "some_tool".into(),
        args: HashMap::new(),
    };
    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("tool_handler"));
}

// --- Tool step: live mode with handler ---

#[tokio::test]
async fn test_tool_step_live_with_handler() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("arg1", "val1");
    let executor = WorkflowExecutor::new().with_tool_handler(Box::new(
        |name: &str, args: &HashMap<String, String>| Ok(format!("tool={}, args={:?}", name, args)),
    ));
    let step_type = StepType::Tool {
        name: "my_tool".into(),
        args: HashMap::from([("key".into(), "${arg1}".into())]),
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.contains("my_tool"));
        assert!(s.contains("val1"));
    } else {
        panic!("Expected String result from tool handler");
    }
}

// --- LLM step: live mode without handler ---

#[tokio::test]
async fn test_llm_step_live_no_handler() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new(); // live mode, no handler
    let step_type = StepType::Llm {
        prompt: "explain".into(),
        context: vec![],
    };
    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("llm_handler"));
}

// --- LLM step: live mode with handler ---

#[tokio::test]
async fn test_llm_step_live_with_handler() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("topic", "rust");
    let executor =
        WorkflowExecutor::new().with_llm_handler(Box::new(|prompt: &str, context: &[String]| {
            Ok(format!("LLM: {} ctx={:?}", prompt, context))
        }));
    let step_type = StepType::Llm {
        prompt: "Explain ${topic}".into(),
        context: vec!["file.rs".into()],
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.contains("Explain rust"));
    } else {
        panic!("Expected String result from LLM handler");
    }
}

// --- LLM step: dry-run with variable substitution ---

#[tokio::test]
async fn test_llm_step_dryrun_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("code", "fn main() {}");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::Llm {
        prompt: "Review: ${code}".into(),
        context: vec!["${code}".into()],
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.starts_with("(dry-run) llm:"));
        assert!(s.contains("fn main()"));
    } else {
        panic!("Expected String result");
    }
}

// --- Pause step: variable substitution ---

#[tokio::test]
async fn test_pause_step_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("stage", "deploy");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::Pause {
        message: "About to ${stage}".into(),
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    assert_eq!(result.as_string(), Some("paused".to_string()));
    // Check log message contains substituted value
    let has_log = ctx
        .logs
        .iter()
        .any(|l| l.message.contains("About to deploy"));
    assert!(has_log);
}

// --- SubWorkflow step: live mode, not found ---

#[tokio::test]
async fn test_sub_workflow_not_found_live() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new(); // live mode
    let step_type = StepType::SubWorkflow {
        workflow_name: "missing_wf".into(),
        inputs: HashMap::new(),
    };
    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// --- SubWorkflow step: dry-run with variable substitution ---

#[tokio::test]
async fn test_sub_workflow_dryrun_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("env", "prod");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::SubWorkflow {
        workflow_name: "deploy".into(),
        inputs: HashMap::from([("environment".into(), "${env}".into())]),
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.contains("(dry-run) sub-workflow: deploy"));
    } else {
        panic!("Expected String result");
    }
    // Check log
    let has_log = ctx.logs.iter().any(|l| l.message.contains("deploy"));
    assert!(has_log);
}

// --- Shell step: live execution with failing command ---

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_step_live_failure() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::Shell {
        command: "exit 1".into(),
        working_dir: None,
    };
    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("exit code") || err.contains("failed"));
}

// --- Shell step: live execution success with stdout ---

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_step_live_success() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::Shell {
        command: "echo foobar".into(),
        working_dir: None,
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.contains("foobar"));
    } else {
        panic!("Expected String output");
    }
}

// --- Shell step: working_dir outside project scope (absolute path) ---

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_step_working_dir_outside_scope() {
    let mut ctx = WorkflowContext::new("/tmp/project");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::Shell {
        command: "echo test".into(),
        working_dir: Some("/etc".into()),
    };
    let result = executor.execute_step_inner(&step_type, &mut ctx).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("outside project scope"));
}

// --- Shell step: dry-run mode ---

#[tokio::test]
async fn test_shell_step_dryrun_with_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("file", "test.txt");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::Shell {
        command: "cat ${file}".into(),
        working_dir: None,
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.starts_with("(dry-run)"));
    } else {
        panic!("Expected String result");
    }
}

// --- Tool step: dry-run with variable substitution ---

#[tokio::test]
async fn test_tool_step_dryrun_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("path", "/src/main.rs");
    let executor = WorkflowExecutor::new_dry_run();
    let step_type = StepType::Tool {
        name: "file_read".into(),
        args: HashMap::from([("path".into(), "${path}".into())]),
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        assert!(s.contains("(dry-run) tool: file_read"));
    } else {
        panic!("Expected String result");
    }
}

// --- Condition step: no else steps, false condition ---

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

// --- Condition step: with variable substitution in condition ---

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

// --- evaluate_condition: step failure ---

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

// --- evaluate_condition: missing step ---

#[test]
fn test_evaluate_condition_missing_step() {
    let ctx = WorkflowContext::new("/tmp");
    assert!(!ctx.evaluate_condition("success(no_such_step)"));
    assert!(!ctx.evaluate_condition("failed(no_such_step)"));
}

// --- evaluate_condition: equality with variable substitution ---

#[test]
fn test_evaluate_condition_equality_with_vars() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("lang", "rust");
    assert!(ctx.evaluate_condition("${lang} == rust"));
    assert!(!ctx.evaluate_condition("${lang} == python"));
}

// --- evaluate_condition: defined with existing var ---

#[test]
fn test_evaluate_condition_defined_existing_var() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("myvar", "something");
    assert!(ctx.evaluate_condition("defined(myvar)"));
    assert!(!ctx.evaluate_condition("defined(other)"));
}

// --- SetVar step with variable substitution ---

#[tokio::test]
async fn test_set_var_step_with_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("base", "hello");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::SetVar {
        name: "greeting".into(),
        value: "${base} world".into(),
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    assert_eq!(result.as_string(), Some("hello world".to_string()));
    assert_eq!(
        ctx.get_var("greeting").and_then(|v| v.as_string()),
        Some("hello world".to_string())
    );
}

// --- Log step with variable substitution ---

#[tokio::test]
async fn test_log_step_with_substitution() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("version", "2.0");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::Log {
        message: "Version is ${version}".into(),
        level: LogLevel::Warn,
    };
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    assert_eq!(result.as_string(), Some("Version is 2.0".to_string()));
    let has_log = ctx
        .logs
        .iter()
        .any(|l| l.message.contains("Version is 2.0"));
    assert!(has_log);
}

// --- Input step: sets variable when default is provided ---

#[tokio::test]
async fn test_input_step_sets_variable() {
    let mut ctx = WorkflowContext::new("/tmp");
    let executor = WorkflowExecutor::new();
    let step_type = StepType::Input {
        prompt: "Name?".into(),
        variable: "user_name".into(),
        default: Some("alice".into()),
    };
    executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    assert_eq!(
        ctx.get_var("user_name").and_then(|v| v.as_string()),
        Some("alice".to_string())
    );
}

// --- Workflow execution: workflow with outputs ---

#[tokio::test]
async fn test_execute_workflow_with_outputs() {
    let yaml = r#"
name: output_wf
description: Workflow with outputs
inputs:
  - name: greeting
    default: "hi"
outputs:
  - name: result
    from: greeting
steps:
  - id: s1
    name: Noop
    type: log
    message: "ok"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("output_wf", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert!(result.get_output("result").is_some());
    assert_eq!(
        result.get_output("result").unwrap().as_string(),
        Some("hi".to_string())
    );
}

// --- Workflow execution: missing output variable is omitted ---

#[tokio::test]
async fn test_execute_workflow_missing_output_variable() {
    let yaml = r#"
name: missing_out
description: Output references missing variable
outputs:
  - name: out
    from: nonexistent_var
steps:
  - id: s1
    name: Noop
    type: log
    message: "ok"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("missing_out", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert!(result.get_output("out").is_none());
}

// --- Workflow execution: dependency failure on required step aborts ---

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

// --- Workflow execution: dependency failure on optional step skips ---

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

// --- Workflow execution: unknown dependency is fatal ---

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

// --- Workflow execution: control-flow managed steps skipped at top level ---

#[tokio::test]
async fn test_control_flow_managed_steps_skipped() {
    let yaml = r#"
name: cf_skip
description: Condition manages steps
steps:
  - id: cond
    name: Condition
    type: condition
    if: "true"
    then:
      - inner
    else:
      - inner2
  - id: inner
    name: Inner step
    type: log
    message: "inside condition"
  - id: inner2
    name: Inner step 2
    type: log
    message: "else branch"
  - id: after
    name: After condition
    type: log
    message: "after"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("cf_skip", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // inner was executed by condition (then branch)
    assert!(result.step_results.contains_key("inner"));
    // inner2 should NOT have been executed (it was in else, condition was true)
    // But it should be in control_flow_managed_steps and skipped at top level.
    // It won't be in step_results because it was not executed.
    // 'after' step should have been executed
    assert_eq!(result.step_results["after"].status, StepStatus::Completed);
}

// --- Condition with unknown step reference ---

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

// --- Loop with unknown step reference ---

#[tokio::test]
async fn test_loop_unknown_step_reference() {
    let yaml = r#"
name: loop_unknown
description: Loop references unknown step
steps:
  - id: loop
    name: Loop
    type: loop
    for: item
    in: "a, b"
    do:
      - does_not_exist
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("loop_unknown", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
}

// --- Loop step: aggregated results ---

#[tokio::test]
async fn test_loop_aggregated_results() {
    let yaml = r#"
name: loop_agg
description: Loop with aggregated results
steps:
  - id: loop
    name: Loop items
    type: loop
    for: val
    in: "x, y, z"
    do:
      - log_val
  - id: log_val
    name: Log val
    type: log
    message: "Processing: ${val}"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("loop_agg", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // Check per-iteration results
    assert!(result.step_results.contains_key("log_val@0"));
    assert!(result.step_results.contains_key("log_val@1"));
    assert!(result.step_results.contains_key("log_val@2"));
    // Check aggregate result
    assert!(result.step_results.contains_key("log_val"));
    let agg = &result.step_results["log_val"];
    assert_eq!(agg.status, StepStatus::Completed);
    if let Some(VarValue::String(s)) = &agg.output {
        assert!(s.contains("3 completed"));
        assert!(s.contains("0 failed"));
    } else {
        panic!("Expected aggregate string output");
    }
}

// --- SubWorkflow: outputs merge into parent context ---

#[tokio::test]
async fn test_sub_workflow_outputs_merge() {
    let parent_yaml = r#"
name: parent_merge
description: Parent that uses sub-workflow output
steps:
  - id: call_child
    name: Call child
    type: sub_workflow
    workflow: child_merge
  - id: use_output
    name: Use output
    type: log
    message: "Got: ${child_result}"
    depends_on:
      - call_child
"#;
    // Build child workflow programmatically (set_var has name collision in YAML)
    let child_wf = Workflow {
        name: "child_merge".into(),
        description: "Child with output".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![WorkflowOutput {
            name: "child_result".into(),
            description: String::new(),
            from: "answer".into(),
        }],
        steps: vec![WorkflowStep {
            id: "set_answer".into(),
            name: "Set answer".into(),
            description: String::new(),
            step_type: StepType::SetVar {
                name: "answer".into(),
                value: "42".into(),
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: None,
            depends_on: vec![],
        }],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(parent_yaml).unwrap();
    executor.register(child_wf);
    let result = executor
        .execute("parent_merge", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // Check that the log used the merged output
    let has_log = result.logs.iter().any(|l| l.message.contains("Got: 42"));
    assert!(
        has_log,
        "Expected child output merged into parent. Logs: {:?}",
        result.logs
    );
}

// --- SubWorkflow: failed child workflow ---

#[tokio::test]
async fn test_sub_workflow_child_fails() {
    let parent_yaml = r#"
name: parent_fail
description: Parent calling failing child
steps:
  - id: call_child
    name: Call child
    type: sub_workflow
    workflow: child_fail
    required: true
"#;
    let child_wf = Workflow {
        name: "child_fail".into(),
        description: "Child that fails".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![WorkflowStep {
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
        }],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(parent_yaml).unwrap();
    executor.register(child_wf);
    let result = executor
        .execute("parent_fail", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
}

// --- SubWorkflow: inputs passed correctly ---

#[tokio::test]
async fn test_sub_workflow_inputs_passed() {
    // Build both workflows programmatically (set_var has name collision in YAML)
    let parent_wf = Workflow {
        name: "parent_input".into(),
        description: "Parent passing inputs to child".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "set_name".into(),
                name: "Set name".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "user_name".into(),
                    value: "alice".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "call_child".into(),
                name: "Call child".into(),
                description: String::new(),
                step_type: StepType::SubWorkflow {
                    workflow_name: "child_input".into(),
                    inputs: HashMap::from([("input_name".into(), "${user_name}".into())]),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec!["set_name".into()],
            },
        ],
        tags: vec![],
    };
    let child_wf = Workflow {
        name: "child_input".into(),
        description: "Child using inputs".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![WorkflowInput {
            name: "input_name".into(),
            description: String::new(),
            required: true,
            default: None,
            param_type: "string".into(),
        }],
        outputs: vec![WorkflowOutput {
            name: "greeting".into(),
            description: String::new(),
            from: "result".into(),
        }],
        steps: vec![WorkflowStep {
            id: "greet".into(),
            name: "Greet".into(),
            description: String::new(),
            step_type: StepType::SetVar {
                name: "result".into(),
                value: "Hello ${input_name}".into(),
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: None,
            depends_on: vec![],
        }],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new();
    executor.register(parent_wf);
    executor.register(child_wf);
    let result = executor
        .execute("parent_input", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
}

// --- Workflow YAML: complex step types ---

#[test]
fn test_yaml_parsing_step_types_without_name_collision() {
    // Note: set_var and tool step types have a `name` field that collides with
    // WorkflowStep.name due to #[serde(flatten)], so we test the types that
    // parse cleanly via YAML and build the rest programmatically.
    let yaml = r#"
name: yaml_types
description: Tests YAML-safe step types
steps:
  - id: s1
    name: Log
    type: log
    message: "hello"
    level: warn
  - id: s2
    name: Shell
    type: shell
    command: "echo hi"
    working_dir: "/tmp"
  - id: s3
    name: LLM
    type: llm
    prompt: "Question"
    context:
      - "file1.rs"
  - id: s4
    name: Input
    type: input
    prompt: "Enter value"
    variable: v
    default: "def"
  - id: s5
    name: Condition
    type: condition
    if: "true"
    then:
      - s1
    else:
      - s2
  - id: s6
    name: Loop
    type: loop
    for: item
    in: "a,b"
    do:
      - s1
  - id: s7
    name: Pause
    type: pause
    message: "Wait"
  - id: s8
    name: SubWorkflow
    type: sub_workflow
    workflow: other
    inputs:
      param: "value"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let wf = executor.get("yaml_types").unwrap();
    assert_eq!(wf.steps.len(), 8);
}

#[test]
fn test_programmatic_all_step_types() {
    // Test set_var and tool step types programmatically (YAML has name collision)
    let wf = Workflow {
        name: "all_types".into(),
        description: "All step types".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "sv".into(),
                name: "SetVar".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "x".into(),
                    value: "1".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "tool".into(),
                name: "Tool".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "my_tool".into(),
                    args: HashMap::from([("key".into(), "value".into())]),
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
    let wf = executor.get("all_types").unwrap();
    assert_eq!(wf.steps.len(), 2);
    assert!(matches!(wf.steps[0].step_type, StepType::SetVar { .. }));
    assert!(matches!(wf.steps[1].step_type, StepType::Tool { .. }));
}

// --- Workflow YAML: serde roundtrip ---

#[test]
fn test_workflow_serde_yaml_roundtrip() {
    let yaml = r#"
name: roundtrip
description: Test roundtrip
version: "2.0.0"
author: "test"
category: ci
inputs:
  - name: branch
    required: true
    param_type: string
outputs:
  - name: result
    from: output_var
steps:
  - id: s1
    name: Step 1
    type: log
    message: "hello"
tags:
  - ci
  - test
"#;
    let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(wf.name, "roundtrip");
    assert_eq!(wf.version, "2.0.0");
    assert_eq!(wf.author, "test");
    assert_eq!(wf.category, "ci");
    assert_eq!(wf.inputs.len(), 1);
    assert_eq!(wf.outputs.len(), 1);
    assert_eq!(wf.tags, vec!["ci", "test"]);

    // Serialize back to YAML and re-parse
    let serialized = serde_yaml::to_string(&wf).unwrap();
    let wf2: Workflow = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(wf2.name, wf.name);
    assert_eq!(wf2.version, wf.version);
}

// --- WorkflowResult::failed_steps with multiple failed ---

#[test]
fn test_workflow_result_multiple_failed_steps() {
    let mut step_results = HashMap::new();
    step_results.insert(
        "s1".into(),
        StepResult {
            step_id: "s1".into(),
            status: StepStatus::Failed,
            output: None,
            error: Some("err1".into()),
            duration_ms: 10,
            retry_count: 0,
        },
    );
    step_results.insert(
        "s2".into(),
        StepResult {
            step_id: "s2".into(),
            status: StepStatus::Completed,
            output: None,
            error: None,
            duration_ms: 20,
            retry_count: 0,
        },
    );
    step_results.insert(
        "s3".into(),
        StepResult {
            step_id: "s3".into(),
            status: StepStatus::Failed,
            output: None,
            error: Some("err3".into()),
            duration_ms: 30,
            retry_count: 1,
        },
    );
    let result = WorkflowResult {
        workflow_name: "test".into(),
        status: WorkflowStatus::Failed,
        outputs: HashMap::new(),
        step_results,
        logs: VecDeque::new(),
        duration_ms: 60,
    };
    let failed = result.failed_steps();
    assert_eq!(failed.len(), 2);
}

// --- WorkflowExecutor::with_tool_handler / with_llm_handler ---

#[test]
fn test_executor_builder_methods() {
    let executor = WorkflowExecutor::new()
        .with_tool_handler(Box::new(|_name, _args| Ok("ok".to_string())))
        .with_llm_handler(Box::new(|_prompt, _ctx| Ok("ok".to_string())));
    // Just verify it compiles and the handlers are set
    assert!(executor.tool_handler.is_some());
    assert!(executor.llm_handler.is_some());
}

// --- load_file: nonexistent file ---

#[test]
fn test_load_file_nonexistent() {
    let mut executor = WorkflowExecutor::new();
    let result = executor.load_file(Path::new("/tmp/nonexistent_workflow_file.yaml"));
    assert!(result.is_err());
}

// --- load_file: valid file ---

#[test]
fn test_load_file_valid() {
    use std::io::Write;
    let yaml = r#"
name: from_file
description: Loaded from file
steps:
  - id: s1
    name: Step
    type: log
    message: "hello"
"#;
    let dir = std::env::temp_dir().join("selfware_test_load_file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test_workflow.yaml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let mut executor = WorkflowExecutor::new();
    assert!(executor.load_file(&path).is_ok());
    assert!(executor.get("from_file").is_some());

    // Cleanup
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

// --- Substitute: no variables set, template unchanged ---

#[test]
fn test_substitute_no_vars() {
    let ctx = WorkflowContext::new("/tmp");
    let result = ctx.substitute("no vars here");
    assert_eq!(result, "no vars here");
}

// --- Substitute: both brace and dollar syntax in same string ---

#[test]
fn test_substitute_mixed_syntax() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("a", "AAA");
    ctx.set_var("b", "BBB");
    let result = ctx.substitute("${a} and $b");
    assert_eq!(result, "AAA and BBB");
}

// --- Shell-safe substitute: empty value ---

#[test]
fn test_substitute_shell_safe_empty_value() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("empty", "");
    let result = ctx.substitute_shell_safe("echo ${empty}");
    assert_eq!(result, "echo ''");
}

// --- Shell-safe substitute: multiple variables ---

#[test]
fn test_substitute_shell_safe_multiple_vars() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("a", "hello world");
    ctx.set_var("b", "foo;bar");
    let result = ctx.substitute_shell_safe("echo ${a} ${b}");
    assert_eq!(result, "echo 'hello world' 'foo;bar'");
}

// --- shell_quote: empty string ---

#[test]
fn test_shell_quote_empty() {
    let result = WorkflowContext::shell_quote("");
    assert_eq!(result, "''");
}

// --- shell_quote: multiple single quotes ---

#[test]
fn test_shell_quote_multiple_single_quotes() {
    let result = WorkflowContext::shell_quote("it's a 'test'");
    assert_eq!(result, "'it'\\''s a '\\''test'\\'''");
}

// --- WorkflowStatus serde ---

#[test]
fn test_workflow_status_serde_json() {
    let status = WorkflowStatus::Running;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"running\"");
    let deserialized: WorkflowStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, WorkflowStatus::Running);
}

#[test]
fn test_step_status_serde_json() {
    let status = StepStatus::Skipped;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"skipped\"");
    let deserialized: StepStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, StepStatus::Skipped);
}

// --- WorkflowStatus serde all variants ---

#[test]
fn test_workflow_status_serde_all_variants() {
    let variants = [
        (WorkflowStatus::Pending, "\"pending\""),
        (WorkflowStatus::Running, "\"running\""),
        (WorkflowStatus::Completed, "\"completed\""),
        (WorkflowStatus::Failed, "\"failed\""),
        (WorkflowStatus::Paused, "\"paused\""),
        (WorkflowStatus::Cancelled, "\"cancelled\""),
    ];
    for (status, expected_json) in variants {
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, expected_json);
    }
}

#[test]
fn test_step_status_serde_all_variants() {
    let variants = [
        (StepStatus::Pending, "\"pending\""),
        (StepStatus::Running, "\"running\""),
        (StepStatus::Completed, "\"completed\""),
        (StepStatus::Failed, "\"failed\""),
        (StepStatus::Skipped, "\"skipped\""),
    ];
    for (status, expected_json) in variants {
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, expected_json);
    }
}

// --- VarValue serde ---

#[test]
fn test_var_value_serde_string() {
    let val = VarValue::String("hello".into());
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: VarValue = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.as_string(), Some("hello".to_string()));
}

#[test]
fn test_var_value_serde_number() {
    let val = VarValue::Number(3.14);
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: VarValue = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.as_string(), Some("3.14".to_string()));
}

#[test]
fn test_var_value_serde_boolean() {
    let val = VarValue::Boolean(true);
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: VarValue = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.as_bool(), Some(true));
}

#[test]
fn test_var_value_serde_null() {
    let val = VarValue::Null;
    let json = serde_json::to_string(&val).unwrap();
    assert_eq!(json, "null");
}

// --- Condition step: live execution with nested step having dependency on prior step ---

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

// --- Condition step: live execution with nested required step that fails ---

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

// --- Loop: required step fails in iteration ---

#[tokio::test]
async fn test_loop_required_step_fails() {
    let wf = Workflow {
        name: "loop_fail".into(),
        description: "Required step fails in loop".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "loop".into(),
                name: "Loop".into(),
                description: String::new(),
                step_type: StepType::Loop {
                    variable: "item".into(),
                    items: "a, b".into(),
                    do_steps: vec!["fail_inner".into()],
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "fail_inner".into(),
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
        .execute("loop_fail", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
}

// --- Loop: optional step fails in iteration, loop continues ---

#[tokio::test]
async fn test_loop_optional_step_fails_continues() {
    let wf = Workflow {
        name: "loop_opt_fail".into(),
        description: "Optional step fails in loop".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "loop".into(),
                name: "Loop".into(),
                description: String::new(),
                step_type: StepType::Loop {
                    variable: "item".into(),
                    items: "a, b".into(),
                    do_steps: vec!["fail_inner".into()],
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "fail_inner".into(),
                name: "Fail optionally".into(),
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
        .execute("loop_opt_fail", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    // Loop completes even though inner steps failed (they are optional)
    assert!(result.is_success());
    // Check aggregate: 0 completed, 2 failed
    let agg = &result.step_results["fail_inner"];
    assert_eq!(agg.status, StepStatus::Failed);
}

// --- Loop step: without workflow context (dry-run style) ---

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

// --- Condition branch: optional step with unsatisfied dependency gets skipped ---

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

// --- Multiple substitutions in same string ---

#[test]
fn test_substitute_repeated_var() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("x", "val");
    let result = ctx.substitute("${x} and ${x} again");
    assert_eq!(result, "val and val again");
}

// --- Substitute: List and Map values not substituted ---

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

// --- WorkflowContext: initial state verification ---

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

// --- Executor: register replaces workflow with same name ---

#[test]
fn test_executor_register_replaces_same_name() {
    let mut executor = WorkflowExecutor::new();
    let wf1 = Workflow {
        name: "test".into(),
        description: "first".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![],
        tags: vec![],
    };
    let wf2 = Workflow {
        name: "test".into(),
        description: "second".into(),
        version: "2.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![],
        tags: vec![],
    };
    executor.register(wf1);
    executor.register(wf2);
    assert_eq!(executor.list().len(), 1);
    assert_eq!(executor.get("test").unwrap().description, "second");
}

// --- Shell step: shell-safe quoting is used for command variables ---

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

// --- Workflow execution: step executed in order ---

#[tokio::test]
async fn test_steps_execute_in_order() {
    let wf = Workflow {
        name: "ordered".into(),
        description: "Steps in order".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "s1".into(),
                name: "First".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "result".into(),
                    value: "first".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s2".into(),
                name: "Second".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "result".into(),
                    value: "second".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s3".into(),
                name: "Third".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "result".into(),
                    value: "third".into(),
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
        .execute("ordered", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    // All 3 steps completed
    assert_eq!(result.step_results.len(), 3);
}

// --- DependencyError: clone ---

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

// --- DependencyError: debug ---

#[test]
fn test_dependency_error_debug() {
    let err = DependencyError::Unknown("x".into());
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("Unknown"));
}

// --- Condition execution: condition references step that was already set up ---

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

// --- Condition execution: condition evaluates failed() ---

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

// --- Workflow execution: with_tool_handler integration in full workflow ---

#[tokio::test]
async fn test_full_workflow_with_tool_handler() {
    let wf = Workflow {
        name: "tool_wf".into(),
        description: "Workflow using tool handler".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![WorkflowStep {
            id: "use_tool".into(),
            name: "Use tool".into(),
            description: String::new(),
            step_type: StepType::Tool {
                name: "test_tool".into(),
                args: HashMap::from([("param".into(), "value".into())]),
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: None,
            depends_on: vec![],
        }],
        tags: vec![],
    };
    let mut executor = WorkflowExecutor::new().with_tool_handler(Box::new(
        |name: &str, _args: &HashMap<String, String>| Ok(format!("result from {}", name)),
    ));
    executor.register(wf);
    let result = executor
        .execute("tool_wf", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    let step = &result.step_results["use_tool"];
    assert_eq!(step.status, StepStatus::Completed);
    if let Some(VarValue::String(s)) = &step.output {
        assert!(s.contains("result from test_tool"));
    }
}

// --- Workflow execution: with_llm_handler integration in full workflow ---

#[tokio::test]
async fn test_full_workflow_with_llm_handler() {
    let yaml = r#"
name: llm_wf
description: Workflow using LLM handler
steps:
  - id: ask_llm
    name: Ask LLM
    type: llm
    prompt: "What is Rust?"
    context:
      - "programming"
"#;
    let mut executor =
        WorkflowExecutor::new().with_llm_handler(Box::new(|prompt: &str, _ctx: &[String]| {
            Ok(format!("Answer to: {}", prompt))
        }));
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("llm_wf", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    let step = &result.step_results["ask_llm"];
    if let Some(VarValue::String(s)) = &step.output {
        assert!(s.contains("Answer to: What is Rust?"));
    }
}

// --- Loop with dependency on step OUTSIDE loop ---

#[tokio::test]
async fn test_loop_dep_on_step_outside_loop() {
    let wf = Workflow {
        name: "loop_ext_dep".into(),
        description: "Loop step depends on external step".into(),
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
                    name: "base".into(),
                    value: "hello".into(),
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "loop".into(),
                name: "Loop".into(),
                description: String::new(),
                step_type: StepType::Loop {
                    variable: "item".into(),
                    items: "x, y".into(),
                    do_steps: vec!["inner".into()],
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec!["setup".into()],
            },
            WorkflowStep {
                id: "inner".into(),
                name: "Inner".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "${base} ${item}".into(),
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
        .execute("loop_ext_dep", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(result.step_results["inner@0"].status, StepStatus::Completed);
    assert_eq!(result.step_results["inner@1"].status, StepStatus::Completed);
}

// --- Workflow: condition with no else and false condition ---

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

// --- RetryConfig: serialization ---

#[test]
fn test_retry_config_serde() {
    let config = RetryConfig {
        max_attempts: 5,
        delay_secs: 10,
        exponential: true,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: RetryConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.max_attempts, 5);
    assert_eq!(deserialized.delay_secs, 10);
    assert!(deserialized.exponential);
}

// --- LogLevel serde ---

#[test]
fn test_log_level_serde() {
    let levels = [
        (LogLevel::Debug, "\"debug\""),
        (LogLevel::Info, "\"info\""),
        (LogLevel::Warn, "\"warn\""),
        (LogLevel::Error, "\"error\""),
    ];
    for (level, expected) in levels {
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, expected);
    }
}

// --- Workflow: multiple steps with mixed required/optional ---

#[tokio::test]
async fn test_mixed_required_optional_steps() {
    let wf = Workflow {
        name: "mixed".into(),
        description: "Mixed required and optional".into(),
        version: "1.0.0".into(),
        author: String::new(),
        category: String::new(),
        inputs: vec![],
        outputs: vec![],
        steps: vec![
            WorkflowStep {
                id: "s1".into(),
                name: "Step 1".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "first".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s2".into(),
                name: "Step 2 (optional fail)".into(),
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
                id: "s3".into(),
                name: "Step 3".into(),
                description: String::new(),
                step_type: StepType::Log {
                    message: "third".into(),
                    level: LogLevel::Info,
                },
                required: true,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s4".into(),
                name: "Step 4 (optional fail)".into(),
                description: String::new(),
                step_type: StepType::Tool {
                    name: "nonexistent2".into(),
                    args: HashMap::new(),
                },
                required: false,
                retry: RetryConfig::default(),
                timeout_secs: None,
                depends_on: vec![],
            },
            WorkflowStep {
                id: "s5".into(),
                name: "Step 5".into(),
                description: String::new(),
                step_type: StepType::SetVar {
                    name: "final_val".into(),
                    value: "done".into(),
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
        .execute("mixed", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(result.step_results["s1"].status, StepStatus::Completed);
    assert_eq!(result.step_results["s2"].status, StepStatus::Failed);
    assert_eq!(result.step_results["s3"].status, StepStatus::Completed);
    assert_eq!(result.step_results["s4"].status, StepStatus::Failed);
    assert_eq!(result.step_results["s5"].status, StepStatus::Completed);
}
