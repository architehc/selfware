use super::*;
use std::path::PathBuf;

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

#[tokio::test]
async fn test_execute_missing_workflow() {
    let executor = WorkflowExecutor::new();
    let result = executor
        .execute("nonexistent", HashMap::new(), PathBuf::from("/tmp"))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

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

#[tokio::test]
async fn test_llm_step_live_with_handler() {
    let mut ctx = WorkflowContext::new("/tmp");
    ctx.set_var("topic", "rust");
    let executor = WorkflowExecutor::new().with_llm_handler(|prompt: &str, context: &[String]| {
        Ok(format!("LLM: {} ctx={:?}", prompt, context))
    });
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

#[test]
fn test_executor_builder_methods() {
    let executor = WorkflowExecutor::new()
        .with_tool_handler(Box::new(|_name, _args| Ok("ok".to_string())))
        .with_llm_handler(|_prompt: &str, _ctx: &[String]| Ok("ok".to_string()));
    // Just verify it compiles and the handlers are set
    assert!(executor.tool_handler.is_some());
    assert!(executor.llm_handler.is_some());
}

#[test]
fn test_load_file_nonexistent() {
    let mut executor = WorkflowExecutor::new();
    let result = executor.load_file(Path::new("/tmp/nonexistent_workflow_file.yaml"));
    assert!(result.is_err());
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tool_step_with_registry_backed_handler() {
    // This test mirrors the CLI's `build_workflow_tool_handler`: it constructs
    // a real ToolRegistry, wraps it in a closure that bridges sync→async, and
    // runs a Tool step through it.  We use `file_read` (a critical tool that's
    // always activated) with a known file to get a deterministic result.
    // Uses multi_thread flavor so `block_in_place` is available.
    use crate::tools::ToolRegistry;
    use std::sync::Arc;

    let registry = Arc::new(ToolRegistry::new());
    let handler: crate::workflows::ToolHandler =
        Box::new(move |name: &str, args: &HashMap<String, String>| {
            let registry = Arc::clone(&registry);
            let mut json_map = serde_json::Map::new();
            for (k, v) in args {
                let parsed = serde_json::from_str::<serde_json::Value>(v)
                    .unwrap_or(serde_json::Value::String(v.clone()));
                json_map.insert(k.clone(), parsed);
            }
            let input = serde_json::Value::Object(json_map);
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async move { registry.execute_any(name, input).await })
            })
            .map(|v| v.to_string())
        });

    let mut ctx = WorkflowContext::new("/tmp");
    // file_read requires a "path" argument; point at this test file for a
    // deterministic, small read.
    let test_file =
        env!("CARGO_MANIFEST_DIR").to_string() + "/src/orchestration/workflows/test_execution.rs";
    let step_type = StepType::Tool {
        name: "file_read".into(),
        args: HashMap::from([("path".into(), test_file)]),
    };
    let executor = WorkflowExecutor::new().with_tool_handler(handler);
    let result = executor
        .execute_step_inner(&step_type, &mut ctx)
        .await
        .unwrap();
    if let VarValue::String(s) = result {
        // file_read returns a JSON object with "content" or "success" — just
        // verify we got a non-empty, JSON-shaped string back from the registry.
        assert!(!s.is_empty(), "tool result should not be empty");
        let parsed: serde_json::Value =
            serde_json::from_str(&s).expect("tool result should be valid JSON from the registry");
        assert!(parsed.is_object(), "file_read should return a JSON object");
    } else {
        panic!("Expected String result from tool handler");
    }
}

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
    let mut executor = WorkflowExecutor::new()
        .with_llm_handler(|prompt: &str, _ctx: &[String]| Ok(format!("Answer to: {}", prompt)));
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

#[tokio::test]
async fn test_step_output_is_available_to_later_steps() {
    let yaml = r#"
name: chained_llm
description: Later LLM steps can reference earlier outputs
outputs:
  - name: final_text
    from: refine
steps:
  - id: draft
    name: Draft
    type: llm
    prompt: "draft brief"
  - id: refine
    name: Refine
    type: llm
    prompt: "refine ${draft}"
    depends_on:
      - draft
"#;

    let mut executor = WorkflowExecutor::new()
        .with_llm_handler(|prompt: &str, _ctx: &[String]| Ok(prompt.to_string()));
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("chained_llm", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(
        result.outputs.get("final_text").and_then(|v| v.as_string()),
        Some("refine draft brief".to_string())
    );
}

#[tokio::test]
async fn test_workflow_telemetry_records_llm_usage() {
    let yaml = r#"
name: llm_telemetry
description: Workflow telemetry should capture LLM usage
steps:
  - id: ask_llm
    name: Ask LLM
    type: llm
    prompt: "status"
"#;

    let mut executor =
        WorkflowExecutor::new().with_llm_handler(|_prompt: &str, _ctx: &[String]| {
            Ok(LlmCallOutput::text("ok")
                .with_model("test-model")
                .with_usage(LlmTokenUsage {
                    prompt_tokens: 11,
                    completion_tokens: 7,
                    total_tokens: 18,
                })
                .with_estimated_cost(0.000138))
        });
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("llm_telemetry", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(result.telemetry.llm_calls, 1);
    assert_eq!(result.telemetry.prompt_tokens, 11);
    assert_eq!(result.telemetry.completion_tokens, 7);
    assert_eq!(result.telemetry.total_tokens, 18);
    assert_eq!(result.telemetry.completed_steps, 1);
    assert_eq!(result.telemetry.failed_steps, 0);
    assert_eq!(result.telemetry.skipped_steps, 0);
    assert!((result.telemetry.estimated_cost_usd - 0.000138).abs() < f64::EPSILON);
}

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
