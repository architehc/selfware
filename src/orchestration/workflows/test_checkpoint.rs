//! Tests for per-step checkpointing and `--resume`.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Executor whose Tool steps count their calls and return "hello".
fn counting_executor(calls: Arc<AtomicUsize>) -> WorkflowExecutor {
    WorkflowExecutor::new().with_tool_handler(Box::new(move |_name, _args| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok("hello".to_string()) })
    }))
}

const THREE_STEPS: &str = r#"
name: three
steps:
  - id: one
    name: One
    type: tool
    tool: fetch
  - id: two
    name: Two
    type: tool
    tool: fetch
  - id: three
    name: Three
    type: shell
    command: "test -f ok.txt && test ${one} = hello"
"#;

#[tokio::test]
async fn resume_skips_completed_steps_and_reruns_the_failed_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::for_workspace(dir.path());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut executor = counting_executor(calls.clone());
    executor.load_yaml(THREE_STEPS).unwrap();

    // First run: step 3 fails (ok.txt missing).
    let run = WorkflowRun::fresh(store.clone(), "three");
    let run_id = run.run_id.clone();
    let first = executor
        .execute_run("three", HashMap::new(), dir.path().to_path_buf(), run)
        .await
        .unwrap();
    assert_eq!(first.status, WorkflowStatus::Failed);
    assert_eq!(first.run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let checkpoint = store.load(&run_id).unwrap();
    assert_eq!(checkpoint.status, WorkflowStatus::Failed);
    assert_eq!(
        checkpoint.completed_steps.keys().collect::<Vec<_>>(),
        vec!["one", "two"]
    );
    assert_eq!(checkpoint.step_results["three"].status, StepStatus::Failed);

    // Fix the environment and resume: steps 1-2 are not re-run.
    std::fs::write(dir.path().join("ok.txt"), "").unwrap();
    let resumed = executor
        .execute_run(
            "three",
            HashMap::new(),
            dir.path().to_path_buf(),
            WorkflowRun::resume(store.clone(), &run_id).unwrap(),
        )
        .await
        .unwrap();

    assert!(resumed.is_success(), "logs: {:?}", resumed.logs);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "steps 1-2 skipped");
    // Step 3 read ${one}, a variable restored from the checkpoint.
    assert_eq!(resumed.step_results["three"].status, StepStatus::Completed);
    assert_eq!(resumed.step_results["one"].status, StepStatus::Completed);
    assert!(resumed
        .logs
        .iter()
        .any(|l| l.message.contains("'one' completed in the resumed run")));
    let checkpoint = store.load(&run_id).unwrap();
    assert_eq!(checkpoint.status, WorkflowStatus::Completed);
    assert_eq!(checkpoint.completed_steps.len(), 3);
}

#[tokio::test]
async fn a_completed_llm_step_is_not_rebilled_on_resume() {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::for_workspace(dir.path());
    let llm_calls = Arc::new(AtomicUsize::new(0));
    let counter = llm_calls.clone();
    let mut executor = WorkflowExecutor::new().with_llm_handler(move |_p: &str, _c: &[String]| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(LlmCallOutput::text("plan").with_usage(LlmTokenUsage {
            prompt_tokens: 10,
            completion_tokens: 10,
            total_tokens: 20,
        }))
    });
    executor
        .load_yaml(
            r#"
name: plan_then_check
steps:
  - id: plan
    name: Plan
    type: llm
    prompt: plan it
  - id: check
    name: Check
    type: shell
    command: "test -f ok.txt"
"#,
        )
        .unwrap();

    let run = WorkflowRun::fresh(store.clone(), "plan_then_check");
    let run_id = run.run_id.clone();
    let first = executor
        .execute_run("plan_then_check", HashMap::new(), dir.path().into(), run)
        .await
        .unwrap();
    assert_eq!(first.status, WorkflowStatus::Failed);
    assert_eq!(first.telemetry.llm_calls, 1);

    std::fs::write(dir.path().join("ok.txt"), "").unwrap();
    let resumed = executor
        .execute_run(
            "plan_then_check",
            HashMap::new(),
            dir.path().into(),
            WorkflowRun::resume(store, &run_id).unwrap(),
        )
        .await
        .unwrap();
    assert!(resumed.is_success());
    assert_eq!(llm_calls.load(Ordering::SeqCst), 1);
    assert_eq!(resumed.telemetry.llm_calls, 0);
    assert_eq!(resumed.telemetry.total_tokens, 0);
}

#[tokio::test]
async fn a_step_edited_since_the_checkpoint_runs_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::for_workspace(dir.path());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut executor = counting_executor(calls.clone());
    executor.load_yaml(THREE_STEPS).unwrap();
    let run = WorkflowRun::fresh(store.clone(), "three");
    let run_id = run.run_id.clone();
    executor
        .execute_run("three", HashMap::new(), dir.path().into(), run)
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // Step `two` now calls a different tool: it must not be skipped.
    let edited = THREE_STEPS.replacen(
        "tool: fetch\n  - id: three",
        "tool: refetch\n  - id: three",
        1,
    );
    assert_ne!(edited, THREE_STEPS);
    let mut executor = counting_executor(calls.clone());
    executor.load_yaml(&edited).unwrap();
    std::fs::write(dir.path().join("ok.txt"), "").unwrap();
    let resumed = executor
        .execute_run(
            "three",
            HashMap::new(),
            dir.path().into(),
            WorkflowRun::resume(store, &run_id).unwrap(),
        )
        .await
        .unwrap();
    assert!(resumed.is_success());
    assert_eq!(calls.load(Ordering::SeqCst), 3, "only `two` re-ran");
    assert!(resumed
        .logs
        .iter()
        .any(|l| l.message.contains("'two' changed since the checkpoint")));
}

