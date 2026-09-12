//! Invariant tests for the evidence ledger.
//!
//! Each names the real failure it guards. The scalar these replace was cleared
//! by seventeen unrun tests and by a green suite in an unrelated module, so the
//! cases below are not hypothetical.

use super::ledger::*;
use std::path::PathBuf;

const T: u64 = 1_700_000_000_000;

fn ledger() -> Ledger {
    Ledger::new()
}

fn paths(p: &[&str]) -> Vec<PathBuf> {
    p.iter().map(PathBuf::from).collect()
}

#[test]
fn a_change_owes_both_a_read_and_a_test() {
    let mut l = ledger();
    l.record_change("src/a.rs", 40, 1, T);
    assert_eq!(l.outstanding().len(), 2);
    assert_eq!(l.outstanding_lines(ObligationKind::UnreviewedChange), 40);
    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 40);
}

#[test]
fn passing_tests_never_discharge_the_review_obligation() {
    // A green suite is not a read. This is the separation the scalar lost.
    let mut l = ledger();
    l.record_change("src/a.rs", 40, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::WorkspaceWithCoverage(paths(&["src/a.rs", "src/b.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 0);
    assert_eq!(
        l.outstanding_lines(ObligationKind::UnreviewedChange),
        40,
        "a test run cannot read the diff for you"
    );
}

#[test]
fn a_human_review_never_discharges_the_test_obligation() {
    let mut l = ledger();
    l.record_change("src/a.rs", 40, 1, T);
    let snap = l.snapshot();
    l.record_human_review(snap, paths(&["src/a.rs"]), Outcome::Passed, None, T);

    assert_eq!(l.outstanding_lines(ObligationKind::UnreviewedChange), 0);
    assert_eq!(
        l.outstanding_lines(ObligationKind::UntestedLogic),
        40,
        "reading the diff does not execute it"
    );
}

#[test]
fn an_unrelated_passing_suite_discharges_nothing() {
    // A green run in module A absolving unread code in module B was the
    // concrete bug this ledger exists to make impossible.
    let mut l = ledger();
    l.record_change("src/b.rs", 30, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::Paths(paths(&["src/a.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(l.outstanding().len(), 2, "module B still owes everything");
}

#[test]
fn a_run_that_started_before_the_change_proves_nothing_about_it() {
    // The staleness invariant: a test cannot have tested an edit that did not
    // exist when it started, however green it is.
    let mut l = ledger();
    let snap = l.snapshot(); // run begins
    l.record_change("src/a.rs", 20, 1, T); // edit lands mid-run
    l.record_test_run(
        snap,
        Scope::WorkspaceWithCoverage(paths(&["src/a.rs", "src/b.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(
        l.outstanding_lines(ObligationKind::UntestedLogic),
        20,
        "a result that predates the change must not discharge it"
    );
}

#[test]
fn a_run_that_raced_an_edit_is_stale_for_that_path() {
    // Edit -> run starts -> the same file is edited again -> run reports green.
    // It read some indeterminate mixture, so it settles nothing for that path.
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    let snap = l.snapshot();
    l.record_change("src/a.rs", 5, 2, T); // lands while the run is in flight
    l.record_test_run(
        snap,
        Scope::WorkspaceWithCoverage(paths(&["src/a.rs", "src/b.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    let untested = l.outstanding_lines(ObligationKind::UntestedLogic);
    assert_eq!(
        untested, 25,
        "both revisions of the raced path stay untested"
    );
}

#[test]
fn a_run_that_raced_an_edit_still_covers_untouched_paths() {
    // Staleness is per-path. A racing edit to a.rs must not invalidate the
    // run's verdict on b.rs.
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    l.record_change("src/b.rs", 10, 1, T);
    let snap = l.snapshot();
    l.record_change("src/a.rs", 5, 2, T);
    l.record_test_run(
        snap,
        Scope::WorkspaceWithCoverage(paths(&["src/a.rs", "src/b.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    let still_owed: Vec<_> = l
        .outstanding()
        .iter()
        .filter(|o| o.kind == ObligationKind::UntestedLogic)
        .map(|o| o.path.clone())
        .collect();
    assert!(still_owed.contains(&PathBuf::from("src/a.rs")));
    assert!(
        !still_owed.contains(&PathBuf::from("src/b.rs")),
        "an untouched path keeps its verdict"
    );
}

#[test]
fn a_failing_run_discharges_nothing() {
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::WorkspaceWithCoverage(paths(&["src/a.rs", "src/b.rs"])),
        Outcome::Failed,
        None,
        T,
    );
    assert_eq!(l.outstanding().len(), 2, "red establishes work remains");
}

#[test]
fn a_partial_review_discharges_only_what_was_read() {
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    l.record_change("src/b.rs", 30, 1, T);
    l.record_change("src/c.rs", 10, 1, T);
    let snap = l.snapshot();
    l.record_human_review(
        snap,
        paths(&["src/a.rs", "src/c.rs"]),
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(
        l.outstanding_lines(ObligationKind::UnreviewedChange),
        30,
        "only src/b.rs is still unread"
    );
}

#[test]
fn one_turn_touching_many_files_owes_for_each() {
    let mut l = ledger();
    for (path, lines) in [("src/a.rs", 10), ("src/b.rs", 20), ("src/c.rs", 30)] {
        l.record_change(path, lines, 7, T);
    }
    assert_eq!(l.outstanding().len(), 6);
    assert_eq!(l.outstanding_lines(ObligationKind::UnreviewedChange), 60);

    // Reviewing one file leaves the other two owing.
    let snap = l.snapshot();
    l.record_human_review(snap, paths(&["src/b.rs"]), Outcome::Passed, None, T);
    assert_eq!(l.outstanding_lines(ObligationKind::UnreviewedChange), 40);
}

#[test]
fn deleting_a_file_retires_its_debt_but_the_deletion_is_itself_a_change() {
    let mut l = ledger();
    l.record_change("src/gone.rs", 80, 1, T);
    assert_eq!(l.outstanding().len(), 2);

    l.record_deletion("src/gone.rs", 2, T);
    let out = l.outstanding();
    assert_eq!(
        out.len(),
        1,
        "nothing left to test, but somebody should read the removal"
    );
    assert_eq!(out[0].kind, ObligationKind::UnreviewedChange);
    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 0);
}

#[test]
fn recording_the_same_evidence_twice_changes_nothing() {
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    let snap = l.snapshot();
    l.record_human_review(snap, paths(&["src/a.rs"]), Outcome::Passed, None, T);
    let after_first = l.outstanding().len();

    l.record_human_review(snap, paths(&["src/a.rs"]), Outcome::Passed, None, T);
    assert_eq!(
        l.outstanding().len(),
        after_first,
        "satisfaction is idempotent"
    );

    // And the first discharge stands; it is not reattributed to the duplicate.
    let reviewed = l
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UnreviewedChange)
        .unwrap();
    assert_eq!(reviewed.satisfied_by, Some(EvidenceId(0)));
}

#[test]
fn a_replayed_session_reconstructs_the_same_ledger() {
    // Restart must not forgive debt, and must not double-count it.
    let build = || {
        let mut l = ledger();
        l.record_change("src/a.rs", 20, 1, T);
        l.record_change("src/b.rs", 30, 1, T);
        let snap = l.snapshot();
        l.record_test_run(
            snap,
            Scope::WorkspaceWithCoverage(paths(&["src/a.rs", "src/b.rs"])),
            Outcome::Passed,
            None,
            T,
        );
        l.record_human_review(snap, paths(&["src/a.rs"]), Outcome::Passed, None, T);
        l
    };
    assert_eq!(build(), build(), "replay is deterministic");

    let original = build();
    let round_tripped: Ledger =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    assert_eq!(
        original, round_tripped,
        "persistence preserves outstanding debt"
    );
    assert_eq!(
        round_tripped.outstanding_lines(ObligationKind::UnreviewedChange),
        30
    );
}

#[test]
fn the_reason_for_non_discharge_is_inspectable() {
    // An outstanding count nobody can explain is the failure mode of the
    // scalar. Every refusal names itself.
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::Paths(paths(&["src/other.rs"])),
        Outcome::Failed,
        None,
        T,
    );

    let evidence = l.evidence()[0].clone();
    let review = l
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UnreviewedChange)
        .unwrap();
    let test = l
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UntestedLogic)
        .unwrap();

    assert_eq!(l.assess(&evidence, review), Err(Unsatisfied::WrongKind));
    assert_eq!(l.assess(&evidence, test), Err(Unsatisfied::OutOfScope));
}

#[test]
fn citations_name_the_file_and_the_turn() {
    let mut l = ledger();
    l.record_change("src/agent/streaming.rs", 42, 3, T);
    let lines = l.citations();
    assert!(lines
        .iter()
        .any(|c| c.contains("src/agent/streaming.rs") && c.contains("turn 3")));
    assert!(lines.iter().any(|c| c.starts_with("unreviewed")));
    assert!(lines.iter().any(|c| c.starts_with("untested")));
}

#[test]
fn a_revision_and_checkpoint_can_be_attached() {
    let mut l = ledger();
    let ids = l.record_change("src/a.rs", 12, 1, T);
    l.annotate_revision(&ids, "sha256:abc123", Some(7));
    let o = &l.obligations()[0];
    assert_eq!(o.revision.as_deref(), Some("sha256:abc123"));
    assert_eq!(o.checkpoint, Some(7));
}

// ---------------------------------------------------------------------------
// Counterexamples.
//
// Each of these PASSED under the previous contract and should not have. They
// are the reason the contract changed, kept as tests so it cannot drift back.
// ---------------------------------------------------------------------------

#[test]
fn a_green_suite_with_no_coverage_data_discharges_nothing() {
    // Counterexample to the old `Scope::Workspace` meaning "everything". A file
    // with no test touching it passes the suite by being ignored; a green run
    // is evidence about the suite, not about that file.
    let mut l = ledger();
    l.record_change("src/untested.rs", 60, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::WorkspaceCoverageUnknown,
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(
        l.outstanding_lines(ObligationKind::UntestedLogic),
        60,
        "a suite that did not say what it exercised discharges nothing"
    );

    let evidence = l.evidence()[0].clone();
    let test = l
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UntestedLogic)
        .unwrap();
    assert_eq!(
        l.assess(&evidence, test),
        Err(Unsatisfied::CoverageUnknown),
        "and it must say it cannot tell, not that the path was out of scope"
    );
}

#[test]
fn unknown_coverage_is_distinct_from_definitely_not_covered() {
    // "I did not test that" and "I cannot say whether I tested that" are
    // different claims and must not collapse into one.
    let mut l = ledger();
    l.record_change("src/a.rs", 10, 1, T);
    let snap = l.snapshot();

    l.record_test_run(
        snap,
        Scope::Paths(paths(&["src/other.rs"])),
        Outcome::Passed,
        None,
        T,
    );
    l.record_test_run(
        snap,
        Scope::WorkspaceCoverageUnknown,
        Outcome::Passed,
        None,
        T,
    );

    let test = l
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UntestedLogic)
        .unwrap();
    assert_eq!(
        l.assess(&l.evidence()[0], test),
        Err(Unsatisfied::OutOfScope)
    );
    assert_eq!(
        l.assess(&l.evidence()[1], test),
        Err(Unsatisfied::CoverageUnknown)
    );
}

#[test]
fn a_reported_coverage_list_does_discharge_what_it_names() {
    // The flip side: a run that says what it exercised is usable evidence.
    let mut l = ledger();
    l.record_change("src/a.rs", 10, 1, T);
    l.record_change("src/b.rs", 20, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::WorkspaceWithCoverage(paths(&["src/a.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(
        l.outstanding_lines(ObligationKind::UntestedLogic),
        20,
        "only the covered path is discharged"
    );
}

#[test]
fn an_external_edit_makes_the_revision_unknown() {
    // Counterexample: the human edits the file in their editor after the agent
    // touched it. The ledger never saw that diff, so it does not know what is
    // in the file, and evidence taken over it settles nothing.
    let mut l = ledger();
    l.record_change("src/a.rs", 30, 1, T);
    l.record_external_change("src/a.rs", T);

    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::Paths(paths(&["src/a.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    let agent_change = l
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UntestedLogic && o.line_count == 30)
        .unwrap();
    assert_eq!(
        l.assess(&l.evidence()[0], agent_change),
        Err(Unsatisfied::RevisionUnknown),
        "a passing test over content the ledger cannot describe proves nothing"
    );
}

#[test]
fn an_external_edit_records_no_line_count_it_did_not_observe() {
    // Inventing a size for a diff nobody saw is the estimate-versus-measurement
    // failure in miniature.
    let mut l = ledger();
    let id = l.record_external_change("src/a.rs", T);
    let obligation = l.obligations().iter().find(|o| o.id == id).unwrap();
    assert_eq!(obligation.line_count, 0);
    assert_eq!(obligation.revision, None, "no revision is claimed");
    assert!(
        obligation.outstanding(),
        "but somebody still has to read it"
    );
}

#[test]
fn a_human_review_that_found_problems_discharges_nothing() {
    // A review is not a rubber stamp. Recording its outcome as always-passed
    // made "I looked and it is wrong" indistinguishable from "I looked and it
    // is fine".
    let mut l = ledger();
    l.record_change("src/a.rs", 25, 1, T);
    let snap = l.snapshot();
    l.record_human_review(snap, paths(&["src/a.rs"]), Outcome::Failed, None, T);

    assert_eq!(
        l.outstanding_lines(ObligationKind::UnreviewedChange),
        25,
        "a review that rejected the change has not discharged it"
    );
}

#[test]
fn a_change_to_a_dependency_does_not_invalidate_evidence_about_its_dependents() {
    // KNOWN GAP, pinned deliberately.
    //
    // b.rs depends on a.rs. A test covering b.rs passes, then a.rs changes. In
    // reality that may well have broken b. The ledger has no dependency graph
    // and does not guess at one, so b's evidence survives.
    //
    // This test exists so the limitation is visible and deliberate rather than
    // discovered later as a silent wrong answer. If dependency data is wired in,
    // this test should FAIL and be rewritten — that is the intended signal.
    let mut l = ledger();
    l.record_change("src/b.rs", 10, 1, T);
    let snap = l.snapshot();
    l.record_test_run(
        snap,
        Scope::Paths(paths(&["src/b.rs"])),
        Outcome::Passed,
        None,
        T,
    );
    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 0);

    l.record_change("src/a.rs", 5, 2, T); // b's dependency moves underneath it

    assert_eq!(
        l.outstanding_lines(ObligationKind::UntestedLogic),
        5,
        "only a.rs owes; b.rs keeps a verdict that may no longer hold"
    );
}

#[test]
fn deleting_a_file_does_not_discharge_other_paths() {
    // Retirement is per-path. Deleting a.rs must not quietly absolve b.rs.
    let mut l = ledger();
    l.record_change("src/a.rs", 10, 1, T);
    l.record_change("src/b.rs", 20, 1, T);
    l.record_deletion("src/a.rs", 2, T);

    let owed: Vec<_> = l.outstanding().iter().map(|o| o.path.clone()).collect();
    assert!(owed.contains(&PathBuf::from("src/b.rs")));
    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 20);
}

#[test]
fn duplicate_change_delivery_records_the_work_twice_on_purpose() {
    // The observer may deliver the same edit twice (a retry, a replayed hook).
    // Two records is the honest outcome: the ledger cannot tell a duplicate
    // delivery from two genuine edits of the same size, and silently collapsing
    // them would discard a real second edit. What matters is that verification
    // still has to cover the latest sequence, which the race check enforces.
    let mut l = ledger();
    l.record_change("src/a.rs", 10, 1, T);
    let snap = l.snapshot();
    l.record_change("src/a.rs", 10, 1, T); // duplicate delivery, mid-run
    l.record_test_run(
        snap,
        Scope::Paths(paths(&["src/a.rs"])),
        Outcome::Passed,
        None,
        T,
    );

    assert_eq!(
        l.outstanding_lines(ObligationKind::UntestedLogic),
        20,
        "a run racing the redelivery covers neither copy"
    );
}

#[test]
fn evidence_recorded_without_taking_a_snapshot_first_cannot_backdate_itself() {
    // Taking the snapshot AFTER the work is the mistake this guards: it would
    // let a result claim to cover changes that landed while it ran.
    let mut l = ledger();
    l.record_change("src/a.rs", 10, 1, T);
    // Correct: snapshot before the run.
    let honest = l.snapshot();
    l.record_test_run(
        honest,
        Scope::Paths(paths(&["src/a.rs"])),
        Outcome::Passed,
        None,
        T,
    );
    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 0);

    // A later change is not retroactively covered by that earlier run.
    l.record_change("src/a.rs", 7, 2, T);
    assert_eq!(l.outstanding_lines(ObligationKind::UntestedLogic), 7);
}
