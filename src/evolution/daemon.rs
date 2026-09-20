//! Evolution Daemon — `selfware evolve`
//!
//! The main evolutionary loop that ties everything together:
//! Mutate → Compile-gate → Sandbox → Fitness → Select/Rollback
//!
//! This module is PROTECTED from self-modification.

use super::ast_tools;
use super::fitness::{self, SabConfig, SabResult};
use super::policy::{
    BreadthFirstPolicy, EarlyStopPlateauPolicy, FixedPopulationPolicy, LegalAction,
    ParetoAdaptivePolicy, PolicyDecision, PrefixObservation, PrefixView, RefineTop1Policy,
    SearchPolicy,
};
use super::telemetry;
use super::tournament::Hypothesis;
use super::tree_log::{
    compute_sha256, tail_lines, ActionType, AttemptNode, AttemptStatus, AttemptTree, FailureClass,
    TreeLogError,
};
use super::{is_protected, EvolutionConfig, FitnessMetrics, GenerationRating, LlmConfig};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// The hall of fame — tracks every successful mutation across generations
#[derive(Debug, Clone)]
pub struct GenerationWinner {
    pub generation: usize,
    pub description: String,
    pub composite_score: f64,
    pub sab_delta: f64,
    /// `None` when either side did not report token usage. It was `f64`, and
    /// two unmeasured runs produced a confident `0.0` delta.
    pub token_delta: Option<f64>,
    pub patch: String,
    pub git_tag: Option<String>,
    pub run_id: Option<String>,
    pub binary_sha256: Option<String>,
    pub report_path: Option<PathBuf>,
}

/// Summary of the evolution run
#[derive(Debug)]
pub struct EvolutionResult {
    pub generations_run: usize,
    pub improvements: Vec<GenerationWinner>,
    pub final_sab_score: f64,
    pub initial_sab_score: f64,
    pub total_duration: std::time::Duration,
    /// Why the run stopped early due to an unrecoverable failure (killswitch, baseline failure, I/O error).
    ///
    /// A run abandoned because its baseline could not be measured used to be
    /// indistinguishable from one that ran fully and found no improvement:
    /// both returned zero improvements. They mean opposite things.
    pub aborted: Option<String>,
    /// Terminal outcome string ("completed", "policy_stopped", "aborted", "killed")
    pub outcome: String,
    /// If stopped by search policy convergence, the typed reason for stopping.
    pub stop_reason: Option<String>,
}

const DEFAULT_TOKEN_BUDGET: u64 = 500_000;
const DEFAULT_TIMEOUT_SECS: f64 = 3600.0;
const EVOLVE_FEATURES: &[&str] = &["self-improvement"];

/// Error returned by cancellable daemon subprocess executions.
#[derive(Debug)]
pub(crate) enum SubprocessError {
    ShutdownRequested,
    Timeout,
    Io(std::io::Error),
}

impl std::fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShutdownRequested => write!(f, "subprocess cancelled: shutdown requested"),
            Self::Timeout => write!(f, "subprocess timed out"),
            Self::Io(e) => write!(f, "subprocess I/O error: {e}"),
        }
    }
}

