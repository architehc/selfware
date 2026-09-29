//! Multi-arm evolution runner: several candidate edits ("arms") built,
//! tested and benchmarked side by side against a baseline, each in its own
//! quarantined source snapshot ([`crate::safety::quarantine`]).
//!
//! For every arm: the candidate source is syntax-checked
//! ([`crate::evolve::ast_scaffold::validate_rust_syntax`], with the error's
//! coordinates), written into the arm's snapshot of the base commit, repaired
//! with rustc's machine-applicable suggestions for at most
//! `max_repair_rounds` passes ([`crate::evolve::compiler_repair`]), compiled
//! (`cargo test --no-run`), tested (`cargo test`) and benchmarked. The
//! baseline (the unmodified base commit) goes through exactly the same steps
//! in its own quarantine, so every number an arm is compared with was
//! measured the same way (Rule 4).
//!
//! Scoring (formal model: `formal/EvolutionBounds.lean`, E4):
//!
//! - an arm PASSES when it compiled, has zero test regressions against the
//!   baseline (no more failures, no fewer passes) and a positive measured
//!   fitness delta: its median benchmark wall time is below the baseline's
//!   AND every one of its samples is faster than every baseline sample (so
//!   run-to-run noise cannot pass an arm);
//! - fitness delta = (baseline median − arm median) / baseline median; the
//!   number of tests does not enter it;
//! - without a benchmark command no fitness is measured and no arm can pass
//!   — the report says so instead of inventing a score;
//! - consensus = at least `consensus_threshold` passing arms; the winner is
//!   the passing arm with the largest delta.
//!
//! Benchmarks run one at a time (a shared lock), baseline included, so
//! parallel builds do not distort the timings. The report carries the
//! quarantine's [`IsolationReport`]: what was and was not isolated.

use crate::evolve::ast_scaffold::{validate_rust_syntax, AstSyntaxError};
use crate::evolve::compiler_repair::{DiagnosticRepairEngine, RepairStop};
use crate::evolve::diagnostics::{report_from_cargo_output, AnalysisKind};
use crate::safety::quarantine::{
    clone_tree, stage_toolchain, ArmQuarantine, HostToolchain, IsolationReport, QuarantineOptions,
    QuarantinedOutput, StagedToolchain,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Output kept per quarantined process (per stream).
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// One candidate edit: the complete new content of one Rust file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationArm {
    /// `[A-Za-z0-9_-]`, unique within a run (names the arm directory).
    pub arm_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Project-relative path of the `.rs` file the arm replaces or adds.
    pub target_file: String,
    /// The full new source of `target_file`.
    #[serde(alias = "proposed_patch")]
    pub proposed_source: String,
}

/// What was measured for the baseline or one arm.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArmMeasurement {
    /// `cargo test --no-run` succeeded (after any repair).
    pub compiled: bool,
    pub tests_passed: usize,
    pub tests_failed: usize,
    /// Wall time of each measured benchmark run, nanoseconds.
    pub bench_samples_ns: Vec<u64>,
    /// Repair passes and fixes applied before the arm compiled (0/0: none).
    pub repair_rounds: usize,
    pub repair_fixes: usize,
    /// Why a step could not complete (timeout, spawn error, failing bench).
    pub error: Option<String>,
    /// Unified diff of the target file against the base (arms only).
    pub diff: Option<String>,
}

/// The scored result of one arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmEvaluation {
    pub arm_id: String,
    pub name: String,
    pub syntax_valid: bool,
    pub syntax_error: Option<AstSyntaxError>,
    /// Refused before anything ran (protected path, bad target path).
    pub rejected: Option<String>,
    pub compile_success: bool,
    pub test_pass_count: usize,
    pub test_fail_count: usize,
    /// Extra failures + lost passes against the baseline.
    pub test_regressions: usize,
    pub bench_samples_ns: Vec<u64>,
    pub median_ns: Option<u64>,
    /// (baseline median − arm median) / baseline median; `None` when either
    /// side has no measurement.
    pub fitness_delta: Option<f64>,
    /// Every arm sample is faster than every baseline sample.
    pub separated_from_baseline: bool,
    pub repair_rounds: usize,
    pub repair_fixes: usize,
    pub passes_consensus_gate: bool,
    /// Why the arm passed or failed, in words.
    pub verdict: String,
    pub error: Option<String>,
    pub diff: Option<String>,
}

/// The baseline as measured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSummary {
    pub measurement: ArmMeasurement,
    pub median_ns: Option<u64>,
}

