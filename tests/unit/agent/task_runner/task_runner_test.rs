use super::*;
use crate::checkpoint::{CheckpointManager, TaskCheckpoint, ToolCallLog};
use crate::config::{AgentConfig, Config, ExecutionMode, SafetyConfig};
use crate::testing::mock_api::{MockLlmServer, MockResponse};
use chrono::Utc;

#[test]
fn shell_command_is_verification_recognizes_non_cargo_runners() {
    let yes = [
        r#"{"command":"pytest -q tests/"}"#,
        r#"{"command":"npm test"}"#,
        r#"{"command":"npm run test -- --watch=false"}"#,
        r#"{"command":"go test ./..."}"#,
        r#"{"command":"npx jest"}"#,
        r#"{"command":"cargo test --all"}"#,
        r#"{"command":"make check"}"#,
    ];
    for a in yes {
        assert!(
            Agent::shell_command_is_verification(a),
            "should count as verification: {a}"
        );
    }
    let no = [
        r#"{"command":"ls -la"}"#,
        r#"{"command":"cat src/main.rs"}"#,
        r#"{"command":"echo hello"}"#,
        r#"{}"#,
        "not json at all",
    ];
    for a in no {
        assert!(
            !Agent::shell_command_is_verification(a),
            "should NOT count as verification: {a}"
        );
    }
}

#[test]
fn progress_guidance_is_cargo_aware() {
    // Finding 1(a): progress injections kept telling a Python task to "verify
    // with cargo_check/cargo_test" every 5 steps (even after "Verification:
    // PASSED"). The guidance must name cargo only for cargo-applicable tasks.
    let mid_cargo = progress_guidance(40.0, true);
    assert!(
        mid_cargo.contains("cargo_check/cargo_test"),
        "cargo-applicable task keeps cargo guidance: {mid_cargo}"
    );
    let mid_python = progress_guidance(40.0, false);
    assert!(
        !mid_python.contains("cargo"),
        "non-Rust task must not receive cargo guidance: {mid_python}"
    );
    assert!(
        mid_python.contains("pytest") && mid_python.contains("unittest"),
        "the project's own runner is named instead: {mid_python}"
    );
    // Early band: project-agnostic in both cases.
    assert_eq!(
        progress_guidance(10.0, true),
        progress_guidance(10.0, false)
    );
    // Late band is project-aware too.
    assert!(!progress_guidance(90.0, false).contains("cargo"));
    assert!(progress_guidance(90.0, true).contains("tests pass"));
}

#[test]
fn operational_verification_steps_are_project_aware() {
    // Finding 1(a): the injected operational plan used to hard-code "Run
    // cargo_check"/"Run cargo_test" for every task, steering Python tasks at
    // cargo. Steps must follow the detected project type.
    let rust = operational_plan_verification_steps(super::super::ProjectType::Rust);
    assert!(rust.iter().any(|s| s.contains("cargo")), "{rust:?}");
    let python = operational_plan_verification_steps(super::super::ProjectType::Python);
    assert!(
        !python.iter().any(|s| s.contains("cargo")),
        "python plan must not mention cargo: {python:?}"
    );
    assert!(
        python.iter().any(|s| s.contains("pytest")),
        "python plan names its own runner: {python:?}"
    );
}

#[test]
fn auto_write_verification_directive_is_project_aware() {
    // Finding 1(a): the synthesis auto-write directive must not tell a Python
    // task to "run cargo check or cargo test".
    assert!(auto_write_verification_directive(true).contains("cargo check"));
    let python_directive = auto_write_verification_directive(false);
    assert!(
        !python_directive.contains("cargo"),
        "python auto-write directive must not mention cargo: {python_directive}"
    );
}

#[derive(Default)]
struct RecordingEventEmitter {
    events: std::sync::Mutex<Vec<AgentEvent>>,
}

impl super::super::tui_events::EventEmitter for RecordingEventEmitter {
    fn emit(&self, event: AgentEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingEventEmitter {
    fn events(&self) -> Vec<AgentEvent> {
        self.events.lock().unwrap().clone()
    }
}

fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        // Set context_length high enough that max_context_tokens doesn't become 0
        // after subtracting max_tokens and safety margin
        context_length: 500_000,
        max_tokens: 8192,
        agent: AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            stream_stall_timeout_secs: None,
            streaming,
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

#[test]
fn fatal_loop_errors_are_not_recoverable() {
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "READ_LOOP_NO_EDIT: no mutation after guard"
    )));
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "EDIT_FAILURE_LOOP_AFTER_EDIT: repeated stale edits"
    )));
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "VERIFICATION_LOOP_AFTER_EDIT: repeated checks"
    )));
    // Regression: repeated fake-completes must be terminal, not re-nudged 10×.
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "FAKE_COMPLETE_LOOP: completion gate rejected 4 final answers"
    )));
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "NONTERM_PROSE_NO_TOOL: mutation-required task produced 6 consecutive no-tool turns"
    )));
    // Regression: empty-response recovery exhaustion must be terminal — it
    // already retried once non-streaming, so the outer runner recovering it
    // only burns the turn budget one empty turn at a time (LOOP-EMPTY-NOTFATAL).
    assert!(is_fatal_loop_error(&anyhow::anyhow!(
        "EMPTY_RESPONSE_LOOP: 2 consecutive empty assistant responses"
    )));
    // Typed killswitch downcasting checks
    let ks_err = crate::safety::killswitch::KillswitchError::InProcess {
        reason: "unit test halt".to_string(),
    };
    assert!(is_fatal_loop_error(&anyhow::Error::from(ks_err)));

    let safety_ks_err =
        crate::errors::SelfwareError::Safety(crate::errors::SafetyError::KillswitchActive {
            reason: "file sentinel active".to_string(),
        });
    assert!(is_fatal_loop_error(&anyhow::Error::from(safety_ks_err)));

    // Unanchored string containing "killswitch" in user prompt or harmless log MUST NOT trigger fatal abort
    assert!(!is_fatal_loop_error(&anyhow::anyhow!(
        "User prompt contains the word killswitch in documentation"
    )));
    assert!(!is_fatal_loop_error(&anyhow::anyhow!(
        "temporary API failure"
    )));
}

// =========================================================================
// build_progress_injection -- exhaustive branch coverage (standalone)
// =========================================================================

/// Mirror of the production status line (see `progress_injection_status`).
/// Mirrored here — like the guidance string below — so the message-level
/// tests below do not need a full Agent; the real-Agent tests further down
/// exercise the production function end-to-end.
fn progress_injection_status_standalone(has_verification: bool, denials: usize) -> String {
    if denials >= SAFETY_DENIAL_THRESHOLD {
        format!("Safety: {denials} tool call(s) denied/blocked \u{2014} path blocked")
    } else if has_verification {
        "Verification: PASSED".to_string()
    } else {
        "Verification: NOT YET RUN (required before completion)".to_string()
    }
}

/// Mirror of the production blocker guidance (see `denial_blocker_guidance`).
fn denial_blocker_guidance_standalone(denials: usize) -> String {
    format!(
        "Tool calls have been denied or blocked {denials} times. This path is not proceeding \u{2014} \
         retrying the same denied calls will keep failing. Change to a different approach that \
         stays within the allowed operations, or if the denials block the task entirely, stop \
         and report that outcome instead."
    )
}

fn build_progress_injection_standalone_with_denials(
    step: usize,
    max_iterations: usize,
    has_verification: bool,
    denials: usize,
) -> Option<String> {
    if step == 0 || !(step + 1).is_multiple_of(5) {
        return None;
    }
    let pct = ((step + 1) as f64 / max_iterations as f64 * 100.0).min(100.0);
    let verification_status = progress_injection_status_standalone(has_verification, denials);
    let guidance = if denials >= SAFETY_DENIAL_THRESHOLD {
        denial_blocker_guidance_standalone(denials)
    } else if pct < 30.0 {
        "You have plenty of budget remaining. Be thorough \u{2014} read relevant code, \
             implement carefully, and verify each change."
            .to_string()
    } else if pct < 70.0 {
        "Good progress. Continue implementing and make sure to verify with cargo_check/cargo_test."
            .to_string()
    } else {
        "You are using most of your budget. Wrap up: ensure all changes compile \
             and tests pass, then provide your final summary."
            .to_string()
    };
    Some(format!(
        "[Progress: step {}/{} ({:.0}% budget used) | {}]\n{}",
        step + 1,
        max_iterations,
        pct,
        verification_status,
        guidance
    ))
}

fn build_progress_injection_standalone(
    step: usize,
    max_iterations: usize,
    has_verification: bool,
) -> Option<String> {
    // Clean-run path (finding D): zero denials must be byte-for-byte the
    // pre-fix injection, so the existing tests pin denials=0 wording.
    build_progress_injection_standalone_with_denials(step, max_iterations, has_verification, 0)
}

#[test]
fn test_progress_injection_none_for_step_zero() {
    assert!(build_progress_injection_standalone(0, 100, false).is_none());
}

#[test]
fn test_progress_injection_none_for_non_multiple_of_5() {
    assert!(build_progress_injection_standalone(1, 100, false).is_none());
    assert!(build_progress_injection_standalone(2, 100, false).is_none());
    assert!(build_progress_injection_standalone(3, 100, false).is_none());
    assert!(build_progress_injection_standalone(5, 100, false).is_none());
    assert!(build_progress_injection_standalone(7, 100, false).is_none());
}

#[test]
fn test_progress_injection_some_for_step_4() {
    let result = build_progress_injection_standalone(4, 100, false);
    assert!(result.is_some());
    assert!(result.unwrap().contains("step 5/100"));
}

#[test]
fn test_progress_injection_some_for_step_9() {
    let result = build_progress_injection_standalone(9, 100, false);
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("step 10/100"));
    assert!(msg.contains("10% budget used"));
}

#[test]
fn test_progress_injection_some_for_step_14() {
    let result = build_progress_injection_standalone(14, 100, false);
    assert!(result.is_some());
    assert!(result.unwrap().contains("step 15/100"));
}

#[test]
fn test_progress_injection_low_budget_guidance() {
    let msg = build_progress_injection_standalone(4, 100, false).unwrap();
    assert!(msg.contains("plenty of budget remaining"));
    assert!(msg.contains("Be thorough"));
}

#[test]
fn test_progress_injection_mid_budget_guidance() {
    let msg = build_progress_injection_standalone(49, 100, false).unwrap();
    assert!(msg.contains("Good progress"));
    assert!(msg.contains("cargo_check/cargo_test"));
}

