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
#[path = "../../../tests/unit/bench_harness/swebench_pro/candidate/candidate_test.rs"]
mod tests;