/// Final report of a multi-arm run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiArmReport {
    pub consensus_threshold: usize,
    pub total_arms: usize,
    pub passing_arms: usize,
    pub consensus_met: bool,
    pub winning_arm: Option<ArmEvaluation>,
    pub evaluations: Vec<ArmEvaluation>,
    pub baseline: BaselineSummary,
    /// Whether a benchmark was configured (no benchmark: no fitness).
    pub fitness_measured: bool,
    pub base_rev: Option<String>,
    pub isolation: Option<IsolationReport>,
}

/// Median of `samples` (lower middle for an even count), `None` if empty.
pub fn median_ns(samples: &[u64]) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut s = samples.to_vec();
    s.sort_unstable();
    Some(s[(s.len() - 1) / 2])
}

impl MultiArmReport {
    /// A PR description naming exactly what was verified.
    pub fn to_pr_markdown(&self) -> String {
        let mut out = String::from("# Autonomous Evolution: Multi-Arm Optimization PR\n\n");
        out.push_str("## Executive Summary\n");
        match (&self.winning_arm, self.consensus_met) {
            (Some(winner), true) => {
                let pct = winner.fitness_delta.unwrap_or(0.0) * 100.0;
                out.push_str(&format!(
                    "Replicate consensus met ({} of {} arms passed; {} required).\n\
                     - **Winning candidate**: `{}` ({})\n\
                     - **Measured benchmark gain**: {:.2}% lower median wall time than the \
                     baseline ({} vs {} ns, {} samples each)\n\
                     - **Verified**: compiled, `cargo test` {}/{} passed with 0 regressions \
                     against the baseline, measured in a quarantined snapshot\n\n",
                    self.passing_arms,
                    self.total_arms,
                    self.consensus_threshold,
                    winner.name,
                    winner.arm_id,
                    pct,
                    winner.median_ns.unwrap_or(0),
                    self.baseline.median_ns.unwrap_or(0),
                    winner.bench_samples_ns.len(),
                    winner.test_pass_count,
                    winner.test_pass_count + winner.test_fail_count,
                ));
            }
            _ => {
                out.push_str(&format!(
                    "Replicate consensus was NOT met ({} of {} arms passed; {} required). \
                     Nothing is promoted.\n",
                    self.passing_arms, self.total_arms, self.consensus_threshold
                ));
                if !self.fitness_measured {
                    out.push_str(
                        "No benchmark command was configured, so no fitness was measured \
                         and no arm could pass.\n",
                    );
                }
                out.push('\n');
            }
        }

        out.push_str("## Arm Telemetry\n\n");
        out.push_str(&format!(
            "Baseline: compiled {}, tests {}/{}, median {} ns ({} samples).\n\n",
            if self.baseline.measurement.compiled {
                "yes"
            } else {
                "NO"
            },
            self.baseline.measurement.tests_passed,
            self.baseline.measurement.tests_passed + self.baseline.measurement.tests_failed,
            self.baseline
                .median_ns
                .map_or("n/a".to_string(), |m| m.to_string()),
            self.baseline.measurement.bench_samples_ns.len(),
        ));
        out.push_str(
            "| Arm ID | Candidate | Syntax | Compile | Tests | Regressions | Median | Delta | Verdict |\n",
        );
        out.push_str("| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |\n");
        for e in &self.evaluations {
            let yes_no = |b: bool| if b { "yes" } else { "no" };
            let ran = e.rejected.is_none() && e.syntax_valid;
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                e.arm_id,
                e.name,
                yes_no(e.syntax_valid),
                if ran {
                    yes_no(e.compile_success)
                } else {
                    "not run"
                },
                if ran && e.compile_success {
                    format!(
                        "{}/{}",
                        e.test_pass_count,
                        e.test_pass_count + e.test_fail_count
                    )
                } else {
                    "not run".to_string()
                },
                if ran && e.compile_success {
                    e.test_regressions.to_string()
                } else {
                    "-".to_string()
                },
                e.median_ns.map_or("n/a".to_string(), |m| format!("{m}ns")),
                e.fitness_delta
                    .map_or("n/a".to_string(), |d| format!("{:+.2}%", d * 100.0)),
                if e.passes_consensus_gate {
                    format!("**PASS** ({})", e.verdict)
                } else {
                    format!("FAIL ({})", e.verdict)
                },
            ));
        }
        if let Some(iso) = &self.isolation {
            out.push_str(&format!(
                "\n## Isolation\n\n- filesystem: {}\n- network: {}\n- toolchain: {}\n- registry: {}\n",
                iso.filesystem, iso.network, iso.toolchain, iso.registry
            ));
        }
        out
    }
}

