use crate::cognitive::compilation_manager::CompilationSandbox;
use crate::cognitive::meta_learning::MetaLearner;
use crate::cognitive::metrics::MetricsStore;
use crate::cognitive::self_edit::{
    AppliedMutation, ImprovementCategory, ImprovementRecord, ImprovementSource, ImprovementTarget,
    SelfEditOrchestrator,
};
use crate::errors::{Result, SelfwareError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

/// Serializable snapshot of RSI loop state for persistence across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RSIState {
    /// Total iterations completed so far (across restarts).
    pub total_iterations: usize,
    /// Consecutive failures at the time of save.
    pub consecutive_failures: usize,
    /// Max iterations limit.
    pub max_iterations: usize,
    /// Circuit-breaker threshold.
    pub max_consecutive_failures: usize,
}

/// The outer loop for Recursive Self-Improvement
pub struct RSIOrchestrator {
    edit_orchestrator: SelfEditOrchestrator,
    meta_learner: MetaLearner,
    _metrics: MetricsStore,
    project_root: PathBuf,
    is_running: bool,
    /// Hard upper bound on the number of improvement iterations before the loop terminates.
    max_iterations: usize,
    /// Total iterations completed (persisted across restarts).
    total_iterations: usize,
    /// Tracks how many improvement cycles have failed in a row without a single success.
    consecutive_failures: usize,
    /// Circuit-breaker threshold: if this many consecutive failures occur, the loop aborts.
    max_consecutive_failures: usize,
    /// Per-run ceiling on improvement iterations. Each iteration runs TWO paid
    /// e2e benchmark suites (baseline + sandbox), so an unbounded run burns
    /// unbounded money — the old default allowed 100 iterations (200 suites)
    /// per run. Override with `SELFWARE_RSI_MAX_ITERATIONS_PER_RUN`.
    max_iterations_per_run: usize,
    /// Path to the persisted RSI state file.
    state_path: PathBuf,
}

/// Default per-run iteration ceiling (10 iterations = 20 paid e2e suites).
const DEFAULT_MAX_ITERATIONS_PER_RUN: usize = 10;

/// Env var overriding the per-run iteration ceiling.
const MAX_ITERATIONS_PER_RUN_ENV: &str = "SELFWARE_RSI_MAX_ITERATIONS_PER_RUN";

/// Read the per-run iteration ceiling from the environment, falling back to
/// the default when unset or unparseable.
fn max_iterations_per_run_from_env() -> usize {
    parse_max_iterations_per_run(std::env::var(MAX_ITERATIONS_PER_RUN_ENV).ok().as_deref())
}

/// Pure parsing core of [`max_iterations_per_run_from_env`]: a positive
/// integer wins; anything else falls back to the default. Kept separate so it
/// is deterministically unit-testable without touching process environment.
fn parse_max_iterations_per_run(value: Option<&str>) -> usize {
    value
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_MAX_ITERATIONS_PER_RUN)
}

impl RSIOrchestrator {
    pub fn new(project_root: PathBuf) -> Self {
        let state_path = Self::default_state_path(&project_root);
        let mut orch = Self {
            edit_orchestrator: SelfEditOrchestrator::new(project_root.clone()),
            meta_learner: MetaLearner::new(),
            _metrics: MetricsStore::new(),
            project_root,
            is_running: false,
            max_iterations: 100,
            total_iterations: 0,
            consecutive_failures: 0,
            max_consecutive_failures: 5,
            max_iterations_per_run: max_iterations_per_run_from_env(),
            state_path,
        };
        // Restore previous state if available.
        if let Ok(state) = orch.load_state() {
            info!(
                "Restored RSI state: {} iterations completed, {} consecutive failures",
                state.total_iterations, state.consecutive_failures
            );
            orch.total_iterations = state.total_iterations;
            orch.consecutive_failures = state.consecutive_failures;
        }
        orch
    }

    fn default_state_path(project_root: &Path) -> PathBuf {
        project_root.join(".selfware").join("rsi_state.json")
    }

