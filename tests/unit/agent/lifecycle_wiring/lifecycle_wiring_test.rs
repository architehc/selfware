use super::*;
use crate::config::{AgentConfig, Config, ExecutionMode, SafetyConfig};
use crate::lifecycle::{EventLog, TransitionRecord};
use crate::testing::mock_api::{MockLlmServer, MockResponse};

fn config_for(endpoint: String) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        context_length: 500_000,
        max_tokens: 8192,
        agent: AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: ExecutionMode::Yolo,
        ..Default::default()
    }
}

/// `(from, to, event)` of every task record, in order.
fn steps(records: &[TransitionRecord]) -> Vec<(Option<String>, String, Option<String>)> {
    records
        .iter()
        .filter(|r| r.entity == Entity::Task)
        .map(|r| (r.from.clone(), r.to.clone(), r.event.clone()))
        .collect()
}

fn step(
    from: Option<&str>,
    to: &str,
    event: Option<&str>,
) -> (Option<String>, String, Option<String>) {
    (
        from.map(str::to_string),
        to.to_string(),
        event.map(str::to_string),
    )
}

fn checkpoint_id(agent: &Agent) -> String {
    agent
        .current_checkpoint
        .as_ref()
        .expect("run_task creates a checkpoint")
        .task_id
        .clone()
}

// ---- pure mapping ------------------------------------------------------------

#[test]
fn run_end_maps_onto_task_events() {
    use crate::ShutdownReason;
    let ok: anyhow::Result<()> = Ok(());
    assert_eq!(terminal_event_for_run(&ok, None).0, TaskEvent::Succeed);
    assert_eq!(
        terminal_event_for_run(&ok, Some(ShutdownReason::UserInterrupt)).0,
        TaskEvent::Interrupt
    );
    let (e, cause) = terminal_event_for_run(&ok, Some(ShutdownReason::SignalTerminate));
    assert_eq!(e, TaskEvent::Interrupt);
    assert!(cause.contains("SIGTERM"), "{cause}");
    // The run's own timeout is a deadline failure.
    assert_eq!(
        terminal_event_for_run(&ok, Some(ShutdownReason::Timeout)).0,
        TaskEvent::Timeout
    );

    let cancelled: anyhow::Result<()> = Err(AgentError::Cancelled.into());
    assert_eq!(
        terminal_event_for_run(&cancelled, None).0,
        TaskEvent::Interrupt
    );
    let terminated: anyhow::Result<()> = Err(AgentError::Terminated("sigterm".into()).into());
    assert_eq!(
        terminal_event_for_run(&terminated, None).0,
        TaskEvent::Interrupt
    );
    let timed_out: anyhow::Result<()> =
        Err(AgentError::CancelledWithReason("timeout".into()).into());
    let (e, cause) = terminal_event_for_run(&timed_out, None);
    assert_eq!(e, TaskEvent::Timeout);
    assert!(cause.starts_with("outcome failed (deadline)"), "{cause}");

    let boom: anyhow::Result<()> = Err(anyhow::anyhow!("boom\nstack line"));
    let (e, cause) = terminal_event_for_run(&boom, None);
    assert_eq!(e, TaskEvent::Fail);
    assert_eq!(cause, "outcome failed: boom");
}