/// Configuration of a quarantined multi-arm run.
#[derive(Debug, Clone)]
pub struct MultiArmConfig {
    pub consensus_threshold: usize,
    /// Measured benchmark runs per arm (after one unmeasured warm-up).
    pub replicates: usize,
    /// Benchmark command run in each snapshot (e.g. `cargo run --release
    /// --example bench`); `None` = no fitness is measured.
    pub bench_command: Option<Vec<String>>,
    pub max_repair_rounds: usize,
    /// Arms built/tested concurrently.
    pub parallel: usize,
    /// Wall-clock limit per step (check, build, test, one bench run).
    pub step_timeout: Duration,
    /// Extra arguments for `cargo check` / `cargo test` (e.g. features).
    pub cargo_args: Vec<String>,
    pub quarantine: QuarantineOptions,
    /// Keep the arm directories after the run (for inspection).
    pub keep_arms: bool,
}

impl Default for MultiArmConfig {
    fn default() -> Self {
        Self {
            consensus_threshold: 2,
            replicates: 3,
            bench_command: None,
            max_repair_rounds: 2,
            parallel: 2,
            step_timeout: Duration::from_secs(1800),
            cargo_args: Vec::new(),
            quarantine: QuarantineOptions::default(),
            keep_arms: false,
        }
    }
}

/// Why `arm` may not run at all, if it may not.
fn rejection(arm: &OptimizationArm) -> Option<String> {
    if arm.arm_id.is_empty()
        || !arm
            .arm_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(format!("arm id `{}` must be [A-Za-z0-9_-]+", arm.arm_id));
    }
    let path = Path::new(&arm.target_file);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path.extension().and_then(|e| e.to_str()) != Some("rs")
    {
        return Some(format!(
            "target `{}` is not a project-relative .rs path",
            arm.target_file
        ));
    }
    if crate::evolution::is_protected(path) {
        return Some(format!(
            "target `{}` is a protected path (evolution::PROTECTED_PATHS)",
            arm.target_file
        ));
    }
    None
}

/// Orchestrates multi-arm evaluation.
pub struct MultiArmEvolutionRunner {
    consensus_threshold: usize,
    scratch_root: PathBuf,
}

impl MultiArmEvolutionRunner {
    /// `scratch_root` holds the run directories (`sw-evolve-run-…`); it must
    /// not be inside the repository.
    pub fn new(consensus_threshold: usize, scratch_root: impl AsRef<Path>) -> Self {
        Self {
            consensus_threshold,
            scratch_root: scratch_root.as_ref().to_path_buf(),
        }
    }

    pub fn consensus_threshold(&self) -> usize {
        self.consensus_threshold
    }

    pub fn scratch_root(&self) -> &Path {
        &self.scratch_root
    }

    /// Score arms from measurements taken elsewhere. `measure` is called only
    /// for arms that pass the path and syntax checks; an `Err` from it is
    /// recorded on the arm (which then fails).
    pub fn evaluate_arms(
        &self,
        baseline: &ArmMeasurement,
        arms: &[OptimizationArm],
        mut measure: impl FnMut(&OptimizationArm) -> Result<ArmMeasurement>,
    ) -> MultiArmReport {
        let evaluations = arms
            .iter()
            .map(|arm| {
                let pre = precheck(arm);
                let measured = match &pre {
                    Precheck::Ok => Some(measure(arm).unwrap_or_else(|e| ArmMeasurement {
                        error: Some(e.to_string()),
                        ..Default::default()
                    })),
                    _ => None,
                };
                score_arm(arm, pre, measured, baseline)
            })
            .collect();
        self.assemble(baseline.clone(), evaluations, None, None)
    }

