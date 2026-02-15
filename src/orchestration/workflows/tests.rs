
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
        logs: Vec::new(),
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
        logs: vec![],
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
        logs: vec![],
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
        logs: vec![],
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
        logs: vec![],
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
