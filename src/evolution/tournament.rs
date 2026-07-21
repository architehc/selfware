//! Tournament Selection — Parallel Hypothesis Evaluation
//!
//! Runs multiple mutation hypotheses concurrently in sandboxes,
//! scores them against the fitness function, and selects winners.

use super::fitness::SabResult;
use super::sandbox::{Sandbox, SandboxConfig, SandboxResult};
use super::{FitnessMetrics, FitnessWeights, GenerationRating};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Simple counting semaphore for thread-pool style concurrency
mod semaphore {
    use std::sync::{Condvar, Mutex};

    pub struct Semaphore {
        count: Mutex<usize>,
        condvar: Condvar,
    }

    impl Semaphore {
        pub fn new(count: usize) -> Self {
            Self {
                count: Mutex::new(count),
                condvar: Condvar::new(),
            }
        }

        pub fn acquire(&self) {
            let mut count = self.count.lock().unwrap();
            while *count == 0 {
                count = self.condvar.wait(count).unwrap();
            }
            *count -= 1;
        }

        pub fn release(&self) {
            let mut count = self.count.lock().unwrap();
            *count += 1;
            self.condvar.notify_one();
        }
    }
}

/// A mutation hypothesis proposed by the agent
#[derive(Debug, Clone)]
pub struct Hypothesis {
    /// Unique identifier
    pub id: String,
    /// Human-readable description of what the mutation does
    pub description: String,
    /// Unified diff (patch format)
    pub patch: String,
    /// Files affected by this mutation
    pub target_files: Vec<PathBuf>,
    /// Optional: property test that should pass after mutation
    pub property_test: Option<String>,
}

/// Result of evaluating a single hypothesis
#[derive(Debug)]
pub struct HypothesisResult {
    pub id: String,
    pub description: String,
    pub compiled: bool,
    pub sandbox_result: Option<SandboxResult>,
    pub sab_result: Option<SabResult>,
    pub fitness: Option<FitnessMetrics>,
    pub composite_score: f64,
    pub rating: GenerationRating,
    pub patch: String,
}

/// Tournament configuration
#[derive(Debug, Clone)]
pub struct TournamentConfig {
    /// Maximum concurrent sandboxes
    pub max_parallel: usize,
    /// Per-hypothesis timeout
    pub timeout: Duration,
    /// Fitness weights for scoring
    pub weights: FitnessWeights,
    /// Sandbox resource config
    pub sandbox: SandboxConfig,
}

impl Default for TournamentConfig {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            timeout: Duration::from_secs(3600),
            weights: FitnessWeights::default(),
            sandbox: SandboxConfig::default(),
        }
    }
}

