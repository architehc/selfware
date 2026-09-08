use super::*;

fn make_candidate(
    trial: u32,
    patch_lines: usize,
    has_source_edit: bool,
    has_test_edit: bool,
    syntax_check_passed: bool,
    official_resolved: Option<bool>,
) -> Candidate {
    make_candidate_num(
        trial,
        1,
        patch_lines,
        has_source_edit,
        has_test_edit,
        syntax_check_passed,
        official_resolved,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_candidate_num(
    trial: u32,
    candidate_num: u32,
    patch_lines: usize,
    has_source_edit: bool,
    has_test_edit: bool,
    syntax_check_passed: bool,
    official_resolved: Option<bool>,
) -> Candidate {
    Candidate {
        trial,
        candidate_num,
        patch: String::new(),
        patch_bytes: patch_lines * 40,
        patch_lines,
        has_source_edit,
        has_test_edit,
        syntax_check_passed,
        test_results: None,
        official_eval: official_resolved.map(|r| OfficialEvalResult { resolved: r }),
    }
}

// --- Frozen (pre-evaluation) selector -------------------------------------

#[test]
fn select_frozen_prefers_source_no_test_edit() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, true, true, None),
        make_candidate(1, 5, true, false, true, None),
        make_candidate(1, 20, false, false, true, None),
    ]);
    let best = pool.select_frozen().unwrap();
    assert_eq!(best.patch_lines, 5); // source edit + no test edit
}

#[test]
fn select_frozen_tiebreaks_on_smaller_diff() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 20, true, false, true, None),
        make_candidate(1, 5, true, false, true, None),
        make_candidate(1, 10, true, false, true, None),
    ]);
    let best = pool.select_frozen().unwrap();
    assert_eq!(best.patch_lines, 5); // smallest diff wins
}

#[test]
fn select_frozen_prefers_syntax_ok() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, false, None),
        make_candidate(1, 10, true, false, true, None),
    ]);
    let best = pool.select_frozen().unwrap();
    assert!(best.syntax_check_passed);
}

/// Regression (a): changing hidden official labels must NOT change the
/// frozen selection.  A failed selected candidate plus a successful
/// alternative stays selected=false / oracle=true.
#[test]
fn frozen_selection_is_invariant_to_official_labels() {
    let build = |first_resolved: bool, second_resolved: bool| {
        CandidatePool::new(vec![
            // Frozen pick by proxy criteria (smaller diff).
            make_candidate_num(1, 1, 5, true, false, true, Some(first_resolved)),
            // Alternative that the oracle would prefer when resolved.
            make_candidate_num(1, 2, 10, true, false, true, Some(second_resolved)),
        ])
    };

    // Selected candidate fails, alternative succeeds.
    let pool = build(false, true);
    assert_eq!(pool.select_frozen().unwrap().candidate_num, 1);
    assert!(
        !pool.pass_at_1(),
        "pass@1 must reflect the frozen selection"
    );
    assert!(
        pool.pass_at_k_oracle(),
        "oracle best-of-k still sees the win"
    );

    // Flip the labels: the frozen selection is identical.
    let flipped = build(true, false);
    assert_eq!(flipped.select_frozen().unwrap().candidate_num, 1);
    assert!(flipped.pass_at_1());
    assert!(flipped.pass_at_k_oracle());

    // No labels at all: still the same selection.
    let unlabelled = CandidatePool::new(vec![
        make_candidate_num(1, 1, 5, true, false, true, None),
        make_candidate_num(1, 2, 10, true, false, true, None),
    ]);
    assert_eq!(unlabelled.select_frozen().unwrap().candidate_num, 1);
    assert!(!unlabelled.pass_at_1());
}

/// Regression (b): pass@1 reflects ONLY the frozen selection — an oracle
/// winner elsewhere in the pool must not inflate it.
#[test]
fn pass_at_1_ignores_oracle_winner() {
    let pool = CandidatePool::new(vec![
        // Frozen pick (smaller diff) — did NOT resolve.
        make_candidate_num(1, 1, 5, true, false, true, Some(false)),
        // Resolved, but not the frozen pick.
        make_candidate_num(1, 2, 10, true, false, true, Some(true)),
    ]);
    assert!(!pool.pass_at_1());
}

