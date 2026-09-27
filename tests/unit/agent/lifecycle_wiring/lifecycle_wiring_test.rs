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

// ---- pause / edit / resume / cancel through the task control (phase 5) ----

async fn agent_with_live_task(log: &EventLog) -> Agent {
    let mut agent = Agent::new(config_for("http://127.0.0.1:9/v1".to_string()))
        .await
        .unwrap();
    agent.event_log = log.clone();
    agent.lifecycle_begin_task("t-live", "add max_words to slugify()");
    agent
}

#[tokio::test]
async fn an_edit_pauses_at_the_safe_point_applies_and_resumes() {
    let _state = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = agent_with_live_task(&log).await;
    let control = agent.task_control();
    let live = control.snapshot().expect("begin publishes the task");
    assert_eq!(live.id, "t-live");
    assert_eq!(live.agent, "main");
    assert_eq!(live.constraints.max_turns, 8);

    let mut edit = crate::lifecycle::control::TaskEdit::from_live(&live);
    edit.description = "add max_words and max_len to slugify()".into();
    edit.max_turns = 20;
    edit.token_budget = Some(50_000);
    control.submit_edit("t-live", edit).unwrap();
    let before = agent.messages.len();

    agent.task_control_safe_point().await;

    let (records, _) = log.read_all();
    let tail: Vec<_> = steps(&records).into_iter().skip(2).collect();
    assert_eq!(
        tail,
        vec![
            step(Some("planning"), "paused", Some("pause")),
            step(Some("paused"), "paused", Some("edit")),
            step(Some("paused"), "executing", Some("resume")),
        ]
    );
    let edit_rec = &records[records.len() - 2];
    assert!(
        edit_rec.cause.contains("max turns 8 → 20"),
        "{}",
        edit_rec.cause
    );
    assert!(edit_rec.cause.contains("token budget unbounded → 50000"));
    assert!(
        edit_rec.usage.is_some(),
        "an edit records the measured usage"
    );
    assert!(records.last().unwrap().usage.is_none(), "usage is one-shot");
    // Delivered to the model as a user message.
    assert_eq!(agent.messages.len(), before + 1);
    let msg = agent.messages.last().unwrap().content.text();
    assert!(msg.starts_with("Task updated: "), "{msg}");
    assert!(msg.contains("add max_words and max_len"), "{msg}");
    // Constraints took effect.
    assert_eq!(agent.loop_control.max_iterations(), 20);
    assert_eq!(agent.config.agent.max_budget_tokens, Some(50_000));
    let after = control.snapshot().unwrap();
    assert_eq!(after.state, TaskState::Executing);
    assert_eq!(after.constraints.token_budget, Some(50_000));
    assert!(!control.pause_requested());
}

