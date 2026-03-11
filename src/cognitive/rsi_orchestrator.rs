use crate::cognitive::compilation_manager::CompilationSandbox;
use crate::cognitive::metrics::MetricsStore;
use crate::cognitive::self_edit::{
    AppliedMutation, ImprovementRecord, ImprovementTarget, SelfEditOrchestrator,
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
    /// Path to the persisted RSI state file.
    state_path: PathBuf,
}

impl RSIOrchestrator {
    pub fn new(project_root: PathBuf) -> Self {
        let state_path = Self::default_state_path(&project_root);
        let mut orch = Self {
            edit_orchestrator: SelfEditOrchestrator::new(project_root.clone()),
            _metrics: MetricsStore::new(),
            project_root,
            is_running: false,
            max_iterations: 100,
            total_iterations: 0,
            consecutive_failures: 0,
            max_consecutive_failures: 5,
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

    /// Save the current loop state to disk so it can be resumed.
    pub fn save_state(&self) -> std::result::Result<(), std::io::Error> {
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
        std::fs::write(&self.state_path, json)
    }

    /// Load previously persisted state.
    fn load_state(&self) -> std::result::Result<RSIState, std::io::Error> {
        let data = std::fs::read_to_string(&self.state_path)?;
        serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Run the RSI outer loop with safety guardrails.
    ///
    /// The loop will terminate if any of the following conditions are met:
    /// - `max_iterations` cycles have been executed.
    /// - `max_consecutive_failures` failures occur in a row (circuit breaker).
    /// - `stop()` is called externally.
    pub async fn run_loop(&mut self) -> Result<()> {
        self.is_running = true;
        // Don't reset consecutive_failures — the restored value from disk
        // carries over so the circuit breaker state survives restarts.
        let mut iteration: usize = 0;

        info!(
            "Starting outer RSI loop (max_iterations={}, total_completed={}, max_consecutive_failures={})...",
            self.max_iterations, self.total_iterations, self.max_consecutive_failures
        );

        while self.is_running && (self.total_iterations + iteration) < self.max_iterations {
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
                    self.consecutive_failures = 0;
                }
                Err(e) => {
                    self.consecutive_failures += 1;
                    error!(
                        "Improvement cycle failed ({} consecutive failure(s)): {}",
                        self.consecutive_failures, e
                    );

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

        // 1. Measure Baseline Fitness
        let baseline_score = self.measure_fitness().await?;
        debug!("Baseline fitness score: {}", baseline_score);

        // 2. Identify Target (Introspect)
        // In a full implementation, this would use LLM introspection to find weaknesses.
        // For now, we rely on the existing analyze_self logic.
        let targets = self.edit_orchestrator.analyze_self();
        if targets.is_empty() {
            info!("No improvement targets found in this cycle.");
            return Ok(false);
        }

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

        // 5. Verify compilation and tests in sandbox
        info!("Verifying compilation in sandbox...");
        if !sandbox.verify()? {
            warn!("Compilation or tests failed in sandbox. Rejecting mutation.");
            self.record_improvement(&target, None, baseline_score, false, true)
                .await?;
            sandbox.cleanup()?;
            return Ok(false);
        }

        // 6. Measure New Fitness in Sandbox
        // Since we can't easily run the benchmark on the sandbox right now without changing paths,
        // we assume the sandbox passed tests and check its score.
        let new_score = self.measure_sandbox_fitness(&sandbox).await?;
        debug!("New fitness score: {}", new_score);

        // 7. Evaluate
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

    /// Measure fitness score using E2E benchmarks
    async fn measure_fitness(&self) -> Result<f64> {
        self.run_benchmark_and_get_score(&self.project_root).await
    }

    /// Measure fitness in the sandbox environment
    async fn measure_sandbox_fitness(&self, sandbox: &CompilationSandbox) -> Result<f64> {
        self.run_benchmark_and_get_score(sandbox.work_dir()).await
    }

    async fn run_benchmark_and_get_score(&self, work_dir: &std::path::Path) -> Result<f64> {
        info!("Running E2E benchmark suite in {:?}", work_dir);
        let script_path = work_dir.join("system_tests/projecte2e/run_projecte2e.sh");

        // This might take a long time
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

        // Parse the TSV
        let reports_dir = work_dir.join("system_tests/projecte2e/reports/latest");
        let results_tsv = reports_dir.join("results.tsv");

        if !results_tsv.exists() {
            return Err(SelfwareError::Internal(
                "Benchmark results.tsv not found".to_string(),
            ));
        }

        let tsv_content = std::fs::read_to_string(&results_tsv)
            .map_err(|e| SelfwareError::Internal(format!("Failed to read results.tsv: {}", e)))?;

        // Calculate average score from the TSV
        // Format: scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes
        let mut total_score = 0.0;
        let mut count = 0;

        for (i, line) in tsv_content.lines().enumerate() {
            if i == 0 {
                continue;
            } // Skip header
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() > 8 {
                if let Ok(score) = parts[8].parse::<f64>() {
                    total_score += score;
                    count += 1;
                }
            }
        }

        if count == 0 {
            return Ok(0.0);
        }

        Ok(total_score / count as f64)
    }

    async fn merge_sandbox(
        &self,
        sandbox: CompilationSandbox,
        applied: &AppliedMutation,
    ) -> Result<()> {
        info!("Merging sandbox changes back to main workspace...");

        for rel_path in &applied.edited_files {
            let source = sandbox.work_dir().join(rel_path);
            let destination = self.project_root.join(rel_path);
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    SelfwareError::Internal(format!("Failed to create merge dir: {}", e))
                })?;
            }
            tokio::fs::copy(&source, &destination).await.map_err(|e| {
                SelfwareError::Internal(format!("Failed to merge sandbox file {}: {}", rel_path, e))
            })?;
        }

        sandbox.cleanup()?;
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
            _metrics: MetricsStore::with_path(project_root.join(".selfware/test-metrics.jsonl")),
            project_root,
            is_running: false,
            max_iterations: 100,
            total_iterations: 0,
            consecutive_failures: 0,
            max_consecutive_failures: 5,
            state_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as StdCommand;

    #[test]
    fn test_rsi_orchestrator_new_defaults() {
        let orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
        assert_eq!(orch.project_root, PathBuf::from("/tmp/test_project"));
        assert!(!orch.is_running);
        assert_eq!(orch.max_iterations, 100);
        assert_eq!(orch.consecutive_failures, 0);
        assert_eq!(orch.max_consecutive_failures, 5);
    }

    #[test]
    fn test_rsi_orchestrator_stop() {
        let mut orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
        // Initially not running
        assert!(!orch.is_running);

        // Simulate the state that run_loop sets
        orch.is_running = true;
        assert!(orch.is_running);

        orch.stop();
        assert!(!orch.is_running);
    }

    #[test]
    fn test_rsi_orchestrator_stop_idempotent() {
        let mut orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
        orch.stop();
        orch.stop(); // second call should be fine
        assert!(!orch.is_running);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        // Test the exponential backoff formula from run_loop:
        // 60 * 2^(failures-1), capped at 3600
        let compute_backoff = |consecutive_failures: usize| -> u64 {
            std::cmp::min(
                60u64.saturating_mul(1u64 << (consecutive_failures - 1)),
                3600,
            )
        };

        assert_eq!(compute_backoff(1), 60); // 60 * 2^0 = 60
        assert_eq!(compute_backoff(2), 120); // 60 * 2^1 = 120
        assert_eq!(compute_backoff(3), 240); // 60 * 2^2 = 240
        assert_eq!(compute_backoff(4), 480); // 60 * 2^3 = 480
        assert_eq!(compute_backoff(5), 960); // 60 * 2^4 = 960
        assert_eq!(compute_backoff(6), 1920); // 60 * 2^5 = 1920
        assert_eq!(compute_backoff(7), 3600); // 60 * 2^6 = 3840, capped at 3600
    }

    #[test]
    fn test_tsv_score_parsing_empty() {
        // Simulate TSV parsing logic from run_benchmark_and_get_score
        let tsv_content = "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n";
        let (total_score, count) = parse_tsv_scores(tsv_content);
        assert_eq!(count, 0);
        assert_eq!(total_score, 0.0);
    }

    #[test]
    fn test_tsv_score_parsing_single_row() {
        let tsv_content = "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy|0.5|0.8|agent1|30|15|0.85|yes||ok\n";
        let (total_score, count) = parse_tsv_scores(tsv_content);
        assert_eq!(count, 1);
        assert!((total_score - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tsv_score_parsing_multiple_rows() {
        let tsv_content = "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy|0.5|0.8|agent1|30|15|0.80|yes||ok\n\
                           test2|unit|medium|0.3|0.7|agent1|60|30|0.90|yes||ok\n\
                           test3|unit|hard|0.1|0.5|agent1|120|60|0.70|no||fail\n";
        let (total_score, count) = parse_tsv_scores(tsv_content);
        assert_eq!(count, 3);
        let avg = total_score / count as f64;
        assert!((avg - 0.80).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tsv_score_parsing_invalid_score() {
        let tsv_content = "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy|0.5|0.8|agent1|30|15|not_a_number|yes||ok\n";
        let (total_score, count) = parse_tsv_scores(tsv_content);
        assert_eq!(count, 0);
        assert_eq!(total_score, 0.0);
    }

    #[test]
    fn test_tsv_score_parsing_short_row() {
        // Row with fewer than 9 columns should be skipped
        let tsv_content = "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy\n";
        let (total_score, count) = parse_tsv_scores(tsv_content);
        assert_eq!(count, 0);
        assert_eq!(total_score, 0.0);
    }

    #[test]
    fn test_consecutive_failures_tracking() {
        let mut orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
        assert_eq!(orch.consecutive_failures, 0);

        // Simulate failure increments
        orch.consecutive_failures += 1;
        assert_eq!(orch.consecutive_failures, 1);

        orch.consecutive_failures += 1;
        assert_eq!(orch.consecutive_failures, 2);

        // Simulate reset on success
        orch.consecutive_failures = 0;
        assert_eq!(orch.consecutive_failures, 0);
    }

    #[test]
    fn test_circuit_breaker_threshold() {
        let orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
        // Verify the circuit breaker triggers at exactly max_consecutive_failures
        assert_eq!(orch.max_consecutive_failures, 5);

        // Simulate reaching threshold
        let mut failures = 0;
        let should_trip = |failures: usize, max: usize| failures >= max;

        for _ in 0..4 {
            failures += 1;
            assert!(
                !should_trip(failures, orch.max_consecutive_failures),
                "Should not trip at {} failures",
                failures
            );
        }
        failures += 1;
        assert!(
            should_trip(failures, orch.max_consecutive_failures),
            "Should trip at {} failures",
            failures
        );
    }

    #[test]
    fn test_rsi_state_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path().to_path_buf();
        let mut orch = RSIOrchestrator::new(project_root);
        orch.total_iterations = 42;
        orch.consecutive_failures = 3;
        orch.save_state().unwrap();

        // Load into a fresh orchestrator
        let orch2 = RSIOrchestrator::new(dir.path().to_path_buf());
        assert_eq!(orch2.total_iterations, 42);
        assert_eq!(orch2.consecutive_failures, 3);
    }

    #[test]
    fn test_rsi_state_missing_file_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let orch = RSIOrchestrator::new(dir.path().to_path_buf());
        // Should start from defaults when no state file exists
        assert_eq!(orch.total_iterations, 0);
        assert_eq!(orch.consecutive_failures, 0);
    }

    #[test]
    fn test_rsi_stop_saves_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut orch = RSIOrchestrator::new(dir.path().to_path_buf());
        orch.total_iterations = 10;
        orch.stop();
        // Verify state was persisted
        let state_path = RSIOrchestrator::default_state_path(dir.path());
        assert!(state_path.exists());
    }

    #[tokio::test]
    async fn test_execute_improvement_cycle_applies_mutation_and_records_result() {
        let dir = create_rsi_fixture_project();
        let project_root = dir.path().to_path_buf();
        let state_path = project_root.join(".selfware/rsi_state.json");
        let history_path = project_root.join(".selfware/history.json");
        let mut orch = RSIOrchestrator::with_paths(project_root.clone(), state_path, history_path);

        let improved = orch.execute_improvement_cycle().await.unwrap();
        assert!(improved, "RSI cycle should merge an improving mutation");

        let content = fs::read_to_string(project_root.join("src/lib.rs")).unwrap();
        assert!(content.contains("Resolved: remove this marker"));
        assert!(!content.contains("TODO"));

        let history = orch.edit_orchestrator.history();
        assert_eq!(history.len(), 1);
        assert!(history[0].verified);
        assert!(!history[0].rolled_back);
        assert!(history[0].effectiveness_score > 0.0);
    }

    /// Helper: replicates the TSV score-parsing logic from run_benchmark_and_get_score.
    fn parse_tsv_scores(tsv_content: &str) -> (f64, usize) {
        let mut total_score = 0.0;
        let mut count = 0;

        for (i, line) in tsv_content.lines().enumerate() {
            if i == 0 {
                continue;
            }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() > 8 {
                if let Ok(score) = parts[8].parse::<f64>() {
                    total_score += score;
                    count += 1;
                }
            }
        }

        (total_score, count)
    }

    fn create_rsi_fixture_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("system_tests/projecte2e")).unwrap();

        fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "rsi_fixture"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
        )
        .unwrap();

        fs::write(
            root.join("src/lib.rs"),
            r#"pub fn demo() -> usize {
    // TODO: remove this marker
    42
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_returns_answer() {
        assert_eq!(super::demo(), 42);
    }
}
"#,
        )
        .unwrap();

        fs::write(
            root.join("system_tests/projecte2e/run_projecte2e.sh"),
            r#"#!/usr/bin/env bash
set -euo pipefail
mkdir -p system_tests/projecte2e/reports/latest
score="0.25"
if ! grep -R -E "TODO|FIXME" src >/dev/null 2>&1; then
  score="0.90"
fi
cat > system_tests/projecte2e/reports/latest/results.tsv <<EOF
scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes
todo_cleanup|unit|easy|0|0|selfware|0|0|${score}|yes||
EOF
"#,
        )
        .unwrap();

        init_git_repo(root);
        dir
    }

    fn init_git_repo(project_root: &Path) {
        run_git(project_root, &["init"]);
        run_git(project_root, &["config", "user.email", "codex@openai.com"]);
        run_git(project_root, &["config", "user.name", "Codex"]);
        run_git(project_root, &["add", "."]);
        run_git(project_root, &["commit", "-m", "initial"]);
    }

    fn run_git(project_root: &Path, args: &[&str]) {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(project_root)
            .status()
            .unwrap();
        assert!(status.success(), "git {:?} should succeed", args);
    }
}