/// Run a command asynchronously with process group isolation and cancellation polling.
/// If shutdown is requested while the command runs, terminates the entire process group with SIGKILL.
pub(crate) async fn run_cancellable_subprocess(
    mut cmd: tokio::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, SubprocessError> {
    if crate::is_shutdown_requested() {
        return Err(SubprocessError::ShutdownRequested);
    }

    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null());

    let child = cmd.spawn().map_err(SubprocessError::Io)?;
    let child_pid = child.id();

    let wait_fut = tokio::time::timeout(timeout, child.wait_with_output());
    tokio::pin!(wait_fut);

    let wait_res = loop {
        if crate::is_shutdown_requested() {
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            return Err(SubprocessError::ShutdownRequested);
        }

        tokio::select! {
            res = &mut wait_fut => break res,
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    };

    match wait_res {
        Ok(output_res) => output_res.map_err(SubprocessError::Io),
        Err(_) => {
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            Err(SubprocessError::Timeout)
        }
    }
}

fn rating_from_score(score: f64) -> GenerationRating {
    match score as u32 {
        85..=100 => GenerationRating::Bloom,
        60..=84 => GenerationRating::Grow,
        30..=59 => GenerationRating::Wilt,
        _ => GenerationRating::Frost,
    }
}

fn first_usize(s: &str) -> usize {
    s.split_whitespace()
        .filter_map(|w| w.parse().ok())
        .next()
        .unwrap_or(0)
}

/// Parse the combined cargo test output and return `(passed, total)`.
///
/// Sums every `test result:` line so multi-crate / multi-target runs are
/// handled correctly. Ignored tests are excluded from the total because they
/// are not run.
pub fn parse_test_summary(output: &str) -> (usize, usize) {
    let mut passed = 0usize;
    let mut failed = 0usize;
    for line in output.lines() {
        if line.contains("test result:") {
            for part in line.split(';') {
                let part = part.trim();
                if part.contains("passed") {
                    passed += first_usize(part);
                } else if part.contains("failed") {
                    failed += first_usize(part);
                }
            }
        }
    }
    (passed, passed + failed)
}

fn features_arg(features: &[&str]) -> String {
    features.join(",")
}

/// Measure baseline fitness from real compile / test / fmt / clippy / build
/// metrics. This replaces the previous synthetic baseline score of 50.
async fn measure_compile_test_baseline(
    dir: &Path,
    features: &[&str],
    timeout_secs: f64,
) -> Result<FitnessMetrics, String> {
    let start = Instant::now();
    let feat = features_arg(features);

    let mut check_cmd = tokio::process::Command::new("cargo");
    check_cmd.arg("check").arg("--all-targets").current_dir(dir);
    if !features.is_empty() {
        check_cmd.arg("--features").arg(&feat);
    }
    let check =
        match run_cancellable_subprocess(check_cmd, std::time::Duration::from_secs(300)).await {
            Ok(o) => o,
            Err(SubprocessError::ShutdownRequested) => return Err("shutdown requested".to_string()),
            Err(SubprocessError::Timeout) => return Err("cargo check timed out (300s)".to_string()),
            Err(SubprocessError::Io(e)) => return Err(format!("cargo check failed to run: {e}")),
        };
    if !check.status.success() {
        return Err(format!(
            "cargo check failed:\n{}",
            String::from_utf8_lossy(&check.stderr)
        ));
    }

    // Time the TEST PHASE ONLY, because that is what the candidate arm times.
    let test_start = Instant::now();
    let mut test_cmd = tokio::process::Command::new("cargo");
    test_cmd.arg("test").arg("--lib").current_dir(dir);
    if !features.is_empty() {
        test_cmd.arg("--features").arg(&feat);
    }
    let test = match run_cancellable_subprocess(
        test_cmd,
        std::time::Duration::from_secs(timeout_secs as u64),
    )
    .await
    {
        Ok(o) => o,
        Err(SubprocessError::ShutdownRequested) => return Err("shutdown requested".to_string()),
        Err(SubprocessError::Timeout) => {
            return Err(format!("cargo test timed out ({timeout_secs}s)"));
        }
        Err(SubprocessError::Io(e)) => return Err(format!("cargo test failed to run: {e}")),
    };
    let test_duration = test_start.elapsed();
    let test_stdout = String::from_utf8_lossy(&test.stdout);
    let test_stderr = String::from_utf8_lossy(&test.stderr);
    let full_output = format!("{}\n{}", test_stdout, test_stderr);
    let (tests_passed, tests_total) = parse_test_summary(&full_output);

    let mut fmt_cmd = tokio::process::Command::new("cargo");
    fmt_cmd.args(["fmt", "--", "--check"]).current_dir(dir);
    let fmt_ok =
        match run_cancellable_subprocess(fmt_cmd, std::time::Duration::from_secs(120)).await {
            Ok(o) => o.status.success(),
            Err(SubprocessError::ShutdownRequested) => return Err("shutdown requested".to_string()),
            Err(SubprocessError::Timeout) => return Err("cargo fmt timed out (120s)".to_string()),
            Err(SubprocessError::Io(e)) => return Err(format!("cargo fmt failed to run: {e}")),
        };

    let mut clippy_cmd = tokio::process::Command::new("cargo");
    clippy_cmd
        .arg("clippy")
        .arg("--all-targets")
        .current_dir(dir);
    if !features.is_empty() {
        clippy_cmd.arg("--features").arg(&feat);
    }
    clippy_cmd.args(["--", "-D", "warnings"]);
    let clippy_ok =
        match run_cancellable_subprocess(clippy_cmd, std::time::Duration::from_secs(300)).await {
            Ok(o) => o.status.success(),
            Err(SubprocessError::ShutdownRequested) => return Err("shutdown requested".to_string()),
            Err(SubprocessError::Timeout) => {
                return Err("cargo clippy timed out (300s)".to_string())
            }
            Err(SubprocessError::Io(e)) => return Err(format!("cargo clippy failed to run: {e}")),
        };

    let mut build_cmd = tokio::process::Command::new("cargo");
    build_cmd.args(["build", "--release"]).current_dir(dir);
    if !features.is_empty() {
        build_cmd.arg("--features").arg(&feat);
    }
    let build =
        match run_cancellable_subprocess(build_cmd, std::time::Duration::from_secs(600)).await {
            Ok(o) => o,
            Err(SubprocessError::ShutdownRequested) => return Err("shutdown requested".to_string()),
            Err(SubprocessError::Timeout) => {
                return Err("cargo build --release timed out (600s)".to_string());
            }
            Err(SubprocessError::Io(e)) => {
                return Err(format!("cargo build --release failed to run: {e}"));
            }
        };
    if !build.status.success() {
        return Err(format!(
            "cargo build --release failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        ));
    }
    let binary_path = dir.join("target/release/selfware");
    let binary_size_mb = std::fs::metadata(&binary_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    let pass_ratio = if tests_total > 0 {
        tests_passed as f64 / tests_total as f64
    } else {
        0.0
    };
    let fmt_factor = if fmt_ok { 1.0 } else { 0.95 };
    let clippy_factor = if clippy_ok { 1.0 } else { 0.90 };
    let sab_score = 100.0 * pass_ratio * fmt_factor * clippy_factor;

    Ok(FitnessMetrics {
        sab_score,
        // Compile/test mode runs no agent, so no tokens are spent OR observed.
        // Recording 0 scored this as perfect token efficiency.
        tokens_used: None,
        token_budget: DEFAULT_TOKEN_BUDGET,
        // Same boundary as the candidate arm: the test phase.
        wall_clock_secs: test_duration.as_secs_f64(),
        timeout_secs,
        full_evaluation_secs: Some(start.elapsed().as_secs_f64()),
        test_pass_pct: pass_ratio * 100.0,
        binary_size_mb,
        max_binary_size_mb: 0.0,
        tests_passed,
        tests_total,
        visual_score: 0.0,
    })
}

/// Build FitnessMetrics for a candidate that has already passed compile, test,
/// fmt and clippy. Runs a release build to capture real binary size.
async fn build_candidate_metrics(
    worktree: &Path,
    test_output: &std::process::Output,
    test_duration: std::time::Duration,
    features: &[&str],
    config: &EvolutionConfig,
) -> Result<FitnessMetrics, SubprocessError> {
    let stdout = String::from_utf8_lossy(&test_output.stdout);
    let stderr = String::from_utf8_lossy(&test_output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    let (tests_passed, tests_total) = parse_test_summary(&combined);

    let feat = features_arg(features);
    let mut build_cmd = tokio::process::Command::new("cargo");
    build_cmd.args(["build", "--release"]).current_dir(worktree);
    if !features.is_empty() {
        build_cmd.arg("--features").arg(&feat);
    }
    let build = run_cancellable_subprocess(build_cmd, std::time::Duration::from_secs(600)).await?;
    if !build.status.success() {
        let err_tail = tail_lines(&String::from_utf8_lossy(&build.stderr), 50);
        return Err(SubprocessError::Io(std::io::Error::other(format!(
            "cargo build --release exited {}: {}",
            build.status, err_tail
        ))));
    }
    let binary_path = worktree.join("target/release/selfware");
    let binary_size_mb = std::fs::metadata(&binary_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    let pass_ratio = if tests_total > 0 {
        tests_passed as f64 / tests_total as f64
    } else {
        0.0
    };

    Ok(FitnessMetrics {
        sab_score: pass_ratio * 100.0,
        tokens_used: None,
        token_budget: DEFAULT_TOKEN_BUDGET,
        wall_clock_secs: test_duration.as_secs_f64(),
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        // The candidate arm does not time its own build phase separately.
        full_evaluation_secs: None,
        test_pass_pct: pass_ratio * 100.0,
        binary_size_mb,
        max_binary_size_mb: config.safety.max_binary_size_mb,
        tests_passed,
        tests_total,
        visual_score: 0.0,
    })
}

// `synthetic_baseline_metrics` used to live here. It returned sab_score 50.0,
// coverage 50.0 and a 15 MB binary when real baseline measurement FAILED, and
// that fiction became the bar every candidate was promoted against. A candidate
// scoring 55 on a suite the baseline never ran looked like an improvement.
//
// There is no honest substitute for a measurement that did not happen: if the
// baseline cannot be measured, the generation cannot be judged, so it is
// abandoned rather than scored against an invention.

/// Convert a SAB result into FitnessMetrics, preserving the real SAB score.
fn metrics_from_sab_result(
    sab: &SabResult,
    binary_path: &Path,
    max_binary_size_mb: f64,
) -> FitnessMetrics {
    let tests_passed = sab
        .scenario_scores
        .iter()
        .filter(|s| s.tests_passed)
        .count();
    let tests_total = sab.scenario_scores.len();
    let mut metrics = fitness::build_fitness_metrics(
        sab,
        DEFAULT_TOKEN_BUDGET,
        DEFAULT_TIMEOUT_SECS,
        binary_path,
        tests_passed,
        tests_total,
        max_binary_size_mb,
    );
    metrics.sab_score = sab.aggregate_score;
    metrics
}

/// Hard gate on the winner: a candidate whose test count regressed relative
/// to the baseline must never be committed, no matter how good its composite
/// score looks — deleting tests inflates the pass ratio. This enforces the
/// invariant the dead `SafetyConfig.min_test_count` field promised.
///
/// Returns `Err(reason)` when the winner regressed; `Ok(())` otherwise.
fn winner_test_count_gate(
    baseline: &FitnessMetrics,
    winner: &FitnessMetrics,
) -> Result<(), String> {
    if winner.tests_total < baseline.tests_total {
        Err(format!(
            "winner rejected: test count regressed {}→{}",
            baseline.tests_total, winner.tests_total
        ))
    } else {
        Ok(())
    }
}

/// Enforces DarwinX non-regression: a candidate must pass all baseline-passed scenarios
/// without regressions, and cannot drop any scenarios from the suite.
///
/// SAB benchmark evidence is evaluated symmetrically: if either baseline or candidate
/// has SAB evidence, the other must also provide evidence. Asymmetric runs (one present,
/// one missing) are rejected fail-closed.
///
/// Returns `Err(reason)` when the candidate regressed or evidence is asymmetric; `Ok(())` otherwise.
pub(crate) fn winner_darwinx_gate(
    base_sab: Option<&SabResult>,
    cand_sab: Option<&SabResult>,
) -> Result<(), String> {
    match (base_sab, cand_sab) {
        (Some(base), Some(cand)) => {
            if let Err(violation) = base.check_darwinx_non_regression(cand) {
                Err(format!("DarwinX non-regression check failed: {violation}"))
            } else {
                Ok(())
            }
        }
        (None, None) => Ok(()),
        (Some(_), None) => Err(
            "DarwinX gate rejected: baseline has SAB benchmark evidence but candidate has none"
                .to_string(),
        ),
        (None, Some(_)) => Err(
            "DarwinX gate rejected: candidate has SAB benchmark evidence but baseline has none"
                .to_string(),
        ),
    }
}

/// Promotion decision for a generation winner candidate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PromotionDecision {
    Promote,
    Reject(String),
}

#[derive(Debug, Clone)]
pub(crate) struct EvaluatedCandidate {
    pub(crate) hypothesis: Hypothesis,
    pub(crate) metrics: FitnessMetrics,
    pub(crate) sab_result: Option<fitness::SabResult>,
    pub(crate) tested_diff: String,
    pub(crate) evaluated_tree: String,
    pub(crate) composite: f64,
    pub(crate) attempt_id: String,
    pub(crate) branch_id: String,
}

/// Default noise margin (epsilon) when empirical scenario variance is unmeasured.
pub const DEFAULT_SAB_NOISE_MARGIN: f64 = 0.5;
pub const SAB_NOISE_MARGIN: f64 = DEFAULT_SAB_NOISE_MARGIN;

/// Empirical noise margin for SAB capability scores (Rule 4: Measured, not estimated).
/// When paired scenario scores are available from benchmark runs, calculates the
/// standard error of the mean scenario score delta across the benchmark suite.
/// Scenarios are paired by name to ensure permutation invariance.
/// Falls back to [`DEFAULT_SAB_NOISE_MARGIN`] when unmeasured.
pub(crate) fn compute_empirical_noise_margin(
    base_sab: Option<&SabResult>,
    cand_sab: Option<&SabResult>,
) -> f64 {
    if let (Some(base), Some(cand)) = (base_sab, cand_sab) {
        if !base.scenario_scores.is_empty() {
            let mut deltas = Vec::new();
            for b in &base.scenario_scores {
                if let Some(c) = cand.scenario_scores.iter().find(|s| s.name == b.name) {
                    deltas.push(c.score - b.score);
                }
            }
            let n = deltas.len() as f64;
            if n >= 2.0 {
                let mean = deltas.iter().sum::<f64>() / n;
                let var = deltas.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0);
                let std_err = (var / n).sqrt();
                return std_err.clamp(0.05, DEFAULT_SAB_NOISE_MARGIN);
            }
        }
    }
    DEFAULT_SAB_NOISE_MARGIN
}

/// Compare two candidates using a consistent, transitive ranking rule (total order):
/// 1. Primary: SAB score discretized into noise-margin tiers (`(sab / DEFAULT_SAB_NOISE_MARGIN).floor() as i64`).
///    A candidate in a higher tier is strictly superior.
/// 2. Secondary: Composite fitness score within the same SAB tier.
/// 3. Tertiary: Raw continuous SAB score as tie-breaker.
pub(crate) fn candidate_rank_cmp(
    a_sab: f64,
    a_composite: f64,
    b_sab: f64,
    b_composite: f64,
) -> std::cmp::Ordering {
    let a_tier = (a_sab / DEFAULT_SAB_NOISE_MARGIN).floor() as i64;
    let b_tier = (b_sab / DEFAULT_SAB_NOISE_MARGIN).floor() as i64;

    match a_tier.cmp(&b_tier) {
        std::cmp::Ordering::Equal => {}
        ord => return ord,
    }

    match a_composite
        .partial_cmp(&b_composite)
        .unwrap_or(std::cmp::Ordering::Equal)
    {
        std::cmp::Ordering::Equal => {}
        ord => return ord,
    }

    a_sab
        .partial_cmp(&b_sab)
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Decide whether candidate A is better than candidate B under the transitive ranking rule.
#[allow(dead_code)]
pub(crate) fn is_candidate_better(
    cand_metrics: &FitnessMetrics,
    cand_composite: f64,
    best_metrics: &FitnessMetrics,
    best_composite: f64,
) -> bool {
    candidate_rank_cmp(
        cand_metrics.sab_score,
        cand_composite,
        best_metrics.sab_score,
        best_composite,
    ) == std::cmp::Ordering::Greater
}

/// Evaluates whether a candidate winner should be promoted to baseline:
/// 1. Must not regress SAB capability beyond the noise margin (constant SAB_NOISE_MARGIN = 0.5).
/// 2. Must pass the DarwinX non-regression gate over SAB results.
/// 3. Must not regress total test count relative to baseline.
/// 4. When SAB improvement is within noise margin, composite score
///    must strictly exceed baseline (secondary metrics: latency, pass rate, etc.).
pub(crate) fn evaluate_candidate_promotion(
    baseline_composite: f64,
    winner_composite: f64,
    base_sab: Option<&SabResult>,
    cand_sab: Option<&SabResult>,
    base_metrics: &FitnessMetrics,
    winner_metrics: &FitnessMetrics,
) -> PromotionDecision {
    let noise_margin = compute_empirical_noise_margin(base_sab, cand_sab);
    if winner_metrics.sab_score < base_metrics.sab_score - noise_margin {
        return PromotionDecision::Reject(format!(
            "winner SAB score ({:.2}) regressed below baseline ({:.2}) beyond noise margin ({:.2})",
            winner_metrics.sab_score, base_metrics.sab_score, noise_margin
        ));
    }

    if let Err(reason) = winner_darwinx_gate(base_sab, cand_sab) {
        return PromotionDecision::Reject(reason);
    }

    if let Err(reason) = winner_test_count_gate(base_metrics, winner_metrics) {
        return PromotionDecision::Reject(reason);
    }

    let sab_delta = winner_metrics.sab_score - base_metrics.sab_score;

    // Latency and binary size are tie-breakers within a generation, not promotion
    // drivers over baseline. Promotion requires either:
    // 1. A genuine SAB capability improvement beyond the noise margin (sab_delta > noise_margin).
    // 2. Or, if SAB capability is within noise margin, a significant measured token efficiency
    //    improvement (> 5% reduction, with both arms measured), and higher composite.
    if sab_delta <= noise_margin {
        if winner_composite <= baseline_composite {
            return PromotionDecision::Reject(format!(
                "winner composite ({:.4}) does not exceed baseline ({:.4})",
                winner_composite, baseline_composite
            ));
        }

        let has_token_improvement = match (winner_metrics.tokens_used, base_metrics.tokens_used) {
            (Some(w_tok), Some(b_tok)) if b_tok > 0 => {
                (b_tok as f64 - w_tok as f64) / b_tok as f64 > 0.05
            }
            _ => false,
        };

        if !has_token_improvement {
            return PromotionDecision::Reject(format!(
                "winner SAB delta ({:.2}) is within noise margin ({:.2}) and has no measured token efficiency improvement; latency and binary size are tie-breakers, not promotion drivers",
                sab_delta, noise_margin
            ));
        }
    }

    PromotionDecision::Promote
}

/// Canonical normalization for search policy names.
/// Handles PascalCase (e.g. `ParetoAdaptivePolicy`), snake_case (`pareto_adaptive_policy`),
/// kebab-case (`pareto-adaptive`), aliases (`bfs`, `fixed`, `pareto`), and names with comments
/// or qualifiers (`FixedPopulation (Incumbent)`).
pub fn normalize_policy_name(raw: &str) -> String {
    let clean = raw.split('(').next().unwrap_or(raw).trim();
    let mut snake = String::new();
    let chars: Vec<char> = clean.chars().collect();
    for i in 0..chars.len() {
        let c = chars[i];
        if c.is_ascii_uppercase() {
            if i > 0
                && !chars[i - 1].is_ascii_uppercase()
                && chars[i - 1] != '_'
                && chars[i - 1] != '-'
            {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else if c == '-' || c == ' ' {
            snake.push('_');
        } else {
            snake.push(c);
        }
    }
    snake
}

/// Instantiates a named search policy, falling back to [`FixedPopulationPolicy`].
pub fn instantiate_search_policy(
    policy_name: &str,
    population_size: usize,
) -> Box<dyn SearchPolicy> {
    let normalized = normalize_policy_name(policy_name);
    match normalized.as_str() {
        "fixed_population"
        | "fixed_population_policy"
        | "fixed"
        | "fixedpopulation"
        | "fixedpopulationpolicy" => Box::new(FixedPopulationPolicy::for_daemon(population_size)),

        "breadth_first"
        | "breadth_first_policy"
        | "breadthfirst"
        | "breadthfirstpolicy"
        | "bfs" => Box::new(BreadthFirstPolicy::new(3)),

        "refine_top1" | "refine_top1_policy" | "refine_top" | "refinetop1" | "refinetop1policy" => {
            Box::new(RefineTop1Policy::new(3))
        }

        "pareto_adaptive"
        | "pareto_adaptive_policy"
        | "paretoadaptive"
        | "paretoadaptivepolicy"
        | "pareto" => Box::new(ParetoAdaptivePolicy::new()),

        "early_stop_plateau"
        | "early_stop_plateau_policy"
        | "earlystopplateau"
        | "earlystopplateaupolicy"
        | "early_stop" => Box::new(EarlyStopPlateauPolicy::new(
            Box::new(FixedPopulationPolicy::for_daemon(population_size)),
            3,
            0.01,
        )),

        _ => {
            tracing::warn!(
                "Unknown search policy '{}' (normalized: '{}'), defaulting to FixedPopulationPolicy",
                policy_name,
                normalized
            );
            Box::new(FixedPopulationPolicy::for_daemon(population_size))
        }
    }
}

/// Metadata and instantiated policy loaded from `.selfware/active_policy.json`.
pub struct LoadedActivePolicy {
    pub policy: Box<dyn SearchPolicy>,
    pub beta: f64,
    pub evidence_hash: Option<String>,
    pub policy_name: String,
    pub fallback_reason: Option<String>,
}

/// Loads and validates the promoted search policy from `.selfware/active_policy.json`.
pub fn load_active_policy(active_policy_path: &Path, population_size: usize) -> LoadedActivePolicy {
    let fallback = |reason: Option<String>| LoadedActivePolicy {
        policy: Box::new(FixedPopulationPolicy::for_daemon(population_size)),
        beta: 1.0,
        evidence_hash: None,
        policy_name: "FixedPopulation (Incumbent)".to_string(),
        fallback_reason: reason,
    };

    if !active_policy_path.exists() {
        return fallback(Some("Active policy file does not exist".to_string()));
    }

    let Ok(content) = std::fs::read_to_string(active_policy_path) else {
        return fallback(Some("Failed to read active policy file".to_string()));
    };

    let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
        return fallback(Some("Failed to parse active policy JSON".to_string()));
    };

    let Some(raw_name) = val.get("policy_name").and_then(|v| v.as_str()) else {
        return fallback(Some(
            "Missing policy_name in active policy file".to_string(),
        ));
    };

    let is_incumbent = raw_name == "FixedPopulation (Incumbent)" || raw_name == "FixedPopulation";

    let beta = val.get("beta").and_then(|v| v.as_f64()).unwrap_or(0.2);
    let evidence_hash = val
        .get("evidence_hash")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // Mandatory provenance: non-incumbent policies must carry a valid evidence_hash
    if !is_incumbent && evidence_hash.is_none() {
        let msg = format!(
            "Promoted non-incumbent policy '{raw_name}' lacks required cryptographic evidence_hash"
        );
        tracing::warn!("{msg}; falling back to incumbent");
        return fallback(Some(msg));
    }

    // Verify cryptographic evidence binding if hash is present
    if let Some(ref expected_hash) = evidence_hash {
        let val_obj = val
            .get("validation_objective")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let inc_obj = val
            .get("incumbent_objective")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let w = val.get("kendall_w").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let tree_digests: Vec<String> = val
            .get("tree_digests")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let report_digest = val
            .get("report_digest")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // If tree_digests are specified, tree_files is mandatory and must verify on disk
        if !tree_digests.is_empty() {
            let tree_files_arr = val.get("tree_files").and_then(|v| v.as_array());
            let Some(tree_files_arr) = tree_files_arr else {
                let msg = format!(
                    "Policy specifies {} tree digest(s) but missing 'tree_files' attestation array",
                    tree_digests.len()
                );
                tracing::warn!("{msg}; falling back to incumbent");
                return fallback(Some(msg));
            };
            if tree_files_arr.len() != tree_digests.len() {
                let msg = format!(
                    "Policy 'tree_files' count ({}) does not match 'tree_digests' count ({})",
                    tree_files_arr.len(),
                    tree_digests.len()
                );
                tracing::warn!("{msg}; falling back to incumbent");
                return fallback(Some(msg));
            }

            for (i, tf_val) in tree_files_arr.iter().enumerate() {
                if let Some(tf_str) = tf_val.as_str() {
                    let tf_path = Path::new(tf_str);
                    match std::fs::read(tf_path) {
                        Ok(bytes) => {
                            let disk_sha = crate::evolution::tree_log::compute_sha256(&bytes);
                            if let Some(expected_digest) = tree_digests.get(i) {
                                if &disk_sha != expected_digest {
                                    let msg = format!(
                                        "Tree file on disk '{}' digest mismatch: expected {expected_digest}, found {disk_sha}",
                                        tf_path.display()
                                    );
                                    tracing::warn!("{msg}; falling back to incumbent");
                                    return fallback(Some(msg));
                                }
                            }
                        }
                        Err(e) => {
                            let msg = format!(
                                "Cannot read tree file on disk '{}': {e}",
                                tf_path.display()
                            );
                            tracing::warn!("{msg}; falling back to incumbent");
                            return fallback(Some(msg));
                        }
                    }
                }
            }
        }

        let computed = crate::evolution::replay::compute_policy_evidence_hash(
            raw_name,
            val_obj,
            inc_obj,
            w,
            beta,
            &tree_digests,
            report_digest,
        );
        if computed != *expected_hash {
            let msg = format!(
                "Active policy evidence hash mismatch for '{raw_name}'! Expected '{expected_hash}', got '{computed}'"
            );
            tracing::warn!("{msg}; falling back to incumbent FixedPopulationPolicy");
            return fallback(Some(msg));
        }
    }

    let policy = instantiate_search_policy(raw_name, population_size);
    let policy_name = policy.name().to_string();
    tracing::info!(
        "Evolution daemon loaded active search policy: {} (beta: {:.2})",
        policy_name,
        beta
    );

    LoadedActivePolicy {
        policy,
        beta,
        evidence_hash,
        policy_name,
        fallback_reason: None,
    }
}

/// Helper to construct a PrefixView and legal actions from the attempts file
/// and invoke the active search policy to select the next exploration/refinement action.
pub fn decide_next_search_action(
    policy: &mut dyn SearchPolicy,
    attempts_file: &Path,
    baseline_score: f64,
    max_parallelism: usize,
    generation: usize,
    beta: f64,
) -> PolicyDecision {
    let mut raw_nodes: Vec<AttemptNode> = Vec::new();
    let mut id_to_node: std::collections::HashMap<String, AttemptNode> =
        std::collections::HashMap::new();

    if let Ok(content) = std::fs::read_to_string(attempts_file) {
        for line in content.lines() {
            if let Ok(node) = serde_json::from_str::<AttemptNode>(line) {
                // Control anchors are excluded from search policy prefix
                if node.branch_id == "control" {
                    continue;
                }
                id_to_node.insert(node.id.clone(), node.clone());
                raw_nodes.push(node);
            }
        }
    }

    let mut observations = Vec::new();
    for node in &raw_nodes {
        let parent_score = node
            .parent_id
            .as_ref()
            .and_then(|pid| id_to_node.get(pid))
            .and_then(|p| p.composite_score);

        let delta_vs_parent = match (node.composite_score, parent_score) {
            (Some(curr), Some(par)) => Some(curr - par),
            _ => None,
        };

        let delta_vs_baseline = node.composite_score.map(|s| s - baseline_score);

        // Calculate depth within branch based on parent chain (matches replay.rs)
        let mut depth = 0;
        let mut curr_pid = node.parent_id.clone();
        while let Some(ref pid) = curr_pid {
            depth += 1;
            curr_pid = id_to_node.get(pid).and_then(|p| p.parent_id.clone());
        }

        observations.push(PrefixObservation {
            id: node.id.clone(),
            branch_id: node.branch_id.clone(),
            attempt_depth: depth,
            parent_id: node.parent_id.clone(),
            score: node.composite_score,
            status: node.status,
            failure_class: node.failure_class,
            failure_reason: node.failure_reason.clone(),
            delta_vs_baseline,
            delta_vs_parent,
            tokens_used: node.tokens_used,
            wall_time_ms: node.wall_time_ms,
        });
    }

    let prefix = PrefixView::new(observations, baseline_score, max_parallelism);

    // Compute legal actions
    let mut legal_actions = Vec::new();
    let mut open_branches = std::collections::HashSet::new();
    for o in &prefix.observations {
        if o.branch_id != "baseline" && o.id != "att-baseline" {
            open_branches.insert(o.branch_id.clone());
        }
    }

    // Existing open frontiers that can be refined
    for bid in &open_branches {
        let traj = prefix.branch_trajectory(bid);
        if let Some(frontier_node) = traj.observations.last() {
            legal_actions.push(LegalAction::RefineFrontier {
                branch_id: bid.clone(),
                parent_id: frontier_node.id.clone(),
                node_id: format!("{}-next", frontier_node.id),
            });
        }
    }

    // Candidate root actions to explore new branches for this generation
    for idx in 0..max_parallelism {
        legal_actions.push(LegalAction::OpenRoot {
            branch_id: format!("branch-g{}-r{}", generation, idx),
            node_id: format!("root-g{}-r{}", generation, idx),
        });
    }

    policy.decide(&prefix, &legal_actions, beta)
}

/// Resolves the parent attempt ID, branch ID, and ActionType for an exploration action.
/// For OpenRoot actions, the parent is the active incumbent parent (or initial baseline if none).
/// For RefineFrontier actions, the parent is the target parent specified in the action.
pub fn resolve_action_dispatch(
    action: &LegalAction,
    active_parent_id: Option<&str>,
    baseline_id: &str,
) -> (String, String, ActionType) {
    let root_parent = active_parent_id.unwrap_or(baseline_id);
    match action {
        LegalAction::RefineFrontier {
            branch_id,
            parent_id,
            ..
        } => (
            parent_id.clone(),
            branch_id.clone(),
            ActionType::RefineFrontier,
        ),
        LegalAction::OpenRoot { branch_id, .. } => (
            root_parent.to_string(),
            branch_id.clone(),
            ActionType::OpenRoot,
        ),
    }
}

/// Resolve the immutable base git commit for an attempt, looking up the parent's base commit
/// or falling back to the current repository HEAD.
fn resolve_attempt_base_commit(
    attempts_file: &Path,
    parent_id: Option<&str>,
    repo_root: &Path,
) -> Option<String> {
    ast_tools::resolve_parent_base_commit(attempts_file, parent_id)
        .or_else(|| ast_tools::get_git_head_commit(repo_root))
}

/// Format the structured failure history and context of a specific parent attempt
/// to instruct the LLM on its refinement/repair task.
fn format_parent_refinement_context(attempts_file: &Path, parent_id: &str) -> String {
    let content = match std::fs::read_to_string(attempts_file) {
        Ok(c) => c,
        Err(_) => return format!("Parent Attempt ID: {parent_id}\n"),
    };
    for line in content.lines() {
        if let Ok(node) = serde_json::from_str::<AttemptNode>(line) {
            if node.id == parent_id {
                let mut out = format!(
                    "Parent Attempt ID: {}\nBranch ID: {}\nStatus: {}\nDescription: {}\n",
                    node.id, node.branch_id, node.status, node.description
                );
                if let Some(ref fc) = node.failure_class {
                    out.push_str(&format!("Failure Class: {:?}\n", fc));
                }
                if let Some(ref fr) = node.failure_reason {
                    out.push_str(&format!("Failure Reason: {}\n", fr));
                }
                if let Some(ref tail) = node.output_tail {
                    out.push_str(&format!(
                        "Failure Diagnostic Output Tail:\n```\n{}\n```\n",
                        tail
                    ));
                }
                if let Some(score) = node.composite_score {
                    out.push_str(&format!("Parent Composite Score: {:.4}\n", score));
                }
                out.push_str("Task: Specifically repair and refine this parent attempt state so that it passes compiler checks, test suites, and improves composite score.\n");
                return out;
            }
        }
    }
    format!("Parent Attempt ID: {parent_id}\n")
}

/// Densely log an attempt node to JSONL, aborting generation if write fails.
fn log_and_append_attempt(
    attempts_file: &Path,
    node: &AttemptNode,
    repo_root: &Path,
    generation: usize,
    gen_start: Instant,
) -> Result<(), TreeLogError> {
    if let Err(err) = AttemptTree::append_node_to_jsonl(attempts_file, node) {
        log_warning(&format!(
            "CRITICAL: Failed to append attempt node '{}' to {}: {err}; aborting generation",
            node.id,
            attempts_file.display()
        ));
        log_event(
            repo_root,
            &serde_json::json!({
                "event": "generation_end",
                "timestamp": chrono_now(),
                "generation": generation,
                "outcome": "aborted",
                "reason": format!("attempt logging failed: {err}"),
                "duration_secs": gen_start.elapsed().as_secs_f64(),
            }),
        );
        return Err(err);
    }
    Ok(())
}

/// Run the evolution daemon
pub async fn evolve(config: EvolutionConfig, repo_root: &Path) -> EvolutionResult {
    let start = Instant::now();
    let mut hall_of_fame: Vec<GenerationWinner> = Vec::new();
    let mut generation: usize = 0;
    let mut total_candidates_attempted: usize = 0;
    let mut total_candidates_evaluated: usize = 0;
    let mut total_infrastructure_failures: usize = 0;

    // Initialize durable attempt tree log directory and run file
    let run_id = format!(
        "run_{}_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S"),
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let attempts_dir = repo_root.join(".selfware").join("attempts");
    let _ = std::fs::create_dir_all(&attempts_dir);
    let attempts_file = attempts_dir.join(format!("{}.jsonl", run_id));

    // Acquire advisory exclusive lock on this run to prevent orphan sweeps from killing live runs
    let _run_lock = match RunLockGuard::acquire(repo_root, &run_id) {
        Ok(guard) => guard,
        Err(e) => {
            log_error(&format!("Fatal: could not acquire run lock: {e}"));
            return EvolutionResult {
                generations_run: 0,
                improvements: Vec::new(),
                final_sab_score: 0.0,
                initial_sab_score: 0.0,
                total_duration: start.elapsed(),
                aborted: Some(format!("could not acquire run lock: {e}")),
                outcome: "aborted".to_string(),
                stop_reason: None,
            };
        }
    };

    // Sweep any orphaned runs from previous sessions (e.g. killed by signal or missing run_end)
    sweep_orphaned_runs(repo_root);

    // Spawn signal listener to trigger cooperative shutdown via crate::request_shutdown()
    let signal_handle = tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).ok();
            let mut sigterm = signal(SignalKind::terminate()).ok();
            tokio::select! {
                _ = async {
                    if let Some(ref mut s) = sigint {
                        s.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {}
                _ = async {
                    if let Some(ref mut s) = sigterm {
                        s.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {}
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        crate::request_shutdown();
    });

    // Load promoted search policy if available from offline held-out replay validation
    let active_policy_file = repo_root.join(".selfware").join("active_policy.json");
    let loaded_active_policy = load_active_policy(&active_policy_file, config.population_size);
    let mut search_policy = loaded_active_policy.policy;
    let active_policy_name = loaded_active_policy.policy_name;
    let active_policy_beta = loaded_active_policy.beta;
    let active_policy_evidence_hash = loaded_active_policy.evidence_hash;
    let active_policy_fallback_reason = loaded_active_policy.fallback_reason;
    if let Some(ref reason) = active_policy_fallback_reason {
        if active_policy_file.exists() {
            log_warning(&format!("Active policy fallback: {reason}"));
        }
    }

    let mut initial_sab = 0.0;

    macro_rules! abort_run {
        ($reason:expr, $gen_count:expr, $final_sab:expr, $hof:expr) => {{
            signal_handle.abort();
            let reason_str = $reason;
            log_event(
                repo_root,
                &serde_json::json!({
                    "event": "run_end",
                    "kind": "run_end",
                    "timestamp": chrono_now(),
                    "run_id": &run_id,
                    "outcome": "aborted",
                    "reason": &reason_str,
                    "generations_run": $gen_count,
                    "final_sab_score": $final_sab,
                    "duration_secs": start.elapsed().as_secs_f64(),
                }),
            );
            return EvolutionResult {
                generations_run: $gen_count,
                improvements: $hof,
                final_sab_score: $final_sab,
                initial_sab_score: initial_sab,
                total_duration: start.elapsed(),
                aborted: Some(reason_str),
                outcome: "aborted".to_string(),
                stop_reason: None,
            };
        }};
    }

    // Fail-closed killswitch check before starting evolution
    if let Err(err) = crate::safety::killswitch::check_killswitch(Some(repo_root)) {
        log_warning(&format!("Killswitch active: {err}; aborting evolution"));
        abort_run!(format!("killswitch active: {err}"), 0, 0.0, Vec::new());
    }

    log_event(
        repo_root,
        &serde_json::json!({
            "event": "start",
            "run_id": run_id,
            "pid": std::process::id(),
            "timestamp": chrono_now(),
            "generations": config.generations,
            "population_size": config.population_size,
            "endpoint": config.llm.endpoint,
            "model": config.llm.model,
            "negative_feedback_history": true,
            "sab_noise_margin_mode": "empirical",
            "sab_noise_margin_default": DEFAULT_SAB_NOISE_MARGIN,
            "active_policy": active_policy_name,
            "active_policy_beta": active_policy_beta,
            "active_policy_evidence_hash": active_policy_evidence_hash,
        }),
    );

    // ═══════════════════════════════════════════════════════
    // MEASURE BASELINE
    // ═══════════════════════════════════════════════════════

    log_phase("Measuring baseline fitness...");
    let mut sab_config = SabConfig::default();
    if !config.llm.endpoint.is_empty() {
        sab_config.endpoint = config.llm.endpoint.clone();
    }
    if !config.llm.model.is_empty() {
        sab_config.model = config.llm.model.clone();
    }
    if let Ok(endpoint) = std::env::var("ENDPOINT") {
        sab_config.endpoint = endpoint;
    }
    if let Ok(model) = std::env::var("MODEL") {
        sab_config.model = model;
    }
    if let Ok(filter) = std::env::var("SAB_FILTER") {
        sab_config.scenario_filter = Some(
            filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        );
    }
    let sab_mode = std::env::var("SELFWARE_EVOLVE_SAB").is_ok();

    // Only run SAB baseline if explicitly requested via env var
    // (SAB runs all 12 scenarios and takes 30+ minutes). Otherwise use real
    // compile / test / fmt / clippy / binary-size metrics.
    let (baseline_metrics, mut current_baseline_sab) = if sab_mode {
        let selfware_binary = std::env::var("SELFWARE_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| repo_root.join("target/release/selfware"));
        match fitness::run_sab(&selfware_binary, &sab_config) {
            Ok(r) => {
                sab_config.exempt_reports.push(r.report_path.clone());
                if sab_config.exempt_reports.len() > 10 {
                    let excess = sab_config.exempt_reports.len() - 10;
                    sab_config.exempt_reports.drain(0..excess);
                }
                let m =
                    metrics_from_sab_result(&r, &selfware_binary, config.safety.max_binary_size_mb);
                (m, Some(r))
            }
            Err(e) => {
                if crate::is_shutdown_requested() {
                    signal_handle.abort();
                    log_warning(
                        "Baseline measurement interrupted by shutdown request; halting cleanly",
                    );
                    log_event(
                        repo_root,
                        &serde_json::json!({
                            "event": "run_end",
                            "kind": "run_end",
                            "timestamp": chrono_now(),
                            "run_id": &run_id,
                            "outcome": "killed",
                            "reason": "shutdown requested during baseline benchmark",
                            "generations_run": 0,
                            "final_sab_score": 0.0,
                            "duration_secs": start.elapsed().as_secs_f64(),
                        }),
                    );
                    return EvolutionResult {
                        generations_run: 0,
                        improvements: Vec::new(),
                        final_sab_score: 0.0,
                        initial_sab_score: 0.0,
                        total_duration: start.elapsed(),
                        aborted: Some("shutdown requested during baseline benchmark".to_string()),
                        outcome: "killed".to_string(),
                        stop_reason: Some("shutdown requested".to_string()),
                    };
                }
                log_warning(&format!(
                    "SAB baseline failed ({e}); refusing to evolve against an unmeasured baseline"
                ));
                abort_run!(
                    format!(
                        "baseline measurement failed: {e}. Promotion needs a real baseline \
                         to compare against; scoring candidates against a placeholder \
                         cannot show improvement."
                    ),
                    0,
                    0.0,
                    Vec::new()
                );
            }
        }
    } else {
        log_phase("Using compile+test fitness (set SELFWARE_EVOLVE_SAB=1 for full SAB)");
        match measure_compile_test_baseline(repo_root, EVOLVE_FEATURES, DEFAULT_TIMEOUT_SECS).await
        {
            Ok(mut m) => {
                m.max_binary_size_mb = config.safety.max_binary_size_mb;
                (m, None)
            }
            Err(e) => {
                if crate::is_shutdown_requested() || e.contains("shutdown requested") {
                    signal_handle.abort();
                    log_warning(
                        "Baseline measurement interrupted by shutdown request; halting cleanly",
                    );
                    log_event(
                        repo_root,
                        &serde_json::json!({
                            "event": "run_end",
                            "kind": "run_end",
                            "timestamp": chrono_now(),
                            "run_id": &run_id,
                            "outcome": "killed",
                            "reason": "shutdown requested during baseline benchmark",
                            "generations_run": 0,
                            "final_sab_score": 0.0,
                            "duration_secs": start.elapsed().as_secs_f64(),
                        }),
                    );
                    return EvolutionResult {
                        generations_run: 0,
                        improvements: Vec::new(),
                        final_sab_score: 0.0,
                        initial_sab_score: 0.0,
                        total_duration: start.elapsed(),
                        aborted: Some("shutdown requested during baseline benchmark".to_string()),
                        outcome: "killed".to_string(),
                        stop_reason: Some("shutdown requested".to_string()),
                    };
                }
                log_warning(&format!(
                    "Compile/test baseline failed ({e}); refusing to evolve against an \
                      unmeasured baseline"
                ));
                abort_run!(
                    format!(
                        "baseline measurement failed: {e}. Promotion needs a real baseline \
                         to compare against; scoring candidates against a placeholder \
                         cannot show improvement."
                    ),
                    0,
                    0.0,
                    Vec::new()
                );
            }
        }
    };

    initial_sab = baseline_metrics.sab_score;
    let mut current_baseline_metrics = baseline_metrics;

    log_baseline(&current_baseline_metrics, sab_mode);

    if !sab_mode && current_baseline_metrics.sab_score >= 100.0 {
        log_phase("  ℹ Baseline compile+test suite is 100% green. Without SELFWARE_EVOLVE_SAB=1, fitness scores are capped at 100.0 with unmeasured tokens. Evolution will operate in tree data-collection mode for offline Dream-RSI policy replay.");
    }

    let initial_baseline_composite = config.fitness_weights.composite(&current_baseline_metrics);
    log_event(
        repo_root,
        &serde_json::json!({
            "event": "baseline",
            "timestamp": chrono_now(),
            "run_id": &run_id,
            "sab_score": current_baseline_metrics.sab_score,
            "composite": initial_baseline_composite,
            "mode": if sab_mode { "sab" } else { "compile_test" },
        }),
    );
    let baseline_tree_id = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["rev-parse", "HEAD^{tree}"])
        .current_dir(repo_root)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !s.is_empty() {
                    Some(s)
                } else {
                    None
                }
            } else {
                None
            }
        });
    let baseline_node = AttemptNode {
        id: "att-baseline".to_string(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".to_string(),
        hypothesis_id: "baseline".to_string(),
        description: "Initial baseline capability measurement".to_string(),
        diff_sha256: compute_sha256(b""),
        patch: None,
        sab_report_path: current_baseline_sab.as_ref().map(|r| r.report_path.clone()),
        metrics: Some(current_baseline_metrics.clone()),
        composite_score: Some(initial_baseline_composite),
        tokens_used: current_baseline_metrics.tokens_used,
        wall_time_ms: 0,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: current_baseline_sab
            .as_ref()
            .map(|r| r.binary_sha256.clone()),
        base_commit: ast_tools::get_git_head_commit(repo_root),
        committed_commit: None,
        action_type: None,
        git_tree_id: baseline_tree_id,
        created_at: chrono_now(),
    };
    if let Err(err) = log_and_append_attempt(&attempts_file, &baseline_node, repo_root, 0, start) {
        abort_run!(
            format!("initial baseline logging failed: {err}"),
            0,
            current_baseline_metrics.sab_score,
            Vec::new()
        );
    }
    let mut active_parent_id: Option<String> = Some(baseline_node.id.clone());
    let mut policy_stopped_reason: Option<String> = None;
    let mut expected_head_commit = ast_tools::get_git_head_commit(repo_root);

    // ═══════════════════════════════════════════════════════
    // MAIN EVOLUTIONARY LOOP
    // ═══════════════════════════════════════════════════════

    loop {
        if crate::is_shutdown_requested() {
            log_warning("Shutdown requested; halting evolutionary loop cleanly");
            break;
        }
        generation += 1;
        if config.generations > 0 && generation > config.generations {
            break;
        }

        // Check for early HEAD drift before running expensive evaluations
        if let Some(ref expected) = expected_head_commit {
            let actual_head = ast_tools::get_git_head_commit(repo_root);
            if let Some(ref actual) = actual_head {
                if actual != expected {
                    log_warning(&format!(
                        "HEAD drift detected at generation {generation}: expected HEAD {expected}, but repo HEAD is {actual} (concurrent commit detected); aborting run early"
                    ));
                    abort_run!(
                        format!("HEAD drift detected: expected {expected}, actual {actual}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
            }
        }

        // Check fail-closed killswitch at each generation start
        if let Err(err) = crate::safety::killswitch::check_killswitch(Some(repo_root)) {
            log_warning(&format!(
                "Killswitch active at generation {generation}: {err}; halting evolution"
            ));
            abort_run!(
                format!("killswitch tripped at generation {generation}: {err}"),
                generation.saturating_sub(1),
                current_baseline_metrics.sab_score,
                hall_of_fame
            );
        }

        log_generation_start(generation);
        let gen_start = Instant::now();
        log_event(
            repo_root,
            &serde_json::json!({
                "event": "generation_start",
                "timestamp": chrono_now(),
                "generation": generation,
            }),
        );

        // ─── Step 0: Consult active search policy ───
        let policy_decision = decide_next_search_action(
            &mut *search_policy,
            &attempts_file,
            initial_baseline_composite,
            config.population_size,
            generation,
            active_policy_beta,
        );

        let mut active_actions = match policy_decision {
            PolicyDecision::Stop { reason } => {
                log_phase(&format!(
                    "🛑 Active search policy '{}' terminated search: {reason}",
                    search_policy.name()
                ));
                log_event(
                    repo_root,
                    &serde_json::json!({
                        "event": "policy_stop",
                        "policy": search_policy.name(),
                        "reason": &reason,
                        "generation": generation,
                        "timestamp": chrono_now(),
                    }),
                );
                policy_stopped_reason = Some(reason);
                break;
            }
            PolicyDecision::SelectBatch(actions) => {
                if actions.is_empty() {
                    log_phase(&format!(
                        "🛑 Active search policy '{}' returned empty action batch; terminating search",
                        search_policy.name()
                    ));
                    policy_stopped_reason =
                        Some("Empty action batch returned by policy".to_string());
                    break;
                }
                actions
            }
        };

        if active_actions.len() > config.population_size {
            log_phase(&format!(
                "Search policy proposed {} actions; capping to population_size {}",
                active_actions.len(),
                config.population_size
            ));
            active_actions.truncate(config.population_size);
        }

        // ─── Step 1: Capture telemetry (sensory data for the agent) ───
        let telemetry_snapshot = telemetry::capture(repo_root, "sab_full").ok();
        let telemetry_prompt = telemetry_snapshot
            .as_ref()
            .map(telemetry::to_agent_prompt)
            .unwrap_or_default();

        // Concise structured failure history (Promptbreeder / AlphaEvolve):
        // Tell the model what failed recently so it does not loop on the same target.
        let history_prompt = format_recent_failure_history(&attempts_file, 5);

        // ─── Step 2: Generate hypotheses via agent swarm (1 per active action) ───
        let llm_start = Instant::now();
        let mut hypotheses_with_actions = Vec::new();
        const MAX_EMPTY_LLM_RETRIES: usize = 3;

        for retry_idx in 0..MAX_EMPTY_LLM_RETRIES {
            if crate::is_shutdown_requested() {
                break;
            }
            if retry_idx > 0 {
                log_warning(&format!(
                    "No valid hypotheses generated, retrying generation {generation} (attempt {}/{MAX_EMPTY_LLM_RETRIES})...",
                    retry_idx + 1
                ));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            for (action_idx, action) in active_actions.iter().enumerate() {
                if crate::is_shutdown_requested() {
                    break;
                }
                let (h_parent_id_str, h_branch_id, h_action_type) =
                    resolve_action_dispatch(action, active_parent_id.as_deref(), &baseline_node.id);
                let h_parent_id = Some(h_parent_id_str.clone());

                // Restore parent/root worktree to inspect exact source state.
                // Read root context from the latest active incumbent/baseline checkout used for testing.
                // Restoration errors produce a recorded failure instead of silently falling back to repo_root.
                let (source_context, parent_context, temp_worktree) = match action {
                    LegalAction::RefineFrontier { parent_id, .. } => {
                        let worktree = match ast_tools::create_shadow_worktree_for_parent(
                            repo_root,
                            &attempts_file,
                            Some(parent_id),
                        ) {
                            Ok(w) => w,
                            Err(e) => {
                                log_warning(&format!(
                                    "Refinement worktree restoration failed for parent '{parent_id}': {e}"
                                ));
                                let node = AttemptNode {
                                    id: format!(
                                        "att-g{generation}-r{retry_idx}-refine-restoration-fail-{action_idx}"
                                    ),
                                    parent_id: Some(parent_id.clone()),
                                    generation,
                                    branch_id: h_branch_id.clone(),
                                    hypothesis_id: format!("hyp-r{retry_idx}-restore-fail-{action_idx}"),
                                    description: format!(
                                        "Refinement restoration failed for parent {parent_id}"
                                    ),
                                    diff_sha256: compute_sha256(b""),
                                    patch: None,
                                    sab_report_path: None,
                                    metrics: None,
                                    composite_score: None,
                                    tokens_used: None,
                                    wall_time_ms: 0,
                                    status: AttemptStatus::InternalError,
                                    failure_class: Some(FailureClass::EnvironmentError),
                                    failure_reason: Some(format!(
                                        "Refinement worktree restoration failed: {e}"
                                    )),
                                    output_tail: None,
                                    binary_sha256: None,
                                    base_commit: resolve_attempt_base_commit(
                                        &attempts_file,
                                        Some(parent_id),
                                        repo_root,
                                    ),
                                    committed_commit: None,
                                    action_type: Some(ActionType::RefineFrontier),
                                    git_tree_id: None,
                                    created_at: chrono_now(),
                                };
                                total_infrastructure_failures += 1;
                                let _ = log_and_append_attempt(
                                    &attempts_file,
                                    &node,
                                    repo_root,
                                    generation,
                                    gen_start,
                                );
                                continue;
                            }
                        };
                        let source = read_mutation_targets(&config.mutation_targets, &worktree);
                        let parent_info =
                            format_parent_refinement_context(&attempts_file, parent_id);
                        (source, parent_info, Some(worktree))
                    }
                    LegalAction::OpenRoot { .. } => {
                        let worktree = match ast_tools::create_shadow_worktree_for_parent(
                            repo_root,
                            &attempts_file,
                            Some(&h_parent_id_str),
                        ) {
                            Ok(w) => w,
                            Err(e) => {
                                log_warning(&format!(
                                    "Root worktree restoration failed for parent '{h_parent_id_str}': {e}"
                                ));
                                let node = AttemptNode {
                                    id: format!(
                                        "att-g{generation}-r{retry_idx}-root-restoration-fail-{action_idx}"
                                    ),
                                    parent_id: Some(h_parent_id_str.clone()),
                                    generation,
                                    branch_id: h_branch_id.clone(),
                                    hypothesis_id: format!("hyp-r{retry_idx}-root-restore-fail-{action_idx}"),
                                    description: format!(
                                        "Root worktree restoration failed for parent '{h_parent_id_str}'"
                                    ),
                                    diff_sha256: compute_sha256(b""),
                                    patch: None,
                                    sab_report_path: None,
                                    metrics: None,
                                    composite_score: None,
                                    tokens_used: None,
                                    wall_time_ms: 0,
                                    status: AttemptStatus::InternalError,
                                    failure_class: Some(FailureClass::EnvironmentError),
                                    failure_reason: Some(format!(
                                        "Root worktree restoration failed: {e}"
                                    )),
                                    output_tail: None,
                                    binary_sha256: None,
                                    base_commit: resolve_attempt_base_commit(
                                        &attempts_file,
                                        Some(&h_parent_id_str),
                                        repo_root,
                                    ),
                                    committed_commit: None,
                                    action_type: Some(ActionType::OpenRoot),
                                    git_tree_id: None,
                                    created_at: chrono_now(),
                                };
                                total_infrastructure_failures += 1;
                                let _ = log_and_append_attempt(
                                    &attempts_file,
                                    &node,
                                    repo_root,
                                    generation,
                                    gen_start,
                                );
                                continue;
                            }
                        };
                        let source = read_mutation_targets(&config.mutation_targets, &worktree);
                        (source, String::new(), Some(worktree))
                    }
                };

                let source_dir = temp_worktree.as_deref().unwrap_or(repo_root);
                let hyp = generate_single_action_hypothesis(
                    &config,
                    &telemetry_prompt,
                    &history_prompt,
                    &source_context,
                    &parent_context,
                    source_dir,
                )
                .await;

                if let Some(ref w) = temp_worktree {
                    let _ = ast_tools::cleanup_worktree(repo_root, w);
                }

                if let Some(mut h) = hyp {
                    h.id = format!("g{}-hyp{}", generation, action_idx);
                    hypotheses_with_actions.push((h, h_parent_id, h_branch_id, h_action_type));
                }
            }

            if !hypotheses_with_actions.is_empty() {
                break;
            }
        }

        log_event(
            repo_root,
            &serde_json::json!({
                "event": "hypotheses_generated",
                "timestamp": chrono_now(),
                "generation": generation,
                "count": hypotheses_with_actions.len(),
                "descriptions": hypotheses_with_actions.iter().map(|(h, _, _, _)| &h.description).collect::<Vec<_>>(),
                "llm_duration_secs": llm_start.elapsed().as_secs_f64(),
            }),
        );

        if hypotheses_with_actions.is_empty() {
            log_warning(
                "No valid hypotheses generated after retries, continuing to next generation",
            );
            continue;
        }

        // ─── Step 3: Safety & duplicate filter ───
        // Gate on the paths the patch ACTUALLY edits, not the LLM-declared
        // target_files metadata (free-form JSON, never cross-checked — an
        // empty array passed trivially). See hypothesis_touches_protected.
        // Also detect and reject duplicate patches matching previously failed diffs.
        let prior_failed_diffs = load_failed_diff_shas(&attempts_file);
        let mut valid = Vec::new();
        for (h, h_parent_id, h_branch_id, h_action_type) in hypotheses_with_actions {
            let diff_sha256 = compute_sha256(h.patch.as_bytes());
            if hypothesis_touches_protected(&h) {
                log_warning(&format!(
                    "Hypothesis '{}' touches protected files, rejected",
                    h.id
                ));
                let node = AttemptNode {
                    id: format!("att-g{}-{}", generation, h.id),
                    parent_id: h_parent_id.clone(),
                    generation,
                    branch_id: h_branch_id.clone(),
                    hypothesis_id: h.id.clone(),
                    description: h.description.clone(),
                    diff_sha256,
                    patch: Some(h.patch.clone()),
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: 0,
                    status: AttemptStatus::SafetyRejected,
                    failure_class: Some(FailureClass::SafetyViolation),
                    failure_reason: Some("Touches protected paths".into()),
                    output_tail: None,
                    binary_sha256: None,
                    base_commit: None,
                    committed_commit: None,
                    action_type: Some(h_action_type),
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                continue;
            }

            if prior_failed_diffs.contains(&diff_sha256) {
                log_warning(&format!(
                    "Hypothesis '{}' matches previously failed patch diff (sha256: {}), rejected as duplicate",
                    h.id, diff_sha256
                ));
                let node = AttemptNode {
                    id: format!("att-g{}-{}", generation, h.id),
                    parent_id: h_parent_id.clone(),
                    generation,
                    branch_id: h_branch_id.clone(),
                    hypothesis_id: h.id.clone(),
                    description: h.description.clone(),
                    diff_sha256: diff_sha256.clone(),
                    patch: Some(h.patch.clone()),
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: 0,
                    status: AttemptStatus::DuplicateRejected,
                    failure_class: Some(FailureClass::Unclassified),
                    failure_reason: Some(format!(
                        "Duplicate of previously failed patch diff (sha256: {})",
                        diff_sha256
                    )),
                    output_tail: None,
                    binary_sha256: None,
                    base_commit: None,
                    committed_commit: None,
                    action_type: Some(h_action_type),
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                continue;
            }

            valid.push((h, h_parent_id, h_branch_id, h_action_type));
        }

        if valid.is_empty() {
            log_warning("All hypotheses rejected by safety/duplicate filter");
            // Backoff to avoid 100% CPU busy-loop.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        log_phase(&format!("Evaluating {} hypotheses...", valid.len()));

        // ─── Step 3.5: Control anchor check (verify sandbox viability) ───
        // Before running mutant evaluations, verify that an unpatched worktree
        // compiles and passes tests cleanly. If the clean sandbox fails, the
        // build harness or environment is compromised (e.g. toolchain, port lock, resource exhaustion).
        // Aborting the generation prevents misclassifying harness failures as mutant slips.
        let control_start = Instant::now();
        let control_base_commit = ast_tools::get_git_head_commit(repo_root);
        let control_worktree = match ast_tools::create_shadow_worktree(repo_root) {
            Ok(w) => w,
            Err(e) => {
                log_warning(&format!(
                    "Control worktree creation failed: {e}; aborting generation {generation}"
                ));
                let node = AttemptNode {
                    id: format!("att-g{}-control", generation),
                    parent_id: active_parent_id.clone(),
                    generation,
                    branch_id: "control".to_string(),
                    hypothesis_id: "control".to_string(),
                    description: "Unpatched control anchor".to_string(),
                    diff_sha256: compute_sha256(b""),
                    patch: None,
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: control_start.elapsed().as_millis() as u64,
                    status: AttemptStatus::InternalError,
                    failure_class: Some(FailureClass::EnvironmentError),
                    failure_reason: Some(format!("Control worktree failed: {e}")),
                    output_tail: None,
                    binary_sha256: None,
                    base_commit: control_base_commit.clone(),
                    committed_commit: None,
                    action_type: None,
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed during control failure: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };
        let _ctrl_guard = WorktreeGuard::new(repo_root, control_worktree.clone());

        let mut ctrl_check_cmd = tokio::process::Command::new("cargo");
        ctrl_check_cmd
            .arg("check")
            .arg("--all-targets")
            .arg("--features")
            .arg(features_arg(EVOLVE_FEATURES))
            .env("CARGO_TARGET_DIR", evolution_target_dir(repo_root))
            .stdin(std::process::Stdio::null())
            .current_dir(&control_worktree);
        let ctrl_check = run_cancellable_subprocess(
            ctrl_check_cmd,
            std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS as u64),
        )
        .await;
        if matches!(&ctrl_check, Err(SubprocessError::ShutdownRequested)) {
            log_warning("Shutdown requested during control check; halting evolution cleanly");
            break;
        }
        let ctrl_check_failed = match &ctrl_check {
            Ok(o) => !o.status.success(),
            Err(_) => true,
        };

        if ctrl_check_failed {
            let tail = ctrl_check.ok().map(|o| {
                tail_lines(
                    &format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    ),
                    50,
                )
            });
            log_warning(&format!(
                "Sandbox control compile check failed at generation {generation}; skipping mutant evaluations"
            ));
            let node = AttemptNode {
                id: format!("att-g{}-control", generation),
                parent_id: active_parent_id.clone(),
                generation,
                branch_id: "control".to_string(),
                hypothesis_id: "control".to_string(),
                description: "Unpatched control anchor compile check".to_string(),
                diff_sha256: compute_sha256(b""),
                patch: None,
                sab_report_path: None,
                metrics: None,
                composite_score: None,
                tokens_used: None,
                wall_time_ms: control_start.elapsed().as_millis() as u64,
                status: AttemptStatus::InternalError,
                failure_class: Some(FailureClass::EnvironmentError),
                failure_reason: Some("Control compile check failed in clean worktree".to_string()),
                output_tail: tail,
                binary_sha256: None,
                base_commit: control_base_commit.clone(),
                committed_commit: None,
                action_type: None,
                git_tree_id: None,
                created_at: chrono_now(),
            };
            if let Err(err) =
                log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
            {
                abort_run!(
                    format!("attempt logging failed during control failure: {err}"),
                    generation.saturating_sub(1),
                    current_baseline_metrics.sab_score,
                    hall_of_fame
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        let mut ctrl_test_cmd = tokio::process::Command::new("cargo");
        ctrl_test_cmd
            .arg("test")
            .arg("--lib")
            .arg("--features")
            .arg(features_arg(EVOLVE_FEATURES))
            .env("CARGO_TARGET_DIR", evolution_target_dir(repo_root))
            .stdin(std::process::Stdio::null())
            .current_dir(&control_worktree);
        let ctrl_test = run_cancellable_subprocess(
            ctrl_test_cmd,
            std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS as u64),
        )
        .await;
        if matches!(&ctrl_test, Err(SubprocessError::ShutdownRequested)) {
            log_warning("Shutdown requested during control test; halting evolution cleanly");
            break;
        }
        let ctrl_test_passed = match &ctrl_test {
            Ok(o) => {
                let combined = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                o.status.success() && combined.lines().any(|l| l.contains("test result:"))
            }
            Err(_) => false,
        };

        if !ctrl_test_passed {
            let tail = ctrl_test.ok().map(|o| {
                tail_lines(
                    &format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    ),
                    50,
                )
            });
            log_warning(&format!(
                "Sandbox control test failed or missing summary at generation {generation}; skipping mutant evaluations"
            ));
            let node = AttemptNode {
                id: format!("att-g{}-control", generation),
                parent_id: active_parent_id.clone(),
                generation,
                branch_id: "control".to_string(),
                hypothesis_id: "control".to_string(),
                description: "Unpatched control anchor test suite".to_string(),
                diff_sha256: compute_sha256(b""),
                patch: None,
                sab_report_path: None,
                metrics: None,
                composite_score: None,
                tokens_used: None,
                wall_time_ms: control_start.elapsed().as_millis() as u64,
                status: AttemptStatus::InternalError,
                failure_class: Some(FailureClass::EnvironmentError),
                failure_reason: Some(
                    "Control test suite failed or had no summary in clean worktree".to_string(),
                ),
                output_tail: tail,
                binary_sha256: None,
                base_commit: control_base_commit,
                committed_commit: None,
                action_type: None,
                git_tree_id: None,
                created_at: chrono_now(),
            };
            if let Err(err) =
                log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
            {
                abort_run!(
                    format!("attempt logging failed during control failure: {err}"),
                    generation.saturating_sub(1),
                    current_baseline_metrics.sab_score,
                    hall_of_fame
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        drop(_ctrl_guard);

        // ─── Step 4: Evaluate each hypothesis (apply → check → test) ───
        let sab_available =
            sab_config.runner_script.exists() && std::env::var("SELFWARE_EVOLVE_SAB").is_ok();
        let mut evaluated_candidates: Vec<EvaluatedCandidate> = Vec::new();

        for (hypothesis, hyp_parent_id, hyp_branch_id, hyp_action_type) in &valid {
            if crate::is_shutdown_requested() {
                log_warning("Shutdown requested; halting candidate evaluations cleanly");
                break;
            }
            let attempt_start = Instant::now();
            let attempt_id = format!("att-g{}-{}", generation, hypothesis.id);
            let attempt_base_commit =
                resolve_attempt_base_commit(&attempts_file, hyp_parent_id.as_deref(), repo_root);
            let raw_diff_sha256 = compute_sha256(hypothesis.patch.as_bytes());

            let make_cancelled_node = |phase: &str| AttemptNode {
                id: attempt_id.clone(),
                parent_id: hyp_parent_id.clone(),
                generation,
                branch_id: hyp_branch_id.clone(),
                hypothesis_id: hypothesis.id.clone(),
                description: hypothesis.description.clone(),
                diff_sha256: raw_diff_sha256.clone(),
                patch: Some(hypothesis.patch.clone()),
                sab_report_path: None,
                metrics: None,
                composite_score: None,
                tokens_used: None,
                wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                status: AttemptStatus::Cancelled,
                failure_class: Some(FailureClass::EnvironmentError),
                failure_reason: Some(format!(
                    "Candidate evaluation cancelled by shutdown during {phase}"
                )),
                output_tail: None,
                binary_sha256: None,
                base_commit: attempt_base_commit.clone(),
                committed_commit: None,
                action_type: Some(*hyp_action_type),
                git_tree_id: None,
                created_at: chrono_now(),
            };

            log_phase(&format!(
                "  Testing '{}' [{}]...",
                hypothesis.description, hypothesis.id
            ));
            total_candidates_attempted += 1;

            // Create worktree restored to the selected parent's exact source state.
            // The guard removes it on EVERY exit path from this iteration.
            let worktree = match ast_tools::create_shadow_worktree_for_parent(
                repo_root,
                &attempts_file,
                hyp_parent_id.as_deref(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    log_warning(&format!("  Worktree failed: {}", e));
                    total_infrastructure_failures += 1;
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: raw_diff_sha256.clone(),
                        patch: Some(hypothesis.patch.clone()),
                        sab_report_path: None,
                        metrics: None,
                        composite_score: None,
                        tokens_used: None,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: AttemptStatus::InternalError,
                        failure_class: Some(FailureClass::EnvironmentError),
                        failure_reason: Some(format!("Worktree creation failed: {e}")),
                        output_tail: None,
                        binary_sha256: None,
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }
            };
            let _worktree_guard = WorktreeGuard::new(repo_root, worktree.clone());
            total_candidates_evaluated += 1;

            // Apply edits (search-and-replace or unified diff)
            if !apply_patch_to_worktree(&worktree, &hypothesis.patch) {
                log_frost(generation, &format!("Patch failed: {}", hypothesis.id));
                let node = AttemptNode {
                    id: attempt_id.clone(),
                    parent_id: hyp_parent_id.clone(),
                    generation,
                    branch_id: hyp_branch_id.clone(),
                    hypothesis_id: hypothesis.id.clone(),
                    description: hypothesis.description.clone(),
                    diff_sha256: raw_diff_sha256.clone(),
                    patch: Some(hypothesis.patch.clone()),
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                    status: AttemptStatus::PatchFailed,
                    failure_class: Some(FailureClass::Unclassified),
                    failure_reason: Some("Patch failed to apply cleanly".into()),
                    output_tail: None,
                    binary_sha256: None,
                    base_commit: attempt_base_commit.clone(),
                    committed_commit: None,
                    action_type: Some(*hyp_action_type),
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                continue;
            }

            // Format FIRST — the fmt auto-fix must not change code after it
            // was tested, otherwise the committed bytes differ from the
            // tested bytes.
            let mut fmt_check_cmd = tokio::process::Command::new("cargo");
            fmt_check_cmd
                .args(["fmt", "--", "--check"])
                .current_dir(&worktree);
            let fmt_check =
                run_cancellable_subprocess(fmt_check_cmd, std::time::Duration::from_secs(120))
                    .await;
            if matches!(&fmt_check, Err(SubprocessError::ShutdownRequested)) {
                log_warning("Shutdown requested during candidate fmt check; halting cleanly");
                let node = make_cancelled_node("fmt check");
                let _ =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start);
                break;
            }

            if fmt_check.map(|o| !o.status.success()).unwrap_or(true) {
                let mut fmt_fix_cmd = tokio::process::Command::new("cargo");
                fmt_fix_cmd.arg("fmt").current_dir(&worktree);
                let fmt_fix =
                    run_cancellable_subprocess(fmt_fix_cmd, std::time::Duration::from_secs(120))
                        .await;
                if matches!(&fmt_fix, Err(SubprocessError::ShutdownRequested)) {
                    log_warning(
                        "Shutdown requested during candidate fmt auto-fix; halting cleanly",
                    );
                    let node = make_cancelled_node("fmt fix");
                    let _ = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    );
                    break;
                }
                let (fmt_failed, fmt_tail) = match &fmt_fix {
                    Ok(o) => (
                        !o.status.success(),
                        Some(tail_lines(
                            &format!(
                                "{}\n{}",
                                String::from_utf8_lossy(&o.stdout),
                                String::from_utf8_lossy(&o.stderr)
                            ),
                            50,
                        )),
                    ),
                    Err(e) => (true, Some(format!("cargo fmt error: {e}"))),
                };
                if fmt_failed {
                    log_frost(generation, &format!("cargo fmt failed: {}", hypothesis.id));
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: raw_diff_sha256.clone(),
                        patch: Some(hypothesis.patch.clone()),
                        sab_report_path: None,
                        metrics: None,
                        composite_score: None,
                        tokens_used: None,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: AttemptStatus::FormatFailed,
                        failure_class: Some(FailureClass::RepairableSyntax),
                        failure_reason: Some("cargo fmt failed".into()),
                        output_tail: fmt_tail,
                        binary_sha256: None,
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }
            }

            // Compile check
            let mut check_cmd = tokio::process::Command::new("cargo");
            check_cmd
                .arg("check")
                .arg("--all-targets")
                .arg("--features")
                .arg(features_arg(EVOLVE_FEATURES))
                .env("CARGO_TARGET_DIR", evolution_target_dir(repo_root))
                .stdin(std::process::Stdio::null())
                .current_dir(&worktree);
            let check =
                run_cancellable_subprocess(check_cmd, std::time::Duration::from_secs(300)).await;
            if matches!(&check, Err(SubprocessError::ShutdownRequested)) {
                log_warning(
                    "Shutdown requested during candidate compilation check; halting cleanly",
                );
                let node = make_cancelled_node("compile check");
                let _ =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start);
                break;
            }
            let (check_failed, check_tail) = match &check {
                Ok(o) => (
                    !o.status.success(),
                    Some(tail_lines(
                        &format!(
                            "{}\n{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr)
                        ),
                        50,
                    )),
                ),
                Err(e) => (true, Some(format!("Check error: {e}"))),
            };

            if check_failed {
                log_frost(generation, &format!("Compile failed: {}", hypothesis.id));
                let node = AttemptNode {
                    id: attempt_id.clone(),
                    parent_id: hyp_parent_id.clone(),
                    generation,
                    branch_id: hyp_branch_id.clone(),
                    hypothesis_id: hypothesis.id.clone(),
                    description: hypothesis.description.clone(),
                    diff_sha256: raw_diff_sha256.clone(),
                    patch: Some(hypothesis.patch.clone()),
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                    status: AttemptStatus::CompileFailed,
                    failure_class: Some(FailureClass::RepairableSyntax),
                    failure_reason: Some("cargo check failed".into()),
                    output_tail: check_tail,
                    binary_sha256: None,
                    base_commit: attempt_base_commit.clone(),
                    committed_commit: None,
                    action_type: Some(*hyp_action_type),
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                continue;
            }

            // Run tests
            let test_start = Instant::now();
            let mut test_cmd = tokio::process::Command::new("cargo");
            test_cmd
                .arg("test")
                .arg("--lib")
                .arg("--features")
                .arg(features_arg(EVOLVE_FEATURES))
                .env("CARGO_TARGET_DIR", evolution_target_dir(repo_root))
                .stdin(std::process::Stdio::null())
                .current_dir(&worktree);
            let test = run_cancellable_subprocess(
                test_cmd,
                std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS as u64),
            )
            .await;
            if matches!(&test, Err(SubprocessError::ShutdownRequested)) {
                log_warning("Shutdown requested during candidate test execution; halting cleanly");
                let node = make_cancelled_node("test execution");
                let _ =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start);
                break;
            }

            let test_output = match test {
                Ok(o) => o,
                Err(SubprocessError::Timeout) => {
                    log_warning(&format!(
                        "  Test execution timed out ({DEFAULT_TIMEOUT_SECS}s)"
                    ));
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: raw_diff_sha256.clone(),
                        patch: Some(hypothesis.patch.clone()),
                        sab_report_path: None,
                        metrics: None,
                        composite_score: None,
                        tokens_used: None,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: AttemptStatus::Timeout,
                        failure_class: Some(FailureClass::Unclassified),
                        failure_reason: Some(format!(
                            "Test execution timed out ({DEFAULT_TIMEOUT_SECS}s)"
                        )),
                        output_tail: None,
                        binary_sha256: None,
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }
                Err(e) => {
                    log_warning(&format!("  Test execution failed: {}", e));
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: raw_diff_sha256.clone(),
                        patch: Some(hypothesis.patch.clone()),
                        sab_report_path: None,
                        metrics: None,
                        composite_score: None,
                        tokens_used: None,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: AttemptStatus::InternalError,
                        failure_class: Some(FailureClass::EnvironmentError),
                        failure_reason: Some(format!("Test execution failed: {e}")),
                        output_tail: None,
                        binary_sha256: None,
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    total_infrastructure_failures += 1;
                    total_candidates_evaluated = total_candidates_evaluated.saturating_sub(1);
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }
            };

            let test_passed = test_output.status.success();
            let test_duration = test_start.elapsed();

            if !test_passed {
                let stdout = String::from_utf8_lossy(&test_output.stdout);
                let stderr = String::from_utf8_lossy(&test_output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);
                let has_test_summary = combined.lines().any(|l| l.contains("test result:"));
                let fail_count = combined
                    .lines()
                    .find(|l| l.contains("test result:"))
                    .unwrap_or("unknown");
                let tail = tail_lines(&combined, 50);

                let (status, failure_class, failure_reason) = if has_test_summary {
                    log_frost(
                        generation,
                        &format!("Tests failed: {} — {}", hypothesis.id, fail_count),
                    );
                    (
                        AttemptStatus::TestFailed,
                        Some(FailureClass::RepairableTestFailure),
                        Some(format!("Tests failed: {fail_count}")),
                    )
                } else {
                    log_warning(&format!(
                        "Tests failed without summary (harness/environment failure): {}",
                        hypothesis.id
                    ));
                    total_infrastructure_failures += 1;
                    total_candidates_evaluated = total_candidates_evaluated.saturating_sub(1);
                    (
                        AttemptStatus::InternalError,
                        Some(FailureClass::EnvironmentError),
                        Some(
                            "Tests failed without summary (harness/environment failure)"
                                .to_string(),
                        ),
                    )
                };

                let node = AttemptNode {
                    id: attempt_id.clone(),
                    parent_id: hyp_parent_id.clone(),
                    generation,
                    branch_id: hyp_branch_id.clone(),
                    hypothesis_id: hypothesis.id.clone(),
                    description: hypothesis.description.clone(),
                    diff_sha256: raw_diff_sha256.clone(),
                    patch: Some(hypothesis.patch.clone()),
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                    status,
                    failure_class,
                    failure_reason,
                    output_tail: Some(tail),
                    binary_sha256: None,
                    base_commit: attempt_base_commit.clone(),
                    committed_commit: None,
                    action_type: Some(*hyp_action_type),
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                continue;
            }

            // Clippy lint gate — reject code with clippy warnings
            let mut clippy_cmd = tokio::process::Command::new("cargo");
            clippy_cmd
                .arg("clippy")
                .arg("--all-targets")
                .arg("--features")
                .arg(features_arg(EVOLVE_FEATURES))
                .env("CARGO_TARGET_DIR", evolution_target_dir(repo_root))
                .stdin(std::process::Stdio::null())
                .current_dir(&worktree);
            clippy_cmd.args(["--", "-D", "warnings"]);
            let clippy =
                run_cancellable_subprocess(clippy_cmd, std::time::Duration::from_secs(300)).await;
            if matches!(&clippy, Err(SubprocessError::ShutdownRequested)) {
                log_warning("Shutdown requested during candidate clippy check; halting cleanly");
                let node = make_cancelled_node("clippy check");
                let _ =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start);
                break;
            }
            let (clippy_failed, clippy_tail) = match &clippy {
                Ok(o) => (
                    !o.status.success(),
                    Some(tail_lines(
                        &format!(
                            "{}\n{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr)
                        ),
                        50,
                    )),
                ),
                Err(e) => (true, Some(format!("Clippy error: {e}"))),
            };

            if clippy_failed {
                log_frost(generation, &format!("Clippy failed: {}", hypothesis.id));
                let node = AttemptNode {
                    id: attempt_id.clone(),
                    parent_id: hyp_parent_id.clone(),
                    generation,
                    branch_id: hyp_branch_id.clone(),
                    hypothesis_id: hypothesis.id.clone(),
                    description: hypothesis.description.clone(),
                    diff_sha256: raw_diff_sha256.clone(),
                    patch: Some(hypothesis.patch.clone()),
                    sab_report_path: None,
                    metrics: None,
                    composite_score: None,
                    tokens_used: None,
                    wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                    status: AttemptStatus::ClippyFailed,
                    failure_class: Some(FailureClass::RepairableClippy),
                    failure_reason: Some("cargo clippy warnings detected".into()),
                    output_tail: clippy_tail,
                    binary_sha256: None,
                    base_commit: attempt_base_commit.clone(),
                    committed_commit: None,
                    action_type: Some(*hyp_action_type),
                    git_tree_id: None,
                    created_at: chrono_now(),
                };
                if let Err(err) =
                    log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
                {
                    abort_run!(
                        format!("attempt logging failed: {err}"),
                        generation.saturating_sub(1),
                        current_baseline_metrics.sab_score,
                        hall_of_fame
                    );
                }
                continue;
            }

            // Compute real fitness metrics. If SAB is available, run the full
            // benchmark; otherwise derive compile/test/binary-size metrics.
            let (winner_metrics, winner_sab) = if sab_available {
                let mut build_cmd = tokio::process::Command::new("cargo");
                build_cmd
                    .args(["build", "--release", "--features", "self-improvement"])
                    .stdin(std::process::Stdio::null())
                    .current_dir(&worktree);
                let build =
                    run_cancellable_subprocess(build_cmd, std::time::Duration::from_secs(600))
                        .await;
                if matches!(&build, Err(SubprocessError::ShutdownRequested)) {
                    log_warning(
                        "Shutdown requested during candidate release build; halting cleanly",
                    );
                    let node = make_cancelled_node("release build");
                    let _ = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    );
                    break;
                }
                let (build_failed, build_status, build_reason, build_tail) = match &build {
                    Ok(o) if o.status.success() => {
                        (false, AttemptStatus::BuildFailed, String::new(), None)
                    }
                    Ok(o) => (
                        true,
                        AttemptStatus::BuildFailed,
                        "Release build failed".to_string(),
                        Some(tail_lines(
                            &format!(
                                "{}\n{}",
                                String::from_utf8_lossy(&o.stdout),
                                String::from_utf8_lossy(&o.stderr)
                            ),
                            50,
                        )),
                    ),
                    Err(SubprocessError::Timeout) => (
                        true,
                        AttemptStatus::Timeout,
                        "cargo build --release timed out (600s)".to_string(),
                        None,
                    ),
                    Err(e) => (
                        true,
                        AttemptStatus::BuildFailed,
                        format!("Build error: {e}"),
                        None,
                    ),
                };

                if build_failed {
                    log_frost(
                        generation,
                        &format!("Release build failed: {}", hypothesis.id),
                    );
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: raw_diff_sha256.clone(),
                        patch: Some(hypothesis.patch.clone()),
                        sab_report_path: None,
                        metrics: None,
                        composite_score: None,
                        tokens_used: None,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: build_status,
                        failure_class: Some(FailureClass::Unclassified),
                        failure_reason: Some(build_reason),
                        output_tail: build_tail,
                        binary_sha256: None,
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }

                let mutated_binary = worktree.join("target/release/selfware");
                match fitness::run_sab(&mutated_binary, &sab_config) {
                    Ok(r) => (
                        metrics_from_sab_result(
                            &r,
                            &mutated_binary,
                            config.safety.max_binary_size_mb,
                        ),
                        Some(r),
                    ),
                    Err(e) => {
                        if crate::is_shutdown_requested() {
                            log_warning(
                                "Shutdown requested during SAB benchmark; halting candidate loop cleanly",
                            );
                            let node = make_cancelled_node("SAB benchmark");
                            let _ = log_and_append_attempt(
                                &attempts_file,
                                &node,
                                repo_root,
                                generation,
                                gen_start,
                            );
                            break;
                        }
                        log_warning(&format!("  SAB failed: {}", e));
                        let node = AttemptNode {
                            id: attempt_id.clone(),
                            parent_id: hyp_parent_id.clone(),
                            generation,
                            branch_id: hyp_branch_id.clone(),
                            hypothesis_id: hypothesis.id.clone(),
                            description: hypothesis.description.clone(),
                            diff_sha256: raw_diff_sha256.clone(),
                            patch: Some(hypothesis.patch.clone()),
                            sab_report_path: None,
                            metrics: None,
                            composite_score: None,
                            tokens_used: None,
                            wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                            status: AttemptStatus::InternalError,
                            failure_class: Some(FailureClass::EnvironmentError),
                            failure_reason: Some(format!("SAB execution failed: {e}")),
                            output_tail: None,
                            binary_sha256: None,
                            base_commit: attempt_base_commit.clone(),
                            committed_commit: None,
                            action_type: Some(*hyp_action_type),
                            git_tree_id: None,
                            created_at: chrono_now(),
                        };
                        total_infrastructure_failures += 1;
                        total_candidates_evaluated = total_candidates_evaluated.saturating_sub(1);
                        if let Err(err) = log_and_append_attempt(
                            &attempts_file,
                            &node,
                            repo_root,
                            generation,
                            gen_start,
                        ) {
                            abort_run!(
                                format!("attempt logging failed: {err}"),
                                generation.saturating_sub(1),
                                current_baseline_metrics.sab_score,
                                hall_of_fame
                            );
                        }
                        continue;
                    }
                }
            } else {
                match build_candidate_metrics(
                    &worktree,
                    &test_output,
                    test_duration,
                    EVOLVE_FEATURES,
                    &config,
                )
                .await
                {
                    Ok(m) => (m, None),
                    Err(SubprocessError::ShutdownRequested) => {
                        log_warning(
                            "Shutdown requested during candidate release build; halting cleanly",
                        );
                        let node = make_cancelled_node("release build");
                        let _ = log_and_append_attempt(
                            &attempts_file,
                            &node,
                            repo_root,
                            generation,
                            gen_start,
                        );
                        break;
                    }
                    Err(SubprocessError::Timeout) => {
                        log_frost(
                            generation,
                            &format!("Release build timed out: {}", hypothesis.id),
                        );
                        let node = AttemptNode {
                            id: attempt_id.clone(),
                            parent_id: hyp_parent_id.clone(),
                            generation,
                            branch_id: hyp_branch_id.clone(),
                            hypothesis_id: hypothesis.id.clone(),
                            description: hypothesis.description.clone(),
                            diff_sha256: raw_diff_sha256.clone(),
                            patch: Some(hypothesis.patch.clone()),
                            sab_report_path: None,
                            metrics: None,
                            composite_score: None,
                            tokens_used: None,
                            wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                            status: AttemptStatus::Timeout,
                            failure_class: Some(FailureClass::Unclassified),
                            failure_reason: Some("Release build timed out (600s)".into()),
                            output_tail: None,
                            binary_sha256: None,
                            base_commit: attempt_base_commit.clone(),
                            committed_commit: None,
                            action_type: Some(*hyp_action_type),
                            git_tree_id: None,
                            created_at: chrono_now(),
                        };
                        if let Err(err) = log_and_append_attempt(
                            &attempts_file,
                            &node,
                            repo_root,
                            generation,
                            gen_start,
                        ) {
                            abort_run!(
                                format!("attempt logging failed: {err}"),
                                generation.saturating_sub(1),
                                current_baseline_metrics.sab_score,
                                hall_of_fame
                            );
                        }
                        continue;
                    }
                    Err(e) => {
                        log_frost(
                            generation,
                            &format!("Release build failed: {}", hypothesis.id),
                        );
                        let node = AttemptNode {
                            id: attempt_id.clone(),
                            parent_id: hyp_parent_id.clone(),
                            generation,
                            branch_id: hyp_branch_id.clone(),
                            hypothesis_id: hypothesis.id.clone(),
                            description: hypothesis.description.clone(),
                            diff_sha256: raw_diff_sha256.clone(),
                            patch: Some(hypothesis.patch.clone()),
                            sab_report_path: None,
                            metrics: None,
                            composite_score: None,
                            tokens_used: None,
                            wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                            status: AttemptStatus::BuildFailed,
                            failure_class: Some(FailureClass::Unclassified),
                            failure_reason: Some(format!("Candidate build failed: {e}")),
                            output_tail: None,
                            binary_sha256: None,
                            base_commit: attempt_base_commit.clone(),
                            committed_commit: None,
                            action_type: Some(*hyp_action_type),
                            git_tree_id: None,
                            created_at: chrono_now(),
                        };
                        if let Err(err) = log_and_append_attempt(
                            &attempts_file,
                            &node,
                            repo_root,
                            generation,
                            gen_start,
                        ) {
                            abort_run!(
                                format!("attempt logging failed: {err}"),
                                generation.saturating_sub(1),
                                current_baseline_metrics.sab_score,
                                hall_of_fame
                            );
                        }
                        continue;
                    }
                }
            };

            // Capture the EXACT tested state as a diff against HEAD before the
            // worktree guard cleans up. This includes the `cargo fmt` auto-fix
            // above, which the raw LLM patch lacks — committing this diff (not
            // the raw patch) is what makes the committed state match the
            // tested state, and strict-applying it later avoids `patch -F3`
            // fuzz landing hunks somewhere other than where they were tested.
            let tested_diff = match capture_tested_diff(&worktree) {
                Some(d) if !d.trim().is_empty() => d,
                _ => {
                    log_frost(
                        generation,
                        &format!("No effective diff after evaluation: {}", hypothesis.id),
                    );
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: raw_diff_sha256.clone(),
                        patch: Some(hypothesis.patch.clone()),
                        sab_report_path: None,
                        metrics: None,
                        composite_score: None,
                        tokens_used: None,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: AttemptStatus::PatchFailed,
                        failure_class: Some(FailureClass::Unclassified),
                        failure_reason: Some("No effective diff after evaluation".into()),
                        output_tail: None,
                        binary_sha256: None,
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }
            };

            let candidate_composite = config.fitness_weights.composite(&winner_metrics);
            let tested_diff_sha256 = compute_sha256(tested_diff.as_bytes());

            let evaluated_tree = match capture_worktree_tree_id(&worktree) {
                Some(tree) if !tree.trim().is_empty() => tree,
                _ => {
                    log_error(&format!(
                        "Failed to capture evaluated tree verification digest for '{}' — candidate ineligible for promotion",
                        hypothesis.description
                    ));
                    let node = AttemptNode {
                        id: attempt_id.clone(),
                        parent_id: hyp_parent_id.clone(),
                        generation,
                        branch_id: hyp_branch_id.clone(),
                        hypothesis_id: hypothesis.id.clone(),
                        description: hypothesis.description.clone(),
                        diff_sha256: tested_diff_sha256,
                        patch: Some(tested_diff),
                        sab_report_path: winner_sab.as_ref().map(|s| s.report_path.clone()),
                        metrics: Some(winner_metrics.clone()),
                        composite_score: Some(candidate_composite),
                        tokens_used: winner_metrics.tokens_used,
                        wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                        status: AttemptStatus::InternalError,
                        failure_class: Some(FailureClass::EnvironmentError),
                        failure_reason: Some(
                            "Failed to capture evaluated tree verification digest".into(),
                        ),
                        output_tail: None,
                        binary_sha256: winner_sab.as_ref().map(|s| s.binary_sha256.clone()),
                        base_commit: attempt_base_commit.clone(),
                        committed_commit: None,
                        action_type: Some(*hyp_action_type),
                        git_tree_id: None,
                        created_at: chrono_now(),
                    };
                    total_infrastructure_failures += 1;
                    total_candidates_evaluated = total_candidates_evaluated.saturating_sub(1);
                    if let Err(err) = log_and_append_attempt(
                        &attempts_file,
                        &node,
                        repo_root,
                        generation,
                        gen_start,
                    ) {
                        abort_run!(
                            format!("attempt logging failed: {err}"),
                            generation.saturating_sub(1),
                            current_baseline_metrics.sab_score,
                            hall_of_fame
                        );
                    }
                    continue;
                }
            };

            let node = AttemptNode {
                id: attempt_id.clone(),
                parent_id: hyp_parent_id.clone(),
                generation,
                branch_id: hyp_branch_id.clone(),
                hypothesis_id: hypothesis.id.clone(),
                description: hypothesis.description.clone(),
                diff_sha256: tested_diff_sha256,
                patch: Some(tested_diff.clone()),
                sab_report_path: winner_sab.as_ref().map(|s| s.report_path.clone()),
                metrics: Some(winner_metrics.clone()),
                composite_score: Some(candidate_composite),
                tokens_used: winner_metrics.tokens_used,
                wall_time_ms: attempt_start.elapsed().as_millis() as u64,
                status: AttemptStatus::Evaluated,
                failure_class: None,
                failure_reason: None,
                output_tail: None,
                binary_sha256: winner_sab.as_ref().map(|s| s.binary_sha256.clone()),
                base_commit: attempt_base_commit.clone(),
                committed_commit: None,
                action_type: Some(*hyp_action_type),
                git_tree_id: Some(evaluated_tree.clone()),
                created_at: chrono_now(),
            };
            if let Err(err) =
                log_and_append_attempt(&attempts_file, &node, repo_root, generation, gen_start)
            {
                abort_run!(
                    format!("attempt logging failed: {err}"),
                    generation.saturating_sub(1),
                    current_baseline_metrics.sab_score,
                    hall_of_fame
                );
            }

            log_phase(&format!(
                "  ✓ '{}' passed (score: {:.0}, composite: {:.4}, {:.1}s)",
                hypothesis.description,
                winner_metrics.sab_score,
                candidate_composite,
                winner_metrics.wall_clock_secs
            ));

            evaluated_candidates.push(EvaluatedCandidate {
                hypothesis: hypothesis.clone(),
                metrics: winner_metrics,
                sab_result: winner_sab,
                tested_diff,
                evaluated_tree,
                composite: candidate_composite,
                attempt_id: attempt_id.clone(),
                branch_id: hyp_branch_id.clone(),
            });
        }

        // ─── Step 5: EMERGE OR DIE (Ranked Promotion) ───
        if crate::is_shutdown_requested() {
            log_warning("Shutdown requested; skipping promotion and halting cleanly");
            break;
        }

        if evaluated_candidates.is_empty() {
            log_frost(generation, "No hypotheses survived evaluation");
            log_event(
                repo_root,
                &serde_json::json!({
                    "event": "generation_end",
                    "timestamp": chrono_now(),
                    "generation": generation,
                    "outcome": "frost",
                    "reason": "no hypotheses survived",
                    "duration_secs": gen_start.elapsed().as_secs_f64(),
                }),
            );
            continue;
        }

        // Rank evaluated candidates best-first using consistent transitive comparison
        evaluated_candidates.sort_by(|a, b| {
            candidate_rank_cmp(
                b.metrics.sab_score,
                b.composite,
                a.metrics.sab_score,
                a.composite,
            )
            .then_with(|| a.attempt_id.cmp(&b.attempt_id))
        });

        let top_candidate = evaluated_candidates.first().cloned();
        let mut promoted_winner = None;
        let baseline_composite = config.fitness_weights.composite(&current_baseline_metrics);

        for (rank_idx, candidate) in evaluated_candidates.into_iter().enumerate() {
            let rank = rank_idx + 1;
            match evaluate_candidate_promotion(
                baseline_composite,
                candidate.composite,
                current_baseline_sab.as_ref(),
                candidate.sab_result.as_ref(),
                &current_baseline_metrics,
                &candidate.metrics,
            ) {
                PromotionDecision::Promote => {
                    promoted_winner = Some((rank, candidate));
                    break;
                }
                PromotionDecision::Reject(reason) => {
                    log_phase(&format!(
                        "  Candidate #{} '{}' (composite: {:.4}) rejected by gate: {reason}; evaluating runner-up",
                        rank, candidate.hypothesis.description, candidate.composite
                    ));
                    log_event(
                        repo_root,
                        &serde_json::json!({
                            "event": "candidate_rejected",
                            "run_id": &run_id,
                            "timestamp": chrono_now(),
                            "generation": generation,
                            "rank": rank,
                            "description": candidate.hypothesis.description,
                            "composite": candidate.composite,
                            "sab_score": candidate.metrics.sab_score,
                            "branch_id": candidate.branch_id,
                            "reason": reason,
                        }),
                    );
                }
            }
        }

        match promoted_winner {
            Some((rank, winner)) => {
                if crate::is_shutdown_requested() {
                    log_warning(
                        "Shutdown requested; refusing to commit winner and halting cleanly",
                    );
                    break;
                }

                let commit_msg = format!(
                    "🧬 Gen {} BLOOM (Rank {}): {:.0} → {:.0} | {}",
                    generation,
                    rank,
                    current_baseline_metrics.sab_score,
                    winner.metrics.sab_score,
                    winner.hypothesis.description
                );
                // Apply the EXACT tested diff (not the raw LLM patch) and commit
                // ONLY the paths it edits — never `git add -A`, which swept every
                // dirty edit and untracked file (.env, scratch, credentials) into
                // the BLOOM commit on whatever branch was checked out.
                // Require the promoted tree to match the evaluated benchmark tree exactly.
                if commit_winner_to_repo(
                    repo_root,
                    &winner.tested_diff,
                    Some(&winner.evaluated_tree),
                    &commit_msg,
                )
                .await
                {
                    log_bloom(
                        generation,
                        &winner.hypothesis.description,
                        current_baseline_metrics.sab_score,
                        winner.metrics.sab_score,
                    );

                    active_parent_id = Some(winner.attempt_id.clone());

                    let new_head = ast_tools::get_git_head_commit(repo_root);
                    if let Some(ref head_sha) = new_head {
                        expected_head_commit = Some(head_sha.clone());
                        if let Err(e) = AttemptTree::record_committed_commit(
                            &attempts_file,
                            &winner.attempt_id,
                            head_sha,
                        ) {
                            log_error(&format!(
                                "Failed to record committed_commit anchor for attempt '{}' at {}: {e}",
                                winner.attempt_id, head_sha
                            ));
                        }
                    } else {
                        log_error(&format!(
                            "Failed to resolve git HEAD commit after committing winner '{}'",
                            winner.attempt_id
                        ));
                    }

                    let git_tag = if generation.is_multiple_of(config.checkpoint_interval) {
                        let tag = format!("evolve-gen-{}", generation);
                        let _ = Command::new("git")
                            .env_remove("GIT_INDEX_FILE")
                            .args(["tag", &tag])
                            .current_dir(repo_root)
                            .output();
                        Some(tag)
                    } else {
                        None
                    };

                    let mut run_id = None;
                    let mut binary_sha256 = None;
                    let mut report_path = None;
                    if let Some(ref sab) = winner.sab_result {
                        sab_config.exempt_reports.push(sab.report_path.clone());
                        if sab_config.exempt_reports.len() > 10 {
                            let excess = sab_config.exempt_reports.len() - 10;
                            sab_config.exempt_reports.drain(0..excess);
                        }
                        run_id = Some(sab.run_id.clone());
                        binary_sha256 = Some(sab.binary_sha256.clone());
                        report_path = Some(sab.report_path.clone());
                    }

                    hall_of_fame.push(GenerationWinner {
                        generation,
                        description: winner.hypothesis.description.clone(),
                        composite_score: winner.composite,
                        sab_delta: winner.metrics.sab_score - current_baseline_metrics.sab_score,
                        token_delta: match (
                            winner.metrics.tokens_used,
                            current_baseline_metrics.tokens_used,
                        ) {
                            (Some(w), Some(b)) => Some(w as f64 - b as f64),
                            _ => None,
                        },
                        // The tested diff actually committed (incl. fmt fixes),
                        // not the raw LLM patch.
                        patch: winner.tested_diff.clone(),
                        git_tag,
                        run_id: run_id.clone(),
                        binary_sha256: binary_sha256.clone(),
                        report_path: report_path.clone(),
                    });

                    log_event(
                        repo_root,
                        &serde_json::json!({
                            "event": "generation_end",
                            "timestamp": chrono_now(),
                            "generation": generation,
                            "outcome": "bloom",
                            "rank": rank,
                            "description": winner.hypothesis.description,
                            "branch_id": winner.branch_id,
                            "attempt_id": winner.attempt_id,
                            "score_before": current_baseline_metrics.sab_score,
                            "score_after": winner.metrics.sab_score,
                            "composite": winner.composite,
                            "duration_secs": gen_start.elapsed().as_secs_f64(),
                            "improvements_total": hall_of_fame.len(),
                            "run_id": run_id,
                            "binary_sha256": binary_sha256,
                            "report_path": report_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                        }),
                    );

                    current_baseline_metrics = winner.metrics;
                    if winner.sab_result.is_some() {
                        current_baseline_sab = winner.sab_result;
                    }
                } else {
                    log_warning("Failed to commit winner diff to repository");
                    log_event(
                        repo_root,
                        &serde_json::json!({
                            "event": "commit_failed",
                            "outcome": "commit_failed",
                            "reason": "git commit or tag creation failed",
                            "timestamp": chrono_now(),
                            "generation": generation,
                            "description": winner.hypothesis.description,
                            "composite": winner.composite,
                            "duration_secs": gen_start.elapsed().as_secs_f64(),
                        }),
                    );
                }
            }
            None => {
                let (rating, winner_sab) = if let Some(ref top) = top_candidate {
                    let score = top.metrics.sab_score;
                    let r = if top.composite < baseline_composite * 0.9 {
                        GenerationRating::Frost
                    } else {
                        GenerationRating::Wilt
                    };
                    (r, score)
                } else {
                    (GenerationRating::Frost, 0.0)
                };
                log_reject(
                    generation,
                    &rating,
                    winner_sab,
                    current_baseline_metrics.sab_score,
                );
                let outcome_str = match rating {
                    GenerationRating::Wilt => "wilt",
                    _ => "frost",
                };
                log_event(
                    repo_root,
                    &serde_json::json!({
                        "event": "generation_end",
                        "timestamp": chrono_now(),
                        "generation": generation,
                        "outcome": outcome_str,
                        "reason": "all evaluated candidates rejected by promotion gates",
                        "duration_secs": gen_start.elapsed().as_secs_f64(),
                    }),
                );
            }
        }
    }

    let (outcome, stop_reason) = compute_run_outcome(
        crate::is_shutdown_requested(),
        policy_stopped_reason,
        total_candidates_evaluated,
        total_infrastructure_failures,
    );

    log_event(
        repo_root,
        &serde_json::json!({
            "event": "run_end",
            "kind": "run_end",
            "timestamp": chrono_now(),
            "run_id": &run_id,
            "outcome": &outcome,
            "stop_reason": &stop_reason,
            "generations_run": generation,
            "candidates_attempted": total_candidates_attempted,
            "candidates_evaluated": total_candidates_evaluated,
            "infrastructure_failures": total_infrastructure_failures,
            "final_sab_score": current_baseline_metrics.sab_score,
            "duration_secs": start.elapsed().as_secs_f64(),
        }),
    );

    signal_handle.abort();

    let aborted = if outcome == "failed" {
        stop_reason.clone()
    } else {
        None
    };

    EvolutionResult {
        generations_run: generation,
        improvements: hall_of_fame,
        final_sab_score: current_baseline_metrics.sab_score,
        initial_sab_score: initial_sab,
        total_duration: start.elapsed(),
        aborted,
        outcome,
        stop_reason,
    }
}

// ═══════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════

pub(crate) fn compute_run_outcome(
    shutdown_requested: bool,
    policy_stopped_reason: Option<String>,
    total_candidates_evaluated: usize,
    total_infrastructure_failures: usize,
) -> (String, Option<String>) {
    if shutdown_requested {
        (
            "killed".to_string(),
            Some("process terminated by shutdown signal".to_string()),
        )
    } else if let Some(reason) = policy_stopped_reason {
        ("policy_stopped".to_string(), Some(reason))
    } else if total_candidates_evaluated == 0 {
        let reason = if total_infrastructure_failures > 0 {
            format!(
                "All attempted candidates failed due to infrastructure errors ({total_infrastructure_failures} failures)"
            )
        } else {
            "No candidates were evaluated during evolution run".to_string()
        };
        ("failed".to_string(), Some(reason))
    } else {
        ("completed".to_string(), None)
    }
}

async fn generate_single_action_hypothesis(
    config: &EvolutionConfig,
    telemetry_prompt: &str,
    history_prompt: &str,
    source_context: &str,
    parent_context: &str,
    source_dir: &Path,
) -> Option<Hypothesis> {
    if source_context.is_empty() {
        log_warning("No mutation target files found or readable for action");
        return None;
    }

    let use_micro_mode = super::micro_mode::is_micro_model(&config.llm.model);
    if use_micro_mode {
        use super::micro_mode;
        let all_paths: Vec<PathBuf> = config
            .mutation_targets
            .prompt_logic
            .iter()
            .chain(config.mutation_targets.tool_code.iter())
            .chain(config.mutation_targets.cognitive.iter())
            .cloned()
            .collect();
        let selected = micro_mode::select_micro_targets(&all_paths, source_dir);
        let micro_context = if !selected.is_empty() {
            micro_mode::build_micro_context(&selected)
        } else {
            source_context.to_string()
        };

        let system_prompt = micro_mode::build_micro_system_prompt(1);
        let user_prompt = build_action_user_prompt(
            telemetry_prompt,
            history_prompt,
            &micro_context,
            parent_context,
        );
        match call_llm(&config.llm, &system_prompt, &user_prompt).await {
            Ok(response) => {
                let hypotheses = parse_hypotheses_response(&response);
                hypotheses
                    .into_iter()
                    .find(|h| match micro_mode::validate_micro_hypothesis(h) {
                        Ok(()) => true,
                        Err(e) => {
                            log_warning(&format!("Micro mode: Rejected hypothesis: {}", e));
                            false
                        }
                    })
            }
            Err(e) => {
                log_warning(&format!("Micro mode LLM call failed for action: {}", e));
                None
            }
        }
    } else {
        let system_prompt = build_system_prompt(1);
        let user_prompt = build_action_user_prompt(
            telemetry_prompt,
            history_prompt,
            source_context,
            parent_context,
        );
        match call_llm(&config.llm, &system_prompt, &user_prompt).await {
            Ok(response) => {
                let hypotheses = parse_hypotheses_response(&response);
                hypotheses.into_iter().next()
            }
            Err(e) => {
                log_warning(&format!("LLM call failed for action: {}", e));
                None
            }
        }
    }
}

/// Max total source context characters
const MAX_CONTEXT_CHARS: usize = 45_000;

/// Extract function signatures from Rust source code
fn extract_function_signatures(source: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    let mut in_impl_block = false;
    let mut impl_context = String::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Track impl blocks for context
        if trimmed.starts_with("impl ") || trimmed.starts_with("pub impl ") {
            in_impl_block = true;
            impl_context = trimmed.to_string();
            continue;
        }
        if trimmed == "}" && in_impl_block {
            in_impl_block = false;
            impl_context.clear();
            continue;
        }

        // Match function signatures (fn or pub fn)
        if (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "))
            && !trimmed.starts_with("fn main()")
        // Skip main functions in tests
        {
            let mut sig = trimmed.to_string();

            // Add impl context if available
            if !impl_context.is_empty() {
                sig = format!("// In: {}\n{}", impl_context, sig);
            }

            // Extract just the signature line (stop at opening brace)
            if let Some(brace_pos) = sig.find('{') {
                sig = sig[..brace_pos].to_string();
            }

            if !sig.is_empty() {
                signatures.push(sig);
            }
        }
    }

    signatures
}

/// Get recent git changes for context
fn get_recent_git_changes(repo_root: &Path, max_commits: usize) -> Option<String> {
    let output = std::process::Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["log", "--oneline", "--no-merges"])
        .arg(format!("-{}", max_commits))
        .current_dir(repo_root)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let log = String::from_utf8_lossy(&o.stdout);
            if log.trim().is_empty() {
                None
            } else {
                Some(format!("Recent commits:\n{}", log))
            }
        }
        _ => None,
    }
}

/// Resolve `candidate` against `base`, guaranteeing the result stays INSIDE
/// `base`. Returns `None` for an absolute path or any `..` sequence that would
/// escape. Used to contain both config-supplied mutation targets (read) and
/// model-produced edit paths (write) so `selfware evolve` can never read or
/// overwrite files outside the repository. Purely lexical (no canonicalize) so
/// it also works for not-yet-existing write targets.
fn contained_path(base: &Path, candidate: &Path) -> Option<PathBuf> {
    use std::path::Component;
    if candidate.is_absolute() {
        return None;
    }
    let mut result = base.to_path_buf();
    for comp in candidate.components() {
        match comp {
            Component::Normal(c) => result.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() || !result.starts_with(base) {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    result.starts_with(base).then_some(result)
}

/// Returns the dedicated target directory used for evolution sandbox evaluations.
/// Reusing this directory allows all candidates to share pre-compiled dependencies,
/// dropping incremental test compilation times from minutes to seconds while
/// remaining fully isolated from the main repo's `target/debug`.
pub fn evolution_target_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("target").join("evolution")
}

/// Build a concise negative-history prompt listing recently failed mutations.
/// Prevents the hypothesis generator from getting trapped in local loops
/// (e.g. repeatedly attempting the same failing patch across multiple generations).
pub fn format_recent_failure_history(attempts_file: &Path, max_entries: usize) -> String {
    let content = match std::fs::read_to_string(attempts_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return String::new(),
        Err(e) => {
            tracing::warn!(
                "Failed to read attempts file {} for failure history: {e}",
                attempts_file.display()
            );
            return String::new();
        }
    };
    let mut failures = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in content.lines().rev() {
        if let Ok(node) = serde_json::from_str::<AttemptNode>(line) {
            // Filter out control anchors and environment errors
            if node.branch_id == "control"
                || node.failure_class == Some(FailureClass::EnvironmentError)
                || node.status == AttemptStatus::InternalError
            {
                continue;
            }
            if node.status != AttemptStatus::Evaluated && node.status != AttemptStatus::Baseline {
                let desc = node.description.trim();
                let reason = node.failure_reason.as_deref().unwrap_or("failed");
                let entry = format!("- Attempted: \"{desc}\" -> FAILED ({reason}). Do not repeat.");
                if seen.insert(entry.clone()) {
                    failures.push(entry);
                    if failures.len() >= max_entries {
                        break;
                    }
                }
            }
        }
    }
    if failures.is_empty() {
        String::new()
    } else {
        failures.reverse();
        format!(
            "## Previous Failed Hypotheses (DO NOT REPEAT)\n{}\nExplore different functions, files, or optimization approaches.\n",
            failures.join("\n")
        )
    }
}

/// Loads SHA-256 hashes of patches from attempts that previously failed due to code defects.
/// Excludes environment/infrastructure failures and control anchors so transient errors can be retried.
pub(crate) fn load_failed_diff_shas(attempts_file: &Path) -> std::collections::HashSet<String> {
    let mut failed = std::collections::HashSet::new();
    let Ok(content) = std::fs::read_to_string(attempts_file) else {
        return failed;
    };
    for line in content.lines() {
        if let Ok(node) = serde_json::from_str::<AttemptNode>(line) {
            if node.status != AttemptStatus::Evaluated
                && node.status != AttemptStatus::Baseline
                && node.status != AttemptStatus::InternalError
                && node.failure_class != Some(FailureClass::EnvironmentError)
                && node.branch_id != "control"
                && !node.diff_sha256.is_empty()
            {
                failed.insert(node.diff_sha256);
            }
        }
    }
    failed
}

pub fn read_mutation_targets(targets: &super::MutationTargets, repo_root: &Path) -> String {
    // Collect all files with their sizes, then sort smallest-first so we
    // maximise the number of files sent in full within the context budget.
    let all_paths: Vec<&PathBuf> = targets
        .prompt_logic
        .iter()
        .chain(targets.tool_code.iter())
        .chain(targets.cognitive.iter())
        .collect();

    let mut file_entries: Vec<(&PathBuf, String, usize, Vec<String>)> = Vec::new();
    for file in &all_paths {
        // Contain the target inside the repo: an `[evolution]` config entry that
        // is absolute or uses `..` must not read arbitrary host files into the
        // model prompt.
        let Some(full_path) = contained_path(repo_root, file) else {
            log_warning(&format!(
                "Refusing mutation target outside the repository: {}",
                file.display()
            ));
            continue;
        };
        match std::fs::read_to_string(&full_path) {
            Ok(contents) => {
                let len = contents.len();
                let signatures = extract_function_signatures(&contents);
                file_entries.push((file, contents, len, signatures));
            }
            Err(e) => {
                log_warning(&format!("Could not read {}: {}", file.display(), e));
            }
        }
    }

    // Sort by size ascending — small files go in full, big files get truncated
    file_entries.sort_by_key(|(_, _, len, _)| *len);

    let mut context = String::new();
    let mut files_full = 0usize;
    let mut files_truncated = 0usize;
    let mut total_signatures = 0usize;

    // Add recent git changes at the top for context
    if let Some(recent_changes) = get_recent_git_changes(repo_root, 10) {
        context.push_str(&format!("## Recent Git Changes\n{}\n\n", recent_changes));
    }

    for (file, contents, _len, signatures) in &file_entries {
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(context.len());
        if remaining < 500 {
            log_warning(&format!(
                "Context limit reached ({} chars), skipping remaining files",
                context.len()
            ));
            break;
        }

        // Add line numbers to source — helps the LLM generate accurate @@ hunk headers
        let numbered = add_line_numbers(contents);

        // Budget for this file: overhead for the header + fences + signatures (~200 chars)
        let sig_overhead = signatures.len() * 50;
        let overhead = 200 + file.display().to_string().len() + sig_overhead;
        let budget = remaining.saturating_sub(overhead);

        let (display_content, was_truncated) = if numbered.len() <= budget {
            (numbered, false)
        } else {
            // Truncate to budget on a line boundary
            let truncated = truncate_to_line_boundary(&numbered, budget);
            let total_lines = contents.lines().count();
            let shown_lines = truncated.lines().count();
            (
                format!(
                    "{}\n// ... [truncated at line {}/{}, {} total chars]",
                    truncated,
                    shown_lines,
                    total_lines,
                    contents.len()
                ),
                true,
            )
        };

        if was_truncated {
            files_truncated += 1;
        } else {
            files_full += 1;
        }

        total_signatures += signatures.len();

        // Add function signatures as a summary before the source
        let sig_summary = if !signatures.is_empty() {
            let sigs: String = signatures
                .iter()
                .take(20) // Limit to 20 signatures per file
                .map(|s| format!("  {}", s))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "\n// Function signatures ({} total):\n{}\n",
                signatures.len(),
                sigs
            )
        } else {
            String::new()
        };

        context.push_str(&format!(
            "\n### {}\n{}\n```rust\n{}\n```\n",
            file.display(),
            sig_summary,
            display_content
        ));
    }
    log_phase(&format!(
        "Source context: {} chars from {} files ({} full, {} truncated, {} signatures)",
        context.len(),
        files_full + files_truncated,
        files_full,
        files_truncated,
        total_signatures,
    ));
    context
}

/// Add line numbers to source code (e.g. "  1| fn main() {")
fn add_line_numbers(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let width = format!("{}", lines.len()).len();
    let mut out = String::with_capacity(source.len() + lines.len() * (width + 2));
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!("{:>width$}| {}\n", i + 1, line, width = width));
    }
    out
}

/// Truncate a string to at most `max_chars` on a line boundary
fn truncate_to_line_boundary(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    // Back off to a UTF-8 char boundary FIRST: a raw `&s[..max_chars]` slice
    // panics when max_chars lands mid-codepoint (multibyte source), which used
    // to abort the whole evolve run before any LLM call was even made.
    let s = truncate_char_boundary(s, max_chars);
    // Find the last newline before the limit
    match s.rfind('\n') {
        Some(pos) => &s[..pos],
        None => s,
    }
}

/// Byte-truncate `s` to at most `max_bytes`, backing off to a UTF-8 char
/// boundary. A raw `&s[..n]` byte slice PANICS when `n` lands mid-codepoint —
/// the same class of bug fixed in `agent/checkpointing.rs`
/// (`truncate_bytes_char_boundary`); duplicated here because that helper is
/// feature-gated and private to the checkpointing module.
fn truncate_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub fn build_system_prompt(population_size: usize) -> String {
    format!(
        r#"You are an evolution engine that generates code mutation hypotheses for a Rust project called selfware.

Your task is to propose exactly {n} mutation hypotheses as improvements. Each hypothesis uses search-and-replace edits.

SOURCE CODE FORMAT:
- Each file is shown with line numbers like "  42| fn foo() {{"
- Line numbers are for your reference only — do NOT include them in search/replace strings
- Some files are truncated — only modify code you can see in full
- CRITICAL: The search string must match EXACT line content from the source files, NOT paraphrased or reformatted code

EDIT FORMAT (critical — edits that can't be found are discarded):
- Each hypothesis has an "edits" array of search-and-replace operations
- "search" must be an EXACT substring of the target file (copy-paste accuracy required)
- "replace" is what replaces that exact substring
- Keep edits small and focused — change the minimum necessary code
- The search string must be unique in the file (not ambiguous)
- Use \n for newlines inside strings (JSON escaped)
- Do NOT include line number prefixes (like "42| ") in search/replace strings
- CRITICAL: When constructing search strings, copy the EXACT text from the source code including:
  - Exact whitespace (spaces vs tabs, indentation level)
  - Exact punctuation and formatting
  - Exact line breaks
  - Do NOT reformat or paraphrase the code you're searching for

RULES:
1. Each hypothesis must target files from the provided source code
2. Never modify files under src/evolution/, src/safety/, system_tests/, or benches/sab_
3. Focus on: bug fixes, performance improvements, correctness, reducing allocations
4. Each hypothesis must be independent — do not assume other hypotheses are applied
5. Only modify code you can fully see — never guess at truncated content

Respond with a JSON array of exactly {n} objects:
- "description": string — what the change does and why
- "edits": array of {{"file": "relative/path.rs", "search": "exact old text", "replace": "new text"}}
- "target_files": string array — relative paths of files changed
- "property_test": string or null — optional property test

Return ONLY the JSON array. No markdown, no commentary, no thinking.

/no_think"#,
        n = population_size
    )
}

pub fn build_user_prompt(telemetry: &str, history: &str, source_context: &str) -> String {
    let mut prompt = String::new();

    if !telemetry.is_empty() {
        prompt.push_str("## Current Telemetry\n\n");
        prompt.push_str(telemetry);
        prompt.push_str("\n\n");
    }

    if !history.is_empty() {
        prompt.push_str(history);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Source Code (mutation targets)\n");
    prompt.push_str(source_context);

    prompt
}

pub fn build_action_user_prompt(
    telemetry: &str,
    history: &str,
    source_context: &str,
    parent_context: &str,
) -> String {
    let mut prompt = build_user_prompt(telemetry, history, source_context);
    if !parent_context.trim().is_empty() {
        prompt.push_str("\n\n## Parent Refinement Context & Task\n");
        prompt.push_str(parent_context);
    }
    prompt
}

async fn call_llm(
    llm: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    crate::config::api_key::assert_credential_endpoint_safe(&llm.endpoint, llm.api_key.is_some())
        .map_err(|e| e.to_string())?;
    let url = format!("{}/chat/completions", llm.endpoint.trim_end_matches('/'));

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    if let Some(ref key) = llm.api_key {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key))
                .map_err(|e| format!("Invalid API key header: {}", e))?,
        );
    }

    let body = serde_json::json!({
        "model": llm.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "max_tokens": llm.max_tokens,
        "temperature": llm.temperature,
        // Disable Qwen3's thinking mode to maximize output tokens for JSON
        "chat_template_kwargs": {"enable_thinking": false},
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LLM API returned {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse LLM response JSON: {}", e))?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in LLM response".to_string())
}

pub fn parse_hypotheses_response(response: &str) -> Vec<Hypothesis> {
    // Find JSON array in the response — handles markdown fences, thinking, preamble
    let json_str = match extract_json_array(response) {
        Some(s) => s,
        None => {
            log_warning("Could not find JSON array in LLM response");
            return vec![];
        }
    };

    let parsed: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log_warning(&format!("Failed to parse hypotheses JSON: {}", e));
            return vec![];
        }
    };

    parsed
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let description = v["description"].as_str()?.to_string();
            let target_files: Vec<PathBuf> = v["target_files"]
                .as_array()?
                .iter()
                .filter_map(|f| f.as_str().map(PathBuf::from))
                .collect();
            let property_test = v["property_test"].as_str().map(|s| s.to_string());

            // Support both formats:
            // 1. New: "edits" array of {file, search, replace}
            // 2. Legacy: "patch" string (unified diff)
            let patch = if let Some(edits) = v["edits"].as_array() {
                // Serialize edits as JSON for the patch field
                serde_json::to_string(edits).ok()?
            } else {
                // Fallback to legacy unified diff format
                v["patch"].as_str()?.to_string()
            };

            if patch.is_empty() {
                return None;
            }

            Some(Hypothesis {
                id: format!("hyp-{}", i),
                description,
                patch,
                target_files,
                property_test,
            })
        })
        .collect()
}

fn extract_json_array(text: &str) -> Option<String> {
    // Try to find a JSON array, handling markdown fences
    let text = text.trim();

    // Strip markdown code fences if present
    let stripped = if text.contains("```") {
        let mut inside_fence = false;
        let mut content = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                inside_fence = !inside_fence;
                continue;
            }
            if inside_fence {
                content.push_str(line);
                content.push('\n');
            }
        }
        if content.is_empty() {
            text.to_string()
        } else {
            content
        }
    } else {
        text.to_string()
    };

    // Find the first '[' and its matching ']'
    let start = stripped.find('[')?;
    let mut depth = 0;
    let mut end = None;
    for (i, ch) in stripped[start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }

    end.map(|e| stripped[start..e].to_string())
}

pub fn format_evolution_history(hall_of_fame: &[GenerationWinner]) -> String {
    if hall_of_fame.is_empty() {
        return String::from("No evolution history yet. This is generation 1.");
    }

    let mut prompt = String::from("## Evolution History (most recent first)\n\n");
    for winner in hall_of_fame.iter().rev().take(10) {
        prompt.push_str(&format!(
            "- Gen {}: {} (SAB +{:.1}, tokens {:.0})\n",
            winner.generation,
            winner.description,
            winner.sab_delta,
            winner
                .token_delta
                .map_or_else(|| "unmeasured".to_string(), |d| format!("{d:+.0}"))
        ));
    }
    prompt
}

/// Strip line-number prefixes the LLM may have left in the patch.
/// Matches patterns like "  42| " at the start of context/add/delete lines.
#[allow(dead_code)] // kept for the next patch-ingestion hardening pass
fn sanitize_patch(patch: &str) -> String {
    let mut out = String::with_capacity(patch.len());
    for line in patch.lines() {
        // Hunk headers, file headers — pass through unchanged
        if line.starts_with("@@")
            || line.starts_with("---")
            || line.starts_with("+++")
            || line.starts_with("diff ")
        {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        // Context/add/delete lines: strip line-number prefix if present
        // Patterns:  " 123| code", "+  45| code", "- 789| code"
        let (prefix, rest) =
            if let Some(r) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) {
                (&line[..1], r)
            } else if let Some(r) = line.strip_prefix(' ') {
                (" ", r)
            } else {
                // Unrecognized line — pass through
                out.push_str(line);
                out.push('\n');
                continue;
            };

        // Check if `rest` looks like "  NNN| actual_code"
        let stripped = rest.trim_start();
        if let Some(pipe_pos) = stripped.find('|') {
            let before_pipe = &stripped[..pipe_pos];
            if !before_pipe.is_empty() && before_pipe.chars().all(|c| c.is_ascii_digit()) {
                // It's a line-number prefix — strip "NNN| " and keep the rest
                let after_pipe = &stripped[pipe_pos + 1..];
                // The format is "NNN| code" — there's exactly one space after |
                let code = after_pipe.strip_prefix(' ').unwrap_or(after_pipe);
                out.push_str(prefix);
                out.push_str(code);
                out.push('\n');
                continue;
            }
        }

        // No line-number prefix — pass through unchanged
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Extract the paths a mutation payload ACTUALLY edits, for the
/// protected-path gate. The hypothesis `target_files` field is LLM-declared
/// metadata that is never cross-checked against the payload — an empty or
/// benign list passes trivially while the patch edits anything (whole-repo
/// review, Evolution P1). This parses the payload itself:
/// 1. Search-and-replace JSON edits: every entry's `file` field.
/// 2. Unified diffs: every `+++ b/<path>` header, plus `--- a/<path>` for
///    pure deletions (whose `+++` target is `/dev/null`).
fn patch_edited_paths(patch: &str) -> Vec<PathBuf> {
    // Search-and-replace format (mirrors apply_edits' dispatch).
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        if !edits.is_empty() && edits[0].get("search").is_some() {
            return edits
                .iter()
                .filter_map(|e| e["file"].as_str().map(PathBuf::from))
                .collect();
        }
    }

    // Unified diff.
    let mut paths = Vec::new();
    for line in patch.lines() {
        let (header, prefix) = if let Some(rest) = line.strip_prefix("+++ ") {
            (rest, "b/")
        } else if let Some(rest) = line.strip_prefix("--- ") {
            (rest, "a/")
        } else {
            continue;
        };
        let p = header.trim().trim_start_matches(prefix);
        if !p.is_empty() && p != "/dev/null" {
            let path = PathBuf::from(p);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// The protected-path gate for a hypothesis. Enforced on the paths the
/// patch ACTUALLY edits (see [`patch_edited_paths`]); the LLM-declared
/// `target_files` metadata is kept only as an additional signal.
fn hypothesis_touches_protected(h: &Hypothesis) -> bool {
    h.target_files.iter().any(|f| is_protected(f))
        || patch_edited_paths(&h.patch).iter().any(|f| is_protected(f))
}

/// Apply edits to a directory. The `patch` field may be:
/// 1. A JSON array of {file, search, replace} edits (new format)
/// 2. A unified diff string (legacy format)
pub fn apply_edits(dir: &Path, patch: &str) -> bool {
    // Try search-and-replace format first
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        if !edits.is_empty() && edits[0].get("search").is_some() {
            return apply_search_replace(dir, &edits);
        }
    }

    // Fall back to unified diff with progressive strategies
    apply_unified_diff(dir, patch)
}

/// Apply search-and-replace edits: for each edit, find the `search` string
/// in the file and replace it with `replace`. Supports fuzzy whitespace matching.
fn apply_search_replace(dir: &Path, edits: &[serde_json::Value]) -> bool {
    // Collect all edits per file, then apply them all at once
    let mut file_edits: std::collections::HashMap<String, Vec<(&str, &str)>> =
        std::collections::HashMap::new();

    for edit in edits {
        let file = match edit["file"].as_str() {
            Some(f) => f,
            None => return false,
        };
        let search = match edit["search"].as_str() {
            Some(s) => s,
            None => return false,
        };
        let replace = match edit["replace"].as_str() {
            Some(r) => r,
            None => return false,
        };
        file_edits
            .entry(file.to_string())
            .or_default()
            .push((search, replace));
    }

    for (file, edits) in &file_edits {
        // Contain the model-produced edit path inside the working dir: an
        // absolute or `../` path must not read or overwrite host files.
        let Some(path) = contained_path(dir, Path::new(file)) else {
            log_warning(&format!(
                "Refusing edit to path outside the working directory: {}",
                file
            ));
            return false;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let mut modified = content.clone();
        for (search, replace) in edits {
            // Try exact match first
            if modified.contains(search) {
                let count = modified.matches(search).count();
                if count > 1 {
                    log_warning(&format!(
                        "  Ambiguous search string in {} ({} matches): {:?}...",
                        file,
                        count,
                        truncate_char_boundary(search, 80)
                    ));
                    return false;
                }
                modified = modified.replacen(search, replace, 1);
                continue;
            }

            // Fuzzy match: try matching by trimmed line content (ignores whitespace diffs)
            match fuzzy_find_and_replace(&modified, search, replace) {
                Some(new_content) => {
                    modified = new_content;
                    continue;
                }
                None => {
                    log_warning(&format!(
                        "  Search string not found in {}: {:?}...",
                        file,
                        truncate_char_boundary(search, 80)
                    ));
                    return false;
                }
            }
        }

        if modified == content {
            log_warning(&format!("  No changes made to {}", file));
            return false;
        }

        if std::fs::write(&path, &modified).is_err() {
            return false;
        }
    }
    true
}

/// Find `search` in `content` with fuzzy whitespace matching, then replace
/// with `replace` (adjusted to match the file's original indentation).
/// Returns the modified content, or None if no match found.
fn fuzzy_find_and_replace(content: &str, search: &str, replace: &str) -> Option<String> {
    let search_lines: Vec<&str> = search.lines().collect();
    if search_lines.is_empty() {
        return None;
    }

    let content_lines: Vec<&str> = content.lines().collect();
    let first_trimmed = search_lines[0].trim();
    if first_trimmed.is_empty() {
        return None;
    }

    // Scan content lines for a match
    for start_idx in 0..content_lines.len() {
        let content_trimmed = content_lines[start_idx].trim();
        if content_trimmed != first_trimmed {
            continue;
        }

        // Check if all subsequent search lines match (trimmed)
        if start_idx + search_lines.len() > content_lines.len() {
            continue;
        }

        let mut all_match = true;
        for (j, search_line) in search_lines.iter().enumerate() {
            let cl = content_lines[start_idx + j].trim();
            let sl = search_line.trim();
            if cl != sl {
                all_match = false;
                break;
            }
        }

        if !all_match {
            continue;
        }

        // Found a match! Now compute the indentation offset.
        // The file's indentation for the first matched line vs the search's indentation.
        let file_indent = leading_whitespace(content_lines[start_idx]);
        let search_indent = leading_whitespace(search_lines[0]);

        // Build the replacement with adjusted indentation
        let replace_lines: Vec<&str> = replace.lines().collect();
        let mut adjusted_replace = String::new();
        for (k, rline) in replace_lines.iter().enumerate() {
            let rline_trimmed_start = rline.trim_start();
            if rline_trimmed_start.is_empty() {
                adjusted_replace.push('\n');
                continue;
            }
            let replace_indent = leading_whitespace(rline);
            // If the replace line has the search indent as a base, rebase to file indent
            let new_indent = if let Some(extra) = replace_indent.strip_prefix(search_indent) {
                format!("{}{}", file_indent, extra)
            } else {
                // Can't rebase — use file_indent for first line, original for rest
                if k == 0 {
                    file_indent.to_string()
                } else {
                    replace_indent.to_string()
                }
            };
            adjusted_replace.push_str(&new_indent);
            adjusted_replace.push_str(rline_trimmed_start);
            adjusted_replace.push('\n');
        }

        // Remove trailing newline if the search didn't end with one
        if !search.ends_with('\n') && adjusted_replace.ends_with('\n') {
            adjusted_replace.pop();
        }

        // Build the result: lines before + adjusted replace + lines after
        let mut result = String::new();
        for line in &content_lines[..start_idx] {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&adjusted_replace);
        let end_idx = start_idx + search_lines.len();
        if end_idx < content_lines.len() {
            if !result.ends_with('\n') {
                result.push('\n');
            }
            for (k, line) in content_lines[end_idx..].iter().enumerate() {
                result.push_str(line);
                if end_idx + k + 1 < content_lines.len() {
                    result.push('\n');
                }
            }
        }

        // Preserve trailing newline if original had one
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        return Some(result);
    }

    None
}

/// Extract the leading whitespace of a line
fn leading_whitespace(line: &str) -> &str {
    let trimmed = line.trim_start();
    &line[..line.len() - trimmed.len()]
}

/// Apply a unified diff with progressive fallback strategies:
/// 1. `git apply` (strict)
/// 2. `git apply --ignore-whitespace -C1`
/// 3. `patch -p1 -F3` (fuzz factor 3)
fn apply_unified_diff(dir: &Path, patch: &str) -> bool {
    let patch_file = dir.join(".evolution-patch");
    if std::fs::write(&patch_file, patch).is_err() {
        return false;
    }

    // Strategy 1: strict git apply
    let strict = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["apply", ".evolution-patch"])
        .current_dir(dir)
        .output();
    if strict.map(|o| o.status.success()).unwrap_or(false) {
        let _ = std::fs::remove_file(&patch_file);
        return true;
    }

    // Strategy 2: git apply with relaxed whitespace and reduced context
    let relaxed = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["apply", "--ignore-whitespace", "-C1", ".evolution-patch"])
        .current_dir(dir)
        .output();
    if relaxed.map(|o| o.status.success()).unwrap_or(false) {
        let _ = std::fs::remove_file(&patch_file);
        return true;
    }

    // Strategy 3: patch -p1 with fuzz factor 3
    let fuzz = Command::new("patch")
        .args([
            "-p1",
            "-F3",
            "--batch",
            "--silent",
            "-i",
            ".evolution-patch",
        ])
        .current_dir(dir)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    fuzz.map(|o| o.status.success()).unwrap_or(false)
}

fn apply_patch_to_worktree(worktree: &Path, patch: &str) -> bool {
    apply_edits(worktree, patch)
}

#[allow(dead_code)] // safety gate reserved for the direct repo-apply path
fn apply_patch_to_repo(repo_root: &Path, patch: &str) -> bool {
    // Final protected-path gate at the highest-blast-radius point: refuse
    // to apply ANY patch that edits protected files to the real repo, even
    // if a future caller bypasses the hypothesis safety filter.
    let edited = patch_edited_paths(patch);
    if let Some(p) = edited.iter().find(|f| is_protected(f)) {
        log_error(&format!(
            "Refusing to apply patch to repo: edits protected path {}",
            p.display()
        ));
        return false;
    }
    apply_edits(repo_root, patch)
}

/// RAII guard that removes a shadow worktree when the evaluation iteration
/// ends — on success, on early `continue`, AND on panic unwind. The old flow
/// called `cleanup_worktree` manually at each exit, so a panic anywhere in
/// the iteration leaked worktrees under `.worktrees/`.
struct WorktreeGuard<'a> {
    repo_root: &'a Path,
    path: PathBuf,
}

impl<'a> WorktreeGuard<'a> {
    fn new(repo_root: &'a Path, path: PathBuf) -> Self {
        Self { repo_root, path }
    }
}

impl Drop for WorktreeGuard<'_> {
    fn drop(&mut self) {
        let _ = ast_tools::cleanup_worktree(self.repo_root, &self.path);
    }
}

/// Capture the exact tested worktree state as a unified diff against HEAD.
///
/// Runs inside the throwaway shadow worktree, so `git add -A` stages into the
/// worktree's OWN index — never the user's. The captured diff includes the
/// `cargo fmt` auto-fix applied during evaluation, which the raw LLM patch
/// lacks. Applying THIS diff to the repo is what makes the committed state
/// byte-identical to the state that passed the gates.
fn capture_tested_diff(worktree: &Path) -> Option<String> {
    let add = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["add", "-A"])
        .current_dir(worktree)
        .output()
        .ok()?;
    if !add.status.success() {
        return None;
    }
    let diff = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["diff", "--cached", "--binary", "HEAD"])
        .current_dir(worktree)
        .output()
        .ok()?;
    if !diff.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&diff.stdout).into_owned())
}

