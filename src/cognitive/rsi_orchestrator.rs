use crate::cognitive::compilation_manager::CompilationSandbox;
use crate::cognitive::metrics::MetricsStore;
use crate::cognitive::self_edit::SelfEditOrchestrator;
use crate::errors::{Result, SelfwareError};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// The outer loop for Recursive Self-Improvement
pub struct RSIOrchestrator {
    edit_orchestrator: SelfEditOrchestrator,
    _metrics: MetricsStore,
    project_root: PathBuf,
    is_running: bool,
    /// Hard upper bound on the number of improvement iterations before the loop terminates.
    max_iterations: usize,
    /// Tracks how many improvement cycles have failed in a row without a single success.
    consecutive_failures: usize,
    /// Circuit-breaker threshold: if this many consecutive failures occur, the loop aborts.
    max_consecutive_failures: usize,
}

impl RSIOrchestrator {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            edit_orchestrator: SelfEditOrchestrator::new(project_root.clone()),
            _metrics: MetricsStore::new(),
            project_root,
            is_running: false,
            max_iterations: 100,
            consecutive_failures: 0,
            max_consecutive_failures: 5,
        }
    }

    /// Run the RSI outer loop with safety guardrails.
    ///
    /// The loop will terminate if any of the following conditions are met:
    /// - `max_iterations` cycles have been executed.
    /// - `max_consecutive_failures` failures occur in a row (circuit breaker).
    /// - `stop()` is called externally.
    pub async fn run_loop(&mut self) -> Result<()> {
        self.is_running = true;
        self.consecutive_failures = 0;
        let mut iteration: usize = 0;

        info!(
            "Starting outer RSI loop (max_iterations={}, max_consecutive_failures={})...",
            self.max_iterations, self.max_consecutive_failures
        );

        while self.is_running && iteration < self.max_iterations {
            iteration += 1;
            info!("RSI iteration {}/{}", iteration, self.max_iterations);

            // Warn when approaching the iteration limit
            let remaining = self.max_iterations - iteration;
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
                    // A cycle that completes without error but produces no improvement is not
                    // counted as a failure for circuit-breaker purposes.
                    self.consecutive_failures = 0;
                }
                Err(e) => {
                    self.consecutive_failures += 1;
                    error!(
                        "Improvement cycle failed ({} consecutive failure(s)): {}",
                        self.consecutive_failures, e
                    );

                    // Warn when approaching the circuit-breaker threshold
                    if self.consecutive_failures >= self.max_consecutive_failures {
                        error!(
                            "Circuit breaker tripped: {} consecutive failures reached the limit of {}. \
                             Aborting RSI loop to prevent runaway damage.",
                            self.consecutive_failures, self.max_consecutive_failures
                        );
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

        if iteration >= self.max_iterations {
            warn!(
                "RSI loop terminated: reached maximum iteration limit of {}",
                self.max_iterations
            );
        }

        Ok(())
    }

    pub fn stop(&mut self) {
        self.is_running = false;
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

        // Pick highest priority target
        let target = targets.into_iter().next().unwrap();
        info!("Selected improvement target: {:?}", target);

        // 3. Create Sandbox
        let sandbox = self.edit_orchestrator.create_sandbox()?;

        // 4. Apply Mutation
        // In reality, the LLM would generate the code. Here we simulate the change being applied
        // by the agent in the sandbox.
        info!("Applying mutation to sandbox...");
        // (Mock applying change)

        // 5. Verify compilation and tests in sandbox
        info!("Verifying compilation in sandbox...");
        if !sandbox.verify()? {
            warn!("Compilation or tests failed in sandbox. Rejecting mutation.");
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
            self.merge_sandbox(sandbox).await?;

            // Record success
            self.record_improvement(target.id, true).await?;
            Ok(true)
        } else {
            info!(
                "Mutation degraded or did not improve fitness ({} <= {}). Rolling back.",
                new_score, baseline_score
            );
            sandbox.cleanup()?;

            // Record failure
            self.record_improvement(target.id, false).await?;
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

    async fn merge_sandbox(&self, sandbox: CompilationSandbox) -> Result<()> {
        // Use git or file copy to merge back from work_dir to original_dir
        info!("Merging sandbox changes back to main workspace...");

        let output = Command::new("rsync")
            .arg("-av")
            .arg("--exclude=.git")
            .arg("--exclude=target")
            .arg(format!("{}/", sandbox.work_dir().display()))
            .arg(format!("{}/", self.project_root.display()))
            .output()
            .map_err(|e| SelfwareError::Internal(e.to_string()))?;

        if !output.status.success() {
            return Err(SelfwareError::Internal(
                "Failed to merge sandbox".to_string(),
            ));
        }

        sandbox.cleanup()?;
        Ok(())
    }

    async fn record_improvement(&mut self, _target_id: String, _success: bool) -> Result<()> {
        // Update meta-learner and store history
        Ok(())
    }
}
