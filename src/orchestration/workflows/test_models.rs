use super::*;

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
fn test_workflow_result_helpers() {
    let result = WorkflowResult {
        workflow_name: "test".to_string(),
        status: WorkflowStatus::Completed,
        outputs: HashMap::from([("out".to_string(), VarValue::String("value".into()))]),
        step_results: HashMap::new(),
        logs: VecDeque::new(),
        duration_ms: 1000,
        telemetry: WorkflowTelemetry::default(),
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
        telemetry: WorkflowTelemetry::default(),
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
        telemetry: WorkflowTelemetry::default(),
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
        telemetry: WorkflowTelemetry::default(),
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
        telemetry: WorkflowTelemetry::default(),
    };

    let failed = result.failed_steps();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].step_id, "step2");
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

#[test]
fn test_yaml_parsing_step_types_without_name_collision() {
    // The set_var, tool, and guardrail step types carry a `name` field that
    // used to collide with `WorkflowStep.name` under #[serde(flatten)],
    // making them unparseable from YAML. Their YAML keys are now `var`,
    // `tool`, and `guardrail` respectively, so every step type parses.
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
  - id: s9
    name: Tool
    type: tool
    tool: file_read
    args:
      path: "src/main.rs"
  - id: s10
    name: SetVar
    type: set_var
    var: x
    value: "1"
  - id: s11
    name: Guardrail
    type: guardrail
    guardrail: policy
    condition: "true"
    on_violation: warn
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let wf = executor.get("yaml_types").unwrap();
    assert_eq!(wf.steps.len(), 11);
    assert!(matches!(wf.steps[8].step_type, StepType::Tool { .. }));
    assert!(matches!(wf.steps[9].step_type, StepType::SetVar { .. }));
    assert!(matches!(wf.steps[10].step_type, StepType::Guardrail { .. }));
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
        telemetry: WorkflowTelemetry::default(),
    };
    let failed = result.failed_steps();
    assert_eq!(failed.len(), 2);
}

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

#[test]
fn test_var_value_serde_string() {
    let val = VarValue::String("hello".into());
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: VarValue = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.as_string(), Some("hello".to_string()));
}

#[test]
fn test_var_value_serde_number() {
    let val = VarValue::Number(1.23);
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: VarValue = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.as_string(), Some("1.23".to_string()));
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