    fn assemble(
        &self,
        baseline: ArmMeasurement,
        evaluations: Vec<ArmEvaluation>,
        base_rev: Option<String>,
        isolation: Option<IsolationReport>,
    ) -> MultiArmReport {
        let passing_arms = evaluations
            .iter()
            .filter(|e| e.passes_consensus_gate)
            .count();
        // A threshold of 0 would promote with no passing arm (E4 needs k > 0).
        let consensus_met =
            self.consensus_threshold > 0 && passing_arms >= self.consensus_threshold;
        let winning_arm = if consensus_met {
            evaluations
                .iter()
                .filter(|e| e.passes_consensus_gate)
                .max_by(|a, b| {
                    let key = |e: &ArmEvaluation| e.fitness_delta.unwrap_or(f64::NEG_INFINITY);
                    key(a).total_cmp(&key(b))
                })
                .cloned()
        } else {
            None
        };
        let fitness_measured = !baseline.bench_samples_ns.is_empty()
            || evaluations.iter().any(|e| !e.bench_samples_ns.is_empty());
        MultiArmReport {
            consensus_threshold: self.consensus_threshold,
            total_arms: evaluations.len(),
            passing_arms,
            consensus_met,
            winning_arm,
            evaluations,
            baseline: BaselineSummary {
                median_ns: median_ns(&baseline.bench_samples_ns),
                measurement: baseline,
            },
            fitness_measured,
            base_rev,
            isolation,
        }
    }

