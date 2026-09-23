//! Concurrency governor for limiting concurrent streaming and tool execution.
//!
//! Provides [`ConcurrencyGovernor`] which uses layered semaphores to bound:
//! - Concurrent LLM streaming responses
//! - Concurrent tool executions per agent
//! - Total inflight operations globally
//!
//! All permits are RAII guards that release automatically on drop.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Process-wide governor cache, keyed by the three limits.
///
/// The endpoint's concurrency limit is a property of the *server*, not of an
/// agent. A per-Agent governor let each swarm/multiagent child hold its own
/// `max_streams`, so N agents put N×`max_streams` streams against a server with
/// a fixed slot count — exactly the oversubscription that surfaced as server
/// queueing and request timeouts. Agents that ask for the same limits now share
/// one budget; distinct limits still get distinct governors so a deliberately
/// different config is not silently merged into another's ceiling.
static SHARED_GOVERNORS: LazyLock<Mutex<HashMap<(usize, usize, usize), Arc<ConcurrencyGovernor>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Governs concurrency across streaming, tool execution, and global operations.
///
/// Uses three independent semaphore layers so that, for example, a burst of
/// tool executions cannot starve streaming slots and vice-versa.  The global
/// semaphore acts as an upper ceiling across both categories.
///
/// **The acquisition order is global-then-category, and that single total
/// order is what keeps the governor deadlock-free under nested acquisition.**
/// A stream task that dispatches a tool or a nested stream acquires its second
/// permit through the SAME order, so no task can ever hold a category permit
/// while waiting on the global semaphore that another task holds — the
/// category↔global cyclic wait that the 2026-09-21 review found (global
/// permits saturated by streams whose tool/nested-stream dispatch then parks
/// against a category held by a task parked on global) cannot form.
///
/// The tradeoff, inherited from the category-first design, is inverted: a
/// caller that waits on its category semaphore now holds a global permit while
/// parked, so a sustained stream backlog can head-of-line block the tool pool
/// on the global ceiling. That is a scheduling cost, not a correctness one —
/// every parked waiter still releases its global permit the moment it proceeds,
/// and the alternative (category-first) is exactly the ordering that deadlocks
/// under nested dispatch.
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

    /// Create a governor from a [`ConcurrencyConfig`](crate::config::ConcurrencyConfig).
    ///
    /// As a safety net, all values are clamped to a minimum of 1 to prevent
    /// semaphore deadlocks even if validation was bypassed.
    pub fn from_config(cfg: &crate::config::ConcurrencyConfig) -> Self {
        Self::new(
            cfg.max_streams.max(1),
            cfg.max_tools.max(1),
            cfg.max_global.max(1),
        )
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

    /// The process-wide governor for these limits.
    ///
    /// Every caller in this process asking for the same limits shares one
    /// governor, so `max_streams` bounds the whole process rather than each
    /// agent — see `SHARED_GOVERNORS`. Use [`Self::new`] for an isolated
    /// governor (tests, tooling that wants its own budget).
    pub fn shared(max_streams: usize, max_tools: usize, max_global: usize) -> Arc<Self> {
        let key = (max_streams.max(1), max_tools.max(1), max_global.max(1));
        let mut cache = SHARED_GOVERNORS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(
            cache
                .entry(key)
                .or_insert_with(|| Arc::new(Self::new(key.0, key.1, key.2))),
        )
    }

    /// [`Self::shared`] from a [`ConcurrencyConfig`](crate::config::ConcurrencyConfig).
    pub fn shared_from_config(cfg: &crate::config::ConcurrencyConfig) -> Arc<Self> {
        Self::shared(cfg.max_streams, cfg.max_tools, cfg.max_global)
    }

    /// Acquire a stream permit, waiting if none are currently available.
    ///
    /// Returns a [`ConcurrencyPermit`] that holds both a stream-level and a
    /// global-level permit.  Both are released when the permit is dropped.
    ///
    /// The global permit is acquired FIRST, then the category permit — the
    /// single total order that keeps nested acquisitions deadlock-free (see
    /// the ordering note on [`ConcurrencyGovernor`]). If the category
    /// semaphore is closed while the caller holds the global permit, the
    /// global permit is returned before the error surfaces.
    pub async fn acquire_stream(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
        let global = Arc::clone(&self.global_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        match Arc::clone(&self.stream_semaphore).acquire_owned().await {
            Ok(stream) => Ok(ConcurrencyPermit {
                _category: stream,
                _global: global,
            }),
            // `global` drops here, returning the permit to the pool.
            Err(_) => Err(ConcurrencyError::SemaphoreClosed),
        }
    }

    /// Acquire a tool execution permit, waiting if none are currently available.
    ///
    /// Returns a [`ConcurrencyPermit`] that holds both a tool-level and a
    /// global-level permit.  Both are released when the permit is dropped.
    ///
    /// Global first, then category — the same total order as
    /// [`Self::acquire_stream`] (see the ordering note on
    /// [`ConcurrencyGovernor`]).
    pub async fn acquire_tool(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
        let global = Arc::clone(&self.global_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        match Arc::clone(&self.tool_semaphore).acquire_owned().await {
            Ok(tool) => Ok(ConcurrencyPermit {
                _category: tool,
                _global: global,
            }),
            // `global` drops here, returning the permit to the pool.
            Err(_) => Err(ConcurrencyError::SemaphoreClosed),
        }
    }

    /// Try to acquire a tool execution permit without blocking.
    ///
    /// Returns `None` if all tool or global permits are currently held.
    ///
    /// Global first, then the category permit — and when the category ceiling
    /// refuses the request, the global permit is dropped again, so a failed
    /// try never leaks a global slot (the global limit still bounds this path
    /// and the refuse path stays leak-free, which
    /// `test_try_acquire_stream_is_non_blocking_and_leak_free` pins).
    pub fn try_acquire_tool(&self) -> Option<ConcurrencyPermit> {
        let global = Arc::clone(&self.global_semaphore)
            .try_acquire_owned()
            .ok()?;
        match Arc::clone(&self.tool_semaphore).try_acquire_owned() {
            Ok(tool) => Some(ConcurrencyPermit {
                _category: tool,
                _global: global,
            }),
            // `global` drops here, returning the permit to the pool.
            Err(_) => None,
        }
    }

    /// Try to acquire a stream permit without blocking.
    ///
    /// Symmetric counterpart to [`Self::try_acquire_tool`]: a caller that must
    /// stay non-blocking (a status refresh, a best-effort call) can probe the
    /// stream pool instead of parking on it. Global first, then category, with
    /// the global permit released when the category ceiling refuses.
    pub fn try_acquire_stream(&self) -> Option<ConcurrencyPermit> {
        let global = Arc::clone(&self.global_semaphore)
            .try_acquire_owned()
            .ok()?;
        match Arc::clone(&self.stream_semaphore).try_acquire_owned() {
            Ok(stream) => Some(ConcurrencyPermit {
                _category: stream,
                _global: global,
            }),
            // `global` drops here, returning the permit to the pool.
            Err(_) => None,
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
#[path = "../tests/unit/concurrency/concurrency_test.rs"]
mod tests;