#[test]
fn pass_at_1_true_when_frozen_selection_resolved() {
    let pool = CandidatePool::new(vec![
        make_candidate_num(1, 1, 5, true, false, true, Some(true)),
        make_candidate_num(1, 2, 10, true, false, true, Some(false)),
    ]);
    assert!(pool.pass_at_1());
}

#[test]
fn pass_at_1_false_when_no_official_eval() {
    let pool = CandidatePool::new(vec![make_candidate(1, 10, true, false, true, None)]);
    assert!(!pool.pass_at_1());
}

// --- First sample ----------------------------------------------------------

#[test]
fn first_sample_prefers_earliest_raw_candidate() {
    let pool = CandidatePool::new(vec![
        // Promoted trial-level copy (candidate_num 0).
        make_candidate_num(1, 0, 5, true, false, true, Some(true)),
        make_candidate_num(1, 2, 10, true, false, true, Some(true)),
        make_candidate_num(1, 1, 7, true, false, true, Some(false)),
    ]);
    let first = pool.first_sample().unwrap();
    assert_eq!(first.candidate_num, 1);
    assert!(!pool.first_sample_resolved());
}

#[test]
fn first_sample_falls_back_to_promoted_when_no_raw_candidates() {
    let pool = CandidatePool::new(vec![make_candidate_num(
        1,
        0,
        5,
        true,
        false,
        true,
        Some(true),
    )]);
    assert_eq!(pool.first_sample().unwrap().candidate_num, 0);
    assert!(pool.first_sample_resolved());
}

// --- Oracle (label-peeking) selector and best-of-k -------------------------

#[test]
fn select_oracle_prefers_official_resolved() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(false)),
        make_candidate(1, 5, true, false, true, Some(true)),
        make_candidate(1, 20, true, false, true, Some(false)),
    ]);
    let best = pool.select_oracle().unwrap();
    assert_eq!(best.patch_lines, 5); // the resolved one
}

/// Regression (c): oracle best-of-k is still computed and reported
/// separately from pass@1 and first-sample resolution.
#[test]
fn oracle_best_of_k_reported_separately() {
    let pool = CandidatePool::new(vec![
        // First sample and frozen pick: not resolved.
        make_candidate_num(1, 1, 5, true, false, true, Some(false)),
        // Only the last candidate resolved.
        make_candidate_num(1, 2, 10, true, false, true, Some(true)),
    ]);
    assert!(!pool.first_sample_resolved());
    assert!(!pool.pass_at_1());
    assert!(pool.pass_at_k_oracle());
    assert_eq!(pool.select_oracle().unwrap().candidate_num, 2);
}

#[test]
fn pass_at_k_oracle_true_when_any_resolved() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(false)),
        make_candidate(1, 5, true, false, true, Some(true)),
    ]);
    assert!(pool.pass_at_k_oracle());
}

#[test]
fn pass_at_k_oracle_uses_proxy_when_no_official_eval() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, None),
        make_candidate(1, 5, false, false, true, None),
    ]);
    // At least one candidate has source edit + no test edit + syntax ok
    assert!(pool.pass_at_k_oracle());
}

#[test]
fn pass_at_k_oracle_proxy_false_when_no_good_candidates() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, false, true, true, None),
        make_candidate(1, 5, false, false, false, None),
    ]);
    assert!(!pool.pass_at_k_oracle());
}

#[test]
fn pass_at_k_oracle_does_not_fallback_to_proxy_when_official_all_fail() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(false)),
        make_candidate(1, 5, true, false, true, None),
    ]);
    assert!(pool.has_any_official_eval());
    assert!(!pool.pass_at_k_oracle());
}

#[test]
fn empty_pool_returns_none() {
    let pool = CandidatePool::new(vec![]);
    assert!(pool.select_frozen().is_none());
    assert!(pool.select_oracle().is_none());
    assert!(pool.first_sample().is_none());
    assert!(!pool.first_sample_resolved());
    assert!(!pool.pass_at_1());
    assert!(!pool.pass_at_k_oracle());
}

#[test]
fn all_have_official_eval_true() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(true)),
        make_candidate(1, 5, true, false, true, Some(false)),
    ]);
    assert!(pool.all_have_official_eval());
}

#[test]
fn all_have_official_eval_false() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(true)),
        make_candidate(1, 5, true, false, true, None),
    ]);
    assert!(!pool.all_have_official_eval());
}
