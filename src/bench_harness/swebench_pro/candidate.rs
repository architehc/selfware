//! Candidate management for multi-candidate SWE-bench Pro generation.
//!
//! Each (quant, instance, trial) may produce multiple candidate patches.
//! `CandidatePool` provides honest selection and pass@k metrics.
//!
//! Metric contract:
//! - The deployable selection is **frozen before official evaluation** by
//!   [`CandidatePool::select_frozen`], a pre-declared selector that never
//!   inspects official labels. `pass@1` is the frozen selection's official
//!   result — nothing else.
//! - [`CandidatePool::first_sample_resolved`] reports the first generated
//!   sample's official result (the k=1 baseline).
//! - [`CandidatePool::pass_at_k_oracle`] is the oracle best-of-k upper bound
//!   (old behaviour, renamed honestly). It may peek at official labels and is
//!   reported separately, never as `pass@1`.
//! - [`CandidatePool::select_oracle`] is the label-peeking ranking kept for
//!   diagnostics only; it must never drive the promoted/deployed patch.

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
    /// 0 = the promoted trial-level copy of the frozen selection, 1..=k = raw
    /// generation samples.  Used only for deterministic tie-breaking.
    #[serde(default)]
    pub candidate_num: u32,
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

    /// The pre-declared, deployable selector.  Uses **only** information
    /// available before official evaluation — it never reads `official_eval`,
    /// so changing hidden labels cannot change the selection.
    ///
    /// Criteria (in order of priority):
    /// 1. Non-empty source diff + no test edits.
    /// 2. Smaller diff (fewer lines changed).
    /// 3. Passes cheap syntax checks.
    /// 4. Earliest candidate (deterministic tie-break).
    pub fn select_frozen(&self) -> Option<&Candidate> {
        self.candidates.iter().max_by(|a, b| {
            // 1. Non-empty source diff + no test edits
            let a_good = a.has_source_edit && !a.has_test_edit;
            let b_good = b.has_source_edit && !b.has_test_edit;
            a_good
                .cmp(&b_good)
                // 2. Smaller diff (fewer lines changed)
                .then_with(|| b.patch_lines.cmp(&a.patch_lines))
                // 3. Passes cheap syntax checks
                .then_with(|| a.syntax_check_passed.cmp(&b.syntax_check_passed))
                // 4. Earliest candidate wins remaining ties
                .then_with(|| {
                    b.candidate_num
                        .cmp(&a.candidate_num)
                        .then_with(|| b.trial.cmp(&a.trial))
                })
        })
    }

    /// Oracle ranking — **peeks at official labels**.  Retained for
    /// diagnostics/reporting only; the deployable selection must come from
    /// [`Self::select_frozen`].
    ///
    /// Criteria (in order of priority):
    /// 1. Official eval `resolved=true` if available.
    /// 2. Non-empty source diff + no test edits.
    /// 3. Smaller diff (fewer lines changed).
    /// 4. Passes cheap syntax checks.
    pub fn select_oracle(&self) -> Option<&Candidate> {
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

    /// The first generated sample (earliest trial, then lowest candidate
    /// number; raw candidates preferred over the promoted trial-level copy).
    pub fn first_sample(&self) -> Option<&Candidate> {
        let key = |c: &&Candidate| (c.trial, c.candidate_num.max(1));
        self.candidates
            .iter()
            .filter(|c| c.candidate_num > 0)
            .min_by_key(key)
            .or_else(|| self.candidates.iter().min_by_key(key))
    }

    /// First-sample resolution — did the first generated sample resolve
    /// according to official eval?  This is the k=1 baseline.
    pub fn first_sample_resolved(&self) -> bool {
        self.first_sample()
            .and_then(|c| c.official_eval.as_ref())
            .map(|e| e.resolved)
            .unwrap_or(false)
    }

    /// `pass@1` — did the **frozen** (pre-evaluation) selection resolve?
    ///
    /// The selection is made by [`Self::select_frozen`], which never sees
    /// official labels, so `pass@1` cannot be inflated by evaluating every
    /// candidate and promoting the winner.
    pub fn pass_at_1(&self) -> bool {
        self.select_frozen()
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
