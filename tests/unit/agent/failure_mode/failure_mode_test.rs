use super::*;
use crate::checkpoint::{TaskCheckpoint, ToolCallLog};
use crate::config::Config;
use chrono::Utc;

fn test_config() -> Config {
    crate::test_support::mock_agent_config_with_limits(
        "http://localhost:0/v1",
        500_000,
        8192,
        8,
        30,
    )
}

async fn make_agent() -> Agent {
    Agent::new(test_config()).await.expect("agent::new")
}

/// Give the agent a checkpoint carrying one successful tool call.
fn seed_tool_call(agent: &mut Agent, tool: &str, args: &str) {
    let mut cp = agent
        .current_checkpoint
        .take()
        .unwrap_or_else(|| TaskCheckpoint::new("t-classify".to_string(), "task".to_string()));
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: tool.to_string(),
        arguments: args.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(5),
    });
    agent.current_checkpoint = Some(cp);
}

#[tokio::test]
async fn classify_success_when_mutating_calls_exist() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(3);
    agent.test_set_total_tool_calls(12);
    agent.test_set_last_assistant_response("Done.".to_string());
    // REAL_EDIT requires file evidence: seed a file_write landing on disk.
    seed_tool_call(&mut agent, "file_write", r#"{"path":"src/lib.rs"}"#);

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(mode.kind, FailureKind::Success);
    assert!(
        mode.evidence.contains("3 mutating tool calls"),
        "evidence: {}",
        mode.evidence
    );
}

/// 2026-09-22 e2e: runs whose only "mutations" were read-shaped shell probes
/// rendered REAL_EDIT with "files changed: none". Probe-only mutation counts
/// must label NoChange, never REAL_EDIT.
#[tokio::test]
async fn classify_probe_only_mutations_label_no_change_not_real_edit() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(3);
    agent.test_set_total_tool_calls(12);
    agent.test_set_last_assistant_response("Done.".to_string());
    // Shell probes: mutating by classification, but nothing reaches disk.
    seed_tool_call(
        &mut agent,
        "shell_exec",
        r#"{"command":"python3 stats.py"}"#,
    );

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(
        mode.kind,
        FailureKind::NoChange,
        "probe-only mutations must not earn REAL_EDIT: {}",
        mode.evidence
    );
    assert!(mode.evidence.contains("no file reached disk"));
}

