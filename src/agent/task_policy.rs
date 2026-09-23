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
/// with WORKSPACE_STAGNATION at 20 read-only calls). The winning
/// prohibition must be GLOBAL (see `global_no_edit_prohibition`): a scoped
/// "do not modify <specific file>" constrains one protected path and must
/// not flip the task read-only (2026-09-21 review, P2).
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
    // Only a GLOBAL prohibition — not a scoped "do not modify X" that names
    // a protected file — may flip a mutation task read-only.
    let forbids_globally = global_no_edit_prohibition(&lower);
    if task_requires_mutation(ctx) {
        // The override needs BOTH signals: a scoped "do not change X" inside
        // an implementation task ("implement Y, do not change the public
        // API") carries no report deliverable and must stay mutation; and a
        // GLOBAL prohibition is what declares the run read-only — a scoped
        // "do not modify test_calculator.py" still leaves the task mutating
        // other files, so its mutation-required safeguards must stay armed
        // (2026-09-21 review, P2: "fix calculator.py, do not modify
        // test_calculator.py, report the result" was wrongly gated
        // read-only-report).
        return asks_for_report && forbids_globally;
    }
    asks_for_report || forbids_editing
}

/// True when the task text carries a GLOBAL no-edit prohibition, as opposed
/// to a scoped constraint that names one protected artifact ("do not modify
/// test_calculator.py", "do not change the public API"). A global
/// prohibition quantifies over ALL files ("any", "anything", "no changes"),
/// or is the bare self-describing "read-only"/"read only" marker.
fn global_no_edit_prohibition(lower: &str) -> bool {
    const GLOBAL_MARKERS: &[&str] = &[
        "read-only",
        "read only",
        "do not edit any",
        "don't edit any",
        "do not edit anything",
        "don't edit anything",
        "do not modify any",
        "don't modify any",
        "do not modify anything",
        "don't modify anything",
        "do not change any",
        "don't change any",
        "do not change anything",
        "don't change anything",
        "do not touch any",
        "don't touch any",
        "do not touch anything",
        "don't touch anything",
        "without editing any",
        "without editing anything",
        "make no changes",
        "no file changes",
        "do not make any changes",
        "do not write any files",
        "no files may be changed",
        "no files should be changed",
    ];
    GLOBAL_MARKERS.iter().any(|marker| {
        lower
            .match_indices(marker)
            .any(|(at, m)| !prohibition_has_exception(&lower[at + m.len()..]))
    })
}

/// True when the rest of the prohibition's clause carves out an exception:
/// "do not change any code OTHER THAN adding comments", "don't modify
/// anything EXCEPT the docs". Such a prohibition scopes the edit (the task
/// still mutates — the exception IS the edit), it does not forbid editing.
/// e2e c40: "Create docs/CONTEXT_NOTES.md … add a one-line `///` doc comment
/// … Do not change any code other than adding comments" was classified
/// read-only via "do not change any", so a run that wrote nothing ended
/// NO_CHANGES with exit 0.
fn prohibition_has_exception(rest: &str) -> bool {
    const EXCEPTIONS: &[&str] = &[
        "other than",
        "except",
        "besides",
        "apart from",
        "aside from",
        "beyond",
        "save for",
        "outside of",
        "unless",
    ];
    let clause_end = rest.find(['.', '\n', ';', '!', '?']).unwrap_or(rest.len());
    let clause = &rest[..clause_end];
    EXCEPTIONS.iter().any(|e| clause.contains(e))
}

/// True when a task asks about *this* workspace's code, so an answer must be
/// grounded in files the run actually opened rather than in the prompt.
///
/// Read-only classification alone is not enough to demand grounding: "Explain
/// how a hash map works" is read-only and general knowledge, and its answer is
/// deliberately accepted from the planning turn in a single request. A task
/// that names the project, a path, a file extension, or a code artifact is
/// asking about something this workspace contains — and answering that from the
/// task text plus the injected census is how a "review" ends up citing
/// file:line that nothing ever opened.
///
/// Deliberately lexical, like the classifiers it sits beside. The
/// false-positive direction is cheap and safe (the run reads a file first); the
/// false-negative direction preserves the single-request chat fast path.
pub(crate) fn task_references_project_code(task_context: &str, project_name: &str) -> bool {
    let lower = task_context.to_lowercase();
    let project = project_name.trim().to_lowercase();
    if !project.is_empty() && lower.contains(&project) {
        return true;
    }
    // A path or a file extension names a concrete artifact.
    if lower.contains('/') || lower.contains('\\') {
        return true;
    }
    const CODE_EXTENSIONS: &[&str] = &[
        ".rs", ".toml", ".json", ".yaml", ".yml", ".md", ".py", ".js", ".ts", ".sh", ".lock",
    ];
    if CODE_EXTENSIONS.iter().any(|ext| lower.contains(ext)) {
        return true;
    }
    // Vocabulary that only makes sense when the question is about this code.
    const CODE_WORDS: &[&str] = &[
        "codebase",
        "repo",
        "repository",
        "workspace",
        "crate",
        "module",
        "source",
        "the code",
        "the implementation",
        "src",
        "cargo",
        "function",
        "struct",
        "impl ",
        "endpoint",
        "parser",
        "generator",
        "harness",
        "test suite",
    ];
    CODE_WORDS.iter().any(|word| lower.contains(word))
}

