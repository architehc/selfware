use super::*;
use crate::checkpoint::TaskCheckpoint;
use crate::testing::mock_api::MockLlmServer;
use std::time::Duration;

// ── the time window (forecast next call + final answer, floor, cap) ─────

fn shape(prompt: u64, completion: u64, ms: u64) -> crate::api::usage::CallShape {
    crate::api::usage::CallShape {
        prompt_tokens: prompt,
        completion_tokens: completion,
        elapsed_ms: ms,
    }
}

/// Four 100-token calls at a 100k prompt in 20 s (prefill 0.2 ms/token)
/// and one 3,000-token call in 150 s (20 tok/s): next call ~25 s, final
/// answer 20 + 6,526 / 20 = ~347 s → window 372 s.
fn record_slow_endpoint(agent: &Agent) {
    for _ in 0..4 {
        agent.client.record_call_shape(100_000, 100, 20_000);
    }
    agent.client.record_call_shape(100_000, 3_000, 150_000);
}

#[test]
fn time_window_is_the_forecast_next_call_plus_answer_with_floor_and_cap() {
    use crate::agent::call_forecast::CallForecast;
    let slow: Vec<_> = (0..4)
        .map(|_| shape(100_000, 100, 20_000))
        .chain([shape(100_000, 3_000, 150_000)])
        .collect();
    let f = CallForecast::from_calls(&slow, None);
    assert_eq!((f.next_call_secs(), f.answer_secs()), (25, 347));
    assert_eq!(time_window_secs(&f, 900), 372);
    // Fast endpoint (0.05 ms/token prefill, 300 tok/s): the 30 s floor.
    let fast = [shape(10_000, 100, 500), shape(10_000, 3_000, 10_000)];
    let f = CallForecast::from_calls(&fast, None);
    assert_eq!(time_window_secs(&f, 900), WRAP_UP_RESERVE_FLOOR_SECS);
    // Very slow endpoint (5 tok/s): capped at two thirds of the budget.
    let crawl = [shape(100_000, 2_000, 400_000)];
    let f = CallForecast::from_calls(&crawl, None);
    assert_eq!(time_window_secs(&f, 900), 600);
    // Sub-minute budgets: the cap wins over the floor.
    assert_eq!(time_window_secs(&f, 40), 26);
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
async fn wrap_up_fires_once_when_remaining_time_drops_below_the_forecast_window() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    record_slow_endpoint(&agent); // window 372 s

    backdate(&mut agent, 500); // 400 s left >= 372 s
    agent.maybe_inject_wrap_up();
    assert_eq!(directive_count(&agent), 0, "too early");

    backdate(&mut agent, 540); // 360 s left < 372 s
    agent.maybe_inject_wrap_up();
    assert_eq!(directive_count(&agent), 1);
    let text = agent.messages.last().unwrap().content.text_all();
    assert!(
        text.contains("a final answer is forecast to take ~347s"),
        "{text}"
    );
    assert!(text.contains("Write your FINAL ANSWER NOW"), "{text}");
    assert!(text.contains("UNFINISHED"), "{text}");
    assert_eq!(agent.wrap_up_issued(), Some(WrapUpCause::Deadline));

    backdate(&mut agent, 700);
    agent.maybe_inject_wrap_up();
    assert_eq!(directive_count(&agent), 1, "fires once per task");
    server.stop().await;
}

#[tokio::test]
async fn wrap_up_uses_the_measurement_not_a_fixed_fraction() {
    // Same elapsed time (540 of 900 s), fast endpoint: the window is the
    // 30 s floor, so nothing fires yet.
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    agent.client.record_call_shape(10_000, 100, 500);
    agent.client.record_call_shape(10_000, 3_000, 10_000);
    backdate(&mut agent, 540);
    agent.maybe_inject_wrap_up();
    assert_eq!(directive_count(&agent), 0);
    backdate(&mut agent, 875); // 25 s left < 30 s floor
    agent.maybe_inject_wrap_up();
    assert_eq!(directive_count(&agent), 1);
    server.stop().await;
}