    /// Override the per-run iteration ceiling (see
    /// `DEFAULT_MAX_ITERATIONS_PER_RUN`). Each iteration runs two paid e2e
    /// benchmark suites, so this is effectively the run's cost ceiling.
    pub fn with_max_iterations_per_run(mut self, max: usize) -> Self {
        self.max_iterations_per_run = max.max(1);
        self
    }

    /// Save the current loop state to disk so it can be resumed.
    ///
    /// The write is atomic: the JSON goes to a temp file in the same
    /// directory, is fsynced, then renamed over the target — a crash
    /// mid-write can never leave a truncated `rsi_state.json` (same pattern
    /// as `session::chat_store`).
    pub fn save_state(&self) -> std::result::Result<(), std::io::Error> {
        use std::io::Write;

        let state = RSIState {
            total_iterations: self.total_iterations,
            consecutive_failures: self.consecutive_failures,
            max_iterations: self.max_iterations,
            max_consecutive_failures: self.max_consecutive_failures,
        };
        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&state).map_err(std::io::Error::other)?;

        let tmp_path = self
            .state_path
            .with_extension(format!("json.tmp.{}", std::process::id()));
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
        }
        if let Err(err) = std::fs::rename(&tmp_path, &self.state_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }
        Ok(())
    }

    /// Load previously persisted state.
    fn load_state(&self) -> std::result::Result<RSIState, std::io::Error> {
        let data = std::fs::read_to_string(&self.state_path)?;
        serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Record one failed (or non-improving) cycle: bump `consecutive_failures`,
    /// warn when the next failure would trip the breaker, and when the
    /// threshold is reached persist state and return the circuit-breaker
    /// error that aborts the loop. Non-improving cycles count too — each one
    /// burned two paid e2e suites without progress, so an unbroken string of
    /// them must abort the loop exactly like unbroken errors. Only a genuine
    /// improvement (`Ok(true)` in `run_loop`) resets the counter.
    fn record_cycle_failure(&mut self, iteration: usize) -> Result<()> {
        self.consecutive_failures += 1;

        if self.consecutive_failures >= self.max_consecutive_failures {
            error!(
                "Circuit breaker tripped: {} consecutive failures reached the limit of {}. \
                 Aborting RSI loop to prevent runaway damage.",
                self.consecutive_failures, self.max_consecutive_failures
            );
            // Persist state before aborting so it survives the restart.
            self.total_iterations += iteration;
            if let Err(save_err) = self.save_state() {
                warn!(
                    "Failed to save RSI state on circuit-breaker abort: {}",
                    save_err
                );
            }
            return Err(SelfwareError::Internal(format!(
                "RSI loop aborted: {} consecutive failures (limit: {})",
                self.consecutive_failures, self.max_consecutive_failures
            )));
        }

        if self.consecutive_failures >= self.max_consecutive_failures - 1 {
            warn!(
                "Next failure will trip the circuit breaker ({}/{} consecutive failures)",
                self.consecutive_failures, self.max_consecutive_failures
            );
        }
        Ok(())
    }

    /// Run the RSI outer loop with safety guardrails.
    ///
    /// The loop will terminate if any of the following conditions are met:
    /// - `max_iterations` cycles have been executed (lifetime, persisted).
    /// - `max_iterations_per_run` cycles have been executed in THIS invocation
    ///   (per-run cost ceiling — each iteration runs two paid e2e suites).
    /// - `max_consecutive_failures` failures occur in a row (circuit breaker).
    /// - `stop()` is called externally.
    pub async fn run_loop(&mut self) -> Result<()> {
        self.is_running = true;
        // Don't reset consecutive_failures — the restored value from disk
        // carries over so the circuit breaker state survives restarts.
        let mut iteration: usize = 0;

        info!(
            "Starting outer RSI loop (max_iterations={}, total_completed={}, max_consecutive_failures={}, max_iterations_per_run={})...",
            self.max_iterations, self.total_iterations, self.max_consecutive_failures, self.max_iterations_per_run
        );

        while self.is_running
            && (self.total_iterations + iteration) < self.max_iterations
            && iteration < self.max_iterations_per_run
        {
            iteration += 1;
            let global_iter = self.total_iterations + iteration;
            info!("RSI iteration {}/{}", global_iter, self.max_iterations);

            // Warn when approaching the iteration limit
            let remaining = self.max_iterations - global_iter;
            if remaining <= 10 && remaining > 0 {
                warn!(
                    "Approaching iteration limit: {} iterations remaining",
                    remaining
                );
            }

            match self.execute_improvement_cycle().await {
                Ok(true) => {
                    info!("Improvement cycle successful and merged.");
                    self.consecutive_failures = 0;
                }
                Ok(false) => {
                    info!("Improvement cycle did not yield a better fitness score. Changes discarded.");
                    // Non-improving cycles count toward the circuit breaker
                    // (see record_cycle_failure); a genuine improvement is
                    // the only thing that resets it.
                    self.record_cycle_failure(iteration)?;
                }
                Err(e) => {
                    error!("Improvement cycle failed: {}", e);
                    self.record_cycle_failure(iteration)?;

                    // Exponential backoff: 60s * 2^(failures-1), capped at 3600s
                    let backoff_secs = std::cmp::min(
                        60u64.saturating_mul(1u64 << (self.consecutive_failures - 1)),
                        3600,
                    );
                    warn!(
                        "Backing off for {} seconds before next attempt",
                        backoff_secs
                    );
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    continue;
                }
            }

            // Normal inter-cycle sleep (only on non-error paths; errors use backoff above)
            tokio::time::sleep(Duration::from_secs(60)).await;
        }

        self.total_iterations += iteration;

        if self.total_iterations >= self.max_iterations {
            warn!(
                "RSI loop terminated: reached maximum iteration limit of {}",
                self.max_iterations
            );
        } else if iteration >= self.max_iterations_per_run && self.is_running {
            // Honest cost-ceiling stop: each iteration runs TWO paid e2e
            // benchmark suites (baseline + sandbox), so this caps the spend
            // of a single `run_loop` invocation.
            warn!(
                "RSI run stopped: per-run cost ceiling reached ({} iterations = {} paid e2e suites this run). \
                 Rerun to continue from persisted state, or raise the ceiling via {}.",
                iteration,
                iteration.saturating_mul(2),
                MAX_ITERATIONS_PER_RUN_ENV
            );
        }

        // Persist state on clean exit so it survives process restarts.
        if let Err(e) = self.save_state() {
            warn!("Failed to save RSI state on exit: {}", e);
        }

        Ok(())
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        // Save state when explicitly stopped (e.g. Ctrl+C handler).
        if let Err(e) = self.save_state() {
            warn!("Failed to save RSI state on stop: {}", e);
        }
    }

    /// Executes a single plan -> act -> verify -> reflect cycle
    async fn execute_improvement_cycle(&mut self) -> Result<bool> {
        info!("Beginning new improvement cycle");

        // NOTE on cost: each cycle that reaches fitness evaluation runs TWO
        // paid e2e benchmark suites (baseline + sandbox). Cheap local gates
        // (target selection, trivial-mutation detection, compilation/tests)
        // therefore run FIRST so mutations that cannot matter never burn a
        // paid suite.

        // 1. Consult meta-learner for strategy priorities
        let strategy_rankings = self.meta_learner.analyze_strategies();
        if !strategy_rankings.is_empty() {
            info!(
                "Meta-learner strategy rankings (top 3): {:?}",
                strategy_rankings.iter().take(3).collect::<Vec<_>>()
            );
        }

        // 2. Identify Target (Introspect)
        let mut targets = self.edit_orchestrator.analyze_self();
        if targets.is_empty() {
            info!("No improvement targets found in this cycle.");
            return Ok(false);
        }

        // Re-weight target priorities using meta-learned category weights
        for target in &mut targets {
            target.priority = self
                .meta_learner
                .weight_priority(&target.category, target.priority);
        }
        // Re-sort by weighted priority (highest first)
        targets.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Pick highest priority target that has a concrete mutation strategy.
        let Some(target) = self.edit_orchestrator.select_target(&targets).cloned() else {
            info!("No supported improvement targets found in this cycle.");
            return Ok(false);
        };
        info!("Selected improvement target: {:?}", target);

        // 3. Create Sandbox
        let sandbox = self.edit_orchestrator.create_sandbox()?;

        // 4. Apply Mutation
        info!("Applying mutation to sandbox...");
        let applied = self
            .edit_orchestrator
            .apply_target_in_sandbox(&target, &sandbox)?;
        info!("Applied mutation: {}", applied.summary);

        // 4b. Trivial-mutation gate (free): if the diff only rewrites comment
        // or documentation lines, the mutation cannot change benchmark
        // behaviour — skip the paid evaluation entirely instead of burning
        // two e2e suites to confirm a no-op.
        if mutation_is_trivial(
            &self.project_root,
            sandbox.work_dir(),
            &applied.edited_files,
        ) {
            info!(
                "Mutation for '{}' only touches comment/doc lines — skipping paid e2e evaluation.",
                target.description
            );
            self.record_improvement(&target, None, 0.0, false, true)
                .await?;
            sandbox.cleanup()?;
            return Ok(false);
        }

        // 5. Verify compilation and tests in sandbox (local, no paid suites)
        info!("Verifying compilation in sandbox...");
        if !sandbox.verify()? {
            warn!("Compilation or tests failed in sandbox. Rejecting mutation.");
            // Baseline was not measured yet (see note above) — record a 0.0
            // baseline; the record's `verified=false` marks this as rejected
            // before evaluation, so the exact baseline is not meaningful.
            self.record_improvement(&target, None, 0.0, false, true)
                .await?;
            sandbox.cleanup()?;
            return Ok(false);
        }

        // 6. Measure Baseline Fitness (PAID suite #1) — deferred until the
        // mutation is known to be non-trivial and compiling.
        let baseline_report = self.run_benchmark_report(&self.project_root).await?;
        let baseline_score = baseline_report.average_score;
        debug!("Baseline fitness score: {}", baseline_score);

        // 7. Measure New Fitness in Sandbox (PAID suite #2)
        let new_report = self.run_benchmark_report(sandbox.work_dir()).await?;
        let new_score = new_report.average_score;
        debug!("New fitness score: {}", new_score);

        // 7a. DarwinX Non-Regression Invariant Gate:
        // Passed(baseline) ∩ Failed(candidate) = ∅
        if let Err(regressed) = baseline_report.check_darwinx_non_regression(&new_report) {
            warn!(
                "DarwinX Non-Regression violation: candidate broke previously passing scenario(s): {:?}. Rejecting mutation.",
                regressed
            );
            self.record_improvement(&target, Some(new_score), baseline_score, true, true)
                .await?;
            sandbox.cleanup()?;
            return Ok(false);
        }

        // 7c. Evaluate
        if new_score > baseline_score {
            info!(
                "Mutation improved fitness ({} > {}). Merging.",
                new_score, baseline_score
            );
            self.merge_sandbox(sandbox, &applied).await?;

            // Record success
            self.record_improvement(&target, Some(new_score), baseline_score, true, false)
                .await?;
            Ok(true)
        } else {
            info!(
                "Mutation degraded or did not improve fitness ({} <= {}). Rolling back.",
                new_score, baseline_score
            );
            self.record_improvement(&target, Some(new_score), baseline_score, true, true)
                .await?;
            sandbox.cleanup()?;
            Ok(false)
        }
    }

    async fn run_benchmark_report(&self, work_dir: &std::path::Path) -> Result<BenchmarkReport> {
        info!("Running E2E benchmark suite in {:?}", work_dir);
        let script_path = work_dir.join("system_tests/projecte2e/run_projecte2e.sh");

        let latest_link = work_dir.join("system_tests/projecte2e/reports/latest");
        if latest_link.exists() || latest_link.is_symlink() {
            let _ = std::fs::remove_file(&latest_link);
        }

        let output = Command::new("bash")
            .arg(&script_path)
            .current_dir(work_dir)
            .output()
            .await
            .map_err(|e| {
                SelfwareError::Internal(format!("Failed to run benchmark script: {}", e))
            })?;

        if !output.status.success() {
            warn!(
                "Benchmark script returned non-zero exit code: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let reports_dir = work_dir.join("system_tests/projecte2e/reports/latest");
        let results_tsv = reports_dir.join("results.tsv");

        if !results_tsv.exists() {
            return Err(SelfwareError::Internal(
                "Benchmark results.tsv not found".to_string(),
            ));
        }

        let tsv_content = std::fs::read_to_string(&results_tsv)
            .map_err(|e| SelfwareError::Internal(format!("Failed to read results.tsv: {}", e)))?;

        Ok(parse_benchmark_report(&tsv_content))
    }

    async fn merge_sandbox(
        &self,
        sandbox: CompilationSandbox,
        applied: &AppliedMutation,
    ) -> Result<()> {
        info!("Merging sandbox changes back to main workspace...");

        let canonical_project_root = self.project_root.canonicalize().map_err(|e| {
            SelfwareError::Internal(format!("Failed to canonicalize project root: {}", e))
        })?;
        let canonical_sandbox_root = sandbox.work_dir().canonicalize().map_err(|e| {
            SelfwareError::Internal(format!("Failed to canonicalize sandbox root: {}", e))
        })?;

        for rel_path in &applied.edited_files {
            let rel = Path::new(rel_path);
            if rel.is_absolute()
                || rel
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(SelfwareError::Internal(format!(
                    "Path traversal detected or absolute path forbidden in merge candidate: {}",
                    rel_path
                )));
            }

            let dummy_target = ImprovementTarget::new(
                ImprovementCategory::CodeQuality,
                "merge_check",
                "merge_check",
                ImprovementSource::TechDebt,
            )
            .with_file(rel_path.clone());
            if self.edit_orchestrator.is_denied(&dummy_target) {
                return Err(SelfwareError::Internal(format!(
                    "Cannot merge denied file: {}",
                    rel_path
                )));
            }

            let source = sandbox.work_dir().join(rel);
            let canonical_source = source.canonicalize().map_err(|e| {
                SelfwareError::Internal(format!(
                    "Failed to canonicalize sandbox source {}: {}",
                    rel_path, e
                ))
            })?;
            if !canonical_source.starts_with(&canonical_sandbox_root) {
                return Err(SelfwareError::Internal(format!(
                    "Sandbox source {} escapes sandbox directory",
                    rel_path
                )));
            }

            let destination = self.project_root.join(rel);
            if let Ok(meta) = destination.symlink_metadata() {
                if meta.file_type().is_symlink() {
                    return Err(SelfwareError::Internal(format!(
                        "Merge destination {} is a symlink",
                        rel_path
                    )));
                }
            }
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    SelfwareError::Internal(format!("Failed to create merge dir: {}", e))
                })?;
            }
            let canonical_dest_parent = destination
                .parent()
                .unwrap_or(&self.project_root)
                .canonicalize()
                .map_err(|e| {
                    SelfwareError::Internal(format!(
                        "Failed to canonicalize destination parent {}: {}",
                        rel_path, e
                    ))
                })?;
            if !canonical_dest_parent.starts_with(&canonical_project_root) {
                return Err(SelfwareError::Internal(format!(
                    "Merge destination {} escapes project root",
                    rel_path
                )));
            }

            tokio::fs::copy(&canonical_source, &destination)
                .await
                .map_err(|e| {
                    SelfwareError::Internal(format!(
                        "Failed to merge sandbox file {}: {}",
                        rel_path, e
                    ))
                })?;
        }

        sandbox.cleanup()?;
        info!("Successfully merged sandbox changes");
        Ok(())
    }

    async fn record_improvement(
        &mut self,
        target: &ImprovementTarget,
        new_score: Option<f64>,
        baseline_score: f64,
        verified: bool,
        rolled_back: bool,
    ) -> Result<()> {
        let effectiveness_score = new_score.map_or(-1.0, |score| score - baseline_score);
        let record = ImprovementRecord {
            target_id: target.id.clone(),
            category: target.category.clone(),
            description: target.description.clone(),
            before_metrics: None,
            after_metrics: None,
            git_commits: Vec::new(),
            verified,
            rolled_back,
            effectiveness_score,
            completed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        // Update meta-learner weights based on the outcome so future cycles
        // can prioritise categories that historically succeed.
        self.meta_learner.update_weights(&record);

        self.edit_orchestrator.record_result(record)?;
        Ok(())
    }

    #[cfg(test)]
    fn with_paths(project_root: PathBuf, state_path: PathBuf, history_path: PathBuf) -> Self {
        Self {
            edit_orchestrator: SelfEditOrchestrator::with_history_path(
                project_root.clone(),
                history_path,
            ),
            meta_learner: MetaLearner::new(),
            _metrics: MetricsStore::with_path(project_root.join(".selfware/test-metrics.jsonl")),
            project_root,
            is_running: false,
            max_iterations: 100,
            total_iterations: 0,
            consecutive_failures: 0,
            max_consecutive_failures: 5,
            max_iterations_per_run: DEFAULT_MAX_ITERATIONS_PER_RUN,
            state_path,
        }
    }
}