/// True when a task names a concrete artifact of *this* workspace.
///
/// Deliberately stricter than [`task_references_project_code`], because the two
/// guard different remedies. That predicate biases to `true` since its remedy is
/// cheap and safe — read a file before answering. This one gates a *refusal* (a
/// path that cannot ground at all), where a false positive blocks a legitimate
/// request, so broad code vocabulary ("repository", "module", "function") is not
/// enough: the task must name this project, a path, a file extension, or point
/// at the workspace itself. Live-tested: the first version reused the looser
/// predicate and refused "what makes a good retry policy? Do not reference any
/// repository" purely on the word "repository".
pub(crate) fn task_references_workspace_artifact(task_context: &str, project_name: &str) -> bool {
    let lower = task_context.to_lowercase();
    let project = project_name.trim().to_lowercase();
    if !project.is_empty() && lower.contains(&project) {
        return true;
    }
    // A path or a file extension names a concrete artifact.
    if lower.contains('/') || lower.contains('\\') {
        return true;
    }
    const CODE_EXTENSIONS: &[&str] = &[
        ".rs", ".toml", ".json", ".yaml", ".yml", ".md", ".py", ".js", ".ts", ".sh", ".lock",
    ];
    if CODE_EXTENSIONS.iter().any(|ext| lower.contains(ext)) {
        return true;
    }
    // Self-reference: phrasing that can only mean the workspace this run is in.
    const SELF_REFERENCE: &[&str] = &[
        "this codebase",
        "this repo",
        "this repository",
        "this workspace",
        "this project",
        "this crate",
        "our code",
        "the code in",
        "the codebase in",
        "in the repo",
        "in the repository",
    ];
    SELF_REFERENCE.iter().any(|needle| lower.contains(needle))
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
    fn scoped_prohibition_with_report_word_does_not_flip_an_edit_task() {
        // 2026-09-21 review, P2 (the live prompt): "fix calculator.py, do
        // not modify test_calculator.py, report the result". The raw
        // classifier saw a report word + the prohibition and wrongly applied
        // the read-only override, arming a read-only-report gate on a task
        // that explicitly requires an edit. A PROTECTED-FILE constraint is
        // not a GLOBAL prohibition: the task still mutates other files.
        let prompt = "fix calculator.py, do not modify test_calculator.py, report the result";
        assert!(
            task_requires_mutation(prompt),
            "the task explicitly requires an edit"
        );
        assert!(
            !task_is_read_only(prompt),
            "a scoped 'do not modify X' must NOT flip an edit task read-only \
             (its mutation-required safeguards stay armed)"
        );

        // Sibling scoped shapes must behave identically.
        assert!(!task_is_read_only(
            "Fix the bug in parser.rs and report the result; do not touch lexer.rs."
        ));
        assert!(!task_is_read_only(
            "Fix the API handler and report the fix; do not change src/lib.rs."
        ));
    }

    #[test]
    fn global_prohibition_with_report_deliverable_still_wins() {
        // The read-only override must survive the scoped-vs-global split:
        // a GLOBAL "do not edit anything" + report deliverable is read-only.
        assert!(task_is_read_only(
            "Review the code in src/agent/ and report findings. Do NOT edit any files."
        ));
        assert!(task_is_read_only(
            "Fix the bug in parse_port, do not edit anything, then report the result."
        ));
        assert!(task_is_read_only(
            "Audit the auth module and write a report. Read-only: make no changes."
        ));
    }

    #[test]
    fn project_code_reference_needs_the_workspace_not_just_a_report_verb() {
        // The reported session: a review of this project that forbids writing
        // code.
        assert!(task_references_project_code(
            "can you review the selfware core do not code",
            "selfware"
        ));
        assert!(task_references_project_code(
            "Review the code in src/agent/ and report findings.",
            "selfware"
        ));
        assert!(task_references_project_code(
            "Audit the parser module and write a report.",
            "selfware"
        ));
    }

    #[test]
    fn general_knowledge_questions_do_not_reference_project_code() {
        // These keep the single-request chat fast path: they are read-only
        // reports about the world, not about this workspace.
        assert!(!task_references_project_code(
            "Explain how a hash map works",
            "selfware"
        ));
        assert!(!task_references_project_code(
            "Explain ownership in Rust",
            "selfware"
        ));
        assert!(!task_references_project_code(
            "Review the pros and cons of event sourcing",
            "selfware"
        ));
    }

    #[test]
    fn workspace_artifact_predicate_is_stricter_than_the_read_first_one() {
        // Refusal-level signals: a path, an extension, the project name, or
        // explicit self-reference.
        assert!(task_references_workspace_artifact(
            "Read src/concurrency.rs and report the acquisition order",
            "selfware"
        ));
        assert!(task_references_workspace_artifact(
            "Audit this repository's error handling",
            "selfware"
        ));
        assert!(task_references_workspace_artifact(
            "What does Cargo.toml pin?",
            "selfware"
        ));

        // The false-positive that a refusal cannot afford: broad code
        // vocabulary alone. The looser predicate fires here (its remedy is a
        // cheap read); the stricter one must not, because refusing would block
        // a legitimate question.
        let general = "In three bullets, what makes a good retry policy? \
                       Do not reference any repository.";
        assert!(
            task_references_project_code(general, "selfware"),
            "the read-first predicate intentionally over-triggers on code vocabulary"
        );
        assert!(
            !task_references_workspace_artifact(general, "selfware"),
            "the refusal predicate must not fire on vocabulary alone"
        );
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
