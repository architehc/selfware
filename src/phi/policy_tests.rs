use std::path::PathBuf;

use super::activity::ActivityPhase;
use super::ledger::{Ledger, Outcome, Scope};
use super::observer::ObservationRecord;
use super::policy::{
    derive_fox_posture, response_is_sourced, AnnotationKind, FoxEmotion, MediationKind,
    PhiMediator, PhiPolicy, PhiPresence, PhiSteward, PresenceChannel, ProposalKind, SignalSeverity,
};

#[test]
fn outbound_mediation_augments_and_gates_on_measured_ledger_debt() {
    let policy = PhiPolicy {
        unreviewed_evidence_threshold: 2,
        debt_narrow_scope_lines: 50,
        debt_gate_lines: 100,
        capitulation_threshold: 2,
        ..PhiPolicy::default()
    };
    let mut mediator = PhiMediator::new(policy);
    let mut ledger = Ledger::new();

    // Two unreviewed changes totaling 120 lines exceed both narrow_scope (50) and gate (100).
    ledger.record_change(PathBuf::from("src/a.rs"), Some(60), 1, 1000);
    ledger.record_change(PathBuf::from("src/b.rs"), Some(60), 2, 2000);

    let out = mediator.evaluate_outbound("Add another feature", &ledger, 2);
    assert_eq!(out.prompt, "Add another feature");
    assert!(out
        .augmentations
        .iter()
        .any(|a| a.kind == MediationKind::DemandDisproof));
    assert!(out
        .augmentations
        .iter()
        .any(|a| a.kind == MediationKind::RequireEvidence));
    assert!(out
        .augmentations
        .iter()
        .any(|a| a.kind == MediationKind::NarrowScope));
    assert!(out.gate.is_some());

    let composed = out.compose();
    assert!(composed.starts_with("Add another feature\n\n[Phi — loop constraints]\n"));

    // Overriding the gate suppresses it on subsequent calls in the same session.
    mediator.override_gate("gate:debt");
    let out_after_override = mediator.evaluate_outbound("Add another feature", &ledger, 2);
    assert!(out_after_override.gate.is_none());
}

#[test]
fn inbound_annotations_detect_unsourced_capitulation_and_scope_creep() {
    let mediator = PhiMediator::new(PhiPolicy::default());

    let annotations = mediator.evaluate_inbound(
        "You're absolutely right! My mistake — this should work now.",
        true,
        450,
    );
    assert!(annotations
        .iter()
        .any(|a| a.kind == AnnotationKind::Capitulation));
    assert!(annotations
        .iter()
        .any(|a| a.kind == AnnotationKind::UnsourcedClaim));
    assert!(annotations
        .iter()
        .any(|a| a.kind == AnnotationKind::ScopeCreep));

    // When backed by a concrete file:line citation, capitulation and unsourced claim do not fire.
    assert!(response_is_sourced("Verified at src/phi/ledger.rs:142."));
    let grounded = mediator.evaluate_inbound(
        "Good catch — see src/phi/ledger.rs:142 where Scope::covers is checked.",
        true,
        40,
    );
    assert!(grounded.is_empty());
}

#[test]
fn steward_ranks_repair_above_verify_and_requires_evidence() {
    let steward = PhiSteward::new(4);
    let mut ledger = Ledger::new();

    // Empty ledger and empty observations -> zero proposals ("no evidence, no proposal").
    assert!(steward.propose(&ledger, &[]).is_empty());

    // Record an edit and a failed run: Repair (rank 0) must sort ahead of Verify (rank 1).
    ledger.record_change(PathBuf::from("src/lib.rs"), Some(30), 1, 1000);
    let failed_obs = vec![ObservationRecord {
        turn: 2,
        tool: "cargo_test".to_string(),
        command: Some("cargo test --lib".to_string()),
        kind: "run_finished".to_string(),
        outcome: Some("Failed".to_string()),
        may_have_mutated: false,
        reason: None,
        call_id: None,
    }];

    let proposals = steward.propose(&ledger, &failed_obs);
    assert!(proposals.len() >= 2);
    assert_eq!(proposals[0].kind, ProposalKind::Repair);
    assert_eq!(proposals[1].kind, ProposalKind::Verify);
}

#[test]
fn fox_posture_and_presence_economy_reflect_ledger_and_back_off_when_ignored() {
    let policy = PhiPolicy {
        debt_gate_lines: 100,
        quiet_after_activity_ms: 100,
        glance_cooldown_ms: 100,
        interrupt_budget: 2,
        ..PhiPolicy::default()
    };
    let mut ledger = Ledger::new();
    let clean_posture = derive_fox_posture(
        &ledger,
        ActivityPhase::Completed,
        &policy,
        PresenceChannel::Ambient,
    );
    assert_eq!(clean_posture.emotion, FoxEmotion::Greeting);

    // High unverified debt shifts Fox posture to Unimpressed even if phase is Completed.
    ledger.record_change(PathBuf::from("src/main.rs"), Some(80), 1, 1000);
    let debt_posture = derive_fox_posture(
        &ledger,
        ActivityPhase::Completed,
        &policy,
        PresenceChannel::Ambient,
    );
    assert_eq!(debt_posture.emotion, FoxEmotion::Unimpressed);

    // Discharging both review and test obligations transitions Completed posture to Flow.
    let snap = ledger.snapshot();
    ledger.record_test_run(
        snap,
        Scope::Paths(vec![PathBuf::from("src/main.rs")]),
        Outcome::Passed,
        None,
        2000,
    );
    ledger.record_human_review(
        snap,
        vec![PathBuf::from("src/main.rs")],
        Outcome::Passed,
        None,
        3000,
    );
    let flow_posture = derive_fox_posture(
        &ledger,
        ActivityPhase::Completed,
        &policy,
        PresenceChannel::Ambient,
    );
    assert_eq!(flow_posture.emotion, FoxEmotion::Flow);

    // Presence attention economy: High severity needs 2 observations before Glance, then Interrupt.
    let mut presence = PhiPresence::new(&policy);
    presence.note_activity(0);
    assert_eq!(
        presence.route("debt", SignalSeverity::High, 200),
        PresenceChannel::Ambient
    );
    assert_eq!(
        presence.route("debt", SignalSeverity::High, 400),
        PresenceChannel::Glance
    );
    // Ignoring raises the quota so the next observation stays Ambient.
    presence.ignore("debt");
    assert_eq!(
        presence.route("debt", SignalSeverity::High, 600),
        PresenceChannel::Ambient
    );
}