/// The honesty fix must not over-rotate: a run whose file change came via a
/// shell redirect IS a real edit (the file-tool ledger cannot see it).
#[tokio::test]
async fn classify_shell_redirect_write_counts_as_real_edit() {
    let mut agent = make_agent().await;
    agent.test_set_mutating_count(2);
    agent.test_set_total_tool_calls(5);
    agent.test_set_last_assistant_response("Done.".to_string());
    seed_tool_call(
        &mut agent,
        "shell_exec",
        r#"{"command":"printf 'x' > out.txt"}"#,
    );

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(
        mode.kind,
        FailureKind::Success,
        "a write-shaped shell command is real edit evidence: {}",
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

/// Regression: a read-only review session ended with verdict
/// `FAKE_COMPLETE: "model emitted 'Final answer' but performed 0 mutating
/// tool calls across 7 total calls"` — but on a read-only task 0 mutations
/// is the CORRECT outcome. The classifier must consult the stored read-only
/// decision and land on the natural NoChange label instead.
#[tokio::test]
async fn classify_read_only_task_with_final_answer_is_no_change_not_fake_complete() {
    let mut agent = make_agent().await;
    agent.test_set_task_read_only(true);
    agent.test_set_mutating_count(0);
    agent.test_set_total_tool_calls(7);
    agent.test_set_last_assistant_response(
        "Final answer: the review findings are as follows.".to_string(),
    );

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(
        mode.kind,
        FailureKind::NoChange,
        "evidence: {}",
        mode.evidence
    );
    assert!(mode.kind.is_nonfailure());
}

/// A mutation task with 0 mutations and a final-answer marker must STILL be
/// FakeComplete — the read-only exemption must not weaken the gate.
#[tokio::test]
async fn classify_mutation_task_with_final_answer_stays_fake_complete() {
    let mut agent = make_agent().await;
    agent.test_set_task_read_only(false);
    agent.test_set_mutating_count(0);
    agent.test_set_total_tool_calls(7);
    agent.test_set_last_assistant_response("Final answer: implementation complete.".to_string());

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(mode.kind, FailureKind::FakeComplete);
}

/// Read-only exemption in the max-iterations path: prose output with 0
/// mutating calls is the deliverable on a read-only task, so the honest
/// label is MaxIterations, not FakeComplete.
#[test]
fn max_iter_failure_read_only_skips_fake_complete() {
    let read_only = classify_max_iter_failure(0, 0, 0, 0, 9, 4_000, None, true);
    assert_eq!(read_only.kind, FailureKind::MaxIterations);
    let mutation = classify_max_iter_failure(0, 0, 0, 0, 9, 4_000, None, false);
    assert_eq!(mutation.kind, FailureKind::FakeComplete);
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

/// A permission/operator-approval stop must NEVER be mislabeled
/// MAX_ITERATIONS (2026-09-21 review, P2): the headless AutoEdit CLI stop —
/// `shell_exec` required confirmation in a non-interactive run — was
/// classified `MAX_ITERATIONS` after only 3 iterations, with the wrong
/// "raise max_iterations" recovery advice. The stop was deliberate: a tool
/// needed interactive approval the mode could not provide.
#[tokio::test]
async fn classify_permission_required_instead_of_max_iterations() {
    let mut agent = make_agent().await;
    // The exact stop shape from the review's auto-edit CLI probe (3 turns,
    // zero tool calls executed).
    agent.test_set_mutating_count(0);
    agent.test_set_total_tool_calls(0);

    let reason = "Agent failed: Tool 'shell_exec' requires confirmation but running in \
                  non-interactive mode. Use --yolo to auto-approve tools, or run interactively."
        .to_string();
    let mode = FailureMode::classify(&agent, RunOutcome::Failed { reason });

    assert_eq!(
        mode.kind,
        FailureKind::PermissionRequired,
        "a permission stop must not be filed as iteration exhaustion; evidence: {}",
        mode.evidence
    );
    assert_eq!(mode.kind.tag(), "PERMISSION_REQUIRED");
    // The JSON artifact must carry the honest category — never
    // MAX_ITERATIONS with its wrong advice.
    let json = serde_json::to_string(&mode).unwrap();
    assert!(
        json.contains("PermissionRequired"),
        "failure_mode.json must say PermissionRequired, got: {json}"
    );
    assert!(
        !json.contains("MaxIterations"),
        "failure_mode.json must not claim MaxIterations, got: {json}"
    );
    assert!(
        mode.advice.contains("--yolo") || mode.advice.contains("interactive"),
        "advice must point at operator action: {}",
        mode.advice
    );
    // The remedy must be operator action, never the iteration-cap advice
    // that MAX_ITERATIONS would have given. The advice may *warn* against
    // raising the cap ("do NOT raise max_iterations"); it must not present
    // that action as the fix.
    let lower_advice = mode.advice.to_lowercase();
    assert!(
        !lower_advice.contains("raise max_iterations or split"),
        "advice must not recommend raising the cap: {}",
        mode.advice
    );
    assert!(
        mode.cli_banner().contains("PERMISSION_REQUIRED"),
        "the CLI banner must tag the permission stop: {}",
        mode.cli_banner()
    );
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
        restored_files: Vec::new(),
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
        restored_files: Vec::new(),
        kind: FailureKind::NontermProse,
        evidence: "30 consecutive prose-only turns".to_string(),
        advice: "try a smaller context budget".to_string(),
    };
    let banner = mode.cli_banner();
    assert!(banner.contains("NONTERM_PROSE"));
    assert!(banner.contains("30 consecutive"));
    assert!(banner.contains("smaller context"));

    let success = FailureMode {
        restored_files: Vec::new(),
        kind: FailureKind::Success,
        evidence: "all good".to_string(),
        advice: "-".to_string(),
    };
    assert!(success.cli_banner().contains("REAL_EDIT"));
    assert!(success.cli_banner().contains("✅"));
}

/// kvstore_nat (2026-09-24): the requirements audit died on 4 gateway 503s
/// and the banner still read "✅ Task completed successfully". Completion is
/// allowed (advisory on infra failure) but never as a clean, audited success.
#[test]
fn audit_not_performed_downgrades_the_success_banner_but_not_the_kind() {
    let base = FailureMode {
        restored_files: Vec::new(),
        kind: FailureKind::Success,
        evidence: "21 mutating tool calls".to_string(),
        advice: "-".to_string(),
    };
    let not_performed = crate::agent::RequirementsAuditStatus::NotPerformed(
        "gateway timeout (HTTP 503 after 300s)".to_string(),
    );
    let mode = with_audit_status(base.clone(), Some(&not_performed));
    assert_eq!(mode.kind, FailureKind::Success, "exit status unchanged");
    assert!(
        mode.evidence.contains(AUDIT_NOT_PERFORMED_NOTE),
        "{}",
        mode.evidence
    );
    assert!(
        mode.evidence.contains("gateway timeout"),
        "{}",
        mode.evidence
    );
    let banner = mode.cli_banner();
    assert!(!banner.contains("completed successfully"), "{banner}");
    assert!(!banner.contains('✅'), "{banner}");
    assert!(banner.contains("NOT PERFORMED"), "{banner}");

    // A performed audit, or no audit at all, leaves the verdict untouched.
    let performed = crate::agent::RequirementsAuditStatus::Performed("ALL ADDRESSED".to_string());
    let clean = with_audit_status(base.clone(), Some(&performed));
    assert!(clean
        .cli_banner()
        .contains("✅ Task completed successfully"));
    assert_eq!(
        with_audit_status(base.clone(), None).evidence,
        base.evidence
    );

    // Failure verdicts pass through unchanged.
    let failed = FailureMode {
        kind: FailureKind::VerificationFailed,
        ..base
    };
    let failed_evidence = failed.evidence.clone();
    assert_eq!(
        with_audit_status(failed, Some(&not_performed)).evidence,
        failed_evidence
    );
}

#[test]
fn failure_kind_serializes_to_json() {
    let mode = FailureMode {
        restored_files: Vec::new(),
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
        restored_files: Vec::new(),
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
    let read_loop = classify_max_iter_failure(0, 1, 0, 0, 5, 0, None, false);
    assert_eq!(read_loop.kind, FailureKind::ReadLoop);
    // RetryLoop branch: a tool was permanently blocked.
    let retry_loop = classify_max_iter_failure(0, 0, 0, 1, 5, 0, None, false);
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
        restored_files: Vec::new(),
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

/// 24k-context e2e: a documentation task that REQUIRED edits spent 40/40
/// iterations re-reading files (its only "mutating" calls were probes) and
/// ended "✅ Completed — no file changes made (NO_CHANGES)", exit 0. On a
/// mutation-required task that is a failure, not an honest no-op.
#[tokio::test]
async fn classify_mutation_task_with_probe_only_calls_is_required_edit_missing() {
    let mut agent = make_agent().await;
    agent.current_task_context =
        "Update README.md to document every CLI flag with an example".to_string();
    assert!(agent.current_task_requires_mutation(), "precondition");
    agent.test_set_mutating_count(2);
    agent.test_set_total_tool_calls(40);
    agent.test_set_last_assistant_response("Task complete.".to_string());
    seed_tool_call(&mut agent, "shell_exec", r#"{"command":"cargo doc"}"#);

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(
        mode.kind,
        FailureKind::RequiredEditMissing,
        "{}",
        mode.evidence
    );
    assert!(!mode.kind.is_nonfailure(), "must not render as success");
    assert_eq!(mode.kind.tag(), "NO_CHANGES_REQUIRED_EDIT");
    assert!(
        mode.evidence.contains("iterations used"),
        "{}",
        mode.evidence
    );
    let banner = mode.cli_banner();
    assert!(banner.contains("❌"), "{banner}");
    assert!(!banner.contains("✅"), "{banner}");
}

/// The same probe-only run on a task that does NOT require edits keeps the
/// honest NoChange label (non-failure).
#[tokio::test]
async fn classify_probe_only_on_non_mutation_task_stays_no_change() {
    let mut agent = make_agent().await;
    agent.current_task_context = "What is the purpose of the scheduler module?".to_string();
    assert!(!agent.current_task_requires_mutation(), "precondition");
    agent.test_set_mutating_count(2);
    agent.test_set_total_tool_calls(5);
    seed_tool_call(&mut agent, "shell_exec", r#"{"command":"cargo doc"}"#);

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert_eq!(mode.kind, FailureKind::NoChange, "{}", mode.evidence);
}

#[test]
fn restored_files_are_serialized_only_when_present() {
    let mut mode = FailureMode {
        restored_files: Vec::new(),
        kind: FailureKind::MaxIterations,
        evidence: "e".to_string(),
        advice: "a".to_string(),
    };
    let json = serde_json::to_string(&mode).unwrap();
    assert!(!json.contains("restored_files"), "{json}");
    mode.restored_files = vec!["Cargo.toml".to_string()];
    let json = serde_json::to_string(&mode).unwrap();
    assert!(
        json.contains(r#""restored_files":["Cargo.toml"]"#),
        "{json}"
    );
}

// ---------------------------------------------------------------------------
// e2e c40: NO_CHANGES + exit 0 on a mutation-required task with failed
// verification.
// ---------------------------------------------------------------------------

const C40_TASK: &str = "Multi-step documentation task in this Rust repo. Do the steps in order.\n\
    1. Read src/agent/context.rs in full.\n\
    4. Create docs/CONTEXT_NOTES.md containing one section per file.\n\
    5. In src/agent/context.rs, add a one-line `///` doc comment directly above every `pub fn` that does not already have a doc comment. Do not change any code other than adding comments.\n\
    6. Finish with a short summary saying how many functions you documented.";

/// The c40 completion path: the task was (mis)classified read-only, so a
/// natural completion with 0 mutating calls took the NoChange branch and the
/// run exited 0. The classification now keeps it mutation-required, and a
/// mutation-required natural completion with zero mutations is a typed
/// failure that `failure_verdict_as_error` turns into a non-zero exit.
#[tokio::test]
async fn c40_mutation_task_with_zero_mutations_is_a_typed_failure() {
    let mut agent = make_agent().await;
    agent.current_task_context = C40_TASK.to_string();
    agent.classify_task_policy();
    assert!(
        !agent.current_task_is_read_only(),
        "c40 task is not read-only"
    );
    assert!(
        agent.current_task_requires_mutation(),
        "c40 task requires edits"
    );
    agent.test_set_mutating_count(0);
    agent.test_set_total_tool_calls(23);
    agent.test_set_last_assistant_response(
        "**Status: complete (verification: `cargo check --lib` -> exit 0, green).**".to_string(),
    );

    let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
    assert!(
        !mode.kind.is_nonfailure(),
        "zero mutations on a mutation-required task must fail: {:?} {}",
        mode.kind,
        mode.evidence
    );
    assert!(!mode.cli_banner().contains("✅"), "{}", mode.cli_banner());
    let exit = crate::agent::task_runner::failure_verdict_as_error(Ok(()), Some(&mode));
    assert!(exit.is_err(), "a failure verdict must not exit 0");
}

fn verdict(kind: FailureKind) -> FailureMode {
    FailureMode {
        kind,
        evidence: "base evidence".to_string(),
        advice: "-".to_string(),
        restored_files: Vec::new(),
    }
}

#[test]
fn failed_verification_turns_a_real_edit_into_a_failure() {
    let mode = with_verification_verdict(verdict(FailureKind::Success), Some((false, 3)), false);
    assert_eq!(mode.kind, FailureKind::VerificationFailed);
    assert!(!mode.kind.is_nonfailure());
    assert_eq!(mode.kind.tag(), "VERIFICATION_FAILED");
    assert!(
        mode.evidence.contains("3 verification check(s)"),
        "{}",
        mode.evidence
    );
    let banner = mode.cli_banner();
    assert!(banner.contains("❌"), "{banner}");
    assert!(!banner.contains("✅"), "{banner}");
}

#[test]
fn failed_verification_on_a_non_read_only_no_change_is_a_failure() {
    let mode = with_verification_verdict(verdict(FailureKind::NoChange), Some((false, 1)), false);
    assert_eq!(mode.kind, FailureKind::VerificationFailed);
    assert!(!mode.kind.is_nonfailure());
}

/// A read-only report whose own check failed keeps its non-failure label
/// (the report is the deliverable) but never renders the ✅ "Completed"
/// banner.
#[test]
fn failed_verification_on_a_read_only_no_change_is_named_not_green() {
    let mode = with_verification_verdict(verdict(FailureKind::NoChange), Some((false, 1)), true);
    assert_eq!(mode.kind, FailureKind::NoChange);
    assert!(
        mode.evidence.contains(VERIFICATION_FAILED_NOTE),
        "{}",
        mode.evidence
    );
    let banner = mode.cli_banner();
    assert!(!banner.contains("✅"), "{banner}");
    assert!(banner.contains("verification FAILED"), "{banner}");
}

#[test]
fn passing_or_absent_verification_and_failure_verdicts_pass_through() {
    for verification in [None, Some((true, 4))] {
        let mode = with_verification_verdict(verdict(FailureKind::Success), verification, false);
        assert_eq!(mode.kind, FailureKind::Success);
        assert_eq!(mode.evidence, "base evidence");
    }
    let mode =
        with_verification_verdict(verdict(FailureKind::FakeComplete), Some((false, 2)), false);
    assert_eq!(mode.kind, FailureKind::FakeComplete);
    assert_eq!(mode.evidence, "base evidence");
}