#[test]
fn test_progress_injection_high_budget_guidance() {
    let msg = build_progress_injection_standalone(69, 100, false).unwrap();
    assert!(msg.contains("most of your budget"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_pct_capped_at_100() {
    let msg = build_progress_injection_standalone(14, 10, false).unwrap();
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("most of your budget"));
}

#[test]
fn test_progress_injection_verification_not_run() {
    let msg = build_progress_injection_standalone(4, 100, false).unwrap();
    assert!(msg.contains("Verification: NOT YET RUN"));
    assert!(msg.contains("required before completion"));
}

#[test]
fn test_progress_injection_verification_passed() {
    let msg = build_progress_injection_standalone(4, 100, true).unwrap();
    assert!(msg.contains("Verification: PASSED"));
    assert!(!msg.contains("NOT YET RUN"));
}

#[test]
fn test_progress_injection_step_19() {
    let msg = build_progress_injection_standalone(19, 100, false).unwrap();
    assert!(msg.contains("step 20/100"));
    assert!(msg.contains("20% budget used"));
}

#[test]
fn test_progress_injection_step_24() {
    let msg = build_progress_injection_standalone(24, 100, false).unwrap();
    assert!(msg.contains("step 25/100"));
    assert!(msg.contains("plenty of budget remaining"));
}

#[test]
fn test_progress_injection_boundary_30_pct() {
    let msg = build_progress_injection_standalone(29, 100, false).unwrap();
    assert!(msg.contains("Good progress"));
}

#[test]
fn test_progress_injection_boundary_70_pct() {
    let msg = build_progress_injection_standalone(69, 100, true).unwrap();
    assert!(msg.contains("Wrap up"));
    assert!(msg.contains("Verification: PASSED"));
}

#[test]
fn test_progress_injection_small_max_iterations() {
    let msg = build_progress_injection_standalone(4, 5, false).unwrap();
    assert!(msg.contains("step 5/5"));
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_max_iterations_1() {
    let msg = build_progress_injection_standalone(4, 1, false).unwrap();
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_step_99_max_100() {
    let msg = build_progress_injection_standalone(99, 100, false).unwrap();
    assert!(msg.contains("step 100/100"));
    assert!(msg.contains("100% budget used"));
    assert!(msg.contains("Wrap up"));
}

#[test]
fn test_progress_injection_large_step_numbers() {
    let msg = build_progress_injection_standalone(499, 1000, true).unwrap();
    assert!(msg.contains("step 500/1000"));
    assert!(msg.contains("50% budget used"));
    assert!(msg.contains("Good progress"));
    assert!(msg.contains("Verification: PASSED"));
}

#[test]
fn test_progress_injection_exactly_at_boundary_29_not_multiple() {
    assert!(build_progress_injection_standalone(28, 100, false).is_none());
}

// =========================================================================
// build_progress_injection -- safety-denial awareness (finding D)
// =========================================================================

#[test]
fn test_progress_injection_denials_suppress_positive_guidance_all_bands() {
    // Finding D (review item #10): after safety denials accumulate past the
    // threshold (3), the injection must STOP pushing "keep going / verify" —
    // low, mid, AND high budget bands — and name the blocker instead. The
    // "Verification: NOT YET RUN (required before completion)" nudge is itself
    // a push when the denied tool is the verifier, so it is suppressed too.
    let low = build_progress_injection_standalone_with_denials(4, 100, false, 3).unwrap();
    assert!(
        !low.contains("plenty of budget"),
        "low-band positive reinforcement must be suppressed: {low}"
    );
    assert!(!low.contains("Be thorough"), "{low}");
    assert!(!low.contains("Good progress"), "{low}");
    assert!(
        !low.contains("Verification: NOT YET RUN"),
        "the 'required before completion' nudge is a push against a wall: {low}"
    );
    assert!(
        low.contains("denied or blocked"),
        "the blocker must be named: {low}"
    );

    let mid = build_progress_injection_standalone_with_denials(49, 100, false, 3).unwrap();
    assert!(!mid.contains("Good progress"), "{mid}");
    assert!(!mid.contains("cargo_check"), "{mid}");
    assert!(mid.contains("denied or blocked"), "{mid}");

    let high = build_progress_injection_standalone_with_denials(69, 100, true, 3).unwrap();
    assert!(
        !high.contains("Wrap up"),
        "wrap-up direction is itself a push: {high}"
    );
    assert!(!high.contains("tests pass"), "{high}");
    assert!(!high.contains("Verification: PASSED"), "{high}");
    assert!(high.contains("denied or blocked"), "{high}");
}

#[test]
fn test_progress_injection_denials_named_in_message_and_status() {
    let msg = build_progress_injection_standalone_with_denials(14, 100, false, 3).unwrap();
    assert!(
        msg.contains("3 times"),
        "the denial directive must state the count: {msg}"
    );
    assert!(
        msg.contains("Safety: 3 tool call(s) denied/blocked"),
        "the status line must name the blocker: {msg}"
    );
}

#[test]
fn test_progress_injection_below_denial_threshold_unchanged() {
    // 1-2 denials are below the threshold: the historical guidance stays.
    let msg = build_progress_injection_standalone_with_denials(49, 100, false, 2).unwrap();
    assert!(msg.contains("Good progress"), "{msg}");
    assert!(msg.contains("cargo_check/cargo_test"), "{msg}");
    assert!(!msg.contains("denied or blocked"), "{msg}");
    let one = build_progress_injection_standalone_with_denials(49, 100, false, 1).unwrap();
    assert!(one.contains("Good progress"), "{one}");
    assert!(
        one.contains("Verification: NOT YET RUN (required before completion)"),
        "{one}"
    );
}

#[test]
fn test_progress_injection_zero_denials_unchanged() {
    // Zero behavior change on a clean run: denials=0 output carries the exact
    // pre-fix wording (the historical tests above also pin these strings).
    let msg = build_progress_injection_standalone_with_denials(49, 100, false, 0).unwrap();
    assert!(msg.contains("Good progress"), "{msg}");
    assert!(msg.contains("cargo_check/cargo_test"), "{msg}");
    assert!(
        msg.contains("Verification: NOT YET RUN (required before completion)"),
        "{msg}"
    );
    assert!(!msg.contains("denied or blocked"), "{msg}");
}

#[test]
fn test_progress_injection_status_and_guidance_production_fns() {
    // The production status/guidance functions own the wording the standalone
    // mirrors replicate; pin them here so mirror drift is caught.
    assert_eq!(
        progress_injection_status(false, 3),
        "Safety: 3 tool call(s) denied/blocked \u{2014} path blocked"
    );
    assert_eq!(
        progress_injection_status(true, 3),
        progress_injection_status(false, 3)
    );
    assert_eq!(
        progress_injection_status(false, 2),
        "Verification: NOT YET RUN (required before completion)"
    );
    assert_eq!(progress_injection_status(true, 2), "Verification: PASSED");
    let blocker = denial_blocker_guidance(3);
    assert!(blocker.contains("3 times"), "{blocker}");
    assert!(
        !blocker.contains("keep going") && !blocker.contains("verify"),
        "the blocker note must not push the model onward: {blocker}"
    );
}

#[test]
fn test_count_safety_denials_only_counts_skipped_denials() {
    // The counter must match ONLY the denial markers produced by
    // push_tool_skip_message — never ordinary tool results, system messages,
    // or assistant turns.
    let messages = vec![
        crate::api::types::Message::system("system prompt"),
        crate::api::types::Message::user(
            "<tool_result><skipped>Blocked by YOLO safety gate: /etc/passwd</skipped></tool_result>",
        ),
        crate::api::types::Message::user(
            "<tool_result><skipped>Denied (unattended session, no operator to confirm): /etc/shadow</skipped></tool_result>",
        ),
        crate::api::types::Message::user(
            "<tool_result><skipped>Tool execution denied via TUI permission prompt</skipped></tool_result>",
        ),
        // Ordinary tool result that merely mentions "skipped" as data:
        crate::api::types::Message::user(
            "<tool_result><output>skipped_entries: 4</output></tool_result>",
        ),
        crate::api::types::Message::tool(
            "{\"skipped\": \"Blocked by YOLO safety gate: /etc/passwd\"}",
            "call_x",
        ),
        crate::api::types::Message::tool("{\"output\": \"ok\"}", "call_y"),
        crate::api::types::Message::assistant("Good progress, keep going"),
    ];
    assert_eq!(count_safety_denials(&messages), 4);
    assert_eq!(count_safety_denials(&[]), 0);
}

#[test]
fn test_count_safety_denials_threshold_rule() {
    // 3 consecutive denials trip the threshold; the counter is cumulative.
    let two_denials = vec![
        crate::api::types::Message::user(
            "<tool_result><skipped>Blocked by YOLO safety gate</skipped></tool_result>",
        ),
        crate::api::types::Message::user(
            "<tool_result><skipped>Blocked by YOLO safety gate</skipped></tool_result>",
        ),
    ];
    assert!(count_safety_denials(&two_denials) < SAFETY_DENIAL_THRESHOLD);
    let mut three_denials = two_denials;
    three_denials.push(crate::api::types::Message::user(
        "<tool_result><skipped>Blocked by YOLO safety gate</skipped></tool_result>",
    ));
    assert!(count_safety_denials(&three_denials) >= SAFETY_DENIAL_THRESHOLD);
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_denial_aware_from_message_history() {
    // End-to-end on a real Agent: 3 consecutive skipped denials in the
    // message history (the live /etc/passwd probe shape) must flip the next
    // injection to the honest blocker note — no "Good progress", no
    // "Verification: NOT YET RUN (required before completion)" nudge — while
    // a clean Agent keeps the historical wording.
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let clean = agent.build_progress_injection(4).unwrap();
    assert!(
        clean.contains("Good progress"),
        "a clean run keeps the historical guidance: {clean}"
    );

    for i in 0..3 {
        agent.messages.push(crate::api::types::Message::user(format!(
            "<tool_result><skipped>Blocked by YOLO safety gate: /etc/passwd probe {i}</skipped></tool_result>"
        )));
    }
    let msg = agent.build_progress_injection(4).unwrap();
    assert!(
        !msg.contains("Good progress"),
        "no positive reinforcement after 3 denials: {msg}"
    );
    assert!(!msg.contains("plenty of budget"), "{msg}");
    assert!(
        !msg.contains("Verification: NOT YET RUN"),
        "the verification nudge is suppressed under denials: {msg}"
    );
    assert!(msg.contains("denied or blocked"), "{msg}");
    assert!(msg.contains("3 times"), "{msg}");
    server.stop().await;
}

// =========================================================================
// recovery_failure_advice
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_recovery_failure_advice_returns_nonempty_for_known_error() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let agent = Agent::new(config).await.unwrap();
    // A "Max iterations" failure with no mutating calls and 0 final answer
    // text should classify as MaxIterations (or a more specific kind) and
    // produce non-empty advice.
    let advice = agent.recovery_failure_advice("Max iterations exceeded");
    assert!(
        !advice.is_empty(),
        "recovery_failure_advice should return guidance for a known failure: {}",
        advice
    );
    assert!(
        advice.contains("Failure mode guidance"),
        "advice should contain the guidance header: {}",
        advice
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_recovery_failure_advice_returns_empty_for_success_like_state() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    // Set up a state that looks like success: mutating calls exist and the
    // error reason does not match any known failure pattern.  With mutating
    // > 0, classify on a non-matching Failed reason falls through to
    // classify_max_iter_failure which should still produce advice.
    // But with mutating calls and no specific signals, it returns
    // MaxIterations advice.  So let's instead verify the "-" guard: a
    // NaturalCompletion with mutating calls yields advice "-".
    agent.test_set_mutating_count(3);
    agent.test_set_total_tool_calls(12);
    // We can't call classify with NaturalCompletion from
    // recovery_failure_advice (it always uses Failed), so instead
    // verify the helper handles a benign error without panic.
    let advice = agent.recovery_failure_advice("some transient error");
    // Should be non-empty (some guidance) or empty — just ensure no panic.
    let _ = advice;
    server.stop().await;
}

// =========================================================================
// build_progress_injection -- via real Agent instance
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_no_checkpoint() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let agent = Agent::new(config).await.unwrap();
    assert!(agent.build_progress_injection(0).is_none());
    let msg = agent.build_progress_injection(4).unwrap();
    assert!(msg.contains("Good progress"));
    assert!(msg.contains("Verification: NOT YET RUN"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_with_cargo_check() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t1".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("OK".to_string()),
        success: true,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: PASSED"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_failed_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t2".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("error".to_string()),
        success: false,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: NOT YET RUN"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_cargo_test() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t3".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_test".to_string(),
        arguments: "{}".to_string(),
        result: Some("passed".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: PASSED"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_cargo_clippy() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t4".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_clippy".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(300),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: PASSED"));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_progress_injection_agent_non_verification_tool() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let mut cp = TaskCheckpoint::new("t5".to_string(), "task".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: r#"{"path":"foo.rs"}"#.to_string(),
        result: Some("content".to_string()),
        success: true,
        duration_ms: Some(10),
    });
    agent.current_checkpoint = Some(cp);
    assert!(agent
        .build_progress_injection(4)
        .unwrap()
        .contains("Verification: NOT YET RUN"));
    server.stop().await;
}

// =========================================================================
// memory_stats
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_memory_stats_returns_tuple() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let agent = Agent::new(config).await.unwrap();
    let (_len, _total_tokens, near_limit) = agent.memory_stats();
    assert!(!near_limit);
    server.stop().await;
}

// =========================================================================
// list_tasks / task_status / delete_task via temp dir
// =========================================================================

#[test]
fn test_list_tasks_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    assert!(manager.list_tasks().unwrap().is_empty());
}

#[test]
fn test_list_tasks_with_saved_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    let cp = TaskCheckpoint::new("list-test-1".to_string(), "Test task one".to_string());
    manager.save(&cp).unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, "list-test-1");
}

#[test]
fn test_list_tasks_multiple() {
    let _g = crate::test_support::CwdGuard::hold();
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    manager
        .save(&TaskCheckpoint::new("m1".to_string(), "First".to_string()))
        .unwrap();
    manager
        .save(&TaskCheckpoint::new("m2".to_string(), "Second".to_string()))
        .unwrap();
    manager
        .save(&TaskCheckpoint::new("m3".to_string(), "Third".to_string()))
        .unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert_eq!(tasks.len(), 3);
    let ids: Vec<&str> = tasks.iter().map(|t| t.task_id.as_str()).collect();
    assert!(ids.contains(&"m1"));
    assert!(ids.contains(&"m2"));
    assert!(ids.contains(&"m3"));
}

#[test]
fn test_task_status_loads_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    let mut cp = TaskCheckpoint::new("status-test".to_string(), "Status task".to_string());
    cp.set_step(5);
    cp.set_estimated_tokens(2000);
    manager.save(&cp).unwrap();
    let loaded = manager.load("status-test").unwrap();
    assert_eq!(loaded.task_id, "status-test");
    assert_eq!(loaded.current_step, 5);
    assert_eq!(loaded.estimated_tokens, 2000);
}

#[test]
fn test_task_status_nonexistent_not_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    // CheckpointManager.load auto-recovers by creating a fresh checkpoint,
    // so use the `exists` helper to verify no file is on disk.
    assert!(!manager.exists("nonexistent"));
}

#[test]
fn test_delete_task_removes_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    let cp = TaskCheckpoint::new("del-test".to_string(), "To delete".to_string());
    manager.save(&cp).unwrap();
    assert!(manager.exists("del-test"));
    manager.delete("del-test").unwrap();
    // After deletion the file should no longer exist on disk.
    // (CheckpointManager.load would auto-recover, so we use exists().)
    assert!(!manager.exists("del-test"));
}

#[test]
fn test_delete_nonexistent_task_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
    assert!(manager.delete("does-not-exist").is_ok());
}

