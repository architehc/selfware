//! Shadow-mode ledger observation, wired to tool dispatch.
//!
//! One function, called from every path that executes a tool. Tools run from
//! two places — `execute_parallel_tools` and `execute_single_tool_in_batch` —
//! and a previous fix to a two-caller bug class patched one caller and shipped.
//! `phi_observation_tests` asserts both PostToolUse sites call this, so a third
//! dispatch path cannot be added without either wiring it or failing tests.
//!
//! Strictly observational. It records; it returns nothing anyone branches on.

use super::Agent;
use crate::phi::ledger::RunSnapshot;
use crate::phi::observer::{apply, classify, unattributed_count, ToolCallRecord};

impl Agent {
    /// Ledger position before a batch of tools runs.
    ///
    /// Taken per batch, not per tool: tools in a parallel batch have no
    /// meaningful order relative to each other, so a verification run in the
    /// same batch as an edit must be treated as having started before that
    /// edit. That is the conservative reading, and the ledger will decline to
    /// discharge on it.
    pub(super) fn ledger_batch_snapshot(&self) -> RunSnapshot {
        self.evidence_ledger.snapshot()
    }

    /// Record one executed tool call against the evidence ledger.
    ///
    /// `args_str` is the raw argument text as dispatched; unparseable arguments
    /// are treated as an unattributable mutation rather than as no mutation.
    pub(super) fn observe_tool_call(
        &mut self,
        name: &str,
        args_str: &str,
        success: bool,
        snapshot: RunSnapshot,
        call_id: Option<&str>,
    ) {
        let parsed: serde_json::Value =
            serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
        let turn_index = self.loop_control.current_iteration();
        let record = ToolCallRecord {
            tool: name,
            arguments: &parsed,
            turn_index,
            succeeded: success,
            output: None,
        };
        let events = classify(&record);
        if events.is_empty() {
            return;
        }
        let unattributed = unattributed_count(&events);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        apply(&mut self.evidence_ledger, &events, snapshot, now_ms);

        // Journal EVERYTHING observable, including the events the ledger cannot
        // hold. An OpaqueRun that is classified and then dropped leaves a shell
        // mutation with no trace, and the session log then claims nothing
        // happened.
        for event in &events {
            let record = match event {
                crate::phi::observer::ObservedEvent::RunFinished {
                    command, outcome, ..
                } => Some(crate::phi::observer::ObservationRecord {
                    turn: turn_index,
                    tool: name.to_string(),
                    command: Some(command.clone()),
                    kind: "run_finished".to_string(),
                    outcome: Some(format!("{outcome:?}")),
                    may_have_mutated: false,
                    reason: None,
                    call_id: call_id.map(str::to_string),
                }),
                crate::phi::observer::ObservedEvent::OpaqueRun {
                    command,
                    outcome,
                    may_have_mutated,
                } => Some(crate::phi::observer::ObservationRecord {
                    turn: turn_index,
                    tool: name.to_string(),
                    command: Some(command.clone()),
                    kind: "opaque_run".to_string(),
                    outcome: Some(format!("{outcome:?}")),
                    may_have_mutated: *may_have_mutated,
                    reason: None,
                    call_id: call_id.map(str::to_string),
                }),
                crate::phi::observer::ObservedEvent::Unattributed { tool, reason } => {
                    Some(crate::phi::observer::ObservationRecord {
                        turn: turn_index,
                        tool: tool.clone(),
                        command: None,
                        kind: "unattributed".to_string(),
                        outcome: None,
                        may_have_mutated: true,
                        reason: Some(reason.clone()),
                        call_id: call_id.map(str::to_string),
                    })
                }
                _ => None,
            };
            if let Some(record) = record {
                self.ledger_journal.push(record);
            }
        }

        if unattributed > 0 {
            // Persisted, not just logged. A debug line cannot be compared
            // against a saved session, and the whole reason this count exists
            // is to find out whether the classifier's schema assumptions hold.
            for event in &events {
                if let crate::phi::observer::ObservedEvent::Unattributed { tool, reason } = event {
                    self.ledger_unattributed
                        .push(crate::phi::observer::UnattributedRecord {
                            turn: turn_index,
                            tool: tool.clone(),
                            reason: reason.clone(),
                            call_id: call_id.map(str::to_string),
                        });
                }
            }
        }
        tracing::debug!(
            tool = name,
            outstanding = self.evidence_ledger.outstanding().len(),
            unreviewed_lines = self
                .evidence_ledger
                .outstanding_lines(crate::phi::ledger::ObligationKind::UnreviewedChange),
            untested_lines = self
                .evidence_ledger
                .outstanding_lines(crate::phi::ledger::ObligationKind::UntestedLogic),
            "phi ledger: recorded"
        );
    }