#[tokio::test]
async fn explicit_inputs_override_restored_variables() {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::for_workspace(dir.path());
    let yaml = r#"
name: greet
inputs:
  - name: who
    required: true
steps:
  - id: gate
    name: Gate
    type: shell
    command: "test ${who} = world"
"#;
    let mut executor = WorkflowExecutor::new();
    executor.load_yaml(yaml).unwrap();
    let run = WorkflowRun::fresh(store.clone(), "greet");
    let run_id = run.run_id.clone();
    let inputs = HashMap::from([("who".to_string(), VarValue::from("nobody"))]);
    let first = executor
        .execute_run("greet", inputs, dir.path().into(), run)
        .await
        .unwrap();
    assert_eq!(first.status, WorkflowStatus::Failed);

    // Without inputs the restored `who` satisfies the required input …
    let again = executor
        .execute_run(
            "greet",
            HashMap::new(),
            dir.path().into(),
            WorkflowRun::resume(store.clone(), &run_id).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(again.status, WorkflowStatus::Failed);

    // … and an explicit input overrides it.
    let inputs = HashMap::from([("who".to_string(), VarValue::from("world"))]);
    let fixed = executor
        .execute_run(
            "greet",
            inputs,
            dir.path().into(),
            WorkflowRun::resume(store, &run_id).unwrap(),
        )
        .await
        .unwrap();
    assert!(fixed.is_success());
}

#[tokio::test]
async fn resuming_a_run_of_another_workflow_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::for_workspace(dir.path());
    let mut executor = counting_executor(Arc::new(AtomicUsize::new(0)));
    executor.load_yaml(THREE_STEPS).unwrap();
    executor
        .load_yaml("name: other\nsteps:\n  - { id: a, name: A, type: log, message: hi }\n")
        .unwrap();
    let run = WorkflowRun::fresh(store.clone(), "three");
    let run_id = run.run_id.clone();
    executor
        .execute_run("three", HashMap::new(), dir.path().into(), run)
        .await
        .unwrap();

    let err = executor
        .execute_run(
            "other",
            HashMap::new(),
            dir.path().into(),
            WorkflowRun::resume(store, &run_id).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not 'other'"), "{err}");
}

#[test]
fn run_ids_can_never_name_a_path() {
    for bad in [
        "",
        "../escape",
        "a/b",
        ".hidden",
        "a\\b",
        "x y",
        &"a".repeat(129),
    ] {
        assert!(validate_run_id(bad).is_err(), "{bad:?} accepted");
    }
    let id = new_run_id("My Workflow/../x");
    validate_run_id(&id).unwrap();
    assert!(id.starts_with("my-workflow----x-"), "{id}");
    let store = CheckpointStore::new("/tmp/store");
    assert!(store.load("../../etc/passwd").is_err());
}

#[test]
fn missing_or_corrupt_checkpoints_are_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::new(dir.path());
    let err = WorkflowRun::resume(store.clone(), "nope").unwrap_err();
    assert!(format!("{err:#}").contains("no checkpoint for workflow run 'nope'"));
    std::fs::write(dir.path().join("bad.json"), "{not json").unwrap();
    let err = store.load("bad").unwrap_err();
    assert!(format!("{err:#}").contains("corrupt workflow checkpoint"));
}

#[test]
fn step_fingerprint_ignores_map_ordering() {
    let make = |pairs: &[(&str, &str)]| WorkflowStep {
        id: "t".into(),
        name: "T".into(),
        description: String::new(),
        step_type: StepType::Tool {
            name: "x".into(),
            args: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        },
        required: true,
        retry: RetryConfig::default(),
        timeout_secs: None,
        depends_on: vec![],
    };
    let pairs: Vec<(String, String)> = (0..32)
        .map(|i| (format!("k{i}"), format!("v{i}")))
        .collect();
    let forward: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut backward = forward.clone();
    backward.reverse();
    assert_eq!(
        step_fingerprint(&make(&forward)),
        step_fingerprint(&make(&backward))
    );
    let mut changed = forward.clone();
    changed[0].1 = "other";
    assert_ne!(
        step_fingerprint(&make(&forward)),
        step_fingerprint(&make(&changed))
    );
}

#[cfg(unix)]
#[tokio::test]
async fn checkpoint_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let store = CheckpointStore::for_workspace(dir.path());
    let mut executor = WorkflowExecutor::new();
    executor
        .load_yaml("name: tiny\nsteps:\n  - { id: a, name: A, type: log, message: hi }\n")
        .unwrap();
    let run = WorkflowRun::fresh(store.clone(), "tiny");
    let path = run.checkpoint_path().unwrap();
    let result = executor
        .execute_run("tiny", HashMap::new(), dir.path().into(), run)
        .await
        .unwrap();
    assert!(result.is_success());
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(path.starts_with(dir.path().join(".selfware").join("workflows")));
}