/// Capture the git tree object ID of the staged files in a worktree or repository.
pub(crate) fn capture_worktree_tree_id(worktree: &Path) -> Option<String> {
    let output = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["write-tree"])
        .current_dir(worktree)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Apply the tested diff to the repo and commit ONLY the paths it edits.
/// On commit failure the apply is reverted so the user's worktree returns to
/// its pre-apply state instead of being left half-winner'd. Returns true only
/// when the winner is fully applied AND committed.
///
/// If `expected_tree` is supplied, the promoted tree must match the evaluated
/// benchmark tree exactly, preventing committing unbenchmarked code combinations.
pub(crate) async fn commit_winner_to_repo(
    repo_root: &Path,
    tested_diff: &str,
    expected_tree: Option<&str>,
    commit_msg: &str,
) -> bool {
    // Re-check shutdown immediately before applying and committing:
    if crate::is_shutdown_requested() {
        log_warning("Shutdown requested before commit — refusing to commit winner mutation");
        return false;
    }

    // Re-check killswitch immediately before applying and committing:
    // even if the cycle started green, a trip during in-flight evaluation must halt mutation.
    if let Err(err) = crate::safety::killswitch::check_killswitch(Some(repo_root)) {
        log_error(&format!(
            "Killswitch active before commit: {err} — refusing to commit mutation"
        ));
        return false;
    }

    // Require an expected evaluated tree digest upfront (fail-closed promotion gate).
    let Some(expected) = expected_tree else {
        log_error("Missing evaluated tree verification digest — refusing to commit unbenchmarked code (fail-closed promotion gate)");
        return false;
    };
    let expected = expected.trim();
    if expected.is_empty() {
        log_error("Empty evaluated tree verification digest — refusing to commit unbenchmarked code (fail-closed promotion gate)");
        return false;
    }

    if !apply_tested_diff_to_repo(repo_root, tested_diff) {
        return false;
    }
    let edited = patch_edited_paths(tested_diff);
    warn_unrelated_dirty_paths(repo_root, &edited);

    // Promote and commit using an isolated index. This guarantees that:
    // 1. The verified tree and the committed tree are byte-identical (both computed from the isolated index).
    // 2. Unrelated staged changes or staged reverts in the user's main index do not distort verification or commit.
    match commit_scoped_paths_isolated(repo_root, &edited, Some(expected), commit_msg).await {
        Ok(_) => true,
        Err(err) => {
            log_error(&format!(
                "Promotion commit failed: {err} — reverting applied diff"
            ));
            revert_applied_diff(repo_root, tested_diff);
            false
        }
    }
}

