use super::*;
use crate::checkpoint::TaskCheckpoint;
use crate::testing::mock_api::MockLlmServer;
use std::time::Duration;

// ── the reserve formula (measured latency, floor, cap) ──────────────────

#[test]
fn reserve_is_measured_from_the_slowest_call_with_floor_and_cap() {
    // The live replays (900 s budget): slowest 164.446 s → 2x = 329 s;
    // slowest 83.558 s → 168 s.
    assert_eq!(wrap_up_reserve_secs(164_446, 900), 329);
    assert_eq!(wrap_up_reserve_secs(83_558, 900), 168);
    // No measurement / fast endpoint: the floor.
    assert_eq!(wrap_up_reserve_secs(0, 900), WRAP_UP_RESERVE_FLOOR_SECS);
    assert_eq!(wrap_up_reserve_secs(800, 900), WRAP_UP_RESERVE_FLOOR_SECS);
    // Slowest call past a quarter of the budget: capped at half of it.
    assert_eq!(wrap_up_reserve_secs(400_000, 900), 450);
    // Sub-minute budgets: the cap wins over the floor.
    assert_eq!(wrap_up_reserve_secs(0, 40), 20);
}

fn directive_count(agent: &Agent) -> usize {
    agent
        .messages
        .iter()
        .filter(|m| m.content.text_all().contains("DEADLINE WRAP-UP"))
        .count()
}

fn backdate(agent: &mut Agent, elapsed_secs: u64) {
    agent.task_start_time = std::time::Instant::now() - Duration::from_secs(elapsed_secs);
}

#[tokio::test]
async fn wrap_up_fires_once_when_remaining_time_drops_below_the_measured_reserve() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    // A slow endpoint: the slowest call so far took 164 s → reserve 329 s.
    agent
        .client
        .record_call_elapsed_for_test(Duration::from_millis(164_446));

    backdate(&mut agent, 500); // 400 s left > 329 s
    agent.maybe_inject_deadline_wrap_up();
    assert_eq!(directive_count(&agent), 0, "too early");

    backdate(&mut agent, 580); // 320 s left <= 329 s
    agent.maybe_inject_deadline_wrap_up();
    assert_eq!(directive_count(&agent), 1);
    let text = agent.messages.last().unwrap().content.text_all();
    assert!(
        text.contains("the slowest model call in this run took 165s"),
        "{text}"
    );
    assert!(text.contains("Write your FINAL ANSWER NOW"), "{text}");
    assert!(text.contains("UNFINISHED"), "{text}");

    backdate(&mut agent, 700);
    agent.maybe_inject_deadline_wrap_up();
    assert_eq!(directive_count(&agent), 1, "fires once per task");
    server.stop().await;
}

#[tokio::test]
async fn wrap_up_uses_the_measurement_not_a_fixed_fraction() {
    // Same elapsed time (580 of 900 s), fast endpoint: the reserve is the
    // 30 s floor, so nothing fires yet.
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    agent
        .client
        .record_call_elapsed_for_test(Duration::from_millis(2_000));
    backdate(&mut agent, 580);
    agent.maybe_inject_deadline_wrap_up();
    assert_eq!(directive_count(&agent), 0);
    backdate(&mut agent, 875); // 25 s left <= 30 s floor
    agent.maybe_inject_deadline_wrap_up();
    assert_eq!(directive_count(&agent), 1);
    server.stop().await;
}

#[tokio::test]
async fn wrap_up_is_silent_without_a_wall_budget() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    backdate(&mut agent, 100_000);
    agent.maybe_inject_deadline_wrap_up();
    assert_eq!(directive_count(&agent), 0);
    server.stop().await;
}

// ── end to end through the execution loop ────────────────────────────────