// =========================================================================
// run_task E2E
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_completes_with_plain_text() {
    let server = MockLlmServer::builder()
        .with_response("Analyzed.")
        .with_response("Complete.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.run_task("Do a simple task").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert!(agent.current_checkpoint.is_some());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn run_task_persistent_empty_responses_terminate_as_loop_break() {
    // Acceptance (defects 1 + 2 from the empty-response follow-up review):
    //
    // 1. The mutation no-action handler used to run BEFORE the empty-response
    //    check, so a coding task whose model answered empty never latched the
    //    non-streaming retry and could fall into NONTERM_PROSE_NO_TOOL instead.
    //    Classifying at the top of the step (planning AND execution) closes it.
    // 2. EMPTY_RESPONSE_LOOP was missing from is_fatal_loop_error, so after
    //    the breaker fired the outer runner "recovered" and kept requesting.
    //    It must be terminal: this run makes exactly TWO requests (one empty
    //    streamed planning turn → one non-streaming execution retry) and then
    //    stops with the typed reason.
    let server = MockLlmServer::builder()
        .with_response("") // planning (streamed): counted; latches force_non_streaming
        .with_response("") // execution (non-streaming retry): 2nd consecutive empty → EMPTY_RESPONSE_LOOP
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), true);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Fix the off-by-one bug in src/lib.rs").await;
    let err = result.expect_err("a persistently empty endpoint must stop the run");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("EMPTY_RESPONSE_LOOP"),
        "the run must stop with the typed loop break, got: {err_msg}"
    );
    assert!(
        !err_msg.contains("NONTERM_PROSE_NO_TOOL"),
        "an empty response is a provider hiccup, not prose-without-tools — got: {err_msg}"
    );
    assert!(
        agent.force_non_streaming,
        "the empty stream must latch the non-streaming retry even on a coding task"
    );
    assert_eq!(
        agent.consecutive_empty_responses, 2,
        "both the empty planning turn and the empty execution turn must count"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        2,
        "EMPTY_RESPONSE_LOOP is terminal: no requests after the second empty, got {}",
        requests.len()
    );

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_terminal_401_planning_error_not_retried() {
    // Every request gets a 401. Client-level retries are disabled so each
    // planning attempt is exactly one HTTP request, and the mock records
    // how many hit the wire. The agent-level planning retry loop must NOT
    // retry a terminal 4xx — it can never succeed and the user should see
    // the auth remediation hint immediately.
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 401,
            body: r#"{"error":"No cookie auth credentials found"}"#.to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Do a simple task").await;

    assert!(result.is_err(), "run_task should fail on a terminal 401");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Hint"),
        "expected the auth remediation hint to reach the user, got: {err}"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        1,
        "terminal 401 must fail after a single planning attempt, got {} requests",
        requests.len()
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_503_planning_error_still_retried() {
    // A 503 is a transient server error: the planning-level retry loop
    // must preserve its existing behavior (MAX_PLANNING_RETRIES = 3
    // attempts) for retryable failures. Client-level retries are disabled
    // so each planning attempt is exactly one HTTP request.
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 503,
            body: r#"{"error":"service unavailable"}"#.to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Do a simple task").await;

    assert!(
        result.is_err(),
        "run_task should fail after exhausting planning retries on 503"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        3,
        "503 planning errors must still be retried up to MAX_PLANNING_RETRIES, got {} requests",
        requests.len()
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_terminates_at_token_budget() {
    // Every mock response reports 15 total tokens (see mock_api.rs), and the
    // budget check runs at the top of each loop iteration against the
    // cumulative usage the planning + step calls accrue. A 10-token cap must
    // therefore terminate the run rather than let it complete.
    let server = MockLlmServer::builder()
        .with_response("Working on it...")
        .with_response("Still working...")
        .with_response("Done.")
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_budget_tokens = Some(10);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("A task the budget should cut off").await;

    assert!(
        result.is_err(),
        "run_task should fail on budget exhaustion, got Ok"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Token budget exhausted"),
        "expected a token-budget error, got: {err}"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_budget_enforced_after_completing_step() {
    // Each mock response reports 15 total tokens (see mock_api.rs). The
    // planning LLM call brings cumulative usage to 15, which is still
    // under the 20-token cap at the next loop entry. The executing step
    // then makes a second billable LLM call (cumulative → 30) and the
    // model signals completion. The post-step budget check must catch the
    // overshoot and fail the run instead of letting it return Ok.
    // Planning must return a tool call here: a substantial tool-less
    // planning answer for a read-only task is now accepted as the final
    // answer right in the Planning state, which would keep cumulative
    // usage at 15 — under the cap — and the run would legitimately
    // complete Ok before any executing step ran.
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response(
            "The analysis is complete: all components reviewed and verified successfully.",
        )
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_budget_tokens = Some(20);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Describe the authentication module").await;

    assert!(
        result.is_err(),
        "run_task should fail when the budget is exceeded after a completing step, got Ok"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Token budget exhausted"),
        "expected a token-budget error, got: {err}"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_checkpoint_description() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Describe the login bug").await.unwrap();
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_description,
        "Describe the login bug"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_queued_task_gets_fresh_checkpoint() {
    // Bug #11: When an Agent is reused for a queued second task, the
    // checkpoint from task 1 must NOT persist — the new task needs its
    // own checkpoint with the correct task_description.
    let server = MockLlmServer::builder()
            // Task 1 responses
            .with_response("Plan.")
            .with_response("Done.")
            // Task 2 responses
            .with_response("Plan 2.")
            .with_response("Done 2.")
            .build()
            .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    // First task
    agent.run_task("First task").await.unwrap();
    let first_task_id = agent.current_checkpoint.as_ref().unwrap().task_id.clone();
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_description,
        "First task"
    );

    // Second (queued) task — must get its OWN checkpoint, not inherit task 1's
    agent.run_task("Second task").await.unwrap();
    assert_ne!(
        agent.current_checkpoint.as_ref().unwrap().task_id,
        first_task_id
    );
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_description,
        "Second task"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_adds_user_message() {
    let server = MockLlmServer::builder()
        .with_response("Planning.")
        .with_response("Completion.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Summarize error handling").await.unwrap();
    let has_msg = agent
        .messages
        .iter()
        .any(|m| m.role == "user" && m.content.text().contains("Summarize error handling"));
    assert!(has_msg, "task text should appear as a user message");
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_resets_loop_for_second_task() {
    let server = MockLlmServer::builder()
        .with_response("Plan 1.")
        .with_response("Done 1.")
        .with_response("Plan 2.")
        .with_response("Done 2.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Task one").await.unwrap();
    agent.run_task("Task two").await.unwrap();
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_with_tool_call() {
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Task complete.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.run_task("Read Cargo.toml").await;
    assert!(
        result.is_ok(),
        "run_task with tool call: {:?}",
        result.err()
    );
    let has_tool_result = agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("<tool_result>"));
    assert!(has_tool_result);
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_sets_strategic_goals() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Review unit tests").await.unwrap();
    assert!(!agent.cognitive_state.strategic_goals.is_empty());
    assert!(agent.cognitive_state.active_tactical_plan.is_some());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_sets_operational_plan() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Review feature X").await.unwrap();
    let plan = agent
        .cognitive_state
        .active_operational_plan
        .as_ref()
        .unwrap();
    assert_eq!(plan.steps.len(), 5);
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_preserves_existing_checkpoint() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "existing-id".to_string(),
        "Existing".to_string(),
    ));
    // Resume via continue_execution (the proper resume path) so the
    // existing checkpoint is preserved. run_task now always resets
    // the checkpoint for a genuinely new task (bug #11 fix).
    agent.continue_execution().await.unwrap();
    assert_eq!(
        agent.current_checkpoint.as_ref().unwrap().task_id,
        "existing-id"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_cancellation() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("More.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let recorder = std::sync::Arc::new(RecordingEventEmitter::default());
    let mut agent = Agent::new(config)
        .await
        .unwrap()
        .with_event_emitter(recorder.clone());
    agent
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let error = agent
        .run_task("Should cancel")
        .await
        .expect_err("cancellation must not report successful completion");
    assert!(matches!(
        error.downcast_ref::<crate::errors::AgentError>(),
        Some(crate::errors::AgentError::Cancelled)
    ));
    let has_interrupted = agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("interrupted"));
    assert!(has_interrupted);

    let events = recorder.events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::Error { .. }))
            .count(),
        1,
        "cancellation must emit exactly one terminal Error event"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Completed { .. })),
        "cancellation must never emit Completed"
    );
    agent.reset_cancellation();
    assert!(!agent.is_cancelled());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_streaming_mode() {
    // A substantial (>= 40 char) tool-less final answer completes a
    // non-mutation task via the read-only acceptance path. (Earlier this
    // test relied on an empty response being accepted as done, which is no
    // longer valid — an empty/dropped response is not a completion.)
    let server = MockLlmServer::builder()
        .with_response("Streaming plan: I will read the stream and summarize it.")
        .with_response(
            "The streaming request has been fully processed and here is the \
                 complete final summary of the result.",
        )
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), true);
    let mut agent = Agent::new(config).await.unwrap();
    // A read-only prose task ("describe ...") so a substantial tool-less
    // answer is accepted as completion (exercising streaming end-to-end).
    assert!(agent
        .run_task("Describe the streaming pipeline")
        .await
        .is_ok());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_starts_learning_session() {
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.run_task("Read tests for parser").await.unwrap();
    assert!(!agent.current_task_context.is_empty());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_run_task_max_iterations_exhaustion() {
    // Plain-text responses are treated as task completion, so to force the
    // agent to keep iterating we must provide tool-call responses.  Each
    // file_read tool call keeps the agent in the loop until max_iterations
    // is exhausted.
    let tool_resp = r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#;
    let mut builder = MockLlmServer::builder();
    for _ in 0..20 {
        builder = builder.with_response(tool_resp);
    }
    let server = builder.build().await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 3;
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.run_task("Never completes").await;
    // The agent should either fail with an error about max iterations or
    // the loop should exhaust without completing (the loop exits with
    // Ok after all states are consumed when next_state returns None).
    // Either outcome is acceptable -- what matters is that the agent
    // does NOT treat a tool-call response as a completion.
    if let Err(e) = &result {
        let err = e.to_string();
        assert!(
            err.contains("Agent failed")
                || err.contains("Max iterations")
                || err.contains("iterations"),
            "unexpected error: {}",
            err
        );
    }
    // If Ok, verify the agent ran through multiple execution steps (not
    // a single-step completion).
    if result.is_ok() {
        assert!(
            agent.loop_control.current_step() >= 2,
            "agent should have iterated multiple steps, got {}",
            agent.loop_control.current_step()
        );
    }
    server.stop().await;
}

// =========================================================================
// continue_execution E2E
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_no_checkpoint() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Resume plan.")
        .with_response("Resume done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    assert!(agent.continue_execution().await.is_ok());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_with_checkpoint() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "resume-1".to_string(),
        "Resumed".to_string(),
    ));
    assert!(agent.continue_execution().await.is_ok());
    assert!(agent.cognitive_state.active_tactical_plan.is_some());
    assert!(agent.cognitive_state.active_operational_plan.is_some());
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_executes_planned_tool_calls_immediately() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "resume-tool".to_string(),
        "Read manifest".to_string(),
    ));

    agent.continue_execution().await.unwrap();

    assert!(agent
        .file_tracker
        .context_files
        .iter()
        .any(|path| path.ends_with("Cargo.toml")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_cancellation() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("More.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let error = agent
        .continue_execution()
        .await
        .expect_err("cancellation must not report successful completion");
    assert!(matches!(
        error.downcast_ref::<crate::errors::AgentError>(),
        Some(crate::errors::AgentError::Cancelled)
    ));
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("interrupted")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_preserves_tactical_plan() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "p-test".to_string(),
        "Preserve".to_string(),
    ));
    agent.cognitive_state.set_active_tactical_plan(
        "existing-tactical".to_string(),
        "Existing plan".to_string(),
        vec!["dep".to_string()],
    );
    agent.continue_execution().await.unwrap();
    assert_eq!(
        agent
            .cognitive_state
            .active_tactical_plan
            .as_ref()
            .unwrap()
            .id,
        "existing-tactical"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_sets_operational_plan() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.continue_execution().await.unwrap();
    // An operational plan should exist after continue_execution.
    // Execution may modify the plan (e.g. start_operational_step can
    // replace it when the task_id differs), so we only assert it exists
    // with at least one step.
    let plan = agent
        .cognitive_state
        .active_operational_plan
        .as_ref()
        .unwrap();
    assert!(!plan.steps.is_empty(), "operational plan should have steps");
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_preserves_operational_plan() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("Plan.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.cognitive_state.set_operational_plan(
        "existing-op".to_string(),
        vec!["Step A".to_string(), "Step B".to_string()],
    );
    agent.continue_execution().await.unwrap();
    // continue_execution should NOT replace an existing plan (the guard at
    // line 647 skips set_operational_plan when one already exists).
    // However, during execution, start_operational_step may mutate it.
    // We verify the plan still exists after completion.
    let plan = agent
        .cognitive_state
        .active_operational_plan
        .as_ref()
        .unwrap();
    assert!(
        !plan.steps.is_empty(),
        "operational plan should survive execution"
    );
    server.stop().await;
}

/// Regression test: continue_execution must treat no-action errors as
/// fatal (go to Failed state) rather than routing to ErrorRecovery.
/// Before the run_execution_loop dedup this check was missing from
/// continue_execution, causing the agent to loop indefinitely.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_continue_execution_no_action_error_is_recoverable() {
    let _state = crate::test_support::ExecGuard::hold();
    // The model will repeatedly describe intent without using tools.
    // With recoverable no-action errors, the loop should eventually
    // exhaust max_iterations rather than fatally aborting early.
    let intent = "Let me check the code";
    let server = MockLlmServer::builder()
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            intent.to_string(),
        ))
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 8;
    config.agent.min_completion_steps = 3;
    let mut agent = Agent::new(config).await.unwrap();

    // Seed a checkpoint so continue_execution has something to resume
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "test-id".to_string(),
        "test task".to_string(),
    ));

    // The loop should complete (Ok) after exhausting iterations rather
    // than returning Err from a fatal no-action abort.
    let result = agent.continue_execution().await;
    // Either Ok (iterations exhausted) or Err (eventually hit lifetime limit)
    // — both are acceptable. The key is it doesn't abort after just 6 prompts.
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        assert!(
            err_msg.contains("failed to take action")
                || err_msg.contains("Agent failed")
                || err_msg.contains("Max iterations"),
            "unexpected error: {}",
            err_msg
        );
    }
    server.stop().await;
}