#[tokio::test]
async fn wrap_up_is_silent_without_a_wall_budget() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    backdate(&mut agent, 100_000);
    agent.maybe_inject_wrap_up();
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
    // A slow endpoint (window 372 s) and 700 s of the 900 s budget gone
    // (resumed segment): 200 s left.
    record_slow_endpoint(&agent);
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
    // A write-up segment (>= PARTIAL_TEXT_MIN_CHARS of prose): shorter
    // narration no longer counts as answer text.
    agent.messages.push(Message::assistant(
        "Interim: parser.rs lex() skips whitespace twice — once in the main loop and \
         again in the token helper — so a run of blanks is consumed as two separate \
         skips and the column counter drifts by one per run; parse() inherits the wrong \
         column. The tests are not reviewed yet.",
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

// ── completion gates vs the deadline (val083 b2_350000) ──────────────────

/// Rule-5 sweep: the requirements audit is itself a model call. Inside the
/// deadline window it steps aside WITHOUT calling the model and records
/// NOT PERFORMED (⚠️ banner); outside it the audit runs as before.
#[tokio::test]
async fn requirements_audit_steps_aside_inside_the_deadline_window() {
    let server = MockLlmServer::builder()
        .with_response("must not be requested")
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "audit-deadline".to_string(),
        "Implement the CSV exporter: write every record with its id, name, created_at and \
         amount columns, quote fields that contain commas, keep the header row, add tests \
         for the quoting and the header, and make sure the existing importer still passes."
            .to_string(),
    ));
    backdate(&mut agent, 890); // 10 s left <= 30 s floor
    assert_eq!(agent.maybe_requirements_audit(false).await, None);
    let status = agent.requirements_audit_status().expect("recorded");
    assert!(status.is_not_performed(), "{status:?}");
    assert!(status.label().contains("deadline"), "{}", status.label());
    assert!(server.captured_request_bodies().await.is_empty());
    server.stop().await;
}

/// Rule-5 sweep: the min-steps floor is pacing, not a result check — it
/// steps aside inside the deadline window and applies outside it.
#[tokio::test]
async fn min_steps_floor_steps_aside_inside_the_deadline_window() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    config.agent.min_completion_steps = 5;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "min-steps".to_string(),
        "Explain how the retry helper works.".to_string(),
    ));
    agent.task_is_read_only = false;
    agent.current_task_context = "Implement the retry helper change.".to_string();
    agent.last_assistant_response = "The retry helper now backs off exponentially.".to_string();
    backdate(&mut agent, 100);
    let early = agent.check_completion_gate().await;
    assert!(
        early
            .as_deref()
            .is_some_and(|m| m.contains("at least 5 are required")),
        "{early:?}"
    );
    backdate(&mut agent, 890);
    let late = agent.check_completion_gate().await;
    assert!(
        !late
            .as_deref()
            .unwrap_or("")
            .contains("at least 5 are required"),
        "{late:?}"
    );
    server.stop().await;
}

#[derive(serde::Deserialize)]
struct Turns350k {
    /// Turn 14: the 4,442-char review the citation gate rejected.
    draft: String,
    /// Turns 15–19: the correction round's tool calls.
    after: Vec<String>,
}

#[derive(serde::Deserialize)]
struct Turns163k {
    /// Turns 10–17: reads, part-by-part write-ups, and a narration turn.
    turns: Vec<String>,
}

fn b2_350000() -> Turns350k {
    serde_json::from_str(include_str!("fixtures/b2_350000_turns.json")).unwrap()
}

fn b2_163840() -> Turns163k {
    serde_json::from_str(include_str!("fixtures/b2_163840_turns.json")).unwrap()
}

fn assert_no_tool_markup(text: &str) {
    for marker in ["<tool>", "</tool>", "<arguments>", "<name>", "<tool_call"] {
        assert!(!text.contains(marker), "tool markup {marker} in: {text}");
    }
}

/// The wrong-count status the gate recorded for the b2_350000 draft.
fn b2_350000_status() -> crate::agent::citation_check::GroundingStatus {
    crate::agent::citation_check::GroundingStatus {
        total: 22,
        verified: 5,
        unverifiable: 6,
        wrong_line: 8,
        symbol_not_found: 3,
        correction_rounds: 1,
        problems: vec![
            "`task_is_read_only` cited at src/agent/task_policy.rs:55 but found at \
             src/agent/task_policy.rs:69"
                .to_string(),
        ],
        read_only: true,
        ..Default::default()
    }
}