/// Scenario outcome parsed from benchmark reports.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioOutcome {
    pub name: String,
    pub score: f64,
    pub passed: bool,
}

/// Structured outcome of a full benchmark evaluation run.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkReport {
    pub average_score: f64,
    pub scenarios: std::collections::HashMap<String, ScenarioOutcome>,
}

impl BenchmarkReport {
    /// DarwinX non-regression invariant:
    /// Passed(baseline) ∩ Failed(candidate) = ∅
    ///
    /// Every scenario that passed in the baseline MUST also pass in the candidate,
    /// and the suite of scenario names must match.
    pub fn check_darwinx_non_regression(
        &self,
        candidate: &BenchmarkReport,
    ) -> std::result::Result<(), Vec<String>> {
        let baseline_names: std::collections::HashSet<&str> =
            self.scenarios.keys().map(|s| s.as_str()).collect();
        let candidate_names: std::collections::HashSet<&str> =
            candidate.scenarios.keys().map(|s| s.as_str()).collect();

        if baseline_names != candidate_names {
            let missing: Vec<String> = baseline_names
                .difference(&candidate_names)
                .map(|s| s.to_string())
                .collect();
            let unexpected: Vec<String> = candidate_names
                .difference(&baseline_names)
                .map(|s| s.to_string())
                .collect();
            let mut errors = Vec::new();
            if !missing.is_empty() {
                errors.push(format!("Missing scenarios in candidate: {:?}", missing));
            }
            if !unexpected.is_empty() {
                errors.push(format!(
                    "Unexpected scenarios in candidate: {:?}",
                    unexpected
                ));
            }
            return Err(errors);
        }

        let mut regressed = Vec::new();
        for (name, baseline_sc) in &self.scenarios {
            if baseline_sc.passed {
                if let Some(cand_sc) = candidate.scenarios.get(name) {
                    if !cand_sc.passed {
                        regressed.push(name.clone());
                    }
                } else {
                    regressed.push(name.clone());
                }
            }
        }
        if regressed.is_empty() {
            Ok(())
        } else {
            Err(regressed)
        }
    }
}

