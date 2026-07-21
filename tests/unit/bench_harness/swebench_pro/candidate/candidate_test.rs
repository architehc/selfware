use super::*;

fn make_candidate(
    trial: u32,
    patch_lines: usize,
    has_source_edit: bool,
    has_test_edit: bool,
    syntax_check_passed: bool,
    official_resolved: Option<bool>,
) -> Candidate {
    Candidate {
        trial,
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

#[test]
fn select_best_prefers_official_resolved() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(false)),
        make_candidate(1, 5, true, false, true, Some(true)),
        make_candidate(1, 20, true, false, true, Some(false)),
    ]);
    let best = pool.select_best().unwrap();
    assert_eq!(best.patch_lines, 5); // the resolved one
}

#[test]
fn select_best_prefers_source_no_test_edit() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, true, true, None),
        make_candidate(1, 5, true, false, true, None),
        make_candidate(1, 20, false, false, true, None),
    ]);
    let best = pool.select_best().unwrap();
    assert_eq!(best.patch_lines, 5); // source edit + no test edit
}

#[test]
fn select_best_tiebreaks_on_smaller_diff() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 20, true, false, true, None),
        make_candidate(1, 5, true, false, true, None),
        make_candidate(1, 10, true, false, true, None),
    ]);
    let best = pool.select_best().unwrap();
    assert_eq!(best.patch_lines, 5); // smallest diff wins
}

#[test]
fn select_best_prefers_syntax_ok() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, false, None),
        make_candidate(1, 10, true, false, true, None),
    ]);
    let best = pool.select_best().unwrap();
    assert!(best.syntax_check_passed);
}

#[test]
fn pass_at_1_uses_best_candidate() {
    let pool = CandidatePool::new(vec![
        make_candidate(1, 10, true, false, true, Some(false)),
        make_candidate(1, 5, true, false, true, Some(true)),
    ]);
    assert!(pool.pass_at_1());
}

#[test]
fn pass_at_1_false_when_no_official_eval() {
    let pool = CandidatePool::new(vec![make_candidate(1, 10, true, false, true, None)]);
    assert!(!pool.pass_at_1());
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
    assert!(pool.select_best().is_none());
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
