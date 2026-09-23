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

/// e2e c40: the documentation task below was classified READ-ONLY because
/// "Do not change any code other than adding comments" matched the global
/// "do not change any" marker — so the run wrote nothing and ended
/// NO_CHANGES with exit 0. A prohibition with an exception clause scopes the
/// edit; it does not forbid editing.
#[test]
fn prohibition_with_exception_clause_does_not_make_a_task_read_only() {
    let c40 = "Multi-step documentation task in this Rust repo. Do the steps in order.\n\
        1. Read src/agent/context.rs in full.\n\
        2. Read src/agent/compression.rs in full.\n\
        3. Read src/agent/context_management.rs in full.\n\
        4. Create docs/CONTEXT_NOTES.md containing one section per file (context.rs, compression.rs, context_management.rs). In each section list every `pub fn` / `pub async fn` defined in that file as a bullet: `name` (line N) - one-sentence description.\n\
        5. In src/agent/context.rs, add a one-line `///` doc comment directly above every `pub fn` / `pub async fn` that does not already have a doc comment. Do not change any code other than adding comments.\n\
        6. Finish with a short summary saying how many functions you documented in step 5 and how many bullets are in docs/CONTEXT_NOTES.md.\n\
        Do not re-read a file you have already read unless you need to verify an edit.";
    assert!(crate::agent::tool_dispatch::task_requires_mutation(c40));
    assert!(
        !task_is_read_only(c40),
        "an exception-scoped prohibition must not flip an edit task read-only"
    );
    for scoped in [
        "Update the README and summarize the result. Do not modify anything except README.md.",
        "Fix the typo, then report back. Don't change anything besides the docs.",
        "Add the tests and explain them. Do not edit any files apart from tests/.",
    ] {
        assert!(!task_is_read_only(scoped), "{scoped}");
    }
    // A genuine global prohibition still wins: the exception must sit in the
    // SAME clause as the prohibition to scope it.
    assert!(task_is_read_only(
        "Review the parser and write a summary. Do not change any files. Report anything except style nits."
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

#[tokio::test]
async fn plain_status_query_does_not_require_mutation() {
    // Review finding #2 (task-policy inversion): two guards used to check
    // `current_task_is_read_only()`, which is FALSE for a plain status /
    // question query — so destructive synthesis (script / assumed-edit
    // synthesis) fired on status queries. Both now consult
    // `!current_task_requires_mutation()`, which is true for read-only tasks
    // AND plain queries. Assert the guard function outcomes for the three
    // task classes the review's matrix calls out: mutation task, review
    // task, plain query.
    let mut agent = Agent::new(crate::config::Config::default())
        .await
        .expect("agent should build");

    // Mutation task: requires mutation → the synthesis guards stay armed.
    agent.start_learning_session("m", "Fix the bug in parse_port.");
    assert!(
        agent.current_task_requires_mutation(),
        "a mutation task must require mutation (guards stay armed)"
    );

    // Review task: read-only classified → requires_mutation is false, so the
    // force-synthesis / scaffold machinery must not fire.
    agent.start_learning_session(
        "r",
        "Review the code in src/agent/ and report findings. Do NOT edit any files.",
    );
    assert!(agent.current_task_is_read_only());
    assert!(
        !agent.current_task_requires_mutation(),
        "a read-only review task must never require mutation"
    );

    // Plain status/question query: NOT read-only-classified and NOT
    // mutation-requiring — the exact class that previously slipped past the
    // read-only guard and hit destructive synthesis.
    agent.start_learning_session(
        "q",
        "What is the status of the test suite? List the failing tests.",
    );
    assert!(
        !agent.current_task_is_read_only(),
        "a status query is not branded read-only, yet ..."
    );
    assert!(
        !agent.current_task_requires_mutation(),
        "... a status query must still NOT require mutation — the guards that
         gate destructive synthesis check `requires_mutation`, so synthesis
         must not fire on it"
    );
}
