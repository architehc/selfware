//! Candidate management for multi-candidate SWE-bench Pro generation.
//!
//! Each (quant, instance, trial) may produce multiple candidate patches.
//! `CandidatePool` provides honest selection and pass@k metrics.

use serde::{Deserialize, Serialize};

/// Result of an official SWE-bench Pro Docker evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialEvalResult {
    pub resolved: bool,
}

/// Placeholder for test-run results (populated when a candidate is
/// evaluated against the instance's test suite).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    pub passed: bool,
}

/// A single generated patch candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub trial: u32,
    pub patch: String,
    pub patch_bytes: usize,
    pub patch_lines: usize,
    pub has_source_edit: bool,
    pub has_test_edit: bool,
    pub syntax_check_passed: bool,
    pub test_results: Option<TestResults>,
    pub official_eval: Option<OfficialEvalResult>,
}

/// Collection of candidates for a single (quant, instance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatePool {
    pub candidates: Vec<Candidate>,
}

impl CandidatePool {
    pub fn new(candidates: Vec<Candidate>) -> Self {
        Self { candidates }
    }

    /// Select the best candidate using honest proxy metrics.
    ///
    /// Selection criteria (in order of priority):
    /// 1. Official eval `resolved=true` if available.
    /// 2. Non-empty source diff + no test edits.
    /// 3. Smaller diff (fewer lines changed).
    /// 4. Passes cheap syntax checks.
    pub fn select_best(&self) -> Option<&Candidate> {
        self.candidates.iter().max_by(|a, b| {
            // 1. Official eval resolved=true
            let a_official = a
                .official_eval
                .as_ref()
                .map(|e| e.resolved)
                .unwrap_or(false);
            let b_official = b
                .official_eval
                .as_ref()
                .map(|e| e.resolved)
                .unwrap_or(false);
            a_official
                .cmp(&b_official)
                // 2. Non-empty source diff + no test edits
                .then_with(|| {
                    let a_good = a.has_source_edit && !a.has_test_edit;
                    let b_good = b.has_source_edit && !b.has_test_edit;
                    a_good.cmp(&b_good)
                })
                // 3. Smaller diff (fewer lines changed)
                .then_with(|| b.patch_lines.cmp(&a.patch_lines))
                // 4. Passes cheap syntax checks
                .then_with(|| a.syntax_check_passed.cmp(&b.syntax_check_passed))
        })
    }

    /// `pass@1` — did the honestly-selected best candidate resolve?
    pub fn pass_at_1(&self) -> bool {
        self.select_best()
            .and_then(|c| c.official_eval.as_ref())
            .map(|e| e.resolved)
            .unwrap_or(false)
    }

    /// `pass@k` oracle — did *any* candidate resolve according to official
    /// eval?  When no candidate has official-eval data this falls back to
    /// proxy metrics (source edit, no test edits, syntax ok) and is therefore
    /// an **upper bound**.
    pub fn pass_at_k_oracle(&self) -> bool {
        if self.has_any_official_eval() {
            return self.candidates.iter().any(|c| {
                c.official_eval
                    .as_ref()
                    .map(|e| e.resolved)
                    .unwrap_or(false)
            });
        }
        // Proxy-based upper bound when official eval is not available.
        self.candidates
            .iter()
            .any(|c| c.has_source_edit && !c.has_test_edit && c.syntax_check_passed)
    }

    /// Returns `true` when at least one candidate has official-eval data.
    pub fn has_any_official_eval(&self) -> bool {
        self.candidates.iter().any(|c| c.official_eval.is_some())
    }

    /// Returns `true` when every candidate in the pool has official-eval
    /// data.  Used by reporting to label `pass@k_oracle` accurately.
    pub fn all_have_official_eval(&self) -> bool {
        !self.candidates.is_empty() && self.candidates.iter().all(|c| c.official_eval.is_some())
    }
}

#[cfg(test)]
mod tests {
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
}