    /// Append a post-execution evidence record for this turn.
    ///
    /// The main turn artifact is written before the turn's tools run, so its
    /// evidence cannot include them. This is written afterwards and named
    /// separately so the two are never confused.
    pub(super) async fn write_post_execution_evidence(&self) {
        // Honour the same capture switch as the main artifact; ignoring it
        // wrote files a user had explicitly turned off.
        if self.config.agent.disable_turn_artifacts {
            return;
        }
        let workdir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let dir = super::turn_artifacts::artifact_dir(&workdir);
        if tokio::fs::create_dir_all(&dir).await.is_err() {
            return;
        }
        let payload = serde_json::json!({
            "turn": self.loop_control.current_iteration(),
            "phase": "post_execution",
            "recorded_at_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            "evidence": self.evidence_snapshot(),
        });
        // Appended as JSONL rather than written to a per-turn filename: the
        // iteration counter is not the artifact sequence, so a filename keyed
        // on it silently overwrote a previous record whenever the two diverged.
        let path = dir.join("evidence.jsonl");
        let Ok(line) = serde_json::to_string(&payload) else {
            return;
        };
        use tokio::io::AsyncWriteExt;
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
        {
            let _ = file.write_all(format!("{line}\n").as_bytes()).await;
        }
    }

    /// The ledger summary written into turn artifacts.
    pub(crate) fn evidence_snapshot(&self) -> super::turn_artifacts::EvidenceSnapshot {
        use crate::phi::ledger::ObligationKind;
        super::turn_artifacts::EvidenceSnapshot {
            outstanding: self.evidence_ledger.outstanding().len(),
            unreviewed_lines: self
                .evidence_ledger
                .outstanding_lines(ObligationKind::UnreviewedChange),
            untested_lines: self
                .evidence_ledger
                .outstanding_lines(ObligationKind::UntestedLogic),
            unknown_size_obligations: self
                .evidence_ledger
                .outstanding_unknown_size(ObligationKind::UnreviewedChange),
            unattributed: self.ledger_unattributed.clone(),
            observations: self.ledger_journal.clone(),
            possible_unrecorded_mutations: self
                .ledger_journal
                .iter()
                .filter(|r| r.may_have_mutated)
                .count(),
            citations: self.evidence_ledger.citations(),
        }
    }
}

#[cfg(test)]
mod phi_observation_tests {
    //! Source-level sweep: both dispatch paths must observe.
    //!
    //! A previous two-caller bug was "fixed" by patching one caller, so this
    //! checks the wiring itself rather than trusting that it was done. It reads
    //! the dispatch source, which is crude but catches the exact regression:
    //! adding a third execution path, or deleting one of these calls.

    const DISPATCH: &str = include_str!("tool_dispatch/mod.rs");

    #[test]
    fn every_post_tool_hook_site_also_observes() {
        let hook_sites = DISPATCH.matches("HookContext::post_tool(").count();
        let observations = DISPATCH.matches("self.observe_tool_call(").count();
        assert!(
            hook_sites > 1,
            "sanity: there should be more than one dispatch path"
        );
        assert_eq!(
            observations, hook_sites,
            "every path that executes a tool must record it: {hook_sites} PostToolUse \
             site(s) but {observations} ledger observation(s). A path that runs tools \
             without observing reports zero debt for real mutations."
        );
    }

    #[test]
    fn the_snapshot_is_taken_before_execution_not_merely_before_observation() {
        // The previous version compared the snapshot against observe_tool_call,
        // so moving it AFTER execution but before observation would still have
        // passed — while silently letting a run claim to cover edits that
        // landed during it. Anchor on execution instead.
        for (name, header, executes) in [
            (
                "execute_parallel_tools",
                "async fn execute_parallel_tools",
                "run_tool_bounded(",
            ),
            (
                "execute_single_tool_in_batch",
                "async fn execute_single_tool_in_batch",
                "let start_time = std::time::Instant::now();",
            ),
        ] {
            let start = DISPATCH
                .find(header)
                .unwrap_or_else(|| panic!("{name} not found; this sweep needs updating"));
            let rest = &DISPATCH[start..];
            let snapshot = rest
                .find("self.ledger_batch_snapshot()")
                .unwrap_or_else(|| panic!("{name} never takes a ledger snapshot"));
            let execution = rest
                .find(executes)
                .unwrap_or_else(|| panic!("{name}: execution anchor {executes:?} not found"));
            assert!(
                snapshot < execution,
                "{name} must snapshot before it executes anything, not merely \
                 before it observes"
            );
        }
    }
}