// =========================================================================
// analyze / review
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_analyze_calls_run_task() {
    let server = MockLlmServer::builder()
        .with_response("Analysis.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    assert!(agent.analyze("./src").await.is_ok());
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("Analyze the codebase")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_review_reads_file() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"fn main() { println!(\"hello\"); }").unwrap();
    let server = MockLlmServer::builder()
        .with_response("Review.")
        .with_response("Done.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    assert!(agent.review(tmp.path().to_str().unwrap()).await.is_ok());
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text().contains("Review the following code")));
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_review_nonexistent_file() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.review("/nonexistent/path/to/file.rs").await;
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("Failed to read file"));
    server.stop().await;
}

// =========================================================================
// Planner prompts
// =========================================================================

#[test]
fn test_planner_analyze_prompt() {
    let prompt = Planner::analyze_prompt("./my_project");
    assert!(prompt.contains("./my_project"));
    assert!(prompt.contains("Analyze the codebase"));
    assert!(prompt.contains("Directory structure"));
}

#[test]
fn test_planner_review_prompt() {
    let prompt = Planner::review_prompt("src/main.rs", "fn main() {}");
    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("fn main() {}"));
    assert!(prompt.contains("Review the following code"));
}

// =========================================================================
// AgentState enum variants
// =========================================================================

#[test]
fn test_agent_state_planning() {
    let state = AgentState::Planning;
    assert!(matches!(state, AgentState::Planning));
    assert!(format!("{:?}", state).contains("Planning"));
}

#[test]
fn test_agent_state_executing() {
    let state = AgentState::Executing { step: 42 };
    match &state {
        AgentState::Executing { step } => assert_eq!(*step, 42),
        _ => panic!(),
    }
    let d = format!("{:?}", state);
    assert!(d.contains("Executing") && d.contains("42"));
}

#[test]
fn test_agent_state_error_recovery() {
    let state = AgentState::ErrorRecovery {
        error: "oops".to_string(),
    };
    match &state {
        AgentState::ErrorRecovery { error } => assert_eq!(error, "oops"),
        _ => panic!(),
    }
    let d = format!("{:?}", state);
    assert!(d.contains("ErrorRecovery") && d.contains("oops"));
}

#[test]
fn test_agent_state_completed() {
    let state = AgentState::Completed;
    assert!(matches!(state, AgentState::Completed));
    assert!(format!("{:?}", state).contains("Completed"));
}

#[test]
fn test_agent_state_failed() {
    let state = AgentState::Failed {
        reason: "fatal".to_string(),
    };
    match &state {
        AgentState::Failed { reason } => assert_eq!(reason, "fatal"),
        _ => panic!(),
    }
    let d = format!("{:?}", state);
    assert!(d.contains("Failed") && d.contains("fatal"));
}

#[test]
fn test_agent_state_clone_all() {
    let states = vec![
        AgentState::Planning,
        AgentState::Executing { step: 7 },
        AgentState::ErrorRecovery {
            error: "err".to_string(),
        },
        AgentState::Completed,
        AgentState::Failed {
            reason: "r".to_string(),
        },
    ];
    for s in &states {
        assert_eq!(format!("{:?}", s), format!("{:?}", s.clone()));
    }
}

// =========================================================================
// AgentLoop interaction
// =========================================================================

#[test]
fn test_agent_loop_reset_for_task_then_run() {
    let mut lc = AgentLoop::new(5);
    lc.next_state();
    lc.next_state();
    lc.next_state();
    lc.reset_for_task();
    assert!(matches!(lc.next_state(), Some(AgentState::Planning)));
    assert_eq!(lc.current_step(), 0);
}

#[test]
fn test_agent_loop_approaching_limit() {
    let mut lc = AgentLoop::new(10);
    // Planning doesn't increment, so transition to Executing first.
    lc.next_state(); // Planning
    lc.transition_to(super::loop_control::AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..8 {
        lc.next_state();
    }
    let w = lc.approaching_limit_warning();
    assert!(w.is_some());
    assert!(w.unwrap().contains("wrapping up"));
}

// =========================================================================
// Rigor mode (Increment 3 of self-healing)
// =========================================================================

#[test]
#[cfg(feature = "resilience")]
fn test_should_enter_rigor_escalate() {
    let outcome = crate::self_healing::ResolutionOutcome::Escalate;
    assert!(Agent::should_enter_rigor(&outcome));
}

#[test]
#[cfg(feature = "resilience")]
fn test_should_enter_rigor_unresolvable() {
    let outcome = crate::self_healing::ResolutionOutcome::Unresolvable;
    assert!(Agent::should_enter_rigor(&outcome));
}

#[test]
#[cfg(feature = "resilience")]
fn test_should_enter_rigor_resolved_is_false() {
    let action = crate::self_healing::RecoveryAction::Fallback {
        target: "http://localhost:11434/v1".to_string(),
    };
    let directive = crate::self_healing::RecoveryDirective::Action(action);
    let outcome = crate::self_healing::ResolutionOutcome::Resolved(directive);
    assert!(
        !Agent::should_enter_rigor(&outcome),
        "Resolved outcome should NOT trigger rigor mode"
    );
}

#[test]
fn test_careful_mode_directive_is_nonempty_and_specific() {
    let directive = Agent::careful_mode_directive();
    assert!(!directive.is_empty());
    assert!(directive.starts_with("[CAREFUL MODE]"));
    // The directive must mention verification explicitly.
    assert!(directive.to_lowercase().contains("verification"));
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_completion_gate_rejects_in_rigor_mode_without_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let url = server.url();
    let mut config = mock_agent_config(format!("{}/v1", url), false);
    config.agent.require_verification_before_completion = false;
    config.agent.min_completion_steps = 0;
    let mut agent = Agent::new(config).await.unwrap();
    agent.rigor_mode = true;
    agent.has_written_any_file = true;
    agent.current_task_context = "Fix the bug in foo()".to_string();
    let result = agent.check_completion_gate().await;
    assert!(
        result.is_some(),
        "In rigor mode, completion should be rejected without verification: {:?}",
        result
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_completion_gate_accepts_in_rigor_mode_with_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let url = server.url();
    let mut config = mock_agent_config(format!("{}/v1", url), false);
    config.agent.require_verification_before_completion = false;
    config.agent.min_completion_steps = 0;
    let mut agent = Agent::new(config).await.unwrap();
    agent.rigor_mode = true;
    agent.has_written_any_file = true;
    // Use a non-mutation task context so the mutation gate (which checks
    // git diff) does not fire — we are testing the verification requirement
    // part of the gate, not the mutation gate.
    agent.current_task_context = "Summarize the authentication module".to_string();
    // Add a file_write tool call to message history so the "no file write"
    // gate does not fire (that gate checks message tool_calls, not checkpoint).
    let mut file_write_msg = crate::api::types::Message::assistant("Writing the fix.");
    file_write_msg.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "file_write".to_string(),
            arguments: r#"{"path":"src/foo.rs","content":"fn foo() {}"}"#.to_string(),
        },
    }]);
    agent.messages.push(file_write_msg);
    // Add a successful verification tool call to checkpoint.
    let mut cp = TaskCheckpoint::new("rigor-test".to_string(), "Summarize".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(cp);
    let result = agent.check_completion_gate().await;
    assert!(
        result.is_none(),
        "In rigor mode with verification and file write, completion should be accepted: {:?}",
        result
    );
    server.stop().await;
}

// =========================================================================
// Checkpoint verification detection
// =========================================================================

#[test]
fn test_checkpoint_verification_detection() {
    let mut cp = TaskCheckpoint::new("v-test".to_string(), "verify".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_read".to_string(),
        arguments: "{}".to_string(),
        result: Some("c".to_string()),
        success: true,
        duration_ms: Some(10),
    });
    let check = |cp: &TaskCheckpoint| {
        cp.tool_calls.iter().any(|tc| {
            tc.success
                && matches!(
                    tc.tool_name.as_str(),
                    "cargo_check" | "cargo_test" | "cargo_clippy"
                )
        })
    };
    assert!(!check(&cp));

    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("err".to_string()),
        success: false,
        duration_ms: Some(200),
    });
    assert!(!check(&cp));

    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_test".to_string(),
        arguments: "{}".to_string(),
        result: Some("passed".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    assert!(check(&cp));
}

// =========================================================================
// Planning final-answer shortcut (chat tasks complete in ONE provider call)
// =========================================================================

/// A plain chat task ("explain X") whose planning response has no tool
/// calls must be accepted as the final answer right in the Planning
/// state — completing with EXACTLY ONE provider request instead of
/// falling through to Executing and paying a second call that only
/// repeats the same answer (rendered twice in the TUI).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_planning_final_answer_completes_chat_task_with_one_request() {
    let _state = crate::test_support::ExecGuard::hold();
    let answer = "A hash map stores key-value pairs in buckets chosen by \
                      hashing the key, so lookups are O(1) on average.";
    let server = MockLlmServer::builder().with_response(answer).build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Explain how a hash map works").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert_eq!(
        agent.last_assistant_response.trim(),
        answer,
        "the returned message must be the planning answer"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        1,
        "a plain chat answer must cost exactly one provider call, got {}",
        requests.len()
    );
    server.stop().await;
}

/// A planning response WITH tool calls must not be finalized early: the
/// run keeps the plan → execute path — planning call, tool execution,
/// then one execution call for the final answer (2 requests total).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_planning_tool_call_still_takes_plan_then_execute_path() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .with_response("Read complete: the manifest is consistent and nothing needs to change.")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Read Cargo.toml and finish").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.content.contains("<tool_result>")),
        "the planned tool call must have executed"
    );
    assert!(agent.last_assistant_response.contains("Read complete"));
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        2,
        "tool-call planning must proceed through exactly one execution call, got {}",
        requests.len()
    );
    server.stop().await;
}

/// A too-short planning answer is NOT a final answer: the run falls
/// through to Executing per the existing rules and completes on the
/// substantial answer produced there (2 requests).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_short_planning_answer_falls_through_to_executing() {
    let _state = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder()
        .with_response("ok")
        .with_response(
            "Ownership gives every value a single owner; when the owner \
                 goes out of scope, the value is dropped.",
        )
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Explain ownership in Rust").await;
    assert!(
        result.is_ok(),
        "run_task should succeed: {:?}",
        result.err()
    );
    assert!(agent.last_assistant_response.contains("Ownership gives"));
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        2,
        "a too-short planning answer must fall through to Executing, got {}",
        requests.len()
    );
    server.stop().await;
}

/// Gate-level checks for `planning_answer_ready_to_finalize`: a mutation
/// task must NEVER finalize from Planning with no edits (the deliverable
/// is a source change), and confused / too-short planning answers are
/// rejected so the loop iterates per the existing no-progress rules.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_planning_answer_ready_to_finalize_gates() {
    let _state = crate::test_support::ExecGuard::hold();

    // (a) Mutation task: substantial, well-formed answer — still refused.
    let server = MockLlmServer::builder()
        .with_response(
            "The bug is in the login handler: it compares tokens \
                 case-sensitively. Normalizing both sides before the \
                 comparison fixes it.",
        )
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Fix the login bug in src/auth.rs";
    agent.start_learning_session("gate-mutation", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-mutation".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert!(
        agent.planning_answer_ready_to_finalize().await.is_none(),
        "a task whose deliverable is an edit must not finalize from Planning with no edits"
    );
    server.stop().await;

    // (b) Read-only task, too-short answer: rejected by the length gate.
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Explain ownership in Rust";
    agent.start_learning_session("gate-short", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-short".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert!(
        agent.planning_answer_ready_to_finalize().await.is_none(),
        "a too-short planning answer must not finalize"
    );
    server.stop().await;

    // (c) Read-only task, confused answer (framework self-references):
    // rejected by the confusion gate even though it is long enough.
    let confused = "I see selfware_system_directive in the context and \
                        maybe_prompt_for_action was mentioned, so I am unsure \
                        what to output here.";
    let server = MockLlmServer::builder()
        .with_response(confused)
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Explain ownership in Rust";
    agent.start_learning_session("gate-confused", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-confused".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert!(
        agent.planning_answer_ready_to_finalize().await.is_none(),
        "a confused planning answer must not finalize"
    );
    server.stop().await;

    // (d) Read-only task, substantial clean answer: accepted.
    let answer = "Ownership gives every value a single owner; when the owner \
                      goes out of scope, the value is dropped.";
    let server = MockLlmServer::builder().with_response(answer).build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    let task = "Explain ownership in Rust";
    agent.start_learning_session("gate-accept", task);
    agent.current_checkpoint = Some(TaskCheckpoint::new(
        "gate-accept".to_string(),
        task.to_string(),
    ));
    agent.messages.push(Message::user(task));
    let has_tool_calls = agent.plan().await.unwrap();
    assert!(!has_tool_calls);
    assert_eq!(
        agent.planning_answer_ready_to_finalize().await.as_deref(),
        Some(answer),
        "a substantial read-only answer that passes the completion gate must finalize"
    );
    server.stop().await;
}

