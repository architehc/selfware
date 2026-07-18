use std::sync::Arc;
use tokio::sync::Mutex;

/// A concurrent task pool that tracks running, completed, and failed tasks.
/// Enforces a maximum concurrency limit.
pub struct TaskPool {
    state: Arc<Mutex<PoolState>>,
    max_concurrent: u32,
}

#[derive(Debug, Clone)]
struct PoolState {
    running: u32,
    completed: u32,
    failed: u32,
}

impl TaskPool {
    /// Create a new task pool with the given concurrency limit.
    pub fn new(max_concurrent: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(PoolState {
                running: 0,
                completed: 0,
                failed: 0,
            })),
            max_concurrent,
        }
    }

    /// Try to acquire a slot in the pool. Returns true if a slot was acquired.
    ///
    /// BUG: TOCTOU race - checks capacity under one lock, then increments
    /// under a separate lock. Another task can slip in between.
    pub async fn acquire(&self) -> bool {
        let has_capacity = {
            let state = self.state.lock().await;
            state.running < self.max_concurrent
        };
        // ^^^ Lock is dropped here. Another task can acquire between
        // the check above and the increment below.
        if has_capacity {
            let mut state = self.state.lock().await;
            state.running += 1;
            true
        } else {
            false
        }
    }

    /// Mark the current task as completed.
    ///
    /// BUG: Decrements running but forgets to increment completed.
    pub async fn complete(&self) {
        let mut state = self.state.lock().await;
        state.running -= 1;
        // BUG: missing `state.completed += 1;`
    }

    /// Mark the current task as failed.
    ///
    /// BUG: Increments failed but forgets to decrement running.
    pub async fn fail(&self) {
        let mut state = self.state.lock().await;
        // BUG: missing `state.running -= 1;`
        state.failed += 1;
    }

    /// Return a snapshot of (running, completed, failed).
    ///
    /// BUG: Split lock - reads running under one lock acquisition,
    /// then reads completed and failed under another. The state can
    /// change between the two reads, giving an inconsistent view.
    pub async fn snapshot(&self) -> (u32, u32, u32) {
        let running = {
            let state = self.state.lock().await;
            state.running
        };
        // ^^^ Lock dropped. State can mutate here.
        let (completed, failed) = {
            let state = self.state.lock().await;
            (state.completed, state.failed)
        };
        (running, completed, failed)
    }
}

impl Clone for TaskPool {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            max_concurrent: self.max_concurrent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::{timeout, Duration};

    /// Basic sequential test - this passes even with the bugs because
    /// there is no concurrency to expose the TOCTOU race, and the
    /// individual counter bugs are not checked thoroughly here.
    #[tokio::test]
    async fn test_sequential_operations() {
        let pool = TaskPool::new(2);

        // Acquire two slots sequentially - no race possible
        assert!(pool.acquire().await);
        assert!(pool.acquire().await);

        // Third should fail (at capacity)
        assert!(!pool.acquire().await);

        // Complete one, then acquire again
        pool.complete().await;
        assert!(pool.acquire().await);

        // Verify running count via snapshot
        let (running, _, _) = pool.snapshot().await;
        assert_eq!(running, 2, "should have 2 running tasks");
    }