    /// Run the baseline and every arm in quarantined snapshots of `rev` of
    /// `repo` and score them. See the module docs.
    pub async fn run_quarantined(
        &self,
        repo: &Path,
        rev: &str,
        arms: &[OptimizationArm],
        host: &HostToolchain,
        config: &MultiArmConfig,
    ) -> Result<MultiArmReport> {
        let mut ids = std::collections::HashSet::new();
        for arm in arms {
            if !ids.insert(arm.arm_id.as_str()) {
                bail!("duplicate arm id `{}`", arm.arm_id);
            }
        }
        std::fs::create_dir_all(&self.scratch_root)?;
        let run_root = self
            .scratch_root
            .join(format!("sw-evolve-run-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&run_root)?;
        let result = self.run_in(&run_root, repo, rev, arms, host, config).await;
        if !config.keep_arms {
            remove_tree(&run_root);
        }
        result
    }

    async fn run_in(
        &self,
        run_root: &Path,
        repo: &Path,
        rev: &str,
        arms: &[OptimizationArm],
        host: &HostToolchain,
        config: &MultiArmConfig,
    ) -> Result<MultiArmReport> {
        let toolchain = stage_toolchain(host, run_root)?;
        let bench_lock = Arc::new(tokio::sync::Mutex::new(()));

        let base_q = ArmQuarantine::prepare(
            &run_root.join("sw-evolve-arm-baseline"),
            repo,
            rev,
            host,
            &toolchain,
            &config.quarantine,
        )
        .context("preparing the baseline quarantine")?;
        let baseline = measure_in(&base_q, None, config, &bench_lock).await;
        let isolation = base_q.isolation.clone();
        let base_target = base_q.target_dir.clone();

        let ctx = ArmCtx {
            run_root: run_root.to_path_buf(),
            repo: repo.to_path_buf(),
            rev: rev.to_string(),
            host: host.clone(),
            toolchain,
            config: config.clone(),
            bench_lock,
            base_target,
        };
        let sem = Arc::new(tokio::sync::Semaphore::new(config.parallel.max(1)));
        let futs = arms.iter().map(|arm| {
            let ctx = &ctx;
            let sem = sem.clone();
            let baseline = &baseline;
            async move {
                let pre = precheck(arm);
                let measured = match &pre {
                    Precheck::Ok => {
                        let _permit = sem.acquire().await.expect("semaphore open");
                        Some(ctx.measure_arm(arm).await)
                    }
                    _ => None,
                };
                score_arm(arm, pre, measured, baseline)
            }
        });
        let evaluations = futures::future::join_all(futs).await;
        Ok(self.assemble(
            baseline,
            evaluations,
            Some(rev.to_string()),
            Some(isolation),
        ))
    }
}

struct ArmCtx {
    run_root: PathBuf,
    repo: PathBuf,
    rev: String,
    host: HostToolchain,
    toolchain: StagedToolchain,
    config: MultiArmConfig,
    bench_lock: Arc<tokio::sync::Mutex<()>>,
    base_target: PathBuf,
}

impl ArmCtx {
    async fn measure_arm(&self, arm: &OptimizationArm) -> ArmMeasurement {
        let q = match ArmQuarantine::prepare(
            &self.run_root.join(format!("sw-evolve-arm-{}", arm.arm_id)),
            &self.repo,
            &self.rev,
            &self.host,
            &self.toolchain,
            &self.config.quarantine,
        ) {
            Ok(q) => q,
            Err(e) => {
                return ArmMeasurement {
                    error: Some(format!("preparing the quarantine: {e:#}")),
                    ..Default::default()
                }
            }
        };
        // Start from the baseline's build products (trusted code) where the
        // filesystem can clone them: incremental instead of cold builds.
        let _ = clone_tree(&self.base_target, &q.target_dir, &self.host.parent_env);
        measure_in(&q, Some(arm), &self.config, &self.bench_lock).await
    }
}

enum Precheck {
    Ok,
    Rejected(String),
    Syntax(AstSyntaxError),
}

fn precheck(arm: &OptimizationArm) -> Precheck {
    if let Some(reason) = rejection(arm) {
        return Precheck::Rejected(reason);
    }
    match validate_rust_syntax(&arm.proposed_source) {
        Ok(()) => Precheck::Ok,
        Err(e) => Precheck::Syntax(e),
    }
}

fn score_arm(
    arm: &OptimizationArm,
    pre: Precheck,
    measured: Option<ArmMeasurement>,
    baseline: &ArmMeasurement,
) -> ArmEvaluation {
    let mut eval = ArmEvaluation {
        arm_id: arm.arm_id.clone(),
        name: arm.name.clone(),
        syntax_valid: !matches!(pre, Precheck::Syntax(_)),
        syntax_error: None,
        rejected: None,
        compile_success: false,
        test_pass_count: 0,
        test_fail_count: 0,
        test_regressions: 0,
        bench_samples_ns: Vec::new(),
        median_ns: None,
        fitness_delta: None,
        separated_from_baseline: false,
        repair_rounds: 0,
        repair_fixes: 0,
        passes_consensus_gate: false,
        verdict: String::new(),
        error: None,
        diff: None,
    };
    match pre {
        Precheck::Rejected(reason) => {
            eval.verdict = format!("rejected: {reason}");
            eval.rejected = Some(reason);
            return eval;
        }
        Precheck::Syntax(err) => {
            eval.verdict = format!("syntax error: {err}");
            eval.syntax_error = Some(err);
            return eval;
        }
        Precheck::Ok => {}
    }
    let m = measured.unwrap_or_default();
    eval.compile_success = m.compiled;
    eval.test_pass_count = m.tests_passed;
    eval.test_fail_count = m.tests_failed;
    eval.repair_rounds = m.repair_rounds;
    eval.repair_fixes = m.repair_fixes;
    eval.error = m.error.clone();
    eval.diff = m.diff.clone();
    eval.bench_samples_ns = m.bench_samples_ns.clone();
    eval.median_ns = median_ns(&m.bench_samples_ns);

    if !m.compiled {
        eval.verdict = match &m.error {
            Some(e) => format!("did not compile: {e}"),
            None => "did not compile".into(),
        };
        return eval;
    }
    eval.test_regressions = m.tests_failed.saturating_sub(baseline.tests_failed)
        + baseline.tests_passed.saturating_sub(m.tests_passed);

    let base_median = median_ns(&baseline.bench_samples_ns);
    if let (Some(b), Some(a)) = (base_median, eval.median_ns) {
        if b > 0 {
            eval.fitness_delta = Some((b as f64 - a as f64) / b as f64);
        }
        let arm_max = m.bench_samples_ns.iter().max().copied().unwrap_or(u64::MAX);
        let base_min = baseline.bench_samples_ns.iter().min().copied().unwrap_or(0);
        eval.separated_from_baseline = arm_max < base_min;
    }

    let delta_positive = eval.fitness_delta.is_some_and(|d| d > 0.0);
    eval.passes_consensus_gate =
        eval.test_regressions == 0 && delta_positive && eval.separated_from_baseline;
    eval.verdict = if eval.test_regressions > 0 {
        format!(
            "{} test regression(s) against the baseline",
            eval.test_regressions
        )
    } else if let Some(e) = &m.error {
        format!("measurement incomplete: {e}")
    } else if eval.fitness_delta.is_none() {
        "fitness not measured (no benchmark samples for the arm or the baseline)".into()
    } else if !delta_positive {
        "not faster than the baseline".into()
    } else if !eval.separated_from_baseline {
        "faster median, but samples overlap the baseline's (within noise)".into()
    } else {
        "faster than every baseline sample, no regressions".into()
    };
    eval
}

fn cargo_args<'a>(base: &[&'a str], extra: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = base.to_vec();
    args.extend(extra.iter().map(String::as_str));
    args
}