fn set_rejected_draft(agent: &Agent, text: &str) {
    agent.citation_gate.lock().unwrap().rejected_draft =
        Some(crate::agent::citation_check::RejectedDraft {
            text: text.to_string(),
            status: b2_350000_status(),
            mutation_sequence: agent.mutation_sequence,
        });
}

#[test]
fn answer_prose_strips_tool_calls_from_the_real_turns() {
    let run = b2_350000();
    for turn in &run.after {
        let prose = answer_prose(turn);
        assert_no_tool_markup(&prose);
        assert!(
            prose.chars().count() < PARTIAL_TEXT_MIN_CHARS,
            "a tool-call turn is not answer text: {prose:?}"
        );
    }
    let draft = answer_prose(&run.draft);
    assert!(draft.starts_with("# Selfware Agent Module — Evidence-Based Review"));
    assert!(draft.chars().count() > 4_000);
    // A review quoting markup in inline code keeps that line.
    assert_eq!(
        answer_prose("The parser accepts `<tool>` blocks only when closed."),
        "The parser accepts `<tool>` blocks only when closed."
    );
}

#[tokio::test]
async fn partial_prefers_the_gate_rejected_draft_over_a_later_tool_call() {
    // val083 b2_350000: partial.last_assistant_text was the turn-19
    // grep_search call; the rejected 4,442-char draft was lost.
    let server = MockLlmServer::builder().with_response("x").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let run = b2_350000();
    agent.messages.push(Message::assistant(run.draft.clone()));
    for turn in &run.after {
        agent.messages.push(Message::assistant(turn.clone()));
    }
    set_rejected_draft(
        &agent,
        &super::super::recovery::strip_think_blocks(&run.draft),
    );
    let text = agent.partial_answer_text().expect("draft carried");
    assert_no_tool_markup(&text);
    assert_eq!(text, answer_prose(&run.draft));

    // Without the gate's draft, the history's write-up is still found (the
    // tool-call turns after it are skipped).
    agent.citation_gate.lock().unwrap().rejected_draft = None;
    let text = agent.partial_answer_text().expect("write-up carried");
    assert_eq!(text, answer_prose(&run.draft));
    server.stop().await;
}

#[tokio::test]
async fn partial_collects_the_part_by_part_write_up_without_narration() {
    // val083 b2_163840: areas 1–4 were written up over turns 12–16; the
    // partial carried only turn 17's "Area 5 — continuing" narration.
    let server = MockLlmServer::builder().with_response("x").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    for turn in b2_163840().turns {
        agent.messages.push(Message::assistant(turn));
    }
    let text = agent.partial_answer_text().expect("write-up carried");
    assert_no_tool_markup(&text);
    for part in ["Area 1 & 2", "Area 2 — completed", "Area 3", "Area 4"] {
        assert!(text.contains(part), "{part} missing");
    }
    assert!(
        !text.contains("Area 5 — continuing"),
        "narration is not answer text"
    );
    let a1 = text.find("Area 1 & 2").unwrap();
    let a4 = text.find("Area 4").unwrap();
    assert!(a1 < a4, "oldest first");
    server.stop().await;
}

/// Live replay of b2_163840 on the fixed build: the 3M token cap stopped
/// the run at 678 s with six areas written up — a budget stop carries the
/// same labelled partial as a timeout, and stays a failure.
#[tokio::test]
async fn token_budget_stop_carries_the_partial_too() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    agent.task_is_read_only = true;
    for turn in b2_163840().turns {
        agent.messages.push(Message::assistant(turn));
    }
    agent.last_run_failure_mode = Some(crate::agent::failure_mode::FailureMode {
        restored_files: Vec::new(),
        kind: FailureKind::BudgetExhausted,
        evidence: "token budget exhausted with 0 mutating tool calls completed".to_string(),
        advice: "-".to_string(),
    });
    let result: anyhow::Result<()> = Err(anyhow::anyhow!(
        "Token budget exhausted: 3036263 >= 3000000 tokens"
    ));
    let partial = agent.partial_progress(&result).expect("partial carried");
    assert_eq!(partial.label, PARTIAL_REVIEW_LABEL);
    let text = partial.last_assistant_text.expect("write-up carried");
    assert!(text.contains("Area 4"));
    assert_no_tool_markup(&text);
    server.stop().await;
}

