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
        result: &str,
    ) {
        let parsed: serde_json::Value =
            serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
        let turn_index = self.loop_control.current_iteration();
        let record = ToolCallRecord {
            tool: name,
            arguments: &parsed,
            turn_index,
            succeeded: success,
            output: Some(result),
        };
        let events = classify(&record);
        if events.is_empty() {
            self.publish_phi_activity(crate::phi::activity::ActivityPhase::Running);
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
                .outstanding_lines(crate::phi::ledger::ObligationKind::UnconfirmedCoverage),
            "phi ledger: recorded"
        );
        self.publish_phi_activity(crate::phi::activity::ActivityPhase::Running);
    }

    /// Publish bounded, content-free evidence to Phi. Commands, source paths,
    /// prompts, and diagnostic messages stay out of this cross-process receipt.
    pub(super) fn publish_phi_activity(&mut self, phase: crate::phi::activity::ActivityPhase) {
        if self.config.agent.disable_turn_artifacts {
            return;
        }
        if std::env::var("SELFWARE_PHI_MODE").is_ok() {
            self.phi_mediator.policy.mode = crate::phi::policy::PhiPolicy::from_env().mode;
        }
        let policy = self.phi_mediator.policy.clone();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let is_running = matches!(phase, crate::phi::activity::ActivityPhase::Running);
        self.phi_presence.set_busy(is_running);
        if is_running {
            self.phi_presence.note_activity(now_ms);
        }

        let mut channel = crate::phi::policy::PresenceChannel::Ambient;
        if policy.mode >= crate::phi::policy::PhiPolicyMode::Advise && !is_running {
            let steward = crate::phi::policy::PhiSteward::new(policy.max_proposals);
            let proposals = steward.propose(&self.evidence_ledger, &self.ledger_journal);
            if let Some(top) = proposals.first() {
                let severity = match top.kind {
                    crate::phi::policy::ProposalKind::Repair
                    | crate::phi::policy::ProposalKind::Verify => {
                        crate::phi::policy::SignalSeverity::High
                    }
                    crate::phi::policy::ProposalKind::Repay => {
                        crate::phi::policy::SignalSeverity::Medium
                    }
                    _ => crate::phi::policy::SignalSeverity::Low,
                };
                channel = self.phi_presence.route(&top.id, severity, now_ms);
            }
            let posture = crate::phi::policy::derive_fox_posture(
                &self.evidence_ledger,
                phase,
                &policy,
                channel,
            );
            tracing::debug!(
                emotion = ?posture.emotion,
                debt_ratio = posture.debt_ratio,
                channel = ?posture.channel,
                note = ?posture.note,
                proposals = proposals.len(),
                "phi fox posture and steward proposals updated"
            );
        } else {
            let posture = crate::phi::policy::derive_fox_posture(
                &self.evidence_ledger,
                phase,
                &policy,
                channel,
            );
            tracing::debug!(
                emotion = ?posture.emotion,
                debt_ratio = posture.debt_ratio,
                channel = ?posture.channel,
                note = ?posture.note,
                "phi fox posture updated"
            );
        }

        let (Some(capture), Some(checkpoint)) = (&self.phi_activity, &self.current_checkpoint)
        else {
            return;
        };
        let mut evidence = self.evidence_snapshot();
        let truncated = evidence.observations.len() > 256 || evidence.unattributed.len() > 256;
        // Keep the earliest evidence too: a later success must not erase an
        // earlier failed run. The explicit truncation flag makes this a floor.
        evidence.observations.truncate(256);
        evidence.unattributed.truncate(256);
        evidence.citations.clear();
        for record in &mut evidence.observations {
            record.command = None;
            record.reason = None;
            record.call_id = None;
        }
        for record in &mut evidence.unattributed {
            record.reason.clear();
            record.call_id = None;
        }
        match serde_json::to_value(evidence)
            .map_err(std::io::Error::other)
            .and_then(|evidence| capture.write(&checkpoint.task_id, phase, evidence, truncated))
        {
            Ok(()) => {
                self.phi_activity_terminal_task =
                    if matches!(phase, crate::phi::activity::ActivityPhase::Running) {
                        None
                    } else {
                        Some(checkpoint.task_id.clone())
                    };
            }
            Err(error) => tracing::warn!(%error, "Phi activity receipt could not be written"),
        }
    }

    /// Evaluate inbound model response annotations (`Capitulation`, `UnsourcedClaim`,
    /// `ScopeCreep`) and, when `SELFWARE_PHI_MODE=mediate` or `enforce`, inject
    /// explicit `[Phi — loop constraints]` before the next turn.
    pub(super) fn apply_phi_turn_mediation(&mut self, response_text: &str) {
        if std::env::var("SELFWARE_PHI_MODE").is_ok() {
            self.phi_mediator.policy.mode = crate::phi::policy::PhiPolicy::from_env().mode;
        }
        let policy = self.phi_mediator.policy.clone();

        let after_pushback =
            self.messages.iter().rev().take(4).any(|m| {
                m.role == "user" && m.content.text().contains("<selfware_system_directive>")
            });
        let generated_lines = response_text.lines().count();
        let annotations =
            self.phi_mediator
                .evaluate_inbound(response_text, after_pushback, generated_lines);
        if annotations
            .iter()
            .any(|a| a.kind == crate::phi::policy::AnnotationKind::Capitulation)
        {
            self.phi_capitulations = self.phi_capitulations.saturating_add(1);
        }
        if policy.mode >= crate::phi::policy::PhiPolicyMode::Advise && !annotations.is_empty() {
            tracing::info!(
                annotations = ?annotations,
                capitulations = self.phi_capitulations,
                "phi inbound annotations recorded"
            );
        }

        if policy.mode < crate::phi::policy::PhiPolicyMode::Mediate {
            return;
        }
        let turn = self
            .loop_control
            .turns_run()
            .max(self.loop_control.current_iteration())
            .max(1);
        if turn <= self.phi_last_mediation_turn {
            return;
        }
        let mediation =
            self.phi_mediator
                .evaluate_outbound("", &self.evidence_ledger, self.phi_capitulations);
        let mut lines: Vec<String> = mediation
            .augmentations
            .iter()
            .map(|aug| format!("- ({:?}) {}: {}", aug.kind, aug.reason, aug.text))
            .collect();
        if policy.mode >= crate::phi::policy::PhiPolicyMode::Enforce {
            if let Some(gate) = &mediation.gate {
                lines.push(format!(
                    "- (Gate {}) {}: {} Read and verify the modified files before generating further changes.",
                    gate.id, gate.reason, gate.question
                ));
                self.phi_mediator.override_gate(&gate.id);
            }
        }
        if !lines.is_empty() {
            self.phi_last_mediation_turn = turn;
            let directive = format!(
                "<selfware_system_directive>\n\
                 [Phi — loop constraints]\n\
                 {}\n\
                 </selfware_system_directive>",
                lines.join("\n")
            );
            tracing::info!(
                turn,
                constraints = lines.len(),
                "phi loop constraints injected"
            );
            self.messages
                .push(crate::api::types::Message::user(directive));
        }
    }

    /// Deep errors can bypass record_task_outcome. Close their running receipt
    /// without replacing an already published Partial/Abandoned/Failed outcome.
    pub(super) fn publish_phi_failure_if_unfinished(&mut self) {
        let needs_terminal = self.current_checkpoint.as_ref().is_some_and(|checkpoint| {
            self.phi_activity_terminal_task.as_deref() != Some(checkpoint.task_id.as_str())
        });
        if needs_terminal {
            self.publish_phi_activity(crate::phi::activity::ActivityPhase::Failed);
        }
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
        let workdir = crate::tools::workspace_root::current_path();
        // Pinned like every other write under .selfware/: a checkout shipping
        // `turns/evidence.jsonl` as a symlink must not get the line appended
        // to the link's target.
        let Ok(dir) = super::scratch_dir::ScratchDir::open(
            &workdir,
            &[super::turn_artifacts::TURNS_SUBDIR],
            true,
        ) else {
            return;
        };
        let policy = crate::phi::policy::PhiPolicy::from_env();
        let fox_posture = crate::phi::policy::derive_fox_posture(
            &self.evidence_ledger,
            crate::phi::activity::ActivityPhase::Running,
            &policy,
            crate::phi::policy::PresenceChannel::Ambient,
        );
        let steward_proposals = crate::phi::policy::PhiSteward::new(policy.max_proposals)
            .propose(&self.evidence_ledger, &self.ledger_journal);
        let payload = serde_json::json!({
            "turn": self.loop_control.current_iteration(),
            "phase": "post_execution",
            "recorded_at_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            "evidence": self.evidence_snapshot(),
            "fox_posture": fox_posture,
            "steward_proposals": steward_proposals,
            "phi_capitulations": self.phi_capitulations,
        });
        // Appended as JSONL rather than written to a per-turn filename: the
        // iteration counter is not the artifact sequence, so a filename keyed
        // on it silently overwrote a previous record whenever the two diverged.
        let Ok(line) = serde_json::to_string(&payload) else {
            return;
        };
        let _ = dir.append("evidence.jsonl", format!("{line}\n").as_bytes());
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
                .outstanding_lines(ObligationKind::UnconfirmedCoverage),
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

    #[cfg(unix)]
    async fn activity_agent(root: &std::path::Path) -> super::Agent {
        let config = crate::config::Config {
            endpoint: "http://127.0.0.1:1/v1".into(),
            model: "offline-test".into(),
            max_tokens: 1024,
            context_length: 32768,
            ..Default::default()
        };
        let mut agent = super::Agent::new(config).await.unwrap();
        agent.config.agent.disable_turn_artifacts = false;
        agent.phi_activity = Some(
            crate::phi::activity::ActivityCapture::new(root, "audit-session", "agent").unwrap(),
        );
        agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
            "task-one".into(),
            "offline task".into(),
        ));
        agent
    }

    #[cfg(unix)]
    fn receipt_phase(root: &std::path::Path, task_id: &str) -> String {
        std::fs::read_dir(root.join(".selfware/phi/activity"))
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_name() == ".lock"
                    || entry.path().extension().and_then(|s| s.to_str()) != Some("json")
                {
                    return None;
                }
                let value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
                (value["task_id"] == task_id).then(|| value["phase"].as_str().unwrap().to_owned())
            })
            .next()
            .unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unrecorded_loop_error_closes_running_phi_receipt() {
        let root = tempfile::tempdir().unwrap();
        let mut agent = activity_agent(root.path()).await;
        agent.publish_phi_activity(crate::phi::activity::ActivityPhase::Running);
        assert_eq!(receipt_phase(root.path(), "task-one"), "running");
        agent.publish_phi_failure_if_unfinished();
        assert_eq!(receipt_phase(root.path(), "task-one"), "failed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn loop_error_preserves_partial_phi_receipt_and_does_not_leak_across_tasks() {
        let root = tempfile::tempdir().unwrap();
        let mut agent = activity_agent(root.path()).await;
        agent.publish_phi_activity(crate::phi::activity::ActivityPhase::Partial);
        agent.publish_phi_failure_if_unfinished();
        assert_eq!(receipt_phase(root.path(), "task-one"), "partial");
        agent.current_checkpoint.as_mut().unwrap().task_id = "task-two".into();
        agent.publish_phi_failure_if_unfinished();
        assert_eq!(receipt_phase(root.path(), "task-one"), "partial");
        assert_eq!(receipt_phase(root.path(), "task-two"), "failed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn resumed_same_task_can_publish_a_new_failed_phi_attempt() {
        let root = tempfile::tempdir().unwrap();
        let mut agent = activity_agent(root.path()).await;
        agent.publish_phi_activity(crate::phi::activity::ActivityPhase::Partial);
        agent.publish_phi_activity(crate::phi::activity::ActivityPhase::Running);
        agent.publish_phi_failure_if_unfinished();
        assert_eq!(receipt_phase(root.path(), "task-one"), "failed");
    }

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

    #[cfg(unix)]
    #[tokio::test]
    async fn phi_turn_mediation_records_capitulation_and_injects_loop_constraints() {
        let root = tempfile::tempdir().unwrap();
        let mut agent = activity_agent(root.path()).await;
        agent.phi_mediator.policy.mode = crate::phi::policy::PhiPolicyMode::Enforce;
        agent.phi_mediator.policy.unreviewed_evidence_threshold = 2;
        agent.phi_mediator.policy.debt_narrow_scope_lines = 40;
        agent.phi_mediator.policy.debt_gate_lines = 80;
        agent.phi_mediator.policy.capitulation_threshold = 1;

        agent.evidence_ledger.record_change(
            std::path::PathBuf::from("src/a.rs"),
            Some(50),
            1,
            1000,
        );
        agent.evidence_ledger.record_change(
            std::path::PathBuf::from("src/b.rs"),
            Some(50),
            1,
            1001,
        );
        agent.messages.push(crate::api::types::Message::user(
            "<selfware_system_directive>\nCheck your edits.\n</selfware_system_directive>"
                .to_string(),
        ));
        agent.loop_control.record_turn();

        let before_len = agent.messages.len();
        agent.apply_phi_turn_mediation("You're absolutely right, my mistake — that should work.");
        assert_eq!(agent.phi_capitulations, 1);
        assert_eq!(agent.messages.len(), before_len + 1);
        let injected = agent.messages.last().unwrap().content.text();
        assert!(injected.contains("[Phi — loop constraints]"));
        assert!(injected.contains("DemandDisproof"));
        assert!(injected.contains("RequireEvidence"));
        assert!(injected.contains("NarrowScope"));
        assert!(injected.contains("Gate gate:debt"));
    }
}
