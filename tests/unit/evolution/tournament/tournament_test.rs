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
