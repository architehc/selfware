//! Tests for the `until` conditional loop step.

use super::*;

/// A fix/re-test loop: `bump` appends a line to `count.txt`, `check`
/// fails until the file has `green_at` lines.
fn fix_until_green_yaml(max_iterations: &str, green_at: u32, extra: &str) -> String {
    format!(
        r#"
name: fix_loop
steps:
  - id: until_green
    name: Fix until green
    type: until
    do: [bump, check]
    until: success(check)
{max_iterations}
{extra}
  - id: bump
    name: Apply a fix
    type: shell
    command: "echo fix >> count.txt"
  - id: check
    name: Re-test
    type: shell
    command: "test $(wc -l < count.txt) -ge {green_at}"
"#
    )
}

fn line_count(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("count.txt"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[tokio::test]
async fn until_runs_passes_until_the_condition_holds() {
    let dir = tempfile::tempdir().unwrap();
    let mut executor = WorkflowExecutor::new();
    executor
        .load_yaml(&fix_until_green_yaml("    max_iterations: 5", 3, ""))
        .unwrap();

    let result = executor
        .execute("fix_loop", HashMap::new(), dir.path().to_path_buf())
        .await
        .unwrap();

    assert!(result.is_success(), "logs: {:?}", result.logs);
    // Exactly three passes: the body steps never ran in the top-level pass.
    assert_eq!(line_count(dir.path()), 3);
    assert_eq!(
        result.step_results["until_green"].status,
        StepStatus::Completed
    );
    // The body results are the final pass's: the re-test is green.
    assert_eq!(result.step_results["check"].status, StepStatus::Completed);
    assert!(result
        .logs
        .iter()
        .any(|l| l.message.contains("met after 3 pass(es)")));
}

#[tokio::test]
async fn until_exhausted_fails_with_a_typed_error_naming_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mut executor = WorkflowExecutor::new();
    executor
        .load_yaml(&fix_until_green_yaml("    max_iterations: 2", 10, ""))
        .unwrap();

    let result = executor
        .execute("fix_loop", HashMap::new(), dir.path().to_path_buf())
        .await
        .unwrap();

    assert_eq!(result.status, WorkflowStatus::Failed);
    assert_eq!(line_count(dir.path()), 2, "stopped at the cap");
    let until = &result.step_results["until_green"];
    assert_eq!(until.status, StepStatus::Failed);
    let err = until.error.as_deref().unwrap();
    assert!(err.contains("max_iterations=2"), "{err}");
    assert!(err.contains("success(check)"), "{err}");
}

#[tokio::test]
async fn until_on_exhausted_continue_lets_the_workflow_proceed() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = r#"
name: tolerant
steps:
  - id: until_green
    name: Try twice
    type: until
    do: [check]
    until: success(check)
    max_iterations: 2
    on_exhausted: continue
  - id: check
    name: Re-test
    type: shell
    command: "exit 1"
    required: false
  - id: after
    name: After
    type: set_var
    var: reached
    value: "yes"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("tolerant", HashMap::new(), dir.path().to_path_buf())
        .await
        .unwrap();

    assert!(result.is_success(), "logs: {:?}", result.logs);
    assert_eq!(
        result.step_results["until_green"].status,
        StepStatus::Completed
    );
    assert_eq!(result.step_results["after"].status, StepStatus::Completed);
    assert!(result
        .logs
        .iter()
        .any(|l| l.message.contains("not met after 2 pass(es); continuing")));
}

