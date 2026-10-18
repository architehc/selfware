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

#[test]
fn rust_phi_policy_twin_matches_lean_formal_table_and_invariants() {
    use super::policy::derive_fox_posture_from_counts;

    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/harness_twin_table.json"
    ));
    let rows: Vec<[String; 3]> = serde_json::from_str(raw).expect("valid harness_twin_table.json");

    let parse_phase = |s: &str| -> ActivityPhase {
        match s {
            "running" => ActivityPhase::Running,
            "completed" => ActivityPhase::Completed,
            "partial" => ActivityPhase::Partial,
            "failed" => ActivityPhase::Failed,
            "abandoned" => ActivityPhase::Abandoned,
            other => panic!("unknown ActivityPhase in formal table: {other}"),
        }
    };

    let emotion_name = |e: FoxEmotion| -> &'static str {
        match e {
            FoxEmotion::Greeting => "greeting",
            FoxEmotion::Working => "working",
            FoxEmotion::Error => "error",
            FoxEmotion::Flow => "flow",
            FoxEmotion::Skeptical => "skeptical",
            FoxEmotion::Unimpressed => "unimpressed",
            other => panic!("unexpected FoxEmotion: {other:?}"),
        }
    };

    let policy = PhiPolicy {
        debt_gate_lines: 100,
        ..PhiPolicy::default()
    };

    let mut checked_posture = 0usize;
    for [domain, phase_str, expected] in &rows {
        let Some(state_label) = domain.strip_prefix("phi_posture:") else {
            continue;
        };
        let (outstanding, unreviewed_lines, passed_runs) = match state_label {
            "clean_verified" => (0usize, 0usize, 1usize),
            "clean_unverified" => (0usize, 0usize, 0usize),
            "medium_debt" => (1usize, 55usize, 0usize),
            "high_debt" => (1usize, 85usize, 0usize),
            other => panic!("unknown phi_posture state label: {other}"),
        };
        let phase = parse_phase(phase_str);
        let posture = derive_fox_posture_from_counts(
            outstanding,
            unreviewed_lines,
            0,
            0,
            passed_runs,
            phase,
            &policy,
            PresenceChannel::Ambient,
        );
        assert_eq!(
            emotion_name(posture.emotion),
            expected,
            "Lean PT3 mismatch for ({domain}, {phase_str})"
        );
        checked_posture += 1;
    }
    assert_eq!(
        checked_posture, 20,
        "all 20 PT3 posture rows must be checked"
    );

    // PT5: Check all 6 `phi_steward` proposal priority rows exported from Lean.
    let kind_name = |k: Option<ProposalKind>| -> &'static str {
        match k {
            Some(ProposalKind::Repair) => "repair",
            Some(ProposalKind::Verify) => "verify",
            Some(ProposalKind::Repay) => "repay",
            Some(ProposalKind::Orient) => "orient",
            Some(ProposalKind::Explore) => "explore",
            None => "none",
        }
    };

    let mut checked_steward = 0usize;
    for [domain, scenario, expected] in &rows {
        if domain != "phi_steward" {
            continue;
        }
        let steward = PhiSteward::new(4);
        let mut ledger = Ledger::new();
        let mut observations = Vec::new();

        match scenario.as_str() {
            "failed_run_plus_debt" => {
                ledger.record_change(PathBuf::from("src/a.rs"), Some(20), 1, 1000);
                observations.push(ObservationRecord {
                    turn: 1,
                    tool: "cargo_test".to_string(),
                    command: Some("cargo test --lib".to_string()),
                    kind: "run_finished".to_string(),
                    outcome: Some("Failed".to_string()),
                    may_have_mutated: true,
                    reason: None,
                    call_id: None,
                });
            }
            "broken_removal_plus_debt" => {
                ledger.record_change(PathBuf::from("src/a.rs"), Some(20), 1, 1000);
                ledger.record_deletion(PathBuf::from("src/old.rs"), None, 2, 2000);
            }
            "unreviewed_plus_opaque" => {
                ledger.record_change(PathBuf::from("src/a.rs"), Some(20), 1, 1000);
                observations.push(ObservationRecord {
                    turn: 1,
                    tool: "shell_exec".to_string(),
                    command: Some("sed -i '' 's/a/b/' src/a.rs".to_string()),
                    kind: "run_finished".to_string(),
                    outcome: Some("Passed".to_string()),
                    may_have_mutated: true,
                    reason: None,
                    call_id: None,
                });
            }
            "untested_plus_opaque" => {
                ledger.record_change(PathBuf::from("src/a.rs"), Some(20), 1, 1000);
                let snap = ledger.snapshot();
                ledger.record_human_review(
                    snap,
                    vec![PathBuf::from("src/a.rs")],
                    Outcome::Passed,
                    None,
                    1500,
                );
                observations.push(ObservationRecord {
                    turn: 2,
                    tool: "shell_exec".to_string(),
                    command: Some("touch build.stamp".to_string()),
                    kind: "run_finished".to_string(),
                    outcome: Some("Passed".to_string()),
                    may_have_mutated: true,
                    reason: None,
                    call_id: None,
                });
            }
            "opaque_only" => {
                observations.push(ObservationRecord {
                    turn: 1,
                    tool: "shell_exec".to_string(),
                    command: Some("touch build.stamp".to_string()),
                    kind: "run_finished".to_string(),
                    outcome: Some("Passed".to_string()),
                    may_have_mutated: true,
                    reason: None,
                    call_id: None,
                });
            }
            "all_clear" => {}
            other => panic!("unknown phi_steward scenario in formal table: {other}"),
        }

        let top_kind = steward
            .propose(&ledger, &observations)
            .first()
            .map(|p| p.kind);
        assert_eq!(
            kind_name(top_kind),
            expected,
            "Lean PT5 steward mismatch for scenario `{scenario}`"
        );
        checked_steward += 1;
    }
    assert_eq!(checked_steward, 6, "all 6 PT5 steward rows must be checked");
}
