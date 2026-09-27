//! Per-turn workload classification: which [`TurnWorkload`] quota a main
//! model call is sent under (`ThinkingMode::Workload`).
//!
//! The quota table itself lives in the matched model profile
//! (`config::model_profiles`, e.g. the measured qwen38 table) and in the
//! user's `[workloads.<kind>]` config; this module only decides the KIND of
//! each turn:
//!
//! - the first call of a task (`plan_with_thinking`) is `Planning`;
//! - a turn that continues right after a tool batch in which every call was a
//!   read (`is_mechanical_read`) is `Mechanical` — picking the next file or
//!   search;
//! - a turn after any other tool batch (edit, write, shell, check, test) is
//!   `Edit`;
//! - anything else — after a gate or correction directive, a user message,
//!   an answer without tools — is `Synthesis`. Unknown is `Synthesis`, the
//!   kind that keeps thinking on: a misclassified reading turn only costs
//!   time, a misclassified synthesis would cost answer quality.
//!
//! In a code review the coverage gate's phase signal (`Agent::review_phase`)
//! outranks the per-step classification once it says `Synthesis`: the turn
//! that writes the review runs under the synthesis quota even when it
//! follows a read batch. In the `Reading` phase the per-step
//! classification applies (the gate refuses a final answer while relevant
//! files are unread).

use crate::config::TurnWorkload;

/// Whether one tool call only READS the workspace — the calls a
/// `Mechanical` continuation follows. The canonical observational
/// predicate (`tool_call_is_observational`), minus the verification tools
/// (`cargo_*` checks/tests, `shell_exec`): their results usually need
/// reasoning (a failing test, a compiler error), so the turn after them is
/// `Edit`, not a mechanical next read.
pub(crate) fn is_mechanical_read(name: &str, args_str: &str) -> bool {
    !name.starts_with("cargo_")
        && name != "shell_exec"
        && super::tool_dispatch::tool_call_is_observational(name, args_str)
}

/// Classify the turn that follows a dispatched tool batch.
pub(crate) fn workload_after_tool_batch<'a>(
    calls: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> TurnWorkload {
    let mut any = false;
    for (name, args) in calls {
        any = true;
        if !is_mechanical_read(name, args) {
            return TurnWorkload::Edit;
        }
    }
    if any {
        TurnWorkload::Mechanical
    } else {
        TurnWorkload::Synthesis
    }
}

/// Per-workload turn counts for the run summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkloadTurnCounts {
    /// `(kind, turns, completion tokens reported by the server)` for every
    /// kind that ran at least once, in [`TurnWorkload::ALL`] order.
    pub rows: Vec<(TurnWorkload, usize, u64)>,
    /// Turns of a kind whose thinking-off reply had no tool call and were
    /// re-asked under the synthesis quota (`(kind, count)`).
    pub escalated: Vec<(TurnWorkload, usize)>,
}

impl WorkloadTurnCounts {
    pub(crate) fn record(&mut self, kind: TurnWorkload, completion_tokens: Option<u32>) {
        let tokens = u64::from(completion_tokens.unwrap_or(0));
        if let Some(row) = self.rows.iter_mut().find(|r| r.0 == kind) {
            row.1 += 1;
            row.2 += tokens;
        } else {
            self.rows.push((kind, 1, tokens));
            self.rows
                .sort_by_key(|r| TurnWorkload::ALL.iter().position(|k| *k == r.0));
        }
    }

    pub(crate) fn record_escalation(&mut self, kind: TurnWorkload) {
        match self.escalated.iter_mut().find(|r| r.0 == kind) {
            Some(row) => row.1 += 1,
            None => self.escalated.push((kind, 1)),
        }
    }

