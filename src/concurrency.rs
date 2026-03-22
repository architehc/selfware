//! Concurrency governor for limiting concurrent streaming and tool execution.
//!
//! Provides [`ConcurrencyGovernor`] which uses layered semaphores to bound:
//! - Concurrent LLM streaming responses
//! - Concurrent tool executions per agent
//! - Total inflight operations globally
//!
//! All permits are RAII guards that release automatically on drop.

use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// Governs concurrency across streaming, tool execution, and global operations.
///
/// Uses three independent semaphore layers so that, for example, a burst of
/// tool executions cannot starve streaming slots and vice-versa.  The global
/// semaphore acts as an upper ceiling across both categories.
pub struct ConcurrencyGovernor {
    /// Limits concurrent LLM streaming responses.
    stream_semaphore: Arc<Semaphore>,
    /// Limits concurrent tool executions per agent.
    tool_semaphore: Arc<Semaphore>,
    /// Global limit on total inflight operations.
    global_semaphore: Arc<Semaphore>,
    /// Maximum permits for streams (stored for stats reporting).
    max_streams: usize,
    /// Maximum permits for tools (stored for stats reporting).
    max_tools: usize,
    /// Maximum global permits (stored for stats reporting).
    max_global: usize,
}

impl ConcurrencyGovernor {
    /// Create a new governor with explicit limits.
    pub fn new(max_streams: usize, max_tools: usize, max_global: usize) -> Self {
        Self {
            stream_semaphore: Arc::new(Semaphore::new(max_streams)),
            tool_semaphore: Arc::new(Semaphore::new(max_tools)),
            global_semaphore: Arc::new(Semaphore::new(max_global)),
            max_streams,
            max_tools,
            max_global,
        }
    }

    /// Create a governor with sensible defaults:
    /// - 4 concurrent streams (LLM API calls)
    /// - 8 concurrent tool executions (file I/O, shell, etc.)
    /// - 12 total inflight operations
    ///
    /// Tuned for 2x4090 with Qwen3.5-27B: 8 concurrent is the sweet spot
    /// (48 tok/s aggregate). 16 concurrent causes OOM/timeouts.
    pub fn with_defaults() -> Self {
        Self::new(4, 8, 12)
    }