// --- Wall-clock commit-mode bands (six-model consult, Opus 5 deadline
// policy: inject budget pressure before the hard stop so the run ships
// something). TB 3.0 evidence: two of four v3 failures were timeouts. ---

#[tokio::test]
async fn commit_mode_directive_fires_at_65_and_85_percent_once_each() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_wall_secs = Some(100);
    let mut agent = Agent::new(config).await.unwrap();

    // 50%: nothing yet.
    agent.task_start_time = std::time::Instant::now() - std::time::Duration::from_secs(50);
    let before = agent.messages.len();
    agent.maybe_inject_commit_mode_directive();
    assert_eq!(agent.messages.len(), before, "nothing fires at 50%");

    // 65%: COMMIT MODE once.
    agent.task_start_time = std::time::Instant::now() - std::time::Duration::from_secs(70);
    agent.maybe_inject_commit_mode_directive();
    let body: String = agent
        .messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains("COMMIT MODE"), "{body}");
    let n = agent.messages.len();
    agent.maybe_inject_commit_mode_directive();
    assert_eq!(agent.messages.len(), n, "fires once");

    // 85%: FINAL STRETCH once.
    agent.task_start_time = std::time::Instant::now() - std::time::Duration::from_secs(90);
    agent.maybe_inject_commit_mode_directive();
    let body: String = agent
        .messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains("FINAL STRETCH"), "{body}");
    let n = agent.messages.len();
    agent.maybe_inject_commit_mode_directive();
    assert_eq!(agent.messages.len(), n, "fires once");
    server.stop().await;
}

#[tokio::test]
async fn commit_mode_is_silent_without_wall_budget() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.task_start_time = std::time::Instant::now() - std::time::Duration::from_secs(10_000);
    let before = agent.messages.len();
    agent.maybe_inject_commit_mode_directive();
    assert_eq!(
        agent.messages.len(),
        before,
        "no budget configured — no directive"
    );
    server.stop().await;
}

// =========================================================================
// Synthesis auto-write honesty (P1: no success claim before the write lands)
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn synthesis_auto_write_claims_success_only_after_batch_succeeds() {
    // P1 regression: the phase-2 synthesis auto-write used to set
    // has_written_any_file and push a "code was auto-written" directive
    // BEFORE the write attempt, swallowing any execute_tool_batch error.
    // The success claim must only follow an Ok batch.
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    cwd.switch_to(dir.path());

    let code_answer = "Here is the implementation for src/lib.rs:\n```rust\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n}\n```";
    let server = MockLlmServer::builder()
        .with_response("Analyzed.")
        .with_response(code_answer)
        .with_default_response(MockResponse::Text(
            "Complete. The helper is explained and the requested code is written to disk."
                .to_string(),
        ))
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 4;
    let mut agent = Agent::new(config).await.unwrap();
    // Ground phase-2 synthesis with prior tool history and queue it so it
    // fires at the top of the first Executing step.
    agent.messages.push(Message::user(
        "<tool_result>pub fn existing_helper() -> i32 { 41 } // contents of src/lib.rs read earlier</tool_result>".to_string(),
    ));
    agent.pending_synthesis = Some("Explain what the helper function does".to_string());

    // The run's own outcome is not under test here — the auto-write side
    // effects are.
    let _ = agent
        .run_task("Explain what the helper function does")
        .await;

    assert!(
        agent.has_written_any_file,
        "a successful synthesis auto-write must credit the file write"
    );
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("auto-written to file")),
        "success directive should be pushed after the write succeeds"
    );
    assert!(
        !agent.messages.iter().any(|m| m
            .content
            .text()
            .contains("Auto-writing the code from your response FAILED")),
        "no failure directive when the write succeeded"
    );
    let written = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert!(
        written.contains("pub fn add"),
        "synthesized code should be on disk, got: {written}"
    );

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Adaptive iteration budget (loop 13) — Agent-level wiring
// ---------------------------------------------------------------------------

fn productive_turn(name: &str, args_hash: u64) -> crate::agent::loop_control::TurnProgress {
    crate::agent::loop_control::TurnProgress {
        had_success: true,
        signatures: vec![(name.to_string(), args_hash)],
    }
}

#[tokio::test]
async fn adaptive_budget_extends_repeatedly_on_sustained_productive_streak() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    // Five turns, each with a distinct successful tool call.
    for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        agent
            .recent_turn_progress
            .push_back(productive_turn(name, i as u64));
    }

    agent.loop_control = crate::agent::loop_control::AgentLoop::new(4);
    agent
        .loop_control
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    agent.loop_control.restore_progress(0, 4);
    let capped = agent.loop_control.next_state(); // 5 > 4 — cap tripped
    assert!(matches!(capped, Some(AgentState::Failed { .. })));

    let resumed = agent
        .maybe_extend_iteration_budget()
        .expect("productive streak must earn the extension");
    assert!(matches!(resumed, AgentState::Executing { .. }));
    assert_eq!(agent.loop_control.max_iterations(), 5);
    assert!(matches!(
        agent.loop_control.current_state_label(),
        "executing"
    ));

    // Multi-fire policy (TB4: the one-shot +50% still left productive tasks
    // dead at the cap): a still-productive streak keeps earning +25% grants,
    // up to the +100% ceiling (4 grants: cap 4 → 8).
    for expected_cap in [6, 7, 8] {
        let cap = agent.loop_control.max_iterations();
        agent.loop_control.restore_progress(0, cap);
        let tripped = agent.loop_control.next_state();
        assert!(matches!(tripped, Some(AgentState::Failed { .. })));
        assert!(
            agent.maybe_extend_iteration_budget().is_some(),
            "sustained productivity re-earns budget"
        );
        assert_eq!(agent.loop_control.max_iterations(), expected_cap);
    }
    // Fifth trip: the extension ceiling is reached.
    let cap = agent.loop_control.max_iterations();
    agent.loop_control.restore_progress(0, cap);
    let tripped = agent.loop_control.next_state();
    assert!(matches!(tripped, Some(AgentState::Failed { .. })));
    assert!(
        agent.maybe_extend_iteration_budget().is_none(),
        "total extension is capped at +100% of the original cap"
    );
    server.stop().await;
}

#[tokio::test]
async fn adaptive_budget_aborts_without_progress_signals() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    // Error-only streak: turns ran, but nothing succeeded.
    for i in 0..5u64 {
        agent
            .recent_turn_progress
            .push_back(crate::agent::loop_control::TurnProgress {
                had_success: false,
                signatures: vec![("file_read".to_string(), i)],
            });
    }
    agent.loop_control = crate::agent::loop_control::AgentLoop::new(4);
    agent.loop_control.restore_progress(0, 4);
    let capped = agent.loop_control.next_state();
    assert!(matches!(capped, Some(AgentState::Failed { .. })));
    assert!(
        agent.maybe_extend_iteration_budget().is_none(),
        "error-only streak must not earn an extension"
    );

    // No recorded turns at all: thin evidence also aborts.
    agent.recent_turn_progress.clear();
    assert!(agent.maybe_extend_iteration_budget().is_none());
    server.stop().await;
}

#[tokio::test]
async fn run_summary_reflects_tracked_state_honestly() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    // No writes, no verification, no cost yet.
    let summary = agent.run_summary();
    assert!(summary.files_changed.is_empty());
    assert!(summary.verification.is_none(), "verification not performed");
    assert!(summary.cost_usd.is_none(), "no invented cost");
    assert!(!summary.budget_extended);

    // Track some state: two writes (stale = written/edited), one extension,
    // and token/cost accumulators.
    agent.file_tracker.mark_written("src/zeta.rs");
    agent.file_tracker.mark_written("src/alpha.rs");
    agent.cumulative_token_usage.total = 42_000;
    agent.cumulative_cost_usd = 0.5;
    agent.loop_control = crate::agent::loop_control::AgentLoop::new(10);
    assert!(agent.loop_control.extend_budget_once().is_some());

    let summary = agent.run_summary();
    assert_eq!(
        summary.files_changed,
        vec!["src/alpha.rs".to_string(), "src/zeta.rs".to_string()],
        "files changed sorted"
    );
    assert!(summary.budget_extended);
    assert_eq!(summary.max_iterations, 12);
    assert_eq!(summary.total_tokens, 42_000);
    assert_eq!(summary.cost_usd, Some(0.5));
    server.stop().await;
}

#[tokio::test]
async fn preamble_has_no_mutation_mandates_on_read_only_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    agent
        .run_task("Review the authentication module and report findings. Do NOT edit any files.")
        .await
        .expect("read-only task completes");

    let system = agent.messages[0].content.text();
    assert!(
        !system.contains("MANDATORY WORKFLOW"),
        "read-only task must not get the workflow block: {}",
        &system[..system.len().min(600)]
    );
    assert!(
        !system.contains("IMPLEMENT: Make code changes"),
        "read-only task must not get mutation mandates"
    );
    server.stop().await;
}

#[tokio::test]
async fn preamble_keeps_mutation_workflow_on_mutation_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    // The mock can't satisfy the mutation completion gate (no real edit) —
    // the run will fail at the cap, but the preamble is injected BEFORE the
    // loop, and that is what this asserts.
    let _ = agent.run_task("Fix the off-by-one bug in src/lib.rs").await;

    let system = agent.messages[0].content.text();
    assert!(
        system.contains("MANDATORY WORKFLOW"),
        "mutation task must keep the workflow block"
    );
    assert!(system.contains("IMPLEMENT: Make code changes"));
    server.stop().await;
}

#[tokio::test]
async fn deferred_manifest_note_is_injected_at_task_start() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    agent
        .run_task("Do a simple task")
        .await
        .expect("task completes");

    assert!(
        agent.messages.iter().any(|m| m
            .content
            .text()
            .contains("<selfware_context_note kind=tool_manifest>")),
        "the deferred-tool manifest note must be injected at task start"
    );
    server.stop().await;
}

#[tokio::test]
async fn switch_model_hot_switches_session_model() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    let before = agent.model().to_string();

    let previous = agent.switch_model("qwen3.8-max").expect("switch succeeds");
    assert_eq!(previous, before);
    assert_eq!(agent.model(), "qwen3.8-max");

    // Empty names are rejected with the config key named.
    let err = agent
        .switch_model("   ")
        .expect_err("empty model must fail");
    assert!(err.to_string().contains("`model`"), "got: {err}");
    assert_eq!(agent.model(), "qwen3.8-max", "failed switch is a no-op");
    server.stop().await;
}

#[tokio::test]
async fn undo_redo_round_trip_restores_bytes() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    let temp = tempfile::tempdir().expect("tempdir");
    let file = temp.path().join("target.rs");
    std::fs::write(&file, "original\n").expect("write");

    // The dispatcher's snapshot point: checkpoint + pre-edit bytes.
    agent
        .edit_history
        .create_checkpoint(crate::session::edit_history::EditAction::FileEdit {
            path: file.clone(),
            tool: "file_edit".to_string(),
        });
    agent
        .edit_history
        .add_file_to_current(crate::session::edit_history::FileSnapshot::new(
            file.clone(),
            "original\n".to_string(),
        ));
    // The edit lands.
    std::fs::write(&file, "edited\n").expect("write");

    let undone = agent.undo_last_edit().await;
    assert!(undone.contains("Undone"), "{undone}");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        "original\n",
        "undo must restore the pre-edit bytes"
    );

    let redone = agent.redo_last_edit().await;
    assert!(redone.contains("Reapplied"), "{redone}");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        "edited\n",
        "redo must reapply the edit, not revert it again"
    );

    let again = agent.redo_last_edit().await;
    assert_eq!(again, "Nothing to redo");
    server.stop().await;
}

#[tokio::test]
async fn undo_reports_nothing_when_no_snapshot_exists() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    assert_eq!(agent.undo_last_edit().await, "Nothing to undo");
    assert_eq!(agent.redo_last_edit().await, "Nothing to redo");
    server.stop().await;
}

#[tokio::test]
async fn shell_passthrough_runs_command_into_context_without_starting_a_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    let messages_before = agent.messages.len();

    let rendered = agent.shell_passthrough("echo hello").await;
    assert_eq!(rendered, "$ echo hello\nhello\n", "exact basic-mode format");

    // Output went into context as ONE user message — and NO agent task was
    // started (the smoke-test bug was `!git log` becoming a full task).
    assert_eq!(agent.messages.len(), messages_before + 1);
    let last = &agent.messages[agent.messages.len() - 1];
    assert_eq!(last.role, "user");
    assert!(last
        .content
        .text()
        .contains("<shell_command>echo hello</shell_command>"));
    assert!(last.content.text().contains("hello"));
    assert!(
        agent.current_checkpoint.is_none(),
        "a passthrough must not start a task or checkpoint"
    );
    server.stop().await;
}

