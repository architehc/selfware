use crate::cognitive::compilation_manager::CompilationSandbox;
use crate::cognitive::self_edit::SelfEditOrchestrator;
use crate::cognitive::metrics::MetricsStore;
use crate::errors::{SelfwareError, Result};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn, error, debug};
use std::process::Command;

/// The outer loop for Recursive Self-Improvement
pub struct RSIOrchestrator {
    edit_orchestrator: SelfEditOrchestrator,
    metrics: MetricsStore,
    project_root: PathBuf,
    is_running: bool,
}

impl RSIOrchestrator {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            edit_orchestrator: SelfEditOrchestrator::new(project_root.clone()),
            metrics: MetricsStore::new(),
            project_root,
            is_running: false,
        }
    }

    /// Run the RSI outer loop indefinitely
    pub async fn run_loop(&mut self) -> Result<()> {
        self.is_running = true;
        info!("Starting outer RSI loop...");

        while self.is_running {
            match self.execute_improvement_cycle().await {
                Ok(true) => {
                    info!("Improvement cycle successful and merged.");
                }
                Ok(false) => {
                    info!("Improvement cycle did not yield a better fitness score. Changes discarded.");
                }
                Err(e) => {
                    error!("Improvement cycle failed: {}", e);
                    // Implement exponential backoff here if needed
                }
            }

            // Sleep before next cycle to prevent thrashing
            tokio::time::sleep(Duration::from_secs(60)).await;
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
            info!("Mutation improved fitness ({} > {}). Merging.", new_score, baseline_score);
            self.merge_sandbox(sandbox).await?;
            
            // Record success
            self.record_improvement(target.id, true).await?;
            Ok(true)
        } else {
            info!("Mutation degraded or did not improve fitness ({} <= {}). Rolling back.", new_score, baseline_score);
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
            .map_err(|e| SelfwareError::Internal(format!("Failed to run benchmark script: {}", e)))?;
            
        if !output.status.success() {
            warn!("Benchmark script returned non-zero exit code: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        // Parse the TSV
        let reports_dir = work_dir.join("system_tests/projecte2e/reports/latest");
        let results_tsv = reports_dir.join("results.tsv");
        
        if !results_tsv.exists() {
            return Err(SelfwareError::Internal("Benchmark results.tsv not found".to_string()));
        }
        
        let tsv_content = std::fs::read_to_string(&results_tsv)
            .map_err(|e| SelfwareError::Internal(format!("Failed to read results.tsv: {}", e)))?;
            
        // Calculate average score from the TSV
        // Format: scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes
        let mut total_score = 0.0;
        let mut count = 0;
        
        for (i, line) in tsv_content.lines().enumerate() {
            if i == 0 { continue; } // Skip header
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
            .output().map_err(|e| SelfwareError::Internal(e.to_string()))?;

        if !output.status.success() {
            return Err(SelfwareError::Internal("Failed to merge sandbox".to_string()));
        }

        sandbox.cleanup()?;
        Ok(())
    }
    
    async fn record_improvement(&mut self, _target_id: String, _success: bool) -> Result<()> {
        // Update meta-learner and store history
        Ok(())
    }
}