    /// Acquire a stream permit, waiting if none are currently available.
    ///
    /// Returns a [`ConcurrencyPermit`] that holds both a stream-level and a
    /// global-level permit.  Both are released when the permit is dropped.
    pub async fn acquire_stream(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
        let global = Arc::clone(&self.global_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        let stream = Arc::clone(&self.stream_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        Ok(ConcurrencyPermit {
            _category: stream,
            _global: global,
        })
    }

    /// Acquire a tool execution permit, waiting if none are currently available.
    ///
    /// Returns a [`ConcurrencyPermit`] that holds both a tool-level and a
    /// global-level permit.  Both are released when the permit is dropped.
    pub async fn acquire_tool(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
        let global = Arc::clone(&self.global_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        let tool = Arc::clone(&self.tool_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        Ok(ConcurrencyPermit {
            _category: tool,
            _global: global,
        })
    }

    /// Try to acquire a tool execution permit without blocking.
    ///
    /// Returns `None` if all tool or global permits are currently held.
    pub fn try_acquire_tool(&self) -> Option<ConcurrencyPermit> {
        let global = Arc::clone(&self.global_semaphore)
            .try_acquire_owned()
            .ok()?;
        match Arc::clone(&self.tool_semaphore).try_acquire_owned() {
            Ok(tool) => Some(ConcurrencyPermit {
                _category: tool,
                _global: global,
            }),
            Err(TryAcquireError::NoPermits) => {
                // Release global permit before returning None
                drop(global);
                None
            }
            Err(TryAcquireError::Closed) => {
                drop(global);
                None
            }
        }
    }

    /// Get current utilization statistics.
    pub fn stats(&self) -> GovernorStats {
        GovernorStats {
            streams_available: self.stream_semaphore.available_permits(),
            streams_max: self.max_streams,
            tools_available: self.tool_semaphore.available_permits(),
            tools_max: self.max_tools,
            global_available: self.global_semaphore.available_permits(),
            global_max: self.max_global,
        }
    }
}

/// RAII guard that holds a category-level permit (stream or tool) and a
/// global-level permit.  Both are released when this value is dropped.
pub struct ConcurrencyPermit {
    _category: OwnedSemaphorePermit,
    _global: OwnedSemaphorePermit,
}

/// Snapshot of governor utilization at a point in time.
#[derive(Debug, Clone)]
pub struct GovernorStats {
    /// Number of stream permits currently available.
    pub streams_available: usize,
    /// Maximum stream permits.
    pub streams_max: usize,
    /// Number of tool permits currently available.
    pub tools_available: usize,
    /// Maximum tool permits.
    pub tools_max: usize,
    /// Number of global permits currently available.
    pub global_available: usize,
    /// Maximum global permits.
    pub global_max: usize,
}

/// Error returned when a semaphore has been closed (should not happen in
/// normal operation).
#[derive(Debug, thiserror::Error)]
pub enum ConcurrencyError {
    #[error("concurrency semaphore was closed unexpectedly")]
    SemaphoreClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_governor_limits_concurrent_operations() {
        let gov = ConcurrencyGovernor::new(2, 2, 4);

        // Acquire two tool permits — should succeed
        let p1 = gov.acquire_tool().await.unwrap();
        let p2 = gov.acquire_tool().await.unwrap();

        // Third acquire should not succeed via try_acquire (at capacity)
        assert!(gov.try_acquire_tool().is_none());

        // Drop one and retry
        drop(p1);
        let p3 = gov.try_acquire_tool();
        assert!(p3.is_some());

        drop(p2);
        drop(p3);
    }

    #[tokio::test]
    async fn test_permits_released_on_drop() {
        let gov = ConcurrencyGovernor::new(1, 1, 2);

        {
            let _permit = gov.acquire_tool().await.unwrap();
            assert_eq!(gov.stats().tools_available, 0);
            assert_eq!(gov.stats().global_available, 1);
        }
        // After drop, permits should be available again
        assert_eq!(gov.stats().tools_available, 1);
        assert_eq!(gov.stats().global_available, 2);
    }

    #[tokio::test]
    async fn test_try_acquire_returns_none_at_capacity() {
        let gov = ConcurrencyGovernor::new(1, 1, 1);

        let _permit = gov.acquire_tool().await.unwrap();
        assert!(gov.try_acquire_tool().is_none());
    }

    #[tokio::test]
    async fn test_stats_report_correct_values() {
        let gov = ConcurrencyGovernor::new(4, 8, 16);

        let stats = gov.stats();
        assert_eq!(stats.streams_available, 4);
        assert_eq!(stats.streams_max, 4);
        assert_eq!(stats.tools_available, 8);
        assert_eq!(stats.tools_max, 8);
        assert_eq!(stats.global_available, 16);
        assert_eq!(stats.global_max, 16);

        // Acquire one stream permit
        let _s = gov.acquire_stream().await.unwrap();
        let stats = gov.stats();
        assert_eq!(stats.streams_available, 3);
        assert_eq!(stats.global_available, 15);

        // Acquire one tool permit
        let _t = gov.acquire_tool().await.unwrap();
        let stats = gov.stats();
        assert_eq!(stats.tools_available, 7);
        assert_eq!(stats.global_available, 14);
    }

    #[tokio::test]
    async fn test_global_limit_caps_total_operations() {
        // Global limit is 2, but stream and tool limits are higher
        let gov = ConcurrencyGovernor::new(4, 4, 2);

        let _p1 = gov.acquire_tool().await.unwrap();
        let _p2 = gov.acquire_stream().await.unwrap();

        // Global is now exhausted — try_acquire should fail
        assert!(gov.try_acquire_tool().is_none());
    }

    #[tokio::test]
    async fn test_default_values() {
        let gov = ConcurrencyGovernor::with_defaults();
        let stats = gov.stats();
        assert_eq!(stats.streams_max, 4);
        assert_eq!(stats.tools_max, 8);
        assert_eq!(stats.global_max, 12);
    }
}