/// Apply the tested diff to the real repository with STRICT `git apply` — no
/// fuzzy `patch -F3` fallback. Fuzz is what let the old flow land hunks in
/// different spots than the tested worktree; a strict apply either reproduces
/// the tested byte content exactly or fails (and failure skips the commit).
/// The protected-path gate runs on the tested diff itself.
fn apply_tested_diff_to_repo(repo_root: &Path, tested_diff: &str) -> bool {
    let edited = patch_edited_paths(tested_diff);
    if let Some(p) = edited.iter().find(|f| is_protected(f)) {
        log_error(&format!(
            "Refusing to apply tested diff to repo: edits protected path {}",
            p.display()
        ));
        return false;
    }
    let patch_file = repo_root.join(".evolution-tested.patch");
    if std::fs::write(&patch_file, tested_diff).is_err() {
        return false;
    }
    let applied = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["apply", ".evolution-tested.patch"])
        .current_dir(repo_root)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    match applied {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            log_warning(&format!(
                "  Tested diff does not apply cleanly to the repo (worktree drift?): {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
            false
        }
        Err(e) => {
            log_warning(&format!("  Failed to run git apply: {}", e));
            false
        }
    }
}

/// Reverse-apply a previously applied tested diff — used when the scoped
/// commit fails — so the user's worktree returns to its pre-apply state.
fn revert_applied_diff(repo_root: &Path, tested_diff: &str) {
    let patch_file = repo_root.join(".evolution-tested.patch");
    if std::fs::write(&patch_file, tested_diff).is_err() {
        return;
    }
    let _ = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["apply", "-R", ".evolution-tested.patch"])
        .current_dir(repo_root)
        .output();
    let _ = std::fs::remove_file(&patch_file);
}

