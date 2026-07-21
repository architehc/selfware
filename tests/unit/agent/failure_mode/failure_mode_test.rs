use super::*;
use crate::config::Config;

fn test_config() -> Config {
    Config {
        endpoint: "http://localhost:0/v1".to_string(),
        model: "mock-model".to_string(),
        context_length: 500_000,
        max_tokens: 8192,
        agent: crate::config::AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            streaming: false,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: crate::config::SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: crate::config::ExecutionMode::Yolo,
        ..Default::default()
    }
}

async fn make_agent() -> Agent {
    Agent::new(test_config()).await.expect("agent::new")
}

#[tokio::test]
async fn classify_success_when_mutating_calls_exist() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(3);
    agent.test_set_total_tool_calls(12);
    agent.test_set_last_assistant_response("Done.".to_string());

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(mode.kind, FailureKind::Success);
    assert!(
        mode.evidence.contains("3 mutating tool calls"),
        "evidence: {}",
        mode.evidence
    );
}

#[tokio::test]
async fn classify_fake_complete_on_natural_with_final_answer_no_mutation() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(0);
    agent.test_set_total_tool_calls(4);
    agent.test_set_last_assistant_response("Final answer: implementation complete.".to_string());

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(mode.kind, FailureKind::FakeComplete);
    assert!(mode.evidence.contains("0 mutating"));
}

#[tokio::test]
async fn classify_nonterm_prose_when_consecutive_no_action_high() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(0);
    agent.test_set_consecutive_no_action(30);
    let big = "x".repeat(47_000);
    agent.test_set_last_assistant_response(big);

    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "Max iterations exceeded".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::NontermProse);
    assert!(
        mode.evidence.contains("30 consecutive"),
        "evidence: {}",
        mode.evidence
    );
    assert!(
        mode.evidence.contains("KB of text"),
        "evidence: {}",
        mode.evidence
    );
}

#[tokio::test]
async fn classify_read_loop_when_progress_guard_fired_no_mutations() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(0);
    agent.test_set_progress_guard_fires(2);
    agent.test_set_total_tool_calls(15);

    let mode = FailureMode::classify(&agent, RunOutcome::Partial);
    assert_eq!(mode.kind, FailureKind::ReadLoop);
    assert!(mode.evidence.contains("progress guard fired 2"));
}

#[tokio::test]
async fn classify_retry_loop_on_permanently_blocked_tool_calls() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(1);
    agent.test_set_permanently_blocked(3);
    agent.test_set_total_tool_calls(20);

    let mode = FailureMode::classify(&agent, RunOutcome::Partial);
    assert_eq!(mode.kind, FailureKind::RetryLoop);
    assert!(mode.evidence.contains("3 tool call"));
}

#[tokio::test]
async fn classify_prefill_breaker_when_circuit_open() {
    let mut agent = make_agent().await;
    agent.test_set_prefill_400s(5);
    agent.test_set_prefill_breaker_open(true);

    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "prefill incompatible".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::PrefillBreaker);
    assert!(mode.evidence.contains("5 prefill-incompatible"));
}

#[tokio::test]
async fn classify_timeout_when_reason_contains_timeout() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(2);

    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "wall-clock timeout".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::Timeout);
}

#[tokio::test]
async fn classify_selfware_error_on_panic_reason() {
    let agent = make_agent().await;
    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "internal: panicked at src/foo.rs:42".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::SelfwareError);
}

#[tokio::test]
async fn classify_max_iterations_when_no_clear_signal() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(2);
    agent.test_set_total_tool_calls(20);

    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "Max iterations exceeded".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::MaxIterations);
    assert!(mode.evidence.contains("max_iterations"));
}

#[tokio::test]
async fn write_artifact_emits_failure_mode_json() {
    let mode = FailureMode {
        kind: FailureKind::ReadLoop,
        evidence: "ev".to_string(),
        advice: "ad".to_string(),
    };
    let dir = tempfile::tempdir().unwrap();
    mode.write_artifact(dir.path()).await.unwrap();
    let path = dir.path().join("failure_mode.json");
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("ReadLoop"));
    assert!(contents.contains("\"evidence\""));
}

#[test]
fn cli_banner_uses_tag_and_evidence() {
    let mode = FailureMode {
        kind: FailureKind::NontermProse,
        evidence: "30 consecutive prose-only turns".to_string(),
        advice: "try a smaller context budget".to_string(),
    };
    let banner = mode.cli_banner();
    assert!(banner.contains("NONTERM_PROSE"));
    assert!(banner.contains("30 consecutive"));
    assert!(banner.contains("smaller context"));

    let success = FailureMode {
        kind: FailureKind::Success,
        evidence: "all good".to_string(),
        advice: "-".to_string(),
    };
    assert!(success.cli_banner().contains("REAL_EDIT"));
    assert!(success.cli_banner().contains("✅"));
}

#[test]
fn failure_kind_serializes_to_json() {
    let mode = FailureMode {
        kind: FailureKind::PrefillBreaker,
        evidence: "x".to_string(),
        advice: "y".to_string(),
    };
    let json = serde_json::to_string(&mode).unwrap();
    assert!(json.contains("PrefillBreaker"));
    assert!(json.contains("\"kind\""));
}

#[tokio::test]
async fn classify_no_change_when_natural_completion_zero_mutations() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(0);
    agent.test_set_total_tool_calls(2);
    agent.test_set_last_assistant_response("Here is the explanation you asked for.".to_string());
    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(mode.kind, FailureKind::NoChange);
    let banner = mode.cli_banner();
    assert!(!banner.contains("REAL_EDIT"), "banner: {banner}");
    assert!(!banner.contains("❌"), "banner: {banner}");
    assert!(banner.contains("NO_CHANGES"), "banner: {banner}");
}