#[tokio::test]
async fn partial_without_answer_text_falls_back_to_the_ledger_alone() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    for turn in b2_350000().after {
        agent.messages.push(Message::assistant(turn));
    }
    assert_eq!(agent.partial_answer_text(), None);
    server.stop().await;
}

#[tokio::test]
async fn rejected_draft_is_taken_only_when_one_call_no_longer_fits() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    // b2_350000: its slowest long call (call 19: 3,495 tokens in 231.2 s at
    // a 154k prompt, 15.1 tok/s) and its rejected draft's call (3,079
    // tokens) → final answer forecast 154k × 0.615 ms + 3,079 / 15.1 ≈ 298 s.
    agent.client.record_call_shape(153_934, 3_495, 231_163);
    agent.wrap_up.lock().unwrap().draft_completion_tokens = Some(3_079);
    let draft = b2_350000().draft;
    set_rejected_draft(&agent, &draft);

    backdate(&mut agent, 525); // 375 s left: the answer still fits
    assert_eq!(agent.take_rejected_draft_at_limit(), None);

    // A draft judged before a later edit is never accepted.
    agent.mutation_sequence += 1;
    backdate(&mut agent, 761); // 139 s left < ~298 s
    assert_eq!(agent.take_rejected_draft_at_limit(), None);
    agent.mutation_sequence -= 1;

    let taken = agent.take_rejected_draft_at_limit().expect("taken");
    assert_eq!(taken, draft.trim());
    assert_eq!(agent.last_assistant_response, draft.trim());
    let status = agent.grounding_status().expect("status");
    assert_eq!(status.not_corrected.as_deref(), Some("deadline"));
    assert_eq!(status.problem_count(), 11);
    assert!(status
        .warning_note()
        .unwrap()
        .ends_with("citations not corrected: deadline"));
    // Taken once.
    assert_eq!(agent.take_rejected_draft_at_limit(), None);
    server.stop().await;
}

/// b2_350000 replayed through the loop: the gate rejected the draft, the
/// correction round ate the budget, and 139 s remain against a 232 s
/// slowest call. The run now finishes with the draft — no model call, exit
/// 0, ⚠️ — instead of a timeout with no report.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn deadline_with_a_pending_rejected_draft_completes_with_it_and_a_warning() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("must not be requested")
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    let mut agent = Agent::new(config).await.unwrap();
    agent.task_is_read_only = true;
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "deadline-draft".to_string(),
        "Review src/agent and report findings. Do not edit files.".to_string(),
    ));
    agent.client.record_call_shape(153_934, 3_495, 231_163);
    agent.prior_elapsed_secs = 761;
    let run = b2_350000();
    agent.messages.push(Message::assistant(run.draft.clone()));
    for turn in &run.after {
        agent.messages.push(Message::assistant(turn.clone()));
    }
    set_rejected_draft(&agent, &run.draft);

    let result = agent.continue_execution().await;
    assert!(result.is_ok(), "completed with the draft: {result:?}");
    assert!(
        server.captured_request_bodies().await.is_empty(),
        "no model call was started"
    );
    assert_eq!(agent.last_assistant_response, run.draft.trim());
    let fm = agent.last_run_failure_mode().expect("classified");
    assert!(fm.kind.is_nonfailure(), "{fm:?}");
    assert!(
        fm.evidence.contains("could not be verified")
            && fm.evidence.contains("citations not corrected: deadline"),
        "{fm:?}"
    );
    let banner = fm.cli_banner();
    assert!(banner.starts_with("⚠️"), "{banner}");
    assert!(!banner.contains('✅'), "{banner}");
    assert!(agent.partial_progress(&result).is_none());
    server.stop().await;
}

// ── budget wrap-up (tokens / cost), one latch shared with the deadline ────

fn budget_directive_count(agent: &Agent) -> usize {
    agent
        .messages
        .iter()
        .filter(|m| m.content.text_all().contains("BUDGET WRAP-UP"))
        .count()
}

fn any_wrap_up_count(agent: &Agent) -> usize {
    directive_count(agent) + budget_directive_count(agent)
}

/// One measured call at a 95k prompt (the b2_163840 replay's size): next
/// call ~96k tokens, final answer 95k + 6,526 → token window ~197.5k.
fn record_95k_prompt(agent: &Agent) {
    agent.client.record_call_shape(95_000, 1_000, 30_000);
}