    /// How many turns of `kind` were escalated.
    pub fn escalations(&self, kind: TurnWorkload) -> usize {
        self.escalated
            .iter()
            .find(|r| r.0 == kind)
            .map_or(0, |r| r.1)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The workload of the next main turn from the review phase (if the task is
/// a code review) and the classification recorded by the last tool batch.
pub(crate) fn resolve_turn_workload(
    review_phase: Option<super::ReviewPhase>,
    after_step: Option<TurnWorkload>,
) -> TurnWorkload {
    if review_phase == Some(super::ReviewPhase::Synthesis) {
        return TurnWorkload::Synthesis;
    }
    after_step.unwrap_or(TurnWorkload::Synthesis)
}

impl super::Agent {
    /// The kind of the main turn about to be sent (consumes the per-step
    /// classification recorded by the last tool batch).
    pub(super) fn next_turn_workload(&mut self) -> TurnWorkload {
        let after_step = self.pending_turn_workload.take();
        resolve_turn_workload(self.review_phase(), after_step)
    }

    /// Count a sent main turn under its kind (run summary).
    pub(super) fn record_turn_workload(&mut self, kind: TurnWorkload, completion: Option<u32>) {
        self.workload_turns.record(kind, completion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_is_mechanical() {
        let calls = [
            ("file_read", r#"{"path":"src/lib.rs"}"#),
            ("grep_search", r#"{"pattern":"fn main"}"#),
            ("glob_find", r#"{"pattern":"*.rs"}"#),
        ];
        assert_eq!(
            workload_after_tool_batch(calls.iter().map(|(n, a)| (*n, *a))),
            TurnWorkload::Mechanical
        );
    }

    #[test]
    fn any_mutation_or_verification_is_edit() {
        for other in [
            ("file_edit", r#"{"path":"a","old_str":"x","new_str":"y"}"#),
            ("file_write", r#"{"path":"a","content":"x"}"#),
            ("cargo_test", "{}"),
            ("cargo_check", "{}"),
            ("shell_exec", r#"{"command":"ls"}"#),
        ] {
            let calls = [("file_read", r#"{"path":"a"}"#), other];
            assert_eq!(
                workload_after_tool_batch(calls.iter().map(|(n, a)| (*n, *a))),
                TurnWorkload::Edit,
                "{other:?}"
            );
        }
    }

    #[test]
    fn no_tools_is_synthesis() {
        assert_eq!(
            workload_after_tool_batch(std::iter::empty()),
            TurnWorkload::Synthesis
        );
    }

    #[test]
    fn review_synthesis_phase_outranks_the_step_classification() {
        use super::super::ReviewPhase;
        let m = Some(TurnWorkload::Mechanical);
        assert_eq!(
            resolve_turn_workload(Some(ReviewPhase::Synthesis), m),
            TurnWorkload::Synthesis
        );
        assert_eq!(
            resolve_turn_workload(Some(ReviewPhase::Reading), m),
            TurnWorkload::Mechanical
        );
        assert_eq!(resolve_turn_workload(None, m), TurnWorkload::Mechanical);
        assert_eq!(
            resolve_turn_workload(Some(ReviewPhase::Reading), None),
            TurnWorkload::Synthesis
        );
    }

    #[test]
    fn counts_accumulate_in_display_order() {
        let mut c = WorkloadTurnCounts::default();
        c.record(TurnWorkload::Synthesis, Some(900));
        c.record(TurnWorkload::Mechanical, Some(100));
        c.record(TurnWorkload::Mechanical, None);
        c.record(TurnWorkload::Planning, Some(50));
        assert_eq!(
            c.rows,
            vec![
                (TurnWorkload::Planning, 1, 50),
                (TurnWorkload::Mechanical, 2, 100),
                (TurnWorkload::Synthesis, 1, 900),
            ]
        );
        c.record_escalation(TurnWorkload::Mechanical);
        c.record_escalation(TurnWorkload::Mechanical);
        assert_eq!(c.escalations(TurnWorkload::Mechanical), 2);
        assert_eq!(c.escalations(TurnWorkload::Edit), 0);
    }
}