/// Run a tournament: evaluate all hypotheses in parallel sandboxes,
/// sort by fitness, return ranked results.
///
/// In the async version, this would use tokio::spawn + Semaphore.
/// For now, we use a thread pool approach.
pub fn run_tournament(
    hypotheses: Vec<Hypothesis>,
    config: &TournamentConfig,
    repo_root: &Path,
) -> Vec<HypothesisResult> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let results = Arc::new(Mutex::new(Vec::new()));
    let semaphore = Arc::new(semaphore::Semaphore::new(config.max_parallel));

    let handles: Vec<_> = hypotheses
        .into_iter()
        .map(|h| {
            let sem = semaphore.clone();
            let res = results.clone();
            let cfg = config.clone();
            let root = repo_root.to_path_buf();

            thread::spawn(move || {
                // Acquire semaphore slot
                sem.acquire();
                let result = evaluate_hypothesis(h, &cfg, &root);
                sem.release();
                res.lock().unwrap().push(result);
            })
        })
        .collect();

    // Wait for all evaluations
    for h in handles {
        let _ = h.join();
    }

    let mut results = Arc::try_unwrap(results)
        .unwrap_or_else(|_| panic!("Failed to unwrap results"))
        .into_inner()
        .unwrap();

    // Sort by composite score (highest first)
    results.sort_by(|a, b| {
        b.composite_score
            .partial_cmp(&a.composite_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

fn evaluate_hypothesis(
    hypothesis: Hypothesis,
    config: &TournamentConfig,
    repo_root: &Path,
) -> HypothesisResult {
    // 1. Create sandbox
    let sandbox = match Sandbox::create(&hypothesis.id, repo_root, config.sandbox.clone()) {
        Ok(s) => s,
        Err(_e) => {
            return HypothesisResult {
                id: hypothesis.id,
                description: hypothesis.description,
                compiled: false,
                sandbox_result: None,
                sab_result: None,
                fitness: None,
                composite_score: 0.0,
                rating: GenerationRating::Frost,
                patch: hypothesis.patch,
            };
        }
    };

    // 2. Apply patch
    if !sandbox.apply_patch(&hypothesis.patch).unwrap_or(false) {
        let _ = sandbox.destroy();
        return HypothesisResult {
            id: hypothesis.id,
            description: hypothesis.description,
            compiled: false,
            sandbox_result: None,
            sab_result: None,
            fitness: None,
            composite_score: 0.0,
            rating: GenerationRating::Frost,
            patch: hypothesis.patch,
        };
    }

    // 3. Evaluate (compile + test + bench)
    let sandbox_result = match sandbox.evaluate() {
        Ok(r) => r,
        Err(_) => {
            let _ = sandbox.destroy();
            return HypothesisResult {
                id: hypothesis.id,
                description: hypothesis.description,
                compiled: false,
                sandbox_result: None,
                sab_result: None,
                fitness: None,
                composite_score: 0.0,
                rating: GenerationRating::Frost,
                patch: hypothesis.patch,
            };
        }
    };

    let compiled = sandbox_result.compiled;
    let tests_passed = sandbox_result.tests_passed;
    let tests_total = sandbox_result.tests_total;

    // 4. Cleanup
    let compile_duration = sandbox_result.compile_duration;
    let peak_memory_bytes = sandbox_result.peak_memory_bytes;
    let test_duration = sandbox_result.test_duration;
    let _ = sandbox.destroy();

    // 5. Compute fitness metrics from the sandbox result.
    // Full SAB runs separately for winners, so sab_result stays None here.
    let timeout_secs = config.timeout.as_secs() as f64;
    let wall_clock_secs = (compile_duration + test_duration).as_secs_f64();
    let test_coverage_pct = if tests_total > 0 {
        (tests_passed as f64 / tests_total as f64) * 100.0
    } else {
        0.0
    };
    // Derive a provisional SAB score from the test pass ratio (0–100).
    let sab_score = if compiled { test_coverage_pct } else { 0.0 };
    let fitness = FitnessMetrics {
        sab_score,
        tokens_used: 0,
        token_budget: 0,
        wall_clock_secs,
        timeout_secs,
        test_coverage_pct,
        binary_size_mb: (peak_memory_bytes as f64) / (1024.0 * 1024.0),
        max_binary_size_mb: 1024.0,
        tests_passed,
        tests_total,
        visual_score: 0.0,
    };
    // Use the weighted composite as the fitness-derived score.
    let fitness_composite = config.weights.composite(&fitness);

    // 6. Rating
    let rating = if !compiled {
        GenerationRating::Frost
    } else if tests_passed == tests_total && tests_total > 0 {
        GenerationRating::Bloom
    } else if tests_passed as f64 / tests_total.max(1) as f64 > 0.95 {
        GenerationRating::Grow
    } else if tests_passed as f64 / tests_total.max(1) as f64 > 0.50 {
        GenerationRating::Wilt
    } else {
        GenerationRating::Frost
    };

    // Composite score: use the weighted fitness composite when available,
    // falling back to the simple test-ratio score.
    let composite = if compiled {
        fitness_composite.max((tests_passed as f64 / tests_total.max(1) as f64) * 100.0)
    } else {
        0.0
    };

    HypothesisResult {
        id: hypothesis.id,
        description: hypothesis.description,
        compiled,
        sandbox_result: Some(sandbox_result),
        sab_result: None, // Full SAB runs separately for winners
        fitness: Some(fitness),
        composite_score: composite,
        rating,
        patch: hypothesis.patch,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/tournament/tournament_test.rs"]
mod tests;