#[tokio::test]
async fn a_pause_waits_until_resumed_and_a_safe_point_without_requests_records_nothing() {
    let _state = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = agent_with_live_task(&log).await;
    let control = agent.task_control();

    agent.task_control_safe_point().await;
    assert_eq!(log.read_all().0.len(), 2, "no request, no record");

    control.request_pause("t-live").unwrap();
    let resumer = control.clone();
    let handle = tokio::spawn(async move {
        // Resume only once the agent is observably paused.
        loop {
            if resumer.snapshot().map(|l| l.state) == Some(TaskState::Paused) {
                resumer.request_resume("t-live").unwrap();
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    });
    agent.task_control_safe_point().await;
    handle.await.unwrap();
    let (records, _) = log.read_all();
    let events: Vec<_> = records.iter().filter_map(|r| r.event.clone()).collect();
    assert_eq!(events, vec!["start", "pause", "resume"]);
    assert_eq!(records.last().unwrap().cause, "resumed by the user");
}

#[tokio::test]
async fn cancel_from_the_pane_ends_the_task_cancelled_not_interrupted() {
    let _state = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = agent_with_live_task(&log).await;
    let control = agent.task_control();
    control.request_pause("t-live").unwrap();
    let canceller = control.clone();
    let handle = tokio::spawn(async move {
        loop {
            if canceller.snapshot().map(|l| l.state) == Some(TaskState::Paused) {
                canceller.request_cancel("t-live").unwrap();
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    });
    agent.task_control_safe_point().await;
    handle.await.unwrap();
    assert!(agent.is_cancelled(), "cancel latches the agent's token");

    let result: anyhow::Result<()> = Err(AgentError::Cancelled.into());
    agent.lifecycle_finish(&result);
    let (records, _) = log.read_all();
    let last = records.last().unwrap();
    assert_eq!(last.from.as_deref(), Some("paused"));
    assert_eq!(last.to, "cancelled");
    assert_eq!(last.event.as_deref(), Some("cancel"));
    assert_eq!(last.cause, "cancelled by the user");
    let usage = last.usage.expect("the terminal record carries the usage");
    assert_eq!(
        usage.main_tokens,
        Some(0),
        "no model call ran: measured zero"
    );
    assert_eq!(usage.cost_usd, None, "no cost reported → none recorded");
    assert_eq!(control.snapshot().unwrap().state, TaskState::Cancelled);
    agent.reset_cancellation();
}

#[tokio::test]
async fn a_fork_is_a_new_task_whose_parent_is_the_original() {
    let _state = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config_for("http://127.0.0.1:9/v1".to_string()))
        .await
        .unwrap();
    agent.event_log = log.clone();
    agent.set_fork_parent("orig-1");
    agent.lifecycle_begin_task("fork-2", "do it better");
    let (records, _) = log.read_all();
    assert!(records.iter().all(|r| r.id == "fork-2"));
    assert!(records
        .iter()
        .all(|r| r.parent.as_deref() == Some("orig-1")));
    assert_eq!(records[0].cause, "forked from task orig-1");
    assert_eq!(
        agent.task_control().snapshot().unwrap().parent.as_deref(),
        Some("orig-1")
    );
    // Consumed: the next task is not a fork.
    agent.lifecycle_begin_task("next-3", "other");
    let (records, _) = log.read_all();
    assert!(records
        .iter()
        .filter(|r| r.id == "next-3")
        .all(|r| r.parent.is_none()));
}

#[test]
fn teardown_is_effect_driven_and_the_fallback_names_why() {
    use crate::lifecycle::Effect;
    assert_eq!(
        teardown_fallback_reason(&[Effect::TeardownOwned], Some(TaskState::Completed)),
        None
    );
    let none = teardown_fallback_reason(&[], None).unwrap();
    assert!(none.contains("no task lifecycle tracker"), "{none}");
    let already = teardown_fallback_reason(&[], Some(TaskState::Failed)).unwrap();
    assert!(already.contains("already `failed`"), "{already}");
    let refused = teardown_fallback_reason(&[], Some(TaskState::Executing)).unwrap();
    assert!(refused.contains("refused"), "{refused}");
}

/// A non-drainable resource (a bound port) owned by `task` in the global
/// registry: its drain can only give up, so it ends `leaked` (LeakAlarm).
fn register_port_for(task: &str) -> String {
    crate::resources::ResourceRegistry::global().register_owned(
        crate::resources::NewResource::new(
            crate::resources::ResourceKind::Port,
            crate::resources::ResourceHandle::Port { port: 1 },
            "test port",
        ),
        task.to_string(),
        None,
    )
}

#[tokio::test]
async fn a_run_without_the_teardown_effect_still_drains_and_the_leak_surfaces() {
    let mut agent = Agent::new(config_for("http://127.0.0.1:9/v1".to_string()))
        .await
        .unwrap();
    let task = format!("no-effect-{}", uuid::Uuid::new_v4().simple());
    let id = register_port_for(&task);
    assert_eq!(agent.task_lifecycle_state(), None);

    let why = agent
        .teardown_after_run(&crate::resources::Owner::for_task(&task), &[])
        .await;

    assert!(
        why.as_deref()
            .is_some_and(|w| w.contains("no task lifecycle tracker")),
        "{why:?}"
    );
    let registry = crate::resources::ResourceRegistry::global();
    assert_eq!(
        registry.get(&id).unwrap().state,
        crate::resources::ResourceState::Leaked,
        "drained (and could not be released) although no effect asked for it"
    );
    // The leak surfaces in the run summary line.
    let line = agent.run_summary().resources.unwrap();
    assert!(
        line.starts_with("resources: 0 released, 1 leaked (port 1: port resources are not stopped automatically)"),
        "{line}"
    );
    // And in the event log, as the abandon that raised the alarm.
    let (records, _) = registry.event_log().read_all();
    let path: Vec<String> = records
        .iter()
        .filter(|r| r.id == id)
        .map(|r| format!("{}:{}", r.event.as_deref().unwrap_or("new"), r.to))
        .collect();
    assert_eq!(path, vec!["new:live", "drain:draining", "abandon:leaked"]);
}

#[tokio::test]
async fn the_teardown_effect_drives_the_drain_without_a_fallback() {
    let mut agent = Agent::new(config_for("http://127.0.0.1:9/v1".to_string()))
        .await
        .unwrap();
    let task = format!("with-effect-{}", uuid::Uuid::new_v4().simple());
    let id = register_port_for(&task);
    let why = agent
        .teardown_after_run(
            &crate::resources::Owner::for_task(&task),
            &[crate::lifecycle::Effect::TeardownOwned],
        )
        .await;
    assert_eq!(why, None);
    assert_eq!(
        crate::resources::ResourceRegistry::global()
            .get(&id)
            .unwrap()
            .state,
        crate::resources::ResourceState::Leaked
    );
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn the_terminal_cause_names_what_the_task_owns_to_drain() {
    let _state = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    let mut agent = Agent::new(config_for("http://127.0.0.1:9/v1".to_string()))
        .await
        .unwrap();
    agent.event_log = log.clone();
    let task = format!("owns-two-{}", uuid::Uuid::new_v4().simple());
    agent.lifecycle_begin_task(&task, "start two servers");
    register_port_for(&task);
    register_port_for(&task);
    let effects = agent.lifecycle_finish(&Ok(()));
    assert_eq!(effects, vec![crate::lifecycle::Effect::TeardownOwned]);
    let (records, _) = log.read_all();
    let last = records.iter().rev().find(|r| r.id == task).unwrap();
    assert_eq!(last.to, "completed");
    assert!(
        last.cause.ends_with("; 2 owned resources to drain"),
        "{}",
        last.cause
    );
    agent
        .teardown_after_run(&crate::resources::Owner::for_task(&task), &effects)
        .await;
}