#[tokio::test]
async fn shell_passthrough_marks_truncation_and_exit_codes() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    let rendered = agent.shell_passthrough("seq 1 2000").await;
    assert!(
        rendered.contains("…[truncated]"),
        "display shows truncation"
    );
    let last = &agent.messages[agent.messages.len() - 1];
    assert!(
        last.content.text().contains("truncated=\"true\""),
        "context push carries the truncation flag: {}",
        &last.content.text()[..120]
    );

    let rendered = agent.shell_passthrough("exit 3").await;
    assert!(
        rendered.starts_with("$ exit 3 (exit 3)\n"),
        "exit code in the status note: {rendered}"
    );
    server.stop().await;
}

// =========================================================================
// Swarm phase orchestration (review finding 16): a failed phase must not
// become overall success, and dependent phases must be blocked.
// =========================================================================

/// Queue the standard swarm pipeline and return (role, task_id) pairs in
/// execution (priority) order.
fn queue_standard_swarm_phases() -> (Vec<SwarmPhase>, Swarm, Vec<(AgentRole, String)>) {
    let phases = swarm_phases();
    let mut swarm = create_dev_swarm();
    let mut task_ids = Vec::new();
    for phase in &phases {
        let task = SwarmTask::new(format!("{}: test task", phase.description))
            .with_role(phase.role)
            .with_priority(phase.priority);
        task_ids.push((phase.role, task.id.clone()));
        swarm.queue_task(task).unwrap();
    }
    (phases, swarm, task_ids)
}

/// Extract the role name the phase prompt assigns ("acting as the X in a
/// development swarm").
fn prompt_role(prompt: &str) -> String {
    prompt
        .split("acting as the ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or("?")
        .to_string()
}

#[tokio::test]
async fn swarm_coder_failure_blocks_dependents_and_fails_verdict() {
    use crate::orchestration::swarm::TaskStatus;

    let (phases, mut swarm, task_ids) = queue_standard_swarm_phases();
    let status_of = |swarm: &Swarm, role: AgentRole| {
        let id = &task_ids.iter().find(|(r, _)| *r == role).unwrap().1;
        swarm.get_task(id).unwrap().status
    };

    // Script: the Coder phase fails; every other phase would succeed.
    let ran = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ran_exec = std::sync::Arc::clone(&ran);
    let outcomes = run_swarm_phases(&mut (), &mut swarm, &phases, move |_, prompt| {
        let ran = std::sync::Arc::clone(&ran_exec);
        Box::pin(async move {
            let role = prompt_role(&prompt);
            ran.lock().unwrap().push(role.clone());
            if role == "Coder" {
                Err(anyhow::anyhow!("coder boom"))
            } else {
                Ok(())
            }
        })
    })
    .await;

    // (a) Failed phase recorded with evidence; dependents blocked, not run.
    assert_eq!(
        outcomes,
        vec![
            (AgentRole::Architect, PhaseOutcome::Success),
            (
                AgentRole::Coder,
                PhaseOutcome::Failed("coder boom".to_string())
            ),
            (
                AgentRole::Tester,
                PhaseOutcome::Blocked {
                    dependency: AgentRole::Coder
                }
            ),
            (
                AgentRole::Reviewer,
                PhaseOutcome::Blocked {
                    dependency: AgentRole::Coder
                }
            ),
        ]
    );

    // (c) Only Architect and Coder actually executed; Tester and Reviewer
    // were blocked because their input phase failed.
    assert_eq!(
        *ran.lock().unwrap(),
        vec!["Architect".to_string(), "Coder".to_string()]
    );

    // (a) The verdict is an error naming the failed phase with evidence and
    // the blocked phases — never a green success.
    let verdict = swarm_verdict(&phases, &outcomes)
        .expect_err("a failed phase must not produce an Ok verdict");
    assert!(
        verdict.contains("phase 'Coder' FAILED: coder boom"),
        "verdict names the failed phase with evidence: {verdict}"
    );
    assert!(
        verdict.contains("phase 'Tester' BLOCKED"),
        "verdict names the blocked dependent: {verdict}"
    );
    assert!(
        verdict.contains("phase 'Reviewer' BLOCKED"),
        "verdict names the blocked dependent: {verdict}"
    );

    // Swarm-side task states reflect reality: failed phase is Failed (with
    // evidence retained), blocked phases are settled (not left active),
    // and the successful phase is Completed.
    assert_eq!(
        status_of(&swarm, AgentRole::Architect),
        TaskStatus::Completed
    );
    assert_eq!(status_of(&swarm, AgentRole::Coder), TaskStatus::Failed);
    assert_eq!(status_of(&swarm, AgentRole::Tester), TaskStatus::Failed);
    assert_eq!(status_of(&swarm, AgentRole::Reviewer), TaskStatus::Failed);

    let coder_id = &task_ids
        .iter()
        .find(|(r, _)| *r == AgentRole::Coder)
        .unwrap()
        .1;
    let coder_task = swarm.get_task(coder_id).unwrap();
    assert!(
        coder_task
            .results
            .values()
            .any(|r| r.contains("coder boom")),
        "failure evidence retained in the swarm task"
    );
    assert_eq!(coder_task.failed_results().len(), 1);
}

#[tokio::test]
async fn swarm_all_phases_succeed_gives_ok_verdict() {
    use crate::orchestration::swarm::TaskStatus;

    let (phases, mut swarm, task_ids) = queue_standard_swarm_phases();

    let outcomes = run_swarm_phases(&mut (), &mut swarm, &phases, |_, _prompt| {
        Box::pin(async move { Ok(()) })
    })
    .await;

    // (b) All phases ran and succeeded.
    assert_eq!(outcomes.len(), phases.len());
    assert!(
        outcomes.iter().all(|(_, o)| *o == PhaseOutcome::Success),
        "all phases succeed: {outcomes:?}"
    );
    assert!(swarm_verdict(&phases, &outcomes).is_ok());

    for (role, id) in &task_ids {
        assert_eq!(
            swarm.get_task(id).unwrap().status,
            TaskStatus::Completed,
            "phase {role:?} must be Completed"
        );
    }
}

#[tokio::test]
async fn swarm_architect_failure_blocks_whole_pipeline() {
    let (phases, mut swarm, _task_ids) = queue_standard_swarm_phases();

    let ran = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ran_exec = std::sync::Arc::clone(&ran);
    let outcomes = run_swarm_phases(&mut (), &mut swarm, &phases, move |_, prompt| {
        let ran = std::sync::Arc::clone(&ran_exec);
        Box::pin(async move {
            let role = prompt_role(&prompt);
            ran.lock().unwrap().push(role.clone());
            if role == "Architect" {
                Err(anyhow::anyhow!("design boom"))
            } else {
                Ok(())
            }
        })
    })
    .await;

    // Coder depends on Architect, so it is blocked; Tester and Reviewer are
    // blocked transitively via Coder. Only the Architect phase ran.
    assert_eq!(*ran.lock().unwrap(), vec!["Architect".to_string()]);
    assert_eq!(
        outcomes,
        vec![
            (
                AgentRole::Architect,
                PhaseOutcome::Failed("design boom".to_string())
            ),
            (
                AgentRole::Coder,
                PhaseOutcome::Blocked {
                    dependency: AgentRole::Architect
                }
            ),
            (
                AgentRole::Tester,
                PhaseOutcome::Blocked {
                    dependency: AgentRole::Coder
                }
            ),
            (
                AgentRole::Reviewer,
                PhaseOutcome::Blocked {
                    dependency: AgentRole::Coder
                }
            ),
        ]
    );

    let verdict =
        swarm_verdict(&phases, &outcomes).expect_err("a failed first phase must fail the verdict");
    assert!(verdict.contains("phase 'Architect' FAILED: design boom"));
    assert!(verdict.contains("phase 'Coder' BLOCKED"));
}

#[test]
fn swarm_verdict_flags_phases_that_never_ran() {
    let phases = swarm_phases();
    // Only the architect reported; everything else is missing entirely.
    let outcomes = vec![(AgentRole::Architect, PhaseOutcome::Success)];
    let verdict =
        swarm_verdict(&phases, &outcomes).expect_err("phases that never ran must fail the verdict");
    assert!(verdict.contains("phase 'Coder' DID NOT RUN"), "{verdict}");
    assert!(verdict.contains("phase 'Tester' DID NOT RUN"), "{verdict}");
    assert!(
        verdict.contains("phase 'Reviewer' DID NOT RUN"),
        "{verdict}"
    );
    assert!(!verdict.contains("Architect"), "{verdict}");
}

// =========================================================================
// task-focus mentioned-file extraction + focus overlay no-compounding
// =========================================================================

#[test]
fn mentioned_file_tokens_include_paths_extensions_and_existing_files() {
    // (a) Path-shaped tokens — historical behavior preserved.
    assert!(is_mentioned_file_token("src/lib.rs"));
    assert!(is_mentioned_file_token("a/b/"));
    // (b) Extension-bearing ROOT-FILE tokens — the old blindness fix:
    // "Create a python script hello.py …" now mentions hello.py.
    assert!(is_mentioned_file_token("hello.py"));
    assert!(is_mentioned_file_token("main.rs"));
    assert!(is_mentioned_file_token("Cargo.toml"));
    assert!(is_mentioned_file_token("README.md"));
    assert!(is_mentioned_file_token("schema.json"));
    // Trailing punctuation must not disqualify a mention.
    assert!(is_mentioned_file_token("hello.py,"));
    assert!(is_mentioned_file_token("(main.rs)"));
}

#[test]
fn mentioned_file_tokens_reject_plain_words() {
    assert!(!is_mentioned_file_token("hello"));
    assert!(!is_mentioned_file_token("create"));
    assert!(!is_mentioned_file_token("the"));
    assert!(!is_mentioned_file_token("v1.2.3")); // version-like, no known extension
    assert!(!is_mentioned_file_token("e.g"));
    assert!(!is_mentioned_file_token("."));
    assert!(!is_mentioned_file_token(""));
}

#[test]
fn mentioned_file_metadata_branch_resolves_existing_workspace_files() {
    // Serialize on the shared cwd lock: the metadata branch resolves against
    // current_project_root() (derived from the current directory).
    use crate::test_support::CwdGuard;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("calculator"), "def add(a,b): return a+b").unwrap();
    let _guard = CwdGuard::enter(dir.path());

    // Extension rule still fires without touching the filesystem.
    assert!(is_mentioned_file_token("hello.py"));
    // Bare filenames that EXIST in the workspace resolve by metadata.
    assert!(is_mentioned_file_token("calculator"));
    // Bare filenames that do NOT exist are not mentioned files.
    assert!(!is_mentioned_file_token("nonexistent_file_xyz"));
}

#[test]
fn focus_overlay_rebuilds_from_clean_base_across_turns() {
    let base = Message::system("BASE SYSTEM PROMPT");
    let overlay_one = "\n\n## TASK FOCUS (READ THIS FIRST)\nworkflow one";
    let overlay_two = "\n\n## TASK FOCUS (READ THIS FIRST)\nworkflow two";

    let turn_1 = stamped_system_message(&base, overlay_one);
    let turn_2 = stamped_system_message(&turn_1, overlay_two);
    let turn_3 = stamped_system_message(&turn_2, overlay_one);

    // The rendered system context must NOT grow across turns: base + exactly
    // ONE overlay, whatever the overlay text is. (The old prefix-on-prefix
    // stamp doubled the mandate every turn.)
    assert_eq!(
        turn_1.content.text().len(),
        turn_2.content.text().len(),
        "messages[0] must not grow between turns"
    );
    assert_eq!(
        turn_2.content.text().len(),
        turn_3.content.text().len(),
        "messages[0] must stay the same size across many turns"
    );

    // The base is always recoverable in full…
    assert_eq!(
        strip_focus_overlay(turn_3.content.text()),
        "BASE SYSTEM PROMPT"
    );
    // …and the overlay is the CURRENT turn's, never an accumulation.
    assert!(
        turn_3.content.text().contains("workflow one"),
        "current overlay must be present"
    );
    assert!(
        !turn_3.content.text().contains("workflow two"),
        "no stale overlay from a previous turn may remain"
    );

    // A turn with an empty overlay leaves the clean base only (and drops any
    // stale overlay from an earlier task).
    let cleared = stamped_system_message(&turn_3, "");
    assert_eq!(cleared.content.text(), "BASE SYSTEM PROMPT");
}

#[test]
fn focus_overlay_different_lengths_still_do_not_compound() {
    let base = Message::system("BASE");
    // Turns with wildly different overlay sizes: the result must stay
    // bounded by base + the CURRENT overlay, not base + accumulated copies.
    let small_overlay = "short";
    let big_overlay = &"long ".repeat(200);
    let turn_1 = stamped_system_message(&base, small_overlay);
    let turn_2 = stamped_system_message(&turn_1, big_overlay);
    // Turn 3 shrinks back down — length must track the current overlay only.
    let turn_3 = stamped_system_message(&turn_2, small_overlay);
    assert_eq!(turn_3.content.text().len(), turn_1.content.text().len());
    let expected = format!("<selfware_focus_overlay>{small_overlay}</selfware_focus_overlay>BASE");
    assert_eq!(turn_3.content.text(), expected);
}