    /// Concurrent stress test - spawns 20 tasks competing for 5 slots.
    /// Each task acquires, does brief async work, then completes or fails.
    /// At the end, running must be 0 and completed + failed must equal 20.
    ///
    /// This test WILL FAIL with the bugs present because:
    /// - The TOCTOU race lets more than max_concurrent tasks run simultaneously
    /// - complete() doesn't increment `completed`, so completed + failed != 20
    /// - fail() doesn't decrement `running`, so running != 0 at the end
    #[tokio::test]
    async fn test_concurrent_race() {
        let pool = TaskPool::new(5);
        let total_tasks: u32 = 20;
        let peak_running = Arc::new(AtomicU32::new(0));
        let mut handles = Vec::new();

        for i in 0..total_tasks {
            let p = pool.clone();
            let peak = Arc::clone(&peak_running);
            handles.push(tokio::spawn(async move {
                // Spin-acquire with timeout to avoid hanging when bugs
                // cause slot leaks. The timeout itself signals a problem.
                let acquired = timeout(Duration::from_secs(5), async {
                    loop {
                        if p.acquire().await {
                            return true;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await;

                if acquired.is_err() {
                    // Timed out trying to acquire - this happens because
                    // fail() leaks running slots. Force-add to running
                    // so the test can finish and assert on counters.
                    let mut state = p.state.lock().await;
                    state.running += 1;
                }

                // Track peak concurrency
                {
                    let state = p.state.lock().await;
                    let current = state.running;
                    peak.fetch_max(current, Ordering::Relaxed);
                }

                // Simulate async work with a yield to encourage interleaving
                tokio::task::yield_now().await;

                // Even-numbered tasks complete, odd-numbered tasks fail
                if i % 2 == 0 {
                    p.complete().await;
                } else {
                    p.fail().await;
                }
            }));
        }

        // Wait for all tasks to finish
        for h in handles {
            h.await.unwrap();
        }

        let (running, completed, failed) = pool.snapshot().await;

        assert_eq!(
            running, 0,
            "all tasks finished, running should be 0 but got {running}"
        );
        assert_eq!(
            completed + failed,
            total_tasks,
            "completed ({completed}) + failed ({failed}) should equal {total_tasks}"
        );
        assert_eq!(completed, total_tasks / 2, "half should complete, got {completed}");
        assert_eq!(failed, total_tasks / 2, "half should fail, got {failed}");

        // The TOCTOU bug allows more tasks than max_concurrent to run at once
        let peak = peak_running.load(Ordering::Relaxed);
        assert!(
            peak <= 5,
            "peak concurrent tasks ({peak}) should not exceed max_concurrent (5)"
        );
    }

    /// Snapshot consistency test - mutates the pool concurrently while
    /// taking snapshots, and verifies that each snapshot is internally
    /// consistent.
    ///
    /// The split-lock bug in snapshot() means the values can come from
    /// different points in time, so running + completed + failed may
    /// not add up correctly.
    #[tokio::test]
    async fn test_snapshot_consistency() {
        let pool = TaskPool::new(100); // High limit so acquire never blocks
        let total_tasks: u32 = 50;
        let mut handles = Vec::new();

        // Spawn mutator tasks: each acquires, yields, then completes.
        for _ in 0..total_tasks {
            let p = pool.clone();
            handles.push(tokio::spawn(async move {
                p.acquire().await;
                tokio::task::yield_now().await;
                p.complete().await;
            }));
        }

        // Spawn snapshot tasks that check consistency
        let snapshot_violations = Arc::new(AtomicU32::new(0));
        let mut snapshot_handles = Vec::new();
        for _ in 0..20 {
            let p = pool.clone();
            let violations = Arc::clone(&snapshot_violations);
            snapshot_handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let (running, completed, failed) = p.snapshot().await;
                    let total = running + completed + failed;
                    // The total should never exceed total_tasks. With the
                    // split-lock bug, we can observe running from time T1
                    // and completed from time T2 > T1, making the sum too large.
                    if total > total_tasks {
                        violations.fetch_add(1, Ordering::Relaxed);
                    }
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Wait for all mutators
        for h in handles {
            h.await.unwrap();
        }
        // Wait for all snapshot checkers
        for h in snapshot_handles {
            h.await.unwrap();
        }

        // Final snapshot must be perfectly consistent
        let (running, completed, failed) = pool.snapshot().await;
        assert_eq!(
            running, 0,
            "all tasks done, running should be 0 but got {running}"
        );
        assert_eq!(
            completed, total_tasks,
            "all tasks completed, expected {total_tasks} but got {completed}"
        );
        assert_eq!(failed, 0, "no tasks failed, but got {failed}");
    }
}