fn describe_failure(step: &str, out: &QuarantinedOutput) -> String {
    if out.timed_out {
        return format!("{step} timed out");
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: String = stderr
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    format!("{step} failed (exit {:?}): {tail}", out.exit_code)
}

/// Build, test and benchmark in `q`; with `arm`, write its source first and
/// run the repair loop.
async fn measure_in(
    q: &ArmQuarantine,
    arm: Option<&OptimizationArm>,
    config: &MultiArmConfig,
    bench_lock: &tokio::sync::Mutex<()>,
) -> ArmMeasurement {
    let mut m = ArmMeasurement::default();
    let timeout = config.step_timeout;

    if let Some(arm) = arm {
        let target = q.worktree.join(&arm.target_file);
        let before = std::fs::read_to_string(&target).unwrap_or_default();
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&target, &arm.proposed_source) {
            m.error = Some(format!("writing {}: {e}", arm.target_file));
            return m;
        }
        let engine = DiagnosticRepairEngine::new();
        let check_args = cargo_args(
            &["check", "--all-targets", "--message-format=json"],
            &config.cargo_args,
        );
        let repaired = engine
            .run_repair_loop(&q.worktree, config.max_repair_rounds, || async {
                let out = q
                    .run("cargo", &check_args, timeout, MAX_OUTPUT_BYTES)
                    .await?;
                if out.timed_out {
                    bail!("cargo check timed out");
                }
                Ok(report_from_cargo_output(
                    AnalysisKind::Check,
                    check_args.iter().map(|a| a.to_string()).collect(),
                    out.success,
                    out.exit_code,
                    out.wall.as_millis() as u64,
                    &out.stdout,
                    &out.stderr,
                ))
            })
            .await;
        match repaired {
            Ok(outcome) => {
                m.repair_rounds = outcome.rounds;
                m.repair_fixes = outcome.fixes_applied;
                if outcome.stop != RepairStop::Compiles {
                    m.error = Some(format!(
                        "cargo check: {} error(s) after {} repair round(s) ({:?})",
                        outcome.final_report.errors, outcome.rounds, outcome.stop
                    ));
                }
            }
            Err(e) => {
                m.error = Some(format!("{e:#}"));
            }
        }
        let after = std::fs::read_to_string(&target).unwrap_or_default();
        m.diff = Some(
            similar::TextDiff::from_lines(&before, &after)
                .unified_diff()
                .header(
                    &format!("a/{}", arm.target_file),
                    &format!("b/{}", arm.target_file),
                )
                .to_string(),
        );
        if m.error.is_some() {
            return m;
        }
    }

    let build_args = cargo_args(&["test", "--no-run"], &config.cargo_args);
    match q.run("cargo", &build_args, timeout, MAX_OUTPUT_BYTES).await {
        Ok(out) if out.success => m.compiled = true,
        Ok(out) => {
            m.error = Some(describe_failure("cargo test --no-run", &out));
            return m;
        }
        Err(e) => {
            m.error = Some(format!("{e:#}"));
            return m;
        }
    }

    let test_args = cargo_args(&["test"], &config.cargo_args);
    match q.run("cargo", &test_args, timeout, MAX_OUTPUT_BYTES).await {
        Ok(out) if out.timed_out => {
            m.error = Some("cargo test timed out".into());
            return m;
        }
        Ok(out) => {
            let (passed, total) =
                crate::evolution::daemon::parse_test_summary(&String::from_utf8_lossy(&out.stdout));
            m.tests_passed = passed;
            m.tests_failed = total - passed;
            if !out.success && m.tests_failed == 0 {
                m.error = Some(describe_failure("cargo test", &out));
                return m;
            }
        }
        Err(e) => {
            m.error = Some(format!("{e:#}"));
            return m;
        }
    }

    if let Some(bench) = &config.bench_command {
        let Some((program, rest)) = bench.split_first() else {
            return m;
        };
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        let _guard = bench_lock.lock().await;
        // One unmeasured warm-up run (builds the bench, warms caches).
        for i in 0..=config.replicates {
            match q.run(program, &rest, timeout, MAX_OUTPUT_BYTES).await {
                Ok(out) if out.success => {
                    if i > 0 {
                        m.bench_samples_ns
                            .push(u64::try_from(out.wall.as_nanos()).unwrap_or(u64::MAX));
                    }
                }
                Ok(out) => {
                    m.bench_samples_ns.clear();
                    m.error = Some(describe_failure("benchmark", &out));
                    break;
                }
                Err(e) => {
                    m.bench_samples_ns.clear();
                    m.error = Some(format!("{e:#}"));
                    break;
                }
            }
        }
    }
    m
}

fn remove_tree(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_dir() {
                let _ =
                    std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o755));
            }
        }
    }
    let _ = std::fs::remove_dir_all(path);
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/multi_arm_runner_test.rs"]
mod tests;