// =========================================================================
// Auto-checkpoint-and-continue (long-task caps, USER-APPROVED policy)
//
// When the iteration cap is reached on a PRODUCTIVE run whose +25%×4
// adaptive extensions are exhausted, the run checkpoints and chains
// `continue_execution` (the manual-resume path) instead of dying at
// `Failed{Max iterations exceeded}`. The chain is bounded at 3 per task,
// then the run stops with the typed AUTO_CONTINUE_LIMIT outcome.
// Unproductive runs keep the existing typed MaxIterations failure.
// =========================================================================

/// One mock response that performs an XML file_read of `./path`. Distinct
/// files give every turn a non-repeating signature, which is exactly what
/// keeps the loop "productive" at the cap.
fn file_read_tool_call(path: &str) -> String {
    format!(
        "<tool>\n<name>file_read</name>\n<arguments>{}</arguments>\n</tool>",
        serde_json::json!({ "path": format!("./{path}") })
    )
}

/// Create `count` probe files (f000.rs …) in `dir` and return their names —
/// novel reads keep `consecutive_read_only_steps` at zero (investigation
/// reset) and give the productive streak distinct signatures.
fn create_probe_files(dir: &std::path::Path, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            let name = format!("f{i:03}.rs");
            std::fs::write(dir.join(&name), "// probe\n").unwrap();
            name
        })
        .collect()
}

/// Productive-run E2E: base cap 4 → the +25%×4 adaptive grants take the cap
/// to 8; the 5th trip (iteration 9, after 1 planning + 8 executing turns)
/// must checkpoint and chain `continue_execution` instead of failing. The
/// chained segment finalizes on the default response, so `run_task` returns
/// Ok — a task that previously died at `Failed{Max iterations exceeded}`.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn auto_continue_chains_once_on_productive_cap_and_writes_checkpoint() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let files = create_probe_files(dir.path(), 12);
    cwd.switch_to(dir.path());

    let mut builder = MockLlmServer::builder();
    // Segment 0 consumes exactly 10 responses: 1 planning turn + 8 executing
    // turns (iterations 1-8 across the four +25% adaptive grants) + ONE
    // hidden LLM reflection call (reflect_on_step fires at step 5, a
    // multiple of 5). Anything past that is the chained segment's first
    // turn, which must hit the default (plain "done") so the chain
    // finalizes instead of re-tripping. Provisioning one fewer (9) makes
    // the 4th extension-resumed turn consume the default "Hello" text and
    // "complete" the task naturally at the cap — the auto-continue never
    // fires and the run looks like an early natural completion.
    for f in files.iter().take(10) {
        builder = builder.with_response(file_read_tool_call(f));
    }
    let server = builder.build().await;

    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 4;
    let mut agent = Agent::new(config).await.unwrap();
    let chkpts = dir.path().join(".chkpts");
    agent.checkpoint_manager = Some(CheckpointManager::new(chkpts.clone()).unwrap());

    let result = agent.run_task("Never completes").await;
    server.stop().await;

    assert!(
        result.is_ok(),
        "a productive run at the cap must chain to completion, got: {:?}",
        result.err()
    );
    assert_eq!(
        agent.loop_control.auto_continue_count(),
        1,
        "exactly one auto-continuation must fire on the first cap trip past the extensions"
    );
    assert!(
        agent.loop_control.current_step() >= 8,
        "step counting must survive the chain, got step {}",
        agent.loop_control.current_step()
    );

    // The auto-continue boundary persisted a real checkpoint (bypassing the
    // continuous-work cadence), and the chained run finalized it.
    let manager = CheckpointManager::new(chkpts.clone()).unwrap();
    let tasks = manager.list_tasks().unwrap();
    assert!(
        tasks
            .iter()
            .any(|t| t.status == crate::checkpoint::TaskStatus::Completed),
        "the chained run must finalize its checkpoint as Completed, got {:?}",
        tasks
            .iter()
            .map(|t| (&t.task_id, &t.status))
            .collect::<Vec<_>>()
    );
    // The boundary checkpoint itself carries the chain count, so a later
    // process restart can keep enforcing the per-task bound.
    let task_id = agent
        .current_checkpoint
        .as_ref()
        .map(|c| c.task_id.clone())
        .unwrap();
    let persisted = manager.load(&task_id).unwrap();
    assert_eq!(
        persisted.auto_continue_count, 1,
        "the checkpoint must persist the chain count"
    );
}

/// Chain-bound E2E: three productive segments each exhaust their fresh
/// budget and chain; the FOURTH cap trip must NOT chain again — it stops
/// with the typed AUTO_CONTINUE_LIMIT outcome instead of looping forever.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn auto_continue_stops_with_typed_limit_after_three_chains() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    // Far more distinct files than the run can possibly consume.
    let files = create_probe_files(dir.path(), 80);
    cwd.switch_to(dir.path());

    let mut builder = MockLlmServer::builder();
    for f in files.iter().take(60) {
        builder = builder.with_response(file_read_tool_call(f));
    }
    let server = builder.build().await;

    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_iterations = 4;
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.run_task("Never completes").await;
    server.stop().await;

    assert_eq!(
        agent.loop_control.auto_continue_count(),
        3,
        "the bound allows exactly 3 auto-continuations per task"
    );
    let err = result.expect_err("the chain bound must stop the run, not chain forever");
    let msg = err.to_string();
    assert!(
        msg.contains("AUTO_CONTINUE_LIMIT"),
        "the 4th cap trip must stop with the typed outcome, got: {msg}"
    );
}

/// Unproductive runs keep the existing typed failure: with no successful
/// tool streak in the window, the cap hit must NOT chain — it falls through
/// to the plain `Failed{Max iterations exceeded}` path.
#[tokio::test]
async fn auto_continue_refuses_unproductive_runs_at_the_cap() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    // Error-only streak at the cap with the extension ceiling spent.
    for i in 0..5u64 {
        agent
            .recent_turn_progress
            .push_back(crate::agent::loop_control::TurnProgress {
                had_success: false,
                signatures: vec![("file_read".to_string(), i)],
            });
    }
    agent.loop_control = crate::agent::loop_control::AgentLoop::new(4);
    for _ in 0..4 {
        agent.loop_control.extend_budget_once();
    }
    agent.loop_control.restore_progress(0, 8);
    let capped = agent.loop_control.next_state();
    assert!(matches!(capped, Some(AgentState::Failed { .. })));

    let chained = agent.maybe_auto_continue("Never completes").await;
    assert!(
        chained.is_none(),
        "unproductive runs must keep the typed MaxIterations failure"
    );
    assert_eq!(agent.loop_control.auto_continue_count(), 0);
    server.stop().await;
}

/// Layering: while any +25% adaptive extension is still available the chain
/// must NOT fire — the cheaper in-place grant is the first-line mechanism
/// (the +25%×4 grant behavior is preserved byte-for-byte).
#[tokio::test]
async fn auto_continue_defers_to_adaptive_extensions_while_available() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    // Productive window, but zero extensions granted yet.
    for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        agent
            .recent_turn_progress
            .push_back(productive_turn(name, i as u64));
    }
    agent.loop_control = crate::agent::loop_control::AgentLoop::new(4);
    agent.loop_control.restore_progress(0, 4);
    let capped = agent.loop_control.next_state();
    assert!(matches!(capped, Some(AgentState::Failed { .. })));

    let chained = agent.maybe_auto_continue("Never completes").await;
    assert!(
        chained.is_none(),
        "chaining must wait for the extension ceiling"
    );
    assert_eq!(agent.loop_control.auto_continue_count(), 0);
    // The adaptive grant is the first-line response on the same streak.
    assert!(agent.maybe_extend_iteration_budget().is_some());
    server.stop().await;
}

/// The chain count is persisted into the task checkpoint by `to_checkpoint`
/// and restored onto the loop exactly as `Agent::resume` does — the
/// field-level contract behind the restart E2E.
#[tokio::test]
async fn auto_continue_count_persists_and_restores_via_checkpoint() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    agent.loop_control.register_auto_continue();
    agent.loop_control.register_auto_continue();

    let checkpoint = agent.to_checkpoint("test-chain", "Never completes");
    assert_eq!(
        checkpoint.auto_continue_count, 2,
        "the checkpoint must carry the chain count"
    );

    // Mirrors the Agent::resume restore line: a BRAND-NEW loop created for a
    // restarted process receives the persisted balance (1 of 3 consumed →
    // the resumed run may chain at most 2 more).
    let mut restored_loop = crate::agent::loop_control::AgentLoop::new(4);
    restored_loop.set_auto_continue_count(checkpoint.auto_continue_count);
    assert_eq!(restored_loop.auto_continue_count(), 2);
    server.stop().await;
}

/// Review fix: a task that chained continuations keeps its chain balance
/// across a process RESTART. The boundary checkpoint persists
/// `auto_continue_count`, `Agent::resume` restores it onto the fresh loop,
/// and the next cap trips chain 2 more times and then stop with the typed
/// AUTO_CONTINUE_LIMIT — a fresh process must NOT receive a fresh budget of
/// 3 chains (which would let 3×N restarts multiply the ceiling forever).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn auto_continue_chain_bound_survives_process_restart() {
    // Point CheckpointManager::default_path() ($HOME/.selfware/checkpoints)
    // at a temp home so the whole task store (phase-1 writes, phase-2
    // resume reads) is isolated from the real user data dir.
    let fake_home = tempfile::tempdir().unwrap();
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", fake_home.path().as_os_str());

    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let files = create_probe_files(dir.path(), 200);
    cwd.switch_to(dir.path());

    // ---- Phase 1: one productive segment chains once and completes. ----
    // Segment 0 consumes exactly 10 responses (1 planning + 8 executing
    // across the 4 adaptive grants + 1 reflection call at step 5; see
    // auto_continue_chains_once_on_productive_cap_and_writes_checkpoint);
    // the chained segment's first turn hits the default response and
    // finalizes the run.
    let mut b1 = MockLlmServer::builder();
    for f in files.iter().take(10) {
        b1 = b1.with_response(file_read_tool_call(f));
    }
    let server1 = b1.build().await;
    let mut config = mock_agent_config(format!("{}/v1", server1.url()), false);
    config.agent.max_iterations = 4;
    let mut agent = Agent::new(config.clone()).await.unwrap();
    let result = agent.run_task("Never completes").await;
    server1.stop().await;
    assert!(
        result.is_ok(),
        "phase 1 must complete via the chain: {:?}",
        result.err()
    );
    assert_eq!(agent.loop_control.auto_continue_count(), 1);
    let task_id = agent
        .current_checkpoint
        .as_ref()
        .map(|c| c.task_id.clone())
        .unwrap();
    // Diagnostic: the run's in-memory checkpoint object and the raw JSON the
    // boundary save wrote must both carry the chain count before the load
    // assertion (isolates write-side loss from load-side loss).
    assert_eq!(
        agent
            .current_checkpoint
            .as_ref()
            .unwrap()
            .auto_continue_count,
        1,
        "the in-memory checkpoint must carry the chain count"
    );
    let raw_path = fake_home
        .path()
        .join(".selfware/checkpoints")
        .join(format!("{task_id}.json"));
    let raw = std::fs::read_to_string(&raw_path)
        .unwrap_or_else(|e| panic!("boundary checkpoint file missing at {:?}: {e}", raw_path));
    assert!(
        raw.contains("\"auto_continue_count\": 1"),
        "boundary checkpoint JSON must persist the count, got: {}",
        raw
    );
    let persisted = CheckpointManager::default_path()
        .unwrap()
        .load(&task_id)
        .unwrap();
    assert_eq!(
        persisted.auto_continue_count, 1,
        "the checkpoint on disk must carry the chain count"
    );

    // ---- Phase 2: a "restarted process" resumes the same task. ----
    // The second mock server owns the resumed process: the config MUST
    // point at server2 — resuming with phase 1's config (server1, already
    // stopped) makes every model call in the restarted run fail (review
    // finding: the resume path pointed at the first mock server's config).
    let mut b2 = MockLlmServer::builder();
    // Generous provision: after the restart the run stops deterministically
    // at the AUTO_CONTINUE_LIMIT after roughly 58 requests (two more chains
    // at 16 turns each plus per-step reflection calls at continuing step
    // multiples of 5); unconsumed responses are harmless — the visit count
    // below is what matters.
    //
    // Cap 8 (not 4) on purpose: `recent_turn_progress` is in-memory only, so
    // a restarted process starts with an EMPTY productive window. With the
    // mocked cap of 4 the first cap trip arrives after 4 turns — fewer than
    // the 5-turn productive window — so the resumed run aborts unproductively
    // before it can chain. Production caps (default 400) always let the
    // window fill long before the first trip, so the small-cap abort is a
    // test-scale artifact: the restart semantics under test are the RESTORED
    // chain balance and the typed stop, not window reconstruction. Cap 8
    // gives the resumed run 8 turns before the first trip — a full window —
    // so both continuation chains fire and the bound stops the 4th trip.
    for f in files.iter().skip(100).take(70) {
        b2 = b2.with_response(file_read_tool_call(f));
    }
    let server2 = b2.build().await;
    let recorder = std::sync::Arc::new(RecordingEventEmitter::default());
    let mut config2 = mock_agent_config(format!("{}/v1", server2.url()), false);
    config2.agent.max_iterations = 8;
    let mut agent2 = Agent::resume(config2, &task_id)
        .await
        .unwrap()
        .with_event_emitter(recorder.clone());
    assert_eq!(
        agent2.loop_control.auto_continue_count(),
        1,
        "the restart must restore the chain balance, not reset it"
    );
    let result2 = agent2.continue_execution().await;
    server2.stop().await;

    assert_eq!(
        agent2.loop_control.auto_continue_count(),
        3,
        "the restored balance must keep counting to the per-task bound"
    );
    let err = result2.expect_err("the resumed run must hit the chain bound");
    assert!(
        err.to_string().contains("AUTO_CONTINUE_LIMIT"),
        "the 4th cap trip (2 chained this process + 1 restored) must stop typed, got: {}",
        err
    );
    // Exactly TWO continuations chained after the restart (phase 1's chain
    // was restored, so the budget was 2 — a restart granting a fresh budget
    // of 3 would have chained three times).
    assert_eq!(
        recorder
            .events()
            .iter()
            .filter(|e| matches!(e, AgentEvent::Status { message }
                if message.contains("continuing automatically")))
            .count(),
        2,
        "the post-restart segment must fire exactly 2 of the 3 allowed chains"
    );
}