// ---- mock-server runs ------------------------------------------------------

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn completed_run_logs_queued_planning_executing_completed() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Analyzed.")
        .with_response("Complete.")
        .build()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config_for(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.event_log = log.clone();

    agent.run_task("Summarize error handling").await.unwrap();

    let (records, skipped) = log.read_all();
    assert_eq!(skipped, 0);
    assert_eq!(
        steps(&records),
        vec![
            step(None, "queued", None),
            step(Some("queued"), "planning", Some("start")),
            step(Some("planning"), "executing", Some("planned")),
            step(Some("executing"), "completed", Some("succeed")),
        ]
    );
    let id = checkpoint_id(&agent);
    assert!(
        records.iter().all(|r| r.id == id),
        "task id = checkpoint task_id"
    );
    assert!(records.iter().all(|r| r.task_type.is_some()));
    assert_eq!(agent.task_lifecycle_state(), Some(TaskState::Completed));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn failed_run_logs_fail_from_the_state_it_failed_in() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 401,
            body: r#"{"error":"No cookie auth credentials found"}"#.to_string(),
        })
        .build()
        .await;
    let mut config = config_for(format!("{}/v1", server.url()));
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config).await.unwrap();
    agent.event_log = log.clone();

    let result = agent.run_task("Do a simple task").await;
    assert!(result.is_err());

    let (records, _) = log.read_all();
    assert_eq!(
        steps(&records),
        vec![
            step(None, "queued", None),
            step(Some("queued"), "planning", Some("start")),
            step(Some("planning"), "failed", Some("fail")),
        ]
    );
    let last = records.last().unwrap();
    assert!(last.cause.starts_with("outcome failed: "), "{}", last.cause);
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn interrupted_run_logs_interrupt_and_resumes_as_a_new_segment() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Text("Done.".to_string()))
        .build()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config_for(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.event_log = log.clone();
    agent
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let err = agent
        .run_task("Summarize error handling")
        .await
        .expect_err("a cancelled run is not a success");
    assert!(matches!(
        err.downcast_ref::<AgentError>(),
        Some(AgentError::Cancelled)
    ));
    let (records, _) = log.read_all();
    assert_eq!(
        steps(&records),
        vec![
            step(None, "queued", None),
            step(Some("queued"), "planning", Some("start")),
            step(Some("planning"), "interrupted", Some("interrupt")),
        ]
    );
    let id = checkpoint_id(&agent);

    // A new process resumes the same checkpoint: the interrupted state is
    // read back from the log and resumed (P1's one allowed exit).
    agent.reset_cancellation();
    let mut resumed = Agent::new(config_for(format!("{}/v1", server.url())))
        .await
        .unwrap();
    resumed.event_log = log.clone();
    resumed.current_checkpoint = agent.current_checkpoint.clone();
    let result = resumed.continue_execution().await;

    let (records, skipped) = log.read_all();
    assert_eq!(skipped, 0);
    let tail: Vec<_> = steps(&records).into_iter().skip(3).collect();
    let end = if result.is_ok() {
        step(Some("executing"), "completed", Some("succeed"))
    } else {
        step(Some("executing"), "failed", Some("fail"))
    };
    assert_eq!(
        tail,
        vec![
            step(Some("interrupted"), "queued", Some("resume")),
            step(Some("queued"), "planning", Some("start")),
            step(Some("planning"), "executing", Some("planned")),
            end,
        ],
        "resume result: {result:?}"
    );
    assert!(result.is_ok(), "the resumed run completes: {result:?}");
    assert!(
        records.iter().all(|r| r.id == id),
        "same task id across segments"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn resuming_a_task_with_no_record_opens_a_new_segment() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Text("Done.".to_string()))
        .build()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config_for(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.event_log = log.clone();
    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "legacy-task".to_string(),
        "Summarize error handling".to_string(),
    ));
    let _ = agent.continue_execution().await;
    let (records, _) = log.read_all();
    let first = &records[0];
    assert_eq!(first.id, "legacy-task");
    assert_eq!(first.from, None);
    assert_eq!(first.to, "queued");
    assert_eq!(first.cause, "new segment: resumed with no earlier record");
    assert_eq!(records[1].event.as_deref(), Some("start"));
    assert!(agent
        .task_lifecycle_state()
        .is_some_and(|s| s.is_terminal()));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn a_live_task_left_behind_is_closed_honestly_by_the_next_task() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Text("Done.".to_string()))
        .build()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config_for(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.event_log = log.clone();
    // A run whose future was dropped mid-flight leaves its task live.
    agent.lifecycle_begin_task("dropped-task", "Summarize error handling");
    assert_eq!(agent.task_lifecycle_state(), Some(TaskState::Planning));
    agent.run_task("Summarize error handling").await.unwrap();

    let (records, _) = log.read_all();
    let dropped: Vec<_> = records.iter().filter(|r| r.id == "dropped-task").collect();
    let last = dropped.last().unwrap();
    assert_eq!(last.to, "interrupted");
    assert!(
        last.cause.contains("without reporting an outcome"),
        "{}",
        last.cause
    );
    server.stop().await;
}