/// Parse TSV benchmark report into a structured `BenchmarkReport`.
pub fn parse_benchmark_report(tsv_content: &str) -> BenchmarkReport {
    let mut total_score = 0.0;
    let mut count = 0;
    let mut scenarios = std::collections::HashMap::new();

    for (i, line) in tsv_content.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('|').collect();
        if parts.len() > 8 {
            let name = parts[0].trim().to_string();
            // Reject duplicate scenario names at parse time
            if scenarios.contains_key(&name) {
                tracing::warn!(
                    "Duplicate scenario '{}' in benchmark TSV, skipping duplicate",
                    name
                );
                continue;
            }
            if let Ok(score) = parts[8].trim().parse::<f64>() {
                let scenario_type = parts.get(1).map(|s| s.trim()).unwrap_or("");
                let post_status = parts.get(4).map(|s| s.trim()).unwrap_or("");
                let agent_status = parts.get(5).map(|s| s.trim()).unwrap_or("");
                let has_error = parts.get(10).is_some_and(|err| {
                    let e = err.trim();
                    if let Ok(hits) = e.parse::<u64>() {
                        hits > 0
                    } else {
                        !e.is_empty() && e != "0"
                    }
                });

                let passed = if scenario_type == "coding" {
                    post_status == "0" && score >= 70.0
                } else {
                    agent_status == "0" && !has_error && score >= 70.0
                };

                scenarios.insert(
                    name.clone(),
                    ScenarioOutcome {
                        name,
                        score,
                        passed,
                    },
                );
                total_score += score;
                count += 1;
            }
        }
    }

    let average_score = if count == 0 {
        0.0
    } else {
        total_score / count as f64
    };

    BenchmarkReport {
        average_score,
        scenarios,
    }
}

