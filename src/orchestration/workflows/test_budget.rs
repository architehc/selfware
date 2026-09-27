//! Tests for workflow-level budgets (`max_wall_secs`, `max_tokens`).

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// An LLM handler that reports `tokens_per_call` measured tokens and counts
/// its calls.
fn metered_executor(tokens_per_call: u64, calls: Arc<AtomicUsize>) -> WorkflowExecutor {
    WorkflowExecutor::new().with_llm_handler(move |_prompt: &str, _ctx: &[String]| {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCallOutput::text("ok").with_usage(LlmTokenUsage {
            prompt_tokens: tokens_per_call / 2,
            completion_tokens: tokens_per_call / 2,
            total_tokens: tokens_per_call,
        }))
    })
}

const THREE_LLM_STEPS: &str = r#"
name: three_calls
max_tokens: 100
steps:
  - id: one
    name: One
    type: llm
    prompt: first
  - id: two
    name: Two
    type: llm
    prompt: second
  - id: three
    name: Three
    type: llm
    prompt: third
"#;

#[test]
fn budget_is_parsed_from_the_workflow_yaml() {
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(THREE_LLM_STEPS).unwrap();
    assert_eq!(
        executor.budget("three_calls"),
        WorkflowBudget {
            max_wall_secs: None,
            max_tokens: Some(100),
        }
    );
    assert!(executor.budget("unknown").is_unbounded());
}

#[tokio::test]
async fn token_budget_stops_between_steps_and_keeps_partial_results() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut executor = metered_executor(60, calls.clone());
    executor.load_yaml(THREE_LLM_STEPS).unwrap();

    let result = executor
        .execute("three_calls", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert_eq!(result.status, WorkflowStatus::Failed);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "third step never started");
    assert_eq!(result.step_results["one"].status, StepStatus::Completed);
    assert_eq!(result.step_results["two"].status, StepStatus::Completed);
    assert!(!result.step_results.contains_key("three"));
    assert_eq!(result.telemetry.total_tokens, 120);
    assert_eq!(
        result.stop_reason,
        Some(WorkflowStopReason::TokenBudget {
            max_tokens: 100,
            used_tokens: 120,
            unmetered_llm_calls: 0,
        })
    );
    assert!(result
        .logs
        .iter()
        .any(|l| l.message.contains("stopped before step 'three'")));
}

#[tokio::test]
async fn unmetered_calls_are_counted_and_named_not_guessed() {
    // A handler that reports no usage cannot be held to max_tokens; the run
    // completes, but the gap is counted and logged (AGENTS.md §3/§4).
    let mut executor = WorkflowExecutor::new()
        .with_llm_handler(|_p: &str, _c: &[String]| Ok("no usage".to_string()));
    executor.load_yaml(THREE_LLM_STEPS).unwrap();

    let result = executor
        .execute("three_calls", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert!(result.is_success());
    assert_eq!(result.telemetry.unmetered_llm_calls, 3);
    assert!(result.stop_reason.is_none());
    assert!(result
        .logs
        .iter()
        .any(|l| l.message.contains("3 LLM call(s) reported no token usage")));
}

#[tokio::test]
async fn wall_clock_budget_stops_before_the_next_step() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = r#"
name: slow
max_wall_secs: 1
steps:
  - id: nap
    name: Nap
    type: shell
    command: "sleep 1.2"
  - id: after
    name: After
    type: shell
    command: "touch after.txt"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();

    let result = executor
        .execute("slow", HashMap::new(), dir.path().to_path_buf())
        .await
        .unwrap();

    assert_eq!(result.status, WorkflowStatus::Failed);
    assert_eq!(result.step_results["nap"].status, StepStatus::Completed);
    assert!(!dir.path().join("after.txt").exists());
    match result.stop_reason {
        Some(WorkflowStopReason::WallClockBudget {
            max_wall_secs: 1,
            elapsed_ms,
        }) => assert!(elapsed_ms >= 1000),
        other => panic!("unexpected stop reason: {other:?}"),
    }
}

#[tokio::test]
async fn token_budget_stops_an_until_loop_between_passes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut executor = metered_executor(60, calls.clone());
    executor
        .load_yaml(
            r#"
name: fix_loop
max_tokens: 100
steps:
  - id: loop
    name: Fix until never
    type: until
    do: [fix]
    until: "false"
    max_iterations: 10
  - id: fix
    name: Fix
    type: llm
    prompt: fix it
"#,
        )
        .unwrap();

    let result = executor
        .execute("fix_loop", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2, "stopped after pass 2");
    assert_eq!(result.status, WorkflowStatus::Failed);
    assert!(matches!(
        result.stop_reason,
        Some(WorkflowStopReason::TokenBudget { .. })
    ));
    let err = result.step_results["loop"].error.clone().unwrap();
    assert!(err.contains("token budget exhausted"), "{err}");
}