#[test]
fn token_window_is_the_forecast_next_call_plus_answer_with_floor_and_cap() {
    use crate::agent::call_forecast::CallForecast;
    let f = CallForecast::from_calls(&[shape(95_000, 1_000, 30_000)], None);
    assert_eq!(token_window(&f, 3_000_000), 96_000 + 101_526);
    // Nothing measured: the floor; small budgets: half the budget.
    let empty = CallForecast::from_calls(&[], None);
    assert_eq!(token_window(&empty, 3_000_000), TOKEN_WRAP_UP_RESERVE_FLOOR);
    assert_eq!(token_window(&f, 300_000), 150_000);
    assert!(token_window_reached(197_525, &f, 3_000_000).is_some());
    assert!(token_window_reached(197_526, &f, 3_000_000).is_none());
    // Cost: priced at this run's measured cost per token; never without one.
    assert_eq!(cost_window_reached(0.01, 0.0, &f, 10.0), None);
    // 197,526 tokens at $0.000001 = ~$0.1975.
    assert!(cost_window_reached(0.19, 0.000_001, &f, 10.0).is_some());
    assert!(cost_window_reached(0.20, 0.000_001, &f, 10.0).is_none());
}

/// Tokens approach the cap: each turn uses 40k tokens against a 200k
/// budget (reserve 80k). The wrap-up fires once, at 120k spent, the next
/// answer lands inside the budget and the run succeeds.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn tokens_approaching_the_cap_get_one_budget_wrap_up_and_the_answer_lands() {
    let _state = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let mut builder = MockLlmServer::builder().with_usage(39_000, 1_000, 40_000);
    for n in 0..3 {
        let path = dir.path().join(format!("f{n}.rs"));
        std::fs::write(&path, format!("fn f{n}() {{}}\n")).unwrap();
        builder = builder.with_response(format!(
            "<tool>\n<name>file_read</name>\n<arguments>{}</arguments>\n</tool>",
            serde_json::json!({ "path": path.to_string_lossy() })
        ));
    }
    let server = builder
        .with_response(
            "Final answer: f0, f1 and f2 are empty stubs; nothing else was checked (UNFINISHED).",
        )
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_budget_tokens = Some(200_000);
    let mut agent = Agent::new(config).await.unwrap();
    agent.task_is_read_only = true;
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "budget-ok".to_string(),
        "Review the three files and report findings. Do not edit files.".to_string(),
    ));

    let result = agent.continue_execution().await;
    assert!(result.is_ok(), "answered inside the budget: {result:?}");
    assert_eq!(budget_directive_count(&agent), 1, "exactly one wrap-up");
    assert_eq!(directive_count(&agent), 0);
    assert_eq!(agent.wrap_up_issued(), Some(WrapUpCause::TokenBudget));
    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 4, "3 exploring turns + the answer");
    assert!(
        !bodies[2].contains("BUDGET WRAP-UP"),
        "not before the reserve"
    );
    assert!(
        bodies[3].contains("BUDGET WRAP-UP"),
        "the directive reached the model"
    );
    assert!(agent.partial_progress(&result).is_none());
    server.stop().await;
}

/// The cap is crossed before an answer: BUDGET_EXHAUSTED, a failure, with
/// the labelled partial carrying the write-up so far.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn crossing_the_token_cap_is_budget_exhausted_with_the_partial() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_usage(150_000, 10_000, 160_000)
        .with_response(
            "<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"x.rs\"}</arguments>\n</tool>",
        )
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_budget_tokens = Some(200_000);
    let mut agent = Agent::new(config).await.unwrap();
    agent.task_is_read_only = true;
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "budget-over".to_string(),
        "Review src/agent and report findings. Do not edit files.".to_string(),
    ));
    for turn in b2_163840().turns {
        agent.messages.push(Message::assistant(turn));
    }
    // 100k already spent this run (seeded like a resumed segment).
    agent.cumulative_token_usage.total = 100_000;
    agent.client.ensure_budget_floor(100_000, 0.0);

    let result = agent.continue_execution().await;
    assert!(result.is_err(), "{result:?}");
    let fm = agent.last_run_failure_mode().expect("classified");
    assert_eq!(fm.kind, FailureKind::BudgetExhausted, "{fm:?}");
    assert_ne!(crate::errors::process_exit_code(&result, None), 0);
    let partial = agent.partial_progress(&result).expect("partial carried");
    let text = partial.last_assistant_text.expect("write-up carried");
    assert!(text.contains("Area 4"));
    assert_no_tool_markup(&text);
    server.stop().await;
}