/// Whether the mutation applied to the sandbox only rewrites comment or
/// documentation lines. Such a mutation cannot change benchmark behaviour, so
/// the paid e2e evaluation is skipped for it.
///
/// Heuristic (deliberately conservative): compare each edited file's content
/// in the project root vs the sandbox after dropping blank lines and lines
/// that consist ENTIRELY of a comment/doc marker (`//…`, `///…`, `//!…`,
/// `/*…`, `*…`, `--…`, plus `#…` only in file types where `#` is a comment —
/// in Rust, `#` starts an attribute and counts as code). Inline trailing
/// comments are NOT stripped, so a line mixing code and comment still counts
/// as code — when in doubt we evaluate (false "non-trivial" only costs one
/// cycle's evaluation; a false "trivial" would silently skip a real change).
fn mutation_is_trivial(project_root: &Path, sandbox_dir: &Path, edited_files: &[String]) -> bool {
    if edited_files.is_empty() {
        return true;
    }
    edited_files.iter().all(|rel| {
        let old = std::fs::read_to_string(project_root.join(rel)).unwrap_or_default();
        let new = std::fs::read_to_string(sandbox_dir.join(rel)).unwrap_or_default();
        let ext = Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let strip_hash = matches!(
            ext,
            "toml" | "sh" | "bash" | "yaml" | "yml" | "py" | "md" | "cfg" | "ini" | "txt"
        );
        // In Rust, `*` can be a dereference operation (`*ptr = ...`) at the
        // start of a line — only strip leading asterisk in non-Rust files.
        let strip_asterisk = ext != "rs";
        code_lines(&old, strip_hash, strip_asterisk) == code_lines(&new, strip_hash, strip_asterisk)
    })
}

/// The content lines of `content` with blank lines and whole-line comments
/// removed — see [`mutation_is_trivial`] for the exact stripping rules.
fn code_lines(content: &str, strip_hash: bool, strip_asterisk: bool) -> Vec<&str> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| {
            !(line.is_empty()
                || line.starts_with("//")
                || (strip_hash && line.starts_with('#'))
                || line.starts_with("/*")
                || (strip_asterisk && line.starts_with('*'))
                || line.starts_with("--"))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/rsi_orchestrator/rsi_orchestrator_test.rs"]
mod tests;