#[tokio::test]
async fn until_required_body_step_still_failing_after_continue_fails_the_workflow() {
    // on_exhausted: continue does not paper over a REQUIRED body step whose
    // last pass failed — the workflow reports Failed (AGENTS.md §3).
    let dir = tempfile::tempdir().unwrap();
    let yaml = r#"
name: strict_body
steps:
  - id: until_green
    name: Try twice
    type: until
    do: [check]
    until: success(check)
    max_iterations: 2
    on_exhausted: continue
  - id: check
    name: Re-test
    type: shell
    command: "exit 1"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("strict_body", HashMap::new(), dir.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
}

#[test]
fn until_without_max_iterations_does_not_parse() {
    let yaml = fix_until_green_yaml("", 1, "");
    let err = WorkflowExecutor::new().load_yaml(&yaml).unwrap_err();
    assert!(err.to_string().contains("max_iterations"), "{err}");
}

#[test]
fn until_rejects_an_unknown_on_exhausted_value() {
    let yaml = fix_until_green_yaml("    max_iterations: 2", 1, "    on_exhausted: retry");
    assert!(WorkflowExecutor::new().load_yaml(&yaml).is_err());
}

#[tokio::test]
async fn until_with_zero_max_iterations_is_a_definition_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut executor = WorkflowExecutor::new();
    executor
        .load_yaml(&fix_until_green_yaml("    max_iterations: 0", 1, ""))
        .unwrap();
    let result = executor
        .execute("fix_loop", HashMap::new(), dir.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
    assert_eq!(line_count(dir.path()), 0, "no pass ran");
    let err = result.step_results["until_green"].error.clone().unwrap();
    assert!(err.contains("at least 1"), "{err}");
}

#[tokio::test]
async fn until_max_iterations_is_clamped_to_the_hard_ceiling() {
    let yaml = r#"
name: runaway
steps:
  - id: forever
    name: Never true
    type: until
    do: [tick]
    until: "false"
    max_iterations: 100000
    timeout_secs: 60
  - id: tick
    name: Tick
    type: log
    message: tick
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let result = executor
        .execute("runaway", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
    let err = result.step_results["forever"].error.clone().unwrap();
    assert!(
        err.contains(&format!("max_iterations={MAX_UNTIL_ITERATIONS}")),
        "{err}"
    );
    assert!(result.logs.iter().any(|l| l.message.contains("clamped")));
}

#[test]
fn until_error_is_typed_and_downcastable() {
    let err: anyhow::Error = UntilExhaustedError {
        max_iterations: 3,
        condition: "success(t)".into(),
    }
    .into();
    let typed = err.downcast_ref::<UntilExhaustedError>().unwrap();
    assert_eq!(typed.max_iterations, 3);
}

#[test]
fn until_default_timeout_scales_with_the_effective_cap() {
    let until = |n| StepType::Until {
        do_steps: vec![],
        condition: "true".into(),
        max_iterations: n,
        on_exhausted: UntilExhausted::Fail,
    };
    assert_eq!(default_step_timeout(&until(4)), Duration::from_secs(1200));
    assert_eq!(
        default_step_timeout(&until(u32::MAX)),
        Duration::from_secs(300 * u64::from(MAX_UNTIL_ITERATIONS))
    );
    assert_eq!(
        default_step_timeout(&StepType::Pause {
            message: String::new()
        }),
        Duration::from_secs(300)
    );
}

#[tokio::test]
async fn until_is_bounded_by_the_run_wide_step_execution_ceiling() {
    // 100 passes x 101 body steps = 10_100 executions > MAX_STEP_EXECUTIONS:
    // the loop stops on the run-wide ceiling, not after 100 passes.
    let body: Vec<String> = (0..101).map(|i| format!("t{i}")).collect();
    let mut yaml = format!(
        "name: wide\nsteps:\n  - id: spin\n    name: Spin\n    type: until\n    do: [{}]\n    until: \"false\"\n    max_iterations: 100\n    timeout_secs: 600\n",
        body.join(", ")
    );
    for id in &body {
        yaml.push_str(&format!(
            "  - {{ id: {id}, name: {id}, type: log, message: x, required: false }}\n"
        ));
    }
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(&yaml).unwrap();
    let result = executor
        .execute("wide", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
    let err = result.step_results["spin"].error.clone().unwrap();
    assert!(err.contains("MAX_STEP_EXECUTIONS"), "{err}");
}
