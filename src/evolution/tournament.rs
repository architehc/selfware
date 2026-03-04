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
    let _ = sandbox.destroy();

    // 5. Score
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

    // Composite score (simplified — full version uses SAB)
    let composite = if compiled {
        (tests_passed as f64 / tests_total.max(1) as f64) * 100.0
    } else {
        0.0
    };

    HypothesisResult {
        id: hypothesis.id,
        description: hypothesis.description,
        compiled,
        sandbox_result: Some(sandbox_result),
        sab_result: None, // Full SAB runs separately for winners
        fitness: None,
        composite_score: composite,
        rating,
        patch: hypothesis.patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hypothesis_result_frost_on_compile_failure() {
        let result = HypothesisResult {
            id: "test".into(),
            description: "test".into(),
            compiled: false,
            sandbox_result: None,
            sab_result: None,
            fitness: None,
            composite_score: 0.0,
            rating: GenerationRating::Frost,
            patch: String::new(),
        };
        assert_eq!(result.rating, GenerationRating::Frost);
        assert_eq!(result.composite_score, 0.0);
    }

    #[test]
    fn test_tournament_config_default() {
        let cfg = TournamentConfig::default();
        assert_eq!(cfg.max_parallel, 4);
        assert_eq!(cfg.timeout, Duration::from_secs(3600));
    }

    #[test]
    fn test_semaphore_basic() {
        let sem = semaphore::Semaphore::new(2);
        sem.acquire();
        sem.acquire();
        sem.release();
        sem.acquire(); // Should not deadlock
        sem.release();
        sem.release();
    }

    #[test]
    fn test_run_tournament_empty_hypotheses() {
        let config = TournamentConfig::default();
        let tmp = std::env::temp_dir();
        let results = run_tournament(vec![], &config, &tmp);
        assert!(results.is_empty());
    }

    #[test]
    fn test_semaphore_concurrent_threads() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let sem = Arc::new(semaphore::Semaphore::new(3));
        let counter = Arc::new(Mutex::new(0usize));
        let max_concurrent = Arc::new(Mutex::new(0usize));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let s = sem.clone();
                let c = counter.clone();
                let m = max_concurrent.clone();
                thread::spawn(move || {
                    s.acquire();
                    {
                        let mut count = c.lock().unwrap();
                        *count += 1;
                        let mut max = m.lock().unwrap();
                        if *count > *max {
                            *max = *count;
                        }
                    }
                    // Simulate work
                    thread::sleep(std::time::Duration::from_millis(10));
                    {
                        let mut count = c.lock().unwrap();
                        *count -= 1;
                    }
                    s.release();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let max = *max_concurrent.lock().unwrap();
        assert!(max <= 3, "Max concurrent should be <= 3, got {}", max);
    }

    #[test]
    fn test_semaphore_single_slot() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let sem = Arc::new(semaphore::Semaphore::new(1));
        let counter = Arc::new(Mutex::new(0usize));
        let max_concurrent = Arc::new(Mutex::new(0usize));

        let handles: Vec<_> = (0..5)
            .map(|_| {
                let s = sem.clone();
                let c = counter.clone();
                let m = max_concurrent.clone();
                thread::spawn(move || {
                    s.acquire();
                    {
                        let mut count = c.lock().unwrap();
                        *count += 1;
                        let mut max = m.lock().unwrap();
                        if *count > *max {
                            *max = *count;
                        }
                    }
                    thread::sleep(std::time::Duration::from_millis(5));
                    {
                        let mut count = c.lock().unwrap();
                        *count -= 1;
                    }
                    s.release();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let max = *max_concurrent.lock().unwrap();
        assert_eq!(max, 1, "Max concurrent should be exactly 1");
    }

    #[test]
    fn test_hypothesis_result_bloom_rating() {
        let result = HypothesisResult {
            id: "bloom-test".into(),
            description: "All tests pass".into(),
            compiled: true,
            sandbox_result: None,
            sab_result: None,
            fitness: None,
            composite_score: 100.0,
            rating: GenerationRating::Bloom,
            patch: "--- a/file\n+++ b/file".into(),
        };
        assert_eq!(result.rating, GenerationRating::Bloom);
        assert_eq!(result.composite_score, 100.0);
        assert!(result.compiled);
    }

    #[test]
    fn test_hypothesis_result_grow_rating() {
        let result = HypothesisResult {
            id: "grow-test".into(),
            description: "Most tests pass".into(),
            compiled: true,
            sandbox_result: None,
            sab_result: None,
            fitness: None,
            composite_score: 96.0,
            rating: GenerationRating::Grow,
            patch: String::new(),
        };
        assert_eq!(result.rating, GenerationRating::Grow);
        assert!(result.composite_score > 0.0);
    }

    #[test]
    fn test_tournament_config_clone() {
        let cfg = TournamentConfig {
            max_parallel: 8,
            timeout: Duration::from_secs(7200),
            weights: FitnessWeights::default(),
            sandbox: SandboxConfig::default(),
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.max_parallel, 8);
        assert_eq!(cloned.timeout, Duration::from_secs(7200));
    }

    #[test]
    fn test_hypothesis_clone() {
        let h = Hypothesis {
            id: "h1".into(),
            description: "Test mutation".into(),
            patch: "diff content".into(),
            target_files: vec![PathBuf::from("src/lib.rs")],
            property_test: Some("test_prop".into()),
        };
        let cloned = h.clone();
        assert_eq!(cloned.id, "h1");
        assert_eq!(cloned.target_files.len(), 1);
        assert_eq!(cloned.property_test, Some("test_prop".into()));
    }
}