/// Warn loudly about dirty/untracked paths in the user's worktree that are
/// UNRELATED to the winner patch. They are left uncommitted — the evolution
/// commit only ever stages the paths the tested diff edits.
fn warn_unrelated_dirty_paths(repo_root: &Path, edited: &[PathBuf]) {
    let status = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output();
    let Ok(status) = status else { return };
    if !status.status.success() {
        return;
    }
    let stdout = String::from_utf8_lossy(&status.stdout);
    let unrelated: Vec<&str> = stdout
        .lines()
        // Porcelain lines are `XY <path>` (path starts at byte 3); renames
        // show `orig -> new` — good enough for a warning list.
        .filter_map(|line| line.get(3..))
        .filter(|p| !p.is_empty())
        .filter(|p| !edited.iter().any(|e| e == Path::new(p)))
        .collect();
    if !unrelated.is_empty() {
        log_warning(&format!(
            "  {} unrelated dirty/untracked path(s) NOT included in the evolution commit: {}",
            unrelated.len(),
            unrelated
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

struct IsolatedIndexGuard {
    index_path: PathBuf,
}

impl IsolatedIndexGuard {
    fn new(repo_root: &Path) -> Self {
        let unique_name = format!("selfware_promote_idx_{}", uuid::Uuid::new_v4().simple());
        let dot_git = repo_root.join(".git");
        let index_path = if dot_git.is_dir() {
            dot_git.join(unique_name)
        } else {
            std::env::temp_dir().join(unique_name)
        };
        Self { index_path }
    }
}

impl Drop for IsolatedIndexGuard {
    fn drop(&mut self) {
        if self.index_path.exists() {
            let _ = std::fs::remove_file(&self.index_path);
        }
    }
}

/// RAII guard holding an advisory exclusive lock on `.selfware/runs/active_evolution.lock`
/// (ensuring only one evolution run operates on the repository at a time) and
/// `.selfware/runs/<run_id>.lock` (acting as a per-run liveness marker for orphan sweepers),
/// releasing and removing them on drop.
pub struct RunLockGuard {
    pub lock_path: PathBuf,
    pub lock_file: Option<std::fs::File>,
    pub global_lock_path: PathBuf,
    pub global_lock_file: Option<std::fs::File>,
}

impl RunLockGuard {
    pub fn acquire(repo_root: &Path, run_id: &str) -> Result<Self, String> {
        let runs_dir = repo_root.join(".selfware").join("runs");
        std::fs::create_dir_all(&runs_dir).map_err(|e| {
            format!(
                "Failed to create runs directory '{}': {e}",
                runs_dir.display()
            )
        })?;

        // 1. Acquire global repository lock to prevent concurrent evolution runs on the same repo
        let global_lock_path = runs_dir.join("active_evolution.lock");
        let mut global_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&global_lock_path)
            .map_err(|e| {
                format!(
                    "Failed to open global evolution lock file '{}': {e}",
                    global_lock_path.display()
                )
            })?;

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let rc = unsafe {
                nix::libc::flock(
                    global_file.as_raw_fd(),
                    nix::libc::LOCK_EX | nix::libc::LOCK_NB,
                )
            };
            if rc != 0 {
                return Err(format!(
                    "Failed to acquire exclusive lock on '{}': another evolution process is already running on this repository",
                    global_lock_path.display()
                ));
            }
        }

        use std::io::{Seek, SeekFrom, Write};
        let _ = global_file.set_len(0);
        let _ = global_file.seek(SeekFrom::Start(0));
        let _ = writeln!(global_file, "{} {}", std::process::id(), run_id);
        let _ = global_file.flush();

        // 2. Acquire per-run lock file as liveness marker
        let lock_path = runs_dir.join(format!("{run_id}.lock"));
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                format!(
                    "Failed to open run lock file '{}': {e}",
                    lock_path.display()
                )
            })?;

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let rc = unsafe {
                nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_EX | nix::libc::LOCK_NB)
            };
            if rc != 0 {
                return Err(format!(
                    "Failed to acquire exclusive advisory lock on run '{}' (another instance may be running)",
                    run_id
                ));
            }
        }

        let _ = file.set_len(0);
        let _ = file.seek(SeekFrom::Start(0));
        let _ = writeln!(file, "{}", std::process::id());
        let _ = file.flush();

        Ok(Self {
            lock_path,
            lock_file: Some(file),
            global_lock_path,
            global_lock_file: Some(global_file),
        })
    }
}