#[tokio::test]
async fn optional_step_stopped_by_budget_still_reports_failed() {
    // The budget reason is the verdict even when the step it cut short was
    // optional and last — never "Completed" for a run that was stopped.
    let calls = Arc::new(AtomicUsize::new(0));
    let mut executor = metered_executor(60, calls.clone());
    executor
        .load_yaml(
            r#"
name: optional_loop
max_tokens: 100
steps:
  - id: loop
    name: Fix until never
    type: until
    do: [fix]
    until: "false"
    max_iterations: 10
    required: false
  - id: fix
    name: Fix
    type: llm
    prompt: fix it
    required: false
"#,
        )
        .unwrap();
    let result = executor
        .execute("optional_loop", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
    assert!(result.stop_reason.is_some());
}

#[tokio::test]
async fn sub_workflow_tokens_count_against_the_parent() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut executor = metered_executor(40, calls.clone());
    executor
        .load_yaml(
            r#"
name: parent
max_tokens: 100
steps:
  - id: child_a
    name: A
    type: sub_workflow
    workflow: child
  - id: child_b
    name: B
    type: sub_workflow
    workflow: child
  - id: child_c
    name: C
    type: sub_workflow
    workflow: child
"#,
        )
        .unwrap();
    executor
        .load_yaml(
            r#"
name: child
steps:
  - id: ask
    name: Ask
    type: llm
    prompt: hi
"#,
        )
        .unwrap();

    let result = executor
        .execute("parent", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();

    assert_eq!(result.telemetry.total_tokens, 120);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(result.stop_reason.is_none(), "stopped only BETWEEN steps");

    // With a fourth call the budget would have stopped it.
    let mut executor = metered_executor(40, Arc::new(AtomicUsize::new(0)));
    executor
        .load_yaml(
            r#"
name: parent4
max_tokens: 100
steps:
  - { id: a, name: A, type: sub_workflow, workflow: child }
  - { id: b, name: B, type: sub_workflow, workflow: child }
  - { id: c, name: C, type: sub_workflow, workflow: child }
  - { id: d, name: D, type: sub_workflow, workflow: child }
"#,
        )
        .unwrap();
    executor
        .load_yaml("name: child\nsteps:\n  - { id: ask, name: Ask, type: llm, prompt: hi }\n")
        .unwrap();
    let result = executor
        .execute("parent4", HashMap::new(), PathBuf::from("/tmp"))
        .await
        .unwrap();
    assert_eq!(result.status, WorkflowStatus::Failed);
    assert!(!result.step_results.contains_key("d"));
}

#[test]
fn measured_tokens_fall_back_to_prompt_plus_completion() {
    let telemetry = WorkflowTelemetry {
        prompt_tokens: 30,
        completion_tokens: 20,
        total_tokens: 0,
        ..Default::default()
    };
    assert_eq!(telemetry.measured_tokens(), 50);
    let budget = WorkflowBudget {
        max_wall_secs: None,
        max_tokens: Some(50),
    };
    assert!(budget.exceeded(0, &telemetry).is_some());
    assert!(WorkflowBudget::default()
        .exceeded(u64::MAX, &telemetry)
        .is_none());
}

#[test]
fn swl_workflow_budget_is_lowered_to_the_executor() {
    let source = r#"
version: "1.0"
name: budgeted
agents:
  runner:
    model: m
    role: tester
    instruction: echo things
workflows:
  main_flow:
    type: sequential
    max_wall_secs: 600
    max_tokens: 5000
    steps:
      - name: setup
        agent: runner
        action: echo
        input: "hello"
"#;
    let doc = crate::swl::parse_document(source).unwrap();
    let lowered = crate::swl::lower_document(&doc).unwrap();
    let mut executor = WorkflowExecutor::new();
    lowered.register_into(&mut executor);
    assert_eq!(
        executor.budget("main_flow"),
        WorkflowBudget {
            max_wall_secs: Some(600),
            max_tokens: Some(5000),
        }
    );
    assert!(executor.get("main_flow").is_some());
}
