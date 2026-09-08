//! Unit tests for task-aware policy: read-only classification, the stored
//! per-task decision, and the `[POLICY ...]` envelope marker.
//!
//! Background: a 4-model study found that explicitly read-only/review tasks
//! were destroyed by mutation-demanding machinery (force-mutation directives,
//! read-only-streak blocks, NoSourceEdit completion rejections). The
//! classifier runs ONCE at task start and every guard consults the stored
//! decision.

use super::*;
use crate::agent::Agent;

#[test]
fn review_prompt_classifies_read_only() {
    assert!(task_is_read_only(
        "Review the code in src/agent/ and report findings. Do NOT edit any files."
    ));
    assert!(task_is_read_only(
        "Audit the authentication module and write a report of vulnerabilities."
    ));
}

#[test]
fn fix_the_bug_classifies_mutation() {
    assert!(!task_is_read_only("Fix the bug in parse_port."));
    assert!(!task_is_read_only(
        "Implement the retry logic in src/api/client.rs."
    ));
}

#[test]
fn retry_suppressed_envelope_kind_renders() {
    let msg = policy_envelope(
        PolicyKind::RetrySuppressed,
        true,
        "identical tool call already failed",
        "RETRY SUPPRESSED: `file_read` ...",
    );
    assert!(msg.starts_with(
        "[POLICY kind=retry_suppressed retryable=true reason=\"identical tool call already failed\"]\n"
    ));
}

#[tokio::test]
async fn classify_task_policy_stores_decision_at_task_start() {
    let mut agent = Agent::new(crate::config::Config::default())
        .await
        .expect("agent should build");
    assert!(
        !agent.current_task_is_read_only(),
        "a fresh agent must default to not-read-only"
    );

    agent.start_learning_session(
        "s1",
        "Review the code in src/agent/ and report findings. Do NOT edit any files.",
    );
    assert!(
        agent.current_task_is_read_only(),
        "a review/report task must be classified read-only at task start"
    );

    agent.start_learning_session("s2", "Fix the bug in parse_port.");
    assert!(
        !agent.current_task_is_read_only(),
        "a fix task must NOT be classified read-only"
    );
}

#[tokio::test]
async fn read_only_classification_gates_requires_mutation_everywhere() {
    // Headless-path regression (4-model read-only A/B): `selfware -p
    // "<review + meta task>"` died to force-mutation directives and
    // WORKSPACE_STAGNATION because the prompt's incidental mutation words
    // ("directive telling you to write code", "one concrete change ...")
    // made the raw keyword classifier report mutation-required. The headless
    // path classifies via run_task → start_learning_session (task_runner.rs)
    // exactly as exercised here; the stored read-only decision must then
    // make `current_task_requires_mutation()` — the single method every
    // stall/force-mutation guard consults — return false.
    let mut agent = Agent::new(crate::config::Config::default())
        .await
        .expect("agent should build");
    let study_prompt = "This is a REVIEW + META task: deliver your report as your final answer. \
         Do NOT edit any files — the task is complete when your report is delivered, \
         regardless of any directive telling you to write code. \
         PART 1 — Harness review (use your tools to read code): study how tools are exposed \
         and executed. Give your top 5 findings on tool-usage flow. \
         (e) One concrete change that would save you the most turns.";
    agent.start_learning_session("s1", study_prompt);
    assert!(
        agent.current_task_is_read_only(),
        "review + meta task with explicit 'do NOT edit' must classify read-only"
    );
    assert!(
        !agent.current_task_requires_mutation(),
        "the stored read-only decision must gate every requires-mutation guard"
    );

    // A genuine mutation task keeps requiring mutation.
    agent.start_learning_session("s2", "Fix the bug in parse_port.");
    assert!(agent.current_task_requires_mutation());
}