impl Drop for RunLockGuard {
    fn drop(&mut self) {
        if let Some(file) = self.lock_file.take() {
            #[cfg(unix)]
            unsafe {
                use std::os::fd::AsRawFd;
                nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_UN);
            }
            drop(file);
        }
        let _ = std::fs::remove_file(&self.lock_path);

        if let Some(global_file) = self.global_lock_file.take() {
            #[cfg(unix)]
            unsafe {
                use std::os::fd::AsRawFd;
                nix::libc::flock(global_file.as_raw_fd(), nix::libc::LOCK_UN);
            }
            drop(global_file);
        }
        // Do NOT delete active_evolution.lock — retain it permanently on disk so concurrent
        // processes always contend on the exact same inode without unlinking races.
    }
}

/// Cleans up any leftover staging worktrees under `.worktrees/` or legacy staging refs
/// from previous interrupted runs, ensuring that startup or pre-commit state is clean.
pub(crate) fn cleanup_stale_staging_artifacts(repo_root: &Path) {
    let worktrees_dir = repo_root.join(".worktrees");
    if let Ok(entries) = std::fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("staging-commit-") {
                let path = entry.path();
                let _ = ast_tools::cleanup_worktree(repo_root, &path);
            }
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads/selfware-staging-*",
        ])
        .current_dir(repo_root)
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let refname = line.trim();
            if !refname.is_empty() {
                let _ = std::process::Command::new("git")
                    .args(["update-ref", "-d", refname])
                    .current_dir(repo_root)
                    .output();
            }
        }
    }
}