/// Slow calls, deadline approaching: the wrap-up is injected exactly once,
/// reaches the model, and the model answers in time → success.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn slow_run_near_the_deadline_gets_one_wrap_up_and_answers_in_time() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Final answer: the module is sound; the parser area is UNFINISHED.")
        .with_latency(200)
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "deadline-ok".to_string(),
        "Review the module and report findings. Do not edit files.".to_string(),
    ));
    // This run's calls have been slow (120 s → reserve 240 s) and 700 s of
    // the 900 s budget are gone (resumed segment): 200 s left.
    agent
        .client
        .record_call_elapsed_for_test(Duration::from_secs(120));
    agent.prior_elapsed_secs = 700;

    let result = agent.continue_execution().await;
    assert!(result.is_ok(), "answered in time: {result:?}");
    assert_eq!(directive_count(&agent), 1, "exactly one wrap-up");
    let bodies = server.captured_request_bodies().await;
    assert!(
        bodies.iter().any(|b| b.contains("DEADLINE WRAP-UP")),
        "the directive reached the model"
    );
    assert!(agent.partial_progress(&result).is_none());
    server.stop().await;
}

/// The deadline passes while a call is in flight: the run stays a TIMEOUT
/// failure (exit status unchanged) and carries the labelled partial.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn deadline_overrun_stays_a_timeout_failure_carrying_the_labelled_partial() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("too late")
        .with_latency(5_000)
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(1);
    let mut agent = Agent::new(config).await.unwrap();
    agent.task_is_read_only = true;
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "deadline-timeout".to_string(),
        "Review src/parser.rs and report findings. Do not edit files.".to_string(),
    ));
    // Progress so far: one read and an interim note.
    let mut call = Message::assistant("");
    call.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "c1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "file_read".to_string(),
            arguments: serde_json::json!({"path": "src/parser.rs"}).to_string(),
        },
    }]);
    agent.messages.push(call);
    agent.messages.push(Message::tool(
        serde_json::json!({"content": "fn parse() {}\nfn lex() {}", "total_lines": 2}).to_string(),
        "c1",
    ));
    agent.messages.push(Message::assistant(
        "Interim: parser.rs lex() skips whitespace twice; the tests are not reviewed yet.",
    ));

    let result = agent.continue_execution().await;
    assert!(result.is_err(), "the deadline passed: {result:?}");
    let fm = agent
        .last_run_failure_mode()
        .unwrap_or_else(|| panic!("classified: {result:?}"));
    assert_eq!(fm.kind, FailureKind::Timeout, "{fm:?}");
    assert_eq!(
        crate::errors::process_exit_code(&result, None),
        1,
        "exit status unchanged"
    );

    let partial = agent.partial_progress(&result).expect("partial carried");
    assert_eq!(partial.label, PARTIAL_REVIEW_LABEL);
    assert!(partial.reason.contains("Wall-clock"), "{partial:?}");
    assert!(partial
        .last_assistant_text
        .as_deref()
        .is_some_and(|t| t.contains("lex() skips whitespace twice")));
    assert!(
        partial
            .work_ledger
            .as_deref()
            .is_some_and(|l| l.contains("src/parser.rs")),
        "{partial:?}"
    );
    let rendered = partial.render();
    assert!(rendered.starts_with("==== PARTIAL — NOT A COMPLETED REVIEW ===="));
    assert!(rendered.contains("not a result"));
    server.stop().await;
}

/// 0.8.2 live validation D1: a call aborted at `agent.max_call_secs` ended
/// labelled MAX_ITERATIONS ("raise max_iterations"). Through the real loop
/// it is now CALL_TIME_CAP, still a failure, and carries the partial.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn per_call_cap_abort_is_labelled_call_time_cap_through_the_loop() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("too slow")
        .with_latency(5_000)
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_call_secs = Some(1);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "call-cap".to_string(),
        "Implement the parser fix.".to_string(),
    ));
    agent
        .messages
        .push(Message::assistant("Interim: the fix belongs in lex()."));
    let result = agent.continue_execution().await;
    assert!(result.is_err(), "{result:?}");
    let fm = agent
        .last_run_failure_mode()
        .unwrap_or_else(|| panic!("classified: {result:?}"));
    assert_eq!(fm.kind, FailureKind::CallTimeCap, "{fm:?}");
    assert_ne!(crate::errors::process_exit_code(&result, None), 0);
    let partial = agent.partial_progress(&result).expect("partial carried");
    assert_eq!(partial.label, PARTIAL_TASK_LABEL);
    server.stop().await;
}
