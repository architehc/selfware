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
    l.record_test_run(snap, Scope::Workspace, Outcome::Passed, None, T);

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
    l.record_human_review(snap, paths(&["src/a.rs"]), None, T);

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
    l.record_test_run(snap, Scope::Workspace, Outcome::Passed, None, T);

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
    l.record_test_run(snap, Scope::Workspace, Outcome::Passed, None, T);

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
    l.record_test_run(snap, Scope::Workspace, Outcome::Passed, None, T);

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
    l.record_test_run(snap, Scope::Workspace, Outcome::Failed, None, T);
    assert_eq!(l.outstanding().len(), 2, "red establishes work remains");
}

#[test]
fn a_partial_review_discharges_only_what_was_read() {
    let mut l = ledger();
    l.record_change("src/a.rs", 20, 1, T);
    l.record_change("src/b.rs", 30, 1, T);
    l.record_change("src/c.rs", 10, 1, T);
    let snap = l.snapshot();
    l.record_human_review(snap, paths(&["src/a.rs", "src/c.rs"]), None, T);

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
    l.record_human_review(snap, paths(&["src/b.rs"]), None, T);
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
    l.record_human_review(snap, paths(&["src/a.rs"]), None, T);
    let after_first = l.outstanding().len();

    l.record_human_review(snap, paths(&["src/a.rs"]), None, T);
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
        l.record_test_run(snap, Scope::Workspace, Outcome::Passed, None, T);
        l.record_human_review(snap, paths(&["src/a.rs"]), None, T);
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