/// Commits the specified paths using an isolated index file.
///
/// If `expected_tree` is provided, verifies that `git write-tree` on the isolated index
/// exactly matches `expected_tree` before committing.
/// Both verification and the commit are executed against the exact same isolated index,
/// ensuring that unrelated staged changes or staged reverts in the main index cannot cause
/// the committed tree to diverge from the verified tree.
pub(crate) async fn commit_scoped_paths_isolated(
    repo_root: &Path,
    paths: &[PathBuf],
    expected_tree: Option<&str>,
    commit_msg: &str,
) -> Result<String, String> {
    commit_scoped_paths_isolated_with_timeout(repo_root, paths, expected_tree, commit_msg, None)
        .await
}

pub(crate) async fn commit_scoped_paths_isolated_with_timeout(
    repo_root: &Path,
    paths: &[PathBuf],
    expected_tree: Option<&str>,
    commit_msg: &str,
    timeout_override_secs: Option<u64>,
) -> Result<String, String> {
    if paths.is_empty() {
        return Err("Winner patch edits no paths — nothing to commit".to_string());
    }

    let guard = IsolatedIndexGuard::new(repo_root);

    // 1. Initialize isolated index with the tree of HEAD
    let read_tree = tokio::process::Command::new("git")
        .env("GIT_INDEX_FILE", &guard.index_path)
        .args(["read-tree", "HEAD"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git read-tree: {e}"))?;
    if !read_tree.status.success() {
        return Err(format!(
            "git read-tree HEAD failed: {}",
            String::from_utf8_lossy(&read_tree.stderr).trim()
        ));
    }

    // 2. Stage ONLY the specified paths into the isolated index
    let mut add = tokio::process::Command::new("git");
    add.env("GIT_INDEX_FILE", &guard.index_path);
    add.arg("add").arg("-A").arg("--");
    for p in paths {
        add.arg(p);
    }
    let add_out = add
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git add into isolated index: {e}"))?;
    if !add_out.status.success() {
        return Err(format!(
            "git add into isolated index failed: {}",
            String::from_utf8_lossy(&add_out.stderr).trim()
        ));
    }

    // 3. Write and capture the tree from the isolated index
    let write_tree = tokio::process::Command::new("git")
        .env("GIT_INDEX_FILE", &guard.index_path)
        .args(["write-tree"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git write-tree on isolated index: {e}"))?;
    if !write_tree.status.success() {
        return Err(format!(
            "git write-tree on isolated index failed: {}",
            String::from_utf8_lossy(&write_tree.stderr).trim()
        ));
    }
    let promoted_tree = String::from_utf8_lossy(&write_tree.stdout)
        .trim()
        .to_string();
    if promoted_tree.is_empty() {
        return Err("git write-tree produced empty tree SHA".to_string());
    }

    // 4. Verify against expected_tree if provided
    if let Some(expected) = expected_tree {
        let expected = expected.trim();
        if promoted_tree != expected {
            return Err(format!(
                "Promoted tree mismatch: evaluated benchmark tree is {expected}, but isolated index produced {promoted_tree} (untested code combination from divergent HEAD) — refusing to commit unbenchmarked code"
            ));
        }
    }

    // 5. Commit using a separate isolated staging worktree to guarantee that the developer
    // checkout's HEAD is NEVER modified or redirected during commit and verification (Finding 1).
    if crate::is_shutdown_requested() {
        return Err("Shutdown requested before commit".to_string());
    }
    if crate::safety::killswitch::is_killswitch_active() {
        return Err("Kill switch is active before commit".to_string());
    }

    cleanup_stale_staging_artifacts(repo_root);

    let head_before_out = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to read HEAD from repository: {e}"))?;
    if !head_before_out.status.success() {
        return Err(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&head_before_out.stderr).trim()
        ));
    }
    let head_before = String::from_utf8_lossy(&head_before_out.stdout)
        .trim()
        .to_string();
    if head_before.is_empty() {
        return Err("git rev-parse HEAD returned empty commit SHA".to_string());
    }

    let symref_out = tokio::process::Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to inspect symbolic-ref HEAD: {e}"))?;
    let original_symref = if symref_out.status.success() {
        let s = String::from_utf8_lossy(&symref_out.stdout)
            .trim()
            .to_string();
        if s.is_empty() {
            return Err("symbolic-ref returned empty branch name".to_string());
        }
        if s.contains("selfware-staging-") {
            return Err(format!(
                "Repository checkout is currently pointing to a leftover staging ref {s}; refusing to promote until checkout is restored to a valid branch"
            ));
        }
        Some(s)
    } else {
        None
    };

    let worktrees_dir = repo_root.join(".worktrees");
    let _ = std::fs::create_dir_all(&worktrees_dir);
    let worktree_name = format!("staging-commit-{}", uuid::Uuid::new_v4().simple());
    let worktree_path = worktrees_dir.join(&worktree_name);

    let add_wt = tokio::process::Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg(&head_before)
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to create staging worktree: {e}"))?;
    if !add_wt.status.success() {
        return Err(format!(
            "git worktree add for staging failed: {}",
            String::from_utf8_lossy(&add_wt.stderr).trim()
        ));
    }

    struct StagingWorktreeGuard<'a> {
        repo_root: &'a Path,
        worktree_path: PathBuf,
    }

    impl<'a> Drop for StagingWorktreeGuard<'a> {
        fn drop(&mut self) {
            let _ = ast_tools::cleanup_worktree(self.repo_root, &self.worktree_path);
        }
    }

    let _wt_guard = StagingWorktreeGuard {
        repo_root,
        worktree_path: worktree_path.clone(),
    };

    // Copy the scoped paths into the staging worktree
    for p in paths {
        let src_path = repo_root.join(p);
        let dst_path = worktree_path.join(p);
        if src_path.exists() {
            if let Some(parent) = dst_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {} to staging worktree: {e}", p.display()))?;
        } else if dst_path.exists() {
            let _ = std::fs::remove_file(&dst_path);
        }
    }

    let mut wt_add = tokio::process::Command::new("git");
    wt_add.env_remove("GIT_INDEX_FILE");
    wt_add.arg("add").arg("-A").arg("--");
    for p in paths {
        wt_add.arg(p);
    }
    let wt_add_out = wt_add
        .current_dir(&worktree_path)
        .output()
        .await
        .map_err(|e| format!("Failed to stage candidate files in staging worktree: {e}"))?;
    if !wt_add_out.status.success() {
        return Err(format!(
            "git add in staging worktree failed: {}",
            String::from_utf8_lossy(&wt_add_out.stderr).trim()
        ));
    }

    let wt_tree_out = tokio::process::Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["write-tree"])
        .current_dir(&worktree_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git write-tree in staging worktree: {e}"))?;
    if !wt_tree_out.status.success() {
        return Err(format!(
            "git write-tree in staging worktree failed: {}",
            String::from_utf8_lossy(&wt_tree_out.stderr).trim()
        ));
    }
    let wt_tree = String::from_utf8_lossy(&wt_tree_out.stdout)
        .trim()
        .to_string();
    if wt_tree != promoted_tree {
        return Err(format!(
            "Staging worktree tree {wt_tree} does not match evaluated tree {promoted_tree}"
        ));
    }

    let mut commit_cmd = tokio::process::Command::new("git");
    commit_cmd
        .env_remove("GIT_INDEX_FILE")
        .args(["commit", "-m", commit_msg])
        .current_dir(&worktree_path);

    let timeout_secs = timeout_override_secs
        .filter(|&s| s > 0 && s <= 86400)
        .or_else(|| {
            std::env::var("SELFWARE_COMMIT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&s| s > 0 && s <= 86400)
        })
        .unwrap_or(600);
    let timeout_dur = std::time::Duration::from_secs(timeout_secs);

    let commit_res = run_cancellable_subprocess(commit_cmd, timeout_dur).await;

    let staging_commit_after = tokio::process::Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["rev-parse", "HEAD"])
        .current_dir(&worktree_path)
        .output()
        .await
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    let mut our_commit_succeeded = false;
    let mut tree_matches = false;
    let mut parent_matches = false;
    if let Some(ref new_commit) = staging_commit_after {
        if new_commit != &head_before {
            let head_tree = tokio::process::Command::new("git")
                .env_remove("GIT_INDEX_FILE")
                .args(["rev-parse", "HEAD^{tree}"])
                .current_dir(&worktree_path)
                .output()
                .await
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            let head_parent = tokio::process::Command::new("git")
                .env_remove("GIT_INDEX_FILE")
                .args(["rev-parse", "HEAD^"])
                .current_dir(&worktree_path)
                .output()
                .await
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            tree_matches = head_tree.as_deref() == Some(&promoted_tree);
            parent_matches = head_parent.as_deref() == Some(&head_before);

            if tree_matches && parent_matches {
                our_commit_succeeded = true;
            } else {
                log_warning(&format!(
                    "Candidate commit created in staging worktree does not match evaluated candidate (tree match: {tree_matches}, parent match: {parent_matches}); refusing to update destination branch"
                ));
            }
        }
    }

    match commit_res {
        Ok(out) if out.status.success() => {
            if !our_commit_succeeded {
                return Err(format!(
                    "git commit succeeded with status 0, but committed tree or parent does not match evaluated candidate (tree match: {tree_matches}, parent match: {parent_matches}); aborting promotion"
                ));
            }
        }
        Err(SubprocessError::ShutdownRequested) => {
            return Err(
                "Shutdown requested during git commit hook; publication aborted".to_string(),
            );
        }
        Err(SubprocessError::Timeout) => {
            return Err(format!(
                "git commit hook timed out after {timeout_secs}s; publication aborted"
            ));
        }
        Ok(out) => {
            if our_commit_succeeded {
                log_warning(&format!(
                    "git commit hook exited non-zero ({}), but candidate commit succeeded in staging worktree (matching tree {promoted_tree}); continuing to publication check",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            } else {
                return Err(format!(
                    "git commit in staging worktree failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        }
        Err(SubprocessError::Io(e)) => {
            return Err(format!(
                "Failed to execute git commit in staging worktree: {e}"
            ));
        }
    };

    // Recheck shutdown and the kill switch immediately before publication (Finding 2)
    if crate::is_shutdown_requested() {
        return Err("Shutdown requested before publication; promotion aborted".to_string());
    }
    if crate::safety::killswitch::is_killswitch_active() {
        return Err("Kill switch is active before publication; promotion aborted".to_string());
    }

    // Promotion verified! Atomically update destination branch to the verified commit via CAS:
    let commit_sha =
        staging_commit_after.ok_or_else(|| "No staging commit was created".to_string())?;
    if let Some(ref dest_branch) = original_symref {
        let mut update_dest = tokio::process::Command::new("git");
        update_dest.args(["update-ref", dest_branch, &commit_sha, &head_before]);
        let update_res = update_dest
            .current_dir(repo_root)
            .output()
            .await
            .map_err(|e| format!("Failed to update destination branch {dest_branch}: {e}"))?;
        if !update_res.status.success() {
            return Err(format!(
                "git update-ref {dest_branch} to {commit_sha} (CAS against {head_before}) failed: {}",
                String::from_utf8_lossy(&update_res.stderr).trim()
            ));
        }
    } else {
        let mut update_head = tokio::process::Command::new("git");
        update_head.args([
            "update-ref",
            "--no-deref",
            "HEAD",
            &commit_sha,
            &head_before,
        ]);
        let update_res = update_head
            .current_dir(repo_root)
            .output()
            .await
            .map_err(|e| format!("Failed to update detached HEAD to {commit_sha}: {e}"))?;
        if !update_res.status.success() {
            return Err(format!(
                "git update-ref --no-deref HEAD {commit_sha} (CAS against {head_before}) failed: {}",
                String::from_utf8_lossy(&update_res.stderr).trim()
            ));
        }
    }

    // 6. Synchronize main repository index for the committed paths so working copy status is clean,
    // without disturbing any unrelated staged changes.
    let mut reset = tokio::process::Command::new("git");
    reset.env_remove("GIT_INDEX_FILE");
    reset.args(["reset", "HEAD", "--"]);
    for p in paths {
        reset.arg(p);
    }
    match reset.current_dir(repo_root).output().await {
        Ok(reset_out) if reset_out.status.success() => {}
        Ok(reset_out) => {
            let err_msg = String::from_utf8_lossy(&reset_out.stderr)
                .trim()
                .to_string();
            log_warning(&format!(
                "git reset HEAD -- <paths> after isolated commit produced warning: {err_msg}; preserving index to avoid disturbing unrelated staged files"
            ));
            log_event(
                repo_root,
                &serde_json::json!({
                    "event": "index_resync_failed",
                    "error": err_msg,
                    "paths": paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                    "timestamp": chrono_now(),
                }),
            );
        }
        Err(e) => {
            log_warning(&format!(
                "Failed to execute git reset HEAD -- <paths> after isolated commit: {e}; preserving index to avoid disturbing unrelated staged files"
            ));
            log_event(
                repo_root,
                &serde_json::json!({
                    "event": "index_resync_failed",
                    "error": format!("{e}"),
                    "paths": paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                    "timestamp": chrono_now(),
                }),
            );
        }
    }

    Ok(promoted_tree)
}

/// Stage and commit ONLY the given paths using an isolated index — never `git add -A`,
/// keeping unrelated staged changes and untracked files completely untouched and uncommitted.
#[allow(dead_code)]
pub(crate) async fn commit_scoped_paths(
    repo_root: &Path,
    paths: &[PathBuf],
    commit_msg: &str,
) -> bool {
    match commit_scoped_paths_isolated(repo_root, paths, None, commit_msg).await {
        Ok(_) => true,
        Err(e) => {
            log_warning(&format!("  commit_scoped_paths failed: {e}"));
            false
        }
    }
}

// ═══════════════════════════════════════════════════════
// LOGGING (using the selfware garden aesthetic)
// ═══════════════════════════════════════════════════════

fn log_phase(msg: &str) {
    eprintln!("  🌱 {}", msg);
}

fn log_warning(msg: &str) {
    eprintln!("  🥀 {}", msg);
}

fn log_error(msg: &str) {
    eprintln!("  ❄️  {}", msg);
}

fn log_baseline(metrics: &FitnessMetrics, sab_mode: bool) {
    let label = if sab_mode { "SAB" } else { "compile/test" };
    eprintln!(
        "  📊 Baseline: {} {:.0}/100 ({}) | {} tokens | {:.0}s",
        label,
        metrics.sab_score,
        rating_from_score(metrics.sab_score),
        metrics
            .tokens_used
            .map_or_else(|| "unmeasured".to_string(), |t| t.to_string()),
        metrics.wall_clock_secs
    );
}

fn log_generation_start(gen: usize) {
    eprintln!(
        "\n╭─── Generation {} ───────────────────────────────────╮",
        gen
    );
}

fn log_bloom(_gen: usize, description: &str, old_sab: f64, new_sab: f64) {
    eprintln!(
        "│  🌸 BLOOM! SAB {:.0} → {:.0} (+{:.1})",
        old_sab,
        new_sab,
        new_sab - old_sab
    );
    eprintln!("│  📝 {}", description);
    eprintln!("╰────────────────────────────────────────────────────╯");
}

fn chrono_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", d.as_secs(), d.subsec_millis())
}

/// Append a structured JSONL event to .evolution-log.jsonl for real-time visualization.
fn log_event(repo_root: &Path, event: &serde_json::Value) {
    use std::io::Write;
    let log_path = repo_root.join(".evolution-log.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(f, "{}", event);
    }
}

/// Helper to parse timestamp (epoch float or ISO8601/RFC3339) into epoch seconds
fn parse_event_or_node_timestamp(ts: &str) -> Option<f64> {
    if let Ok(v) = ts.parse::<f64>() {
        return Some(v);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return Some(dt.timestamp() as f64 + dt.timestamp_subsec_millis() as f64 / 1000.0);
    }
    None
}

/// Information recovered about an unended run
#[derive(Default)]
struct RecoveredRunProgress {
    pid: Option<i32>,
    start_time: Option<f64>,
    max_generation: usize,
    last_incumbent_score: f64,
    best_attempted_score: f64,
    last_activity_time: Option<f64>,
}

/// Scans `.evolution-log.jsonl` for runs that have an `event: "start"` but no corresponding
/// `event: "run_end"`. For any truly dead run (PID dead and no active flock held), recovers
/// actual recorded progress (generations, SAB score, duration) and logs `run_end{outcome: "killed"}`.
/// Returns the number of orphaned runs closed.
pub fn sweep_orphaned_runs(repo_root: &Path) -> usize {
    let log_path = repo_root.join(".evolution-log.jsonl");
    let content = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let mut started_runs: Vec<String> = Vec::new();
    let mut ended_runs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut run_progress: std::collections::HashMap<String, RecoveredRunProgress> =
        std::collections::HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let event = val.get("event").and_then(|v| v.as_str()).unwrap_or("");
            let run_id = val
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if run_id.is_empty() {
                continue;
            }

            let entry = run_progress.entry(run_id.clone()).or_default();

            let event_ts = val
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(parse_event_or_node_timestamp);
            if let Some(ts) = event_ts {
                if entry.last_activity_time.map(|t| ts > t).unwrap_or(true) {
                    entry.last_activity_time = Some(ts);
                }
            }

            if event == "start" {
                if !started_runs.contains(&run_id) {
                    started_runs.push(run_id.clone());
                }
                if let Some(pid_val) = val.get("pid").and_then(|v| v.as_i64()) {
                    entry.pid = Some(pid_val as i32);
                }
                if let Some(ts) = event_ts {
                    entry.start_time = Some(ts);
                }
            } else if event == "run_end" {
                ended_runs.insert(run_id);
            } else if event == "baseline" {
                if let Some(score) = val.get("sab_score").and_then(|v| v.as_f64()) {
                    entry.last_incumbent_score = score;
                    if score > entry.best_attempted_score {
                        entry.best_attempted_score = score;
                    }
                }
            } else if event == "generation_end" || event == "generation_start" {
                if let Some(gen) = val.get("generation").and_then(|v| v.as_u64()) {
                    entry.max_generation = entry.max_generation.max(gen as usize);
                }
                if event == "generation_end"
                    && val.get("outcome").and_then(|v| v.as_str()) == Some("bloom")
                {
                    if let Some(score) = val.get("score_after").and_then(|v| v.as_f64()) {
                        entry.last_incumbent_score = score;
                        if score > entry.best_attempted_score {
                            entry.best_attempted_score = score;
                        }
                    }
                }
            } else if event == "candidate_rejected" {
                if let Some(score) = val
                    .get("sab_score")
                    .or_else(|| val.get("score"))
                    .and_then(|v| v.as_f64())
                {
                    if score > entry.best_attempted_score {
                        entry.best_attempted_score = score;
                    }
                }
            }
        }
    }

    let runs_dir = repo_root.join(".selfware").join("runs");
    let attempts_dir = repo_root.join(".selfware").join("attempts");
    let mut closed_count = 0;

    for run_id in started_runs {
        if ended_runs.contains(&run_id) {
            continue;
        }

        let mut progress = run_progress.remove(&run_id).unwrap_or_default();
        let lock_path = runs_dir.join(format!("{run_id}.lock"));

        // If lock file exists and records PID, read it if not already in start event
        if lock_path.exists() && progress.pid.is_none() {
            if let Ok(pid_str) = std::fs::read_to_string(&lock_path) {
                if let Ok(p) = pid_str.trim().parse::<i32>() {
                    progress.pid = Some(p);
                }
            }
        }

        // Check if the run is active via flock
        #[cfg(unix)]
        let lock_is_held = if lock_path.exists() {
            use std::os::fd::AsRawFd;
            match std::fs::File::open(&lock_path) {
                Ok(file) => {
                    let rc = unsafe {
                        nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_EX | nix::libc::LOCK_NB)
                    };
                    if rc != 0 {
                        true // Another process actively holds this lock!
                    } else {
                        // We acquired it; unlock now
                        unsafe {
                            nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_UN);
                        }
                        false
                    }
                }
                Err(_) => false,
            }
        } else {
            false
        };
        #[cfg(not(unix))]
        let lock_is_held = false;

        if lock_is_held {
            // Live concurrent run holding the lock! Do not sweep.
            continue;
        }

        // Check if recorded PID is alive
        #[cfg(unix)]
        let pid_is_alive = if let Some(pid) = progress.pid {
            let res = unsafe { nix::libc::kill(pid, 0) };
            if res == 0 {
                true
            } else {
                let err = std::io::Error::last_os_error();
                err.raw_os_error() == Some(nix::libc::EPERM)
            }
        } else {
            false
        };
        #[cfg(not(unix))]
        let pid_is_alive = false;

        if pid_is_alive {
            // Process is still running! Do not sweep.
            continue;
        }

        // The run is confirmed dead. Recover any additional progress from attempts file
        let run_attempts_file = attempts_dir.join(format!("{run_id}.jsonl"));
        if let Ok(attempts_content) = std::fs::read_to_string(&run_attempts_file) {
            for line in attempts_content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(node) = serde_json::from_str::<AttemptNode>(trimmed) {
                    progress.max_generation = progress.max_generation.max(node.generation);
                    if let Some(ref m) = node.metrics {
                        if m.sab_score > progress.best_attempted_score {
                            progress.best_attempted_score = m.sab_score;
                        }
                        // Only confirmed incumbent updates last_incumbent_score:
                        // baseline root node, or any promoted winner with committed_commit anchor
                        let is_baseline = node.status == AttemptStatus::Baseline
                            || node.id == "baseline"
                            || node.id.starts_with("baseline-")
                            || node.id == "att-baseline";
                        if is_baseline || node.committed_commit.is_some() {
                            progress.last_incumbent_score = m.sab_score;
                        }
                    }
                    if let Some(ts) = parse_event_or_node_timestamp(&node.created_at) {
                        if progress.last_activity_time.map(|t| ts > t).unwrap_or(true) {
                            progress.last_activity_time = Some(ts);
                        }
                    }
                }
            }
        }

        // Calculate recovered duration
        let duration_secs = match (progress.start_time, progress.last_activity_time) {
            (Some(st), Some(lt)) if lt >= st => lt - st,
            _ => 0.0,
        };

        // Write recovered honest run_end event
        log_event(
            repo_root,
            &serde_json::json!({
                "event": "run_end",
                "kind": "run_end",
                "timestamp": chrono_now(),
                "run_id": run_id,
                "outcome": "killed",
                "reason": "orphaned run detected during startup sweep (process terminated without clean run_end)",
                "generations_run": progress.max_generation,
                "final_sab_score": progress.last_incumbent_score,
                "best_attempted_score": progress.best_attempted_score,
                "duration_secs": duration_secs,
            }),
        );

        // Remove leftover lock file if any
        let _ = std::fs::remove_file(&lock_path);
        closed_count += 1;
    }

    if closed_count > 0 {
        log_warning(&format!(
            "Startup sweep: marked {closed_count} orphaned run(s) as killed in .evolution-log.jsonl"
        ));
    }

    closed_count
}

fn log_frost(_gen: usize, reason: &str) {
    eprintln!("│  ❄️  FROST: {}", reason);
    eprintln!("╰────────────────────────────────────────────────────╯");
}

fn log_reject(_gen: usize, rating: &GenerationRating, winner_sab: f64, baseline_sab: f64) {
    eprintln!(
        "│  {} SAB {:.0} vs baseline {:.0} — rejected",
        rating, winner_sab, baseline_sab
    );
    eprintln!("╰────────────────────────────────────────────────────╯");
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/daemon/daemon_test.rs"]
mod tests;