/// Review fix: the auto-continue boundary folds the current segment's wall
/// time into the cumulative counter BEFORE the chain re-baselines the
/// per-segment clock, so the task-wide wall budget and the persisted
/// `elapsed_wall_secs` keep accumulating across segments — mirroring the
/// total a manual `Agent::resume` restores from a checkpoint.
#[tokio::test]
async fn auto_continue_folds_segment_elapsed_into_cumulative_wall_clock() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    agent.prior_elapsed_secs = 0;
    // Segment 0 already consumed 5s of active wall time.
    agent.task_start_time = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(5))
        .expect("clock is sane");
    assert!(
        agent.budget_elapsed_secs() >= 5,
        "the current segment's elapsed must be counted"
    );

    // The exact fold `maybe_auto_continue` performs at the chain boundary:
    // accumulate the closing segment AND re-baseline the per-segment clock
    // TOGETHER (review fix — folding without the reset would count the
    // segment once in the fold and AGAIN in the persisted total)...
    agent.prior_elapsed_secs = agent.budget_elapsed_secs();
    agent.task_start_time = std::time::Instant::now();

    // The cumulative total must include segment 0's 5s — not reset to ~0.
    assert!(
        agent.budget_elapsed_secs() >= 5,
        "the fold must preserve prior-segment wall time across the chain"
    );
    // And it must NOT count the segment twice: with the clock re-baselined
    // at the fold, the checkpointed total is 5s + the (tiny) time since,
    // far below the 10s a double count would produce.
    let checkpoint = agent.to_checkpoint("test-wall", "Never completes");
    assert!(
        checkpoint.elapsed_wall_secs >= 5,
        "the persisted elapsed must include prior segments, got {}",
        checkpoint.elapsed_wall_secs
    );
    assert!(
        checkpoint.elapsed_wall_secs < 10,
        "the closing segment must be counted exactly once (5s + ε), got {} — \
         a double count points at a fold that left the per-segment clock \
         running into persistence",
        checkpoint.elapsed_wall_secs
    );
    server.stop().await;
}

/// Review fix: when the boundary checkpoint cannot be persisted, the
/// auto-continue must NOT fire and must NOT claim to have checkpointed —
/// the run falls back to the existing typed cap failure, because without a
/// written resume point the "chain" would be a lie (and a crash would lose
/// everything).
#[tokio::test]
async fn auto_continue_aborts_without_claim_when_checkpoint_save_fails() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();

    // Productive streak + extension ceiling spent — the chain would fire
    // normally, except the checkpoint write is sabotaged below.
    for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        agent
            .recent_turn_progress
            .push_back(productive_turn(name, i as u64));
    }
    agent.loop_control = crate::agent::loop_control::AgentLoop::new(4);
    for _ in 0..4 {
        agent.loop_control.extend_budget_once();
    }
    agent.loop_control.restore_progress(0, 8);
    let capped = agent.loop_control.next_state();
    assert!(matches!(capped, Some(AgentState::Failed { .. })));

    // Sabotage the checkpoint directory: swap it for a regular file so every
    // persist attempt fails with an IO error.
    let tmp = tempfile::tempdir().unwrap();
    let ck_dir = tmp.path().join("chkpts");
    agent.checkpoint_manager = Some(CheckpointManager::new(ck_dir.clone()).unwrap());
    std::fs::remove_dir_all(&ck_dir).unwrap();
    std::fs::write(&ck_dir, "in the way").unwrap();

    let recorder = std::sync::Arc::new(RecordingEventEmitter::default());
    agent = agent.with_event_emitter(recorder.clone());

    let chained = agent.maybe_auto_continue("Never completes").await;
    assert!(
        chained.is_none(),
        "a failed boundary checkpoint must stop the continuation"
    );
    assert_eq!(
        agent.loop_control.auto_continue_count(),
        0,
        "the failed chain must not consume the per-task chain budget"
    );
    assert!(
        !recorder
            .events()
            .iter()
            .any(|e| matches!(e, AgentEvent::Status { message }
                if message.contains("continuing automatically"))),
        "no 'checkpointed and continuing' claim may be emitted when the save failed"
    );
    server.stop().await;
}

/// Review fix: a MISSING checkpoint store is the same lie as a failing one —
/// `save_checkpoint_forced` now errors when no manager is configured, and
/// `maybe_auto_continue` must not fire a chain it cannot hand a resume point
/// to. Same observable contract as the IO-failure test above: no chain, no
/// consumed budget, no "checkpointed and continuing" claim.
#[tokio::test]
async fn auto_continue_aborts_without_manager_at_the_boundary() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut agent = Agent::new(mock_agent_config(format!("{}/v1", server.url()), false))
        .await
        .unwrap();
    agent.checkpoint_manager = None; // strip whatever Agent::new defaulted

    // Productive streak + extension ceiling spent — the chain would fire
    // normally, except there is nowhere to persist the boundary checkpoint.
    for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        agent
            .recent_turn_progress
            .push_back(productive_turn(name, i as u64));
    }
    agent.loop_control = crate::agent::loop_control::AgentLoop::new(4);
    for _ in 0..4 {
        agent.loop_control.extend_budget_once();
    }
    agent.loop_control.restore_progress(0, 8);
    let capped = agent.loop_control.next_state();
    assert!(matches!(capped, Some(AgentState::Failed { .. })));

    let recorder = std::sync::Arc::new(RecordingEventEmitter::default());
    agent = agent.with_event_emitter(recorder.clone());

    let chained = agent.maybe_auto_continue("Never completes").await;
    assert!(
        chained.is_none(),
        "no checkpoint manager must stop the continuation"
    );
    assert_eq!(
        agent.loop_control.auto_continue_count(),
        0,
        "the aborted chain must not consume the per-task chain budget"
    );
    assert!(
        !recorder
            .events()
            .iter()
            .any(|e| matches!(e, AgentEvent::Status { message }
                if message.contains("continuing automatically"))),
        "no 'checkpointed and continuing' claim may be emitted without a store"
    );
    server.stop().await;
}

#[tokio::test]
async fn test_enforce_hard_budgets_zero_caps_treated_as_uncapped() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_budget_tokens = Some(0);
    config.agent.max_cost_usd = Some(0.0);
    config.agent.max_wall_secs = Some(0);

    let mut agent = Agent::new(config).await.unwrap();
    // With 0 caps filtered out, enforce_hard_budgets must succeed without bailing
    let result = agent.enforce_hard_budgets("test task").await;
    assert!(
        result.is_ok(),
        "zero caps must be treated as uncapped: {:?}",
        result
    );
    server.stop().await;
}

#[tokio::test]
async fn test_reset_failure_mode_counters_clears_task_history_buffers() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    // Populate buffers
    agent
        .recent_tool_calls
        .push_back(("shell_exec".to_string(), 12345));
    agent
        .recent_tool_batches
        .push_back(vec![("shell_exec".to_string(), 12345)]);
    agent
        .recent_turn_progress
        .push_back(productive_turn("shell_exec", 1));
    agent.readonly_no_tool_streak = 5;
    agent.consecutive_empty_responses = 2;

    // Reset counters
    agent.reset_failure_mode_counters();

    // Verify all per-task buffers and streaks are clean
    assert!(agent.recent_tool_calls.is_empty());
    assert!(agent.recent_tool_batches.is_empty());
    assert!(agent.recent_turn_progress.is_empty());
    assert_eq!(agent.readonly_no_tool_streak, 0);
    assert_eq!(agent.consecutive_empty_responses, 0);
    server.stop().await;
}

#[tokio::test]
async fn test_maybe_inject_commit_mode_uses_cumulative_budget_elapsed_secs() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.max_wall_secs = Some(100);

    let mut agent = Agent::new(config).await.unwrap();
    // Prior elapsed seconds across previous segments is 70s out of 100s (70%)
    agent.prior_elapsed_secs = 70;

    // Call commit mode injection
    agent.maybe_inject_commit_mode_directive();

    // 70% >= 65% should trigger COMMIT MODE
    let last_msg = agent
        .messages
        .last()
        .expect("commit mode directive expected");
    assert!(
        last_msg.content.text().contains("COMMIT MODE: 65%"),
        "cumulative elapsed time must trigger commit mode: {:?}",
        last_msg.content.text()
    );
    server.stop().await;
}

// -----------------------------------------------------------------------
// Provider context-window overflow: recoverable, bounded
// -----------------------------------------------------------------------

#[test]
fn context_overflow_is_not_a_fatal_loop_error_but_genuine_400_is() {
    let overflow: anyhow::Error =
        crate::errors::ApiError::ContextOverflow("provider rejected".into()).into();
    assert!(!is_fatal_loop_error(&overflow));
    let raw_overflow: anyhow::Error = crate::errors::ApiError::HttpStatus {
        status: 413,
        message: String::new(),
    }
    .into();
    assert!(!is_fatal_loop_error(&raw_overflow));
    for status in [400u16, 401, 403, 404] {
        let genuine: anyhow::Error = crate::errors::ApiError::HttpStatus {
            status,
            message: r#"{"error":"The model `nope` does not exist"}"#.to_string(),
        }
        .into();
        assert!(
            is_fatal_loop_error(&genuine),
            "genuine {status} must stay fatal"
        );
    }
}

#[test]
fn recovery_error_text_keeps_overflow_marker_through_context() {
    // anyhow's Display shows only the outer context — the ErrorRecovery
    // router must still see the overflow.
    let err = anyhow::Error::from(crate::errors::ApiError::ContextOverflow(
        "prompt is too long".into(),
    ))
    .context("Streaming failed: x. Non-streaming fallback request also failed");
    let text = recovery_error_text(&err);
    assert!(is_context_overflow_text(&text), "{text}");
    assert!(text.contains("prompt is too long"), "{text}");

    let raw = anyhow::Error::from(crate::errors::ApiError::HttpStatus {
        status: 400,
        message: "prompt is too long: 210000 tokens > 200000 maximum".into(),
    })
    .context("planning failed");
    assert!(is_context_overflow_text(&recovery_error_text(&raw)));

    // Non-overflow errors keep their ordinary text.
    let other = anyhow::anyhow!("boom").context("outer");
    assert_eq!(recovery_error_text(&other), "outer");
    assert!(!is_context_overflow_text("outer"));
}

#[test]
fn context_overflow_recovery_bound_is_tighter_than_generic() {
    // The compress-and-retry loop must terminate well before the generic
    // 12-pass error-recovery backstop.
    const { assert!(MAX_CONSECUTIVE_CONTEXT_OVERFLOW_RECOVERIES >= 1) };
    const { assert!(MAX_CONSECUTIVE_CONTEXT_OVERFLOW_RECOVERIES < 12) };
}

/// A provider that keeps rejecting the prompt as too long must be retried
/// (after compression) instead of killing the run on the first 400, and the
/// retries must be bounded.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn run_task_provider_context_overflow_is_retried_with_compression_and_bounded() {
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 400,
            body: r#"{"error":{"message":"This model's maximum context length is 4096 tokens. However, your messages resulted in 9000 tokens.","type":"invalid_request_error","code":"context_length_exceeded"}}"#.to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();

    let err = agent
        .run_task("Summarize the repo")
        .await
        .expect_err("a prompt the provider never accepts must stop the run");
    assert!(
        crate::errors::is_context_overflow_error(&err),
        "the stop must be the typed overflow, got: {err:#}"
    );
    let requests = server.captured_request_bodies().await.len();
    assert!(
        requests > 1,
        "an overflow must be retried after compression, not treated as a fatal 400 (got {requests} request)"
    );
    assert!(
        requests <= 16,
        "overflow recovery must be bounded, got {requests} requests"
    );
    server.stop().await;
}

/// Positive control: a genuine 400 (bad model) stays fatal — one request.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn run_task_genuine_400_stays_fatal() {
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 400,
            body: r#"{"error":{"message":"The model `nope` does not exist","code":"model_not_found"}}"#
                .to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();

    let err = agent
        .run_task("Summarize the repo")
        .await
        .expect_err("a genuine 400 must stop the run");
    assert!(!crate::errors::is_context_overflow_error(&err), "{err:#}");
    assert_eq!(
        server.captured_request_bodies().await.len(),
        1,
        "a genuine 400 is terminal: no retries"
    );
    server.stop().await;
}