#[tokio::test]
async fn classify_budget_exhausted_distinct_from_timeout() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(1);
    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "token budget exhausted".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::BudgetExhausted);
    assert!(
        mode.advice.contains("max-budget-tokens"),
        "advice: {}",
        mode.advice
    );
}

/// A correctly safety-blocked task burns its whole budget and must NOT be
/// mislabeled TIMEOUT — no budget increase fixes a safety refusal.
#[tokio::test]
async fn classify_blocked_by_safety_instead_of_timeout() {
    let mut agent = make_agent().await;
    // Distinct refused operations (the failure window dedups identical
    // attempts, so each entry must differ).
    for command in [
        "rm -rf /",
        "sudo dd if=/dev/zero of=/dev/sda",
        "mkfs.ext4 /dev/sda",
    ] {
        agent.record_failed_tool_attempt(
            "shell_exec",
            &serde_json::json!({"command": command}).to_string(),
            "safety",
            "Safety check failed: blocked command",
        );
    }

    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "wall-clock timeout after 600s".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::BlockedBySafety);
    assert_eq!(mode.kind.tag(), "BLOCKED_BY_SAFETY");
    assert!(
        mode.evidence.contains("3 of the last 3"),
        "{}",
        mode.evidence
    );
    assert!(
        mode.advice.contains("safety"),
        "advice must point at the safety configuration: {}",
        mode.advice
    );
}

#[tokio::test]
async fn classify_blocked_by_safety_on_partial_exit() {
    let mut agent = make_agent().await;
    for command in ["rm -rf /", "sudo shutdown now"] {
        agent.record_failed_tool_attempt(
            "shell_exec",
            &serde_json::json!({"command": command}).to_string(),
            "safety",
            "Safety check failed: blocked command",
        );
    }

    let mode = FailureMode::classify(&agent, RunOutcome::Partial);
    assert_eq!(mode.kind, FailureKind::BlockedBySafety);
}

#[tokio::test]
async fn scattered_safety_blocks_do_not_relabel_a_real_timeout() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(1);
    // One safety refusal among many execution failures — not dominant.
    agent.record_failed_tool_attempt(
        "shell_exec",
        r#"{"command":"rm -rf /"}"#,
        "safety",
        "Safety check failed: blocked command",
    );
    for i in 0..5 {
        agent.record_failed_tool_attempt(
            "file_edit",
            &serde_json::json!({"path": format!("f{i}.rs")}).to_string(),
            "execution",
            "edit failed",
        );
    }

    let mode = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: "wall-clock timeout after 600s".to_string(),
        },
    );
    assert_eq!(mode.kind, FailureKind::Timeout);
}

#[test]
fn cli_banner_no_change_is_neither_success_nor_abort() {
    let m = FailureMode {
        kind: FailureKind::NoChange,
        evidence: "e".to_string(),
        advice: "a".to_string(),
    };
    let b = m.cli_banner();
    assert!(b.contains("NO_CHANGES"));
    assert!(!b.contains("REAL_EDIT"));
    assert!(!b.contains("❌"));
}

#[test]
fn advice_is_operator_facing_not_selfware_internal() {
    // ReadLoop branch: progress guard fired, zero mutations.
    let read_loop = classify_max_iter_failure(0, 1, 0, 0, 5, 0, None);
    assert_eq!(read_loop.kind, FailureKind::ReadLoop);
    // RetryLoop branch: a tool was permanently blocked.
    let retry_loop = classify_max_iter_failure(0, 0, 0, 1, 5, 0, None);
    assert_eq!(retry_loop.kind, FailureKind::RetryLoop);
    for m in [&read_loop, &retry_loop] {
        let a = m.advice.to_lowercase();
        assert!(
            !a.contains("selfware"),
            "advice leaks selfware internals: {}",
            m.advice
        );
        assert!(
            !a.contains("completion gate"),
            "advice leaks selfware internals: {}",
            m.advice
        );
        assert!(
            !a.contains("block threshold"),
            "advice leaks selfware internals: {}",
            m.advice
        );
    }
}

#[test]
fn nonfailure_covers_success_and_no_change_only() {
    assert!(FailureKind::Success.is_nonfailure());
    assert!(FailureKind::NoChange.is_nonfailure());
    // Everything else is a failure.
    for k in [
        FailureKind::BudgetExhausted,
        FailureKind::BlockedBySafety,
        FailureKind::Timeout,
        FailureKind::FakeComplete,
        FailureKind::ReadLoop,
        FailureKind::MaxIterations,
        FailureKind::Unknown,
    ] {
        assert!(!k.is_nonfailure(), "{:?} must be a failure", k);
    }
    // A NoChange banner must read as completed, never aborted.
    let m = FailureMode {
        kind: FailureKind::NoChange,
        evidence: "e".to_string(),
        advice: "a".to_string(),
    };
    assert!(m.cli_banner().contains("✅"));
    assert!(!m.cli_banner().contains("❌"));
}

#[test]
fn truncate_splits_at_char_boundaries() {
    // "αβγδ" is 4 Greek letters; slicing at byte index 3 would panic.
    let s = "αβγδ";
    assert_eq!(truncate(s, 4), "αβγδ");
    assert_eq!(truncate(s, 3), "αβγ…");
    assert_eq!(truncate(s, 0), "…");

    // ASCII fallback.
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello", 3), "hel…");
}
