//! Task-aware policy (validation change 1 + 2).
//!
//! Two small, deterministic pieces shared by the dispatch, progress, and
//! completion-gate machinery:
//!
//! 1. **Read-only task classification** — a task is read-only when it asks
//!    for review/analysis/report AND does not require mutation. The
//!    classifier inverts the existing `task_requires_mutation` machinery
//!    (no duplicated keyword lists) and is computed ONCE at task start
//!    (`Agent::classify_task_policy`) and stored on the agent, so every
//!    guard consults the same stored decision.
//! 2. **Policy envelope** — every injected guard/gate rejection or
//!    directive message is prefixed with a structured marker line
//!    (`[POLICY kind=... retryable=... reason="..."]`) before the
//!    human-readable text, via the single `policy_envelope` helper.

use crate::agent::tool_dispatch::task_requires_mutation;

/// Policy kinds that appear in the `[POLICY ...]` envelope marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyKind {
    /// Force-mutation directive ("write code NOW" / read-loop recovery).
    ForceMutation,
    /// Stagnation / progress-guard warnings and blocks.
    Stagnation,
    /// Retry-suppression notice for an identical, already-failed tool call.
    RetrySuppressed,
    /// Completion-gate rejection (retryable: the model can act and retry).
    Gate,
    /// A failed tool call's unified error feedback (the single channel —
    /// see `tool_dispatch::push_tool_result_message`).
    ToolError,
}

impl PolicyKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            PolicyKind::ForceMutation => "force_mutation",
            PolicyKind::Stagnation => "stagnation",
            PolicyKind::RetrySuppressed => "retry_suppressed",
            PolicyKind::Gate => "gate",
            PolicyKind::ToolError => "tool_error",
        }
    }
}