/// Deadline and token reserves crossed together: exactly one injection, the
/// deadline recorded (it is checked first); later turns inject nothing.
#[tokio::test]
async fn deadline_and_budget_reserves_together_inject_exactly_once() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    config.agent.max_budget_tokens = Some(3_000_000);
    let mut agent = Agent::new(config).await.unwrap();
    record_95k_prompt(&agent); // token window ~197.5k
    agent.client.ensure_budget_floor(2_900_000, 0.0); // 100k left
    backdate(&mut agent, 800); // 100 s left, inside the time window too
    agent.maybe_inject_wrap_up();
    agent.maybe_inject_wrap_up();
    assert_eq!(any_wrap_up_count(&agent), 1);
    assert_eq!(agent.wrap_up_issued(), Some(WrapUpCause::Deadline));
    server.stop().await;
}

/// Budget first, deadline later in the same task: still one injection.
#[tokio::test]
async fn budget_wrap_up_blocks_a_later_deadline_wrap_up() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_wall_secs = Some(900);
    config.agent.max_budget_tokens = Some(3_000_000);
    let mut agent = Agent::new(config).await.unwrap();
    record_95k_prompt(&agent);
    agent.client.ensure_budget_floor(2_850_000, 0.0); // 150k left < ~197.5k
    backdate(&mut agent, 0); // 900 s left: outside the time window
    agent.maybe_inject_wrap_up();
    assert_eq!(agent.wrap_up_issued(), Some(WrapUpCause::TokenBudget));
    let text = agent.messages.last().unwrap().content.text_all();
    assert!(
        text.contains("150000 tokens of the token budget remain")
            && text.contains("forecast to use ~101526 tokens"),
        "{text}"
    );
    backdate(&mut agent, 850); // now inside the deadline reserve too
    agent.maybe_inject_wrap_up();
    assert_eq!(any_wrap_up_count(&agent), 1);
    server.stop().await;
}

/// A tool-less answer of substance is a draft: its call's completion size
/// becomes the forecast answer size. A tool-call turn is not a draft.
#[tokio::test]
async fn draft_calls_set_the_forecast_answer_size() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let run = b2_350000();
    agent.client.record_call_shape(144_738, 3_079, 115_568);
    agent.messages.push(Message::assistant(run.draft.clone()));
    agent.observe_turn_usage();
    assert_eq!(agent.call_forecast().answer_completion_tokens, 3_079);
    // A later tool-call turn (the correction round's grep) changes nothing.
    agent.client.record_call_shape(153_934, 3_495, 231_163);
    agent
        .messages
        .push(Message::assistant(run.after[4].clone()));
    agent.observe_turn_usage();
    assert_eq!(agent.call_forecast().answer_completion_tokens, 3_079);
    server.stop().await;
}

/// A citation-rejected draft is taken when one more turn no longer fits
/// the TOKEN budget; the note says "budget".
#[tokio::test]
async fn rejected_draft_is_taken_when_one_turn_no_longer_fits_the_budget() {
    let server = MockLlmServer::builder().with_response("x").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.agent.max_budget_tokens = Some(3_000_000);
    let mut agent = Agent::new(config).await.unwrap();
    record_95k_prompt(&agent);
    let draft = b2_350000().draft;
    set_rejected_draft(&agent, &draft);
    agent.client.ensure_budget_floor(2_850_000, 0.0); // 150k left: a turn fits
    assert_eq!(agent.take_rejected_draft_at_limit(), None);
    agent.client.ensure_budget_floor(2_920_000, 0.0); // 80k left < ~101.5k answer
    assert_eq!(
        agent.take_rejected_draft_at_limit().as_deref(),
        Some(draft.trim())
    );
    let status = agent.grounding_status().unwrap();
    assert_eq!(status.not_corrected.as_deref(), Some("budget"));
    assert!(status
        .warning_note()
        .unwrap()
        .ends_with("citations not corrected: budget"));
    server.stop().await;
}