/// Deterministic read-only task classification.
///
/// A task is read-only when it has a non-empty context, does NOT require
/// mutation (per the existing `task_requires_mutation` classifier), and
/// asks for a review/analysis/report-style deliverable. Pure inversion of
/// `task_requires_mutation` alone is not enough: a plain chat prompt
/// ("what is 2+2") is not a review task and should not be branded
/// read-only *report* mode; conversely "fix the bug" must stay a mutation
/// task. Kept as a free function so it is unit-testable without an Agent.
///
/// Exception: an explicit "do not edit / read-only" instruction together
/// with a genuine report deliverable WINS over `task_requires_mutation`.
/// Review + meta tasks quote mutation words incidentally ("regardless of
/// any directive telling you to write code", "one concrete change that
/// would save you the most turns") and the raw keyword classifier then
/// flips them to mutation-required — arming every force-mutation and
/// stagnation guard against a task that must never edit (4-model read-only
/// A/B: exactly this misclassification killed a "do NOT edit" review run
/// with WORKSPACE_STAGNATION at 20 read-only calls).
pub(crate) fn task_is_read_only(task_context: &str) -> bool {
    let ctx = task_context.trim();
    if ctx.is_empty() {
        return false;
    }
    let lower = ctx.to_lowercase();
    // Genuine review/analysis/report deliverable signals.
    let asks_for_report = [
        "review",
        "analyz",
        "analys",
        "report",
        "audit",
        "summar",
        "explain",
        "describe",
        "assess",
        "evaluat",
        "investigat",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    // Explicit "do not mutate" instructions.
    let forbids_editing = [
        "read-only",
        "read only",
        "do not edit",
        "don't edit",
        "do not modify",
        "don't modify",
        "do not change",
        "without editing",
        "no changes",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if task_requires_mutation(ctx) {
        // The override needs BOTH signals: a scoped "do not change X" inside
        // an implementation task ("implement Y, do not change the public
        // API") carries no report deliverable and must stay mutation.
        return asks_for_report && forbids_editing;
    }
    asks_for_report || forbids_editing
}

/// Structured policy envelope: prefix `body` with a single marker line.
///
/// All injected guard/gate messages share this format so downstream
/// tooling (and tests) can recognize harness-injected policy text:
/// `[POLICY kind=<kind> retryable=<true|false> reason="<reason>"]`.
pub(crate) fn policy_envelope(
    kind: PolicyKind,
    retryable: bool,
    reason: &str,
    body: &str,
) -> String {
    // Keep the reason a single line: a newline would break the
    // "marker on the first line" contract.
    let reason = reason.replace(['\n', '\r'], " ");
    format!(
        "[POLICY kind={} retryable={} reason=\"{}\"]\n{}",
        kind.as_str(),
        retryable,
        reason,
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_prompt_is_read_only() {
        assert!(task_is_read_only(
            "Review the code in src/agent/ and report findings. Do NOT edit any files."
        ));
        assert!(task_is_read_only(
            "Analyze the repository structure and write a report as your final answer."
        ));
        assert!(task_is_read_only("Summarize what this codebase does."));
    }

    #[test]
    fn fix_the_bug_is_not_read_only() {
        assert!(!task_is_read_only("Fix the bug in parse_port."));
        assert!(!task_is_read_only(
            "Implement the classifier in src/lib.rs."
        ));
    }

    #[test]
    fn empty_and_plain_chat_are_not_read_only() {
        assert!(!task_is_read_only(""));
        assert!(!task_is_read_only("   "));
        assert!(!task_is_read_only("what is 2+2"));
    }

    #[test]
    fn review_meta_task_quoting_mutation_words_is_read_only() {
        // Verbatim key sentences from the 4-model read-only study prompt
        // (/tmp/sw_study/prompt.txt in the failed A/B run): the raw keyword
        // classifier sees unnegated "write"/"change" and reports
        // mutation-required, but the explicit "do NOT edit" + review/report
        // deliverable must win.
        let prompt = "You are one of 4 different models independently studying this repository's \
             agent harness, so the harness authors can compare how different models work with it. \
             This is a REVIEW + META task: deliver your report as your final answer. \
             Do NOT edit any files — the task is complete when your report is delivered, \
             regardless of any directive telling you to write code.\n\n\
             PART 1 — Harness review (use your tools to read code): study how tools are exposed \
             and executed. Give your top 5 findings on tool-usage flow.\n\n\
             PART 2 — Meta: answer as the model DRIVING this harness, candidly:\n\
             (e) One concrete change that would save you the most turns.\n\n\
             End with: PART1 findings (numbered), PART2 answers (a-e). Cite file:line where relevant.";
        assert!(
            task_requires_mutation(prompt),
            "the raw keyword classifier still flags the incidental mutation words \
             (the override lives in task_is_read_only)"
        );
        assert!(
            task_is_read_only(prompt),
            "explicit 'do NOT edit' + review/report deliverable must override \
             incidental mutation words"
        );
    }

    #[test]
    fn scoped_no_edit_inside_implementation_task_stays_mutation() {
        // The override requires a genuine report deliverable: "do not change"
        // scoped to one artifact inside an implementation task must NOT flip
        // the task to read-only.
        assert!(!task_is_read_only(
            "Implement the retry logic in src/api/client.rs, but do not change the public API."
        ));
        assert!(!task_is_read_only(
            "Fix the parser bug; do not edit the config file."
        ));
    }

    #[test]
    fn envelope_marker_is_first_line() {
        let msg = policy_envelope(
            PolicyKind::Gate,
            true,
            "no source edit in diff",
            "NoSourceEdit: edit a source file before completing.",
        );
        assert!(msg
            .starts_with("[POLICY kind=gate retryable=true reason=\"no source edit in diff\"]\n"));
        assert!(msg.contains("NoSourceEdit: edit a source file before completing."));
    }

    #[test]
    fn envelope_flattens_newlines_in_reason() {
        let msg = policy_envelope(PolicyKind::Stagnation, false, "multi\nline reason", "body");
        let first_line = msg.lines().next().unwrap();
        assert!(first_line
            .starts_with("[POLICY kind=stagnation retryable=false reason=\"multi line reason\"]"));
        assert_eq!(msg.lines().count(), 2);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/task_policy/task_policy_test.rs"]
mod task_policy_test;
