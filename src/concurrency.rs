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
/// **The acquisition order is category-then-global, and that order is what
/// makes the isolation above real.** A caller that takes the global permit
/// first holds it while parked on its category semaphore, so a burst of stream
/// requesters consumes the whole global budget with *waiters* and leaves a
/// completely idle tool pool unreachable until a stream happens to finish.
/// Taking the category permit first means only admitted operations ever hold a
/// global permit, so the global limit behaves as a ceiling on inflight work
/// rather than a reservation parked on by whoever queued first.
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
    /// agent — see [`SHARED_GOVERNORS`]. Use [`Self::new`] for an isolated
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
    /// The stream permit is acquired first: a caller parked here holds no
    /// global permit, so it cannot starve the tool category (see the ordering
    /// note on [`ConcurrencyGovernor`]).
    pub async fn acquire_stream(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
        let stream = Arc::clone(&self.stream_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        let global = Arc::clone(&self.global_semaphore)
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
    ///
    /// The tool permit is acquired first, for the same reason as
    /// [`Self::acquire_stream`].
    pub async fn acquire_tool(&self) -> Result<ConcurrencyPermit, ConcurrencyError> {
        let tool = Arc::clone(&self.tool_semaphore)
            .acquire_owned()
            .await
            .map_err(|_| ConcurrencyError::SemaphoreClosed)?;
        let global = Arc::clone(&self.global_semaphore)
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
    ///
    /// Category first, then global — and the category permit is dropped again
    /// when the global ceiling rejects the request, so a failed try never
    /// leaks a tool slot (the global limit still bounds this path, which
    /// `test_global_limit_caps_total_operations` pins).
    pub fn try_acquire_tool(&self) -> Option<ConcurrencyPermit> {
        let tool = Arc::clone(&self.tool_semaphore).try_acquire_owned().ok()?;
        match Arc::clone(&self.global_semaphore).try_acquire_owned() {
            Ok(global) => Some(ConcurrencyPermit {
                _category: tool,
                _global: global,
            }),
            // `tool` drops here, releasing the category permit.
            Err(_) => None,
        }
    }

    /// Try to acquire a stream permit without blocking.
    ///
    /// Symmetric counterpart to [`Self::try_acquire_tool`]: a caller that must
    /// stay non-blocking (a status refresh, a best-effort call) can probe the
    /// stream pool instead of parking on it. Category first, then global, with
    /// the category permit released when the global ceiling refuses.
    pub fn try_acquire_stream(&self) -> Option<ConcurrencyPermit> {
        let stream = Arc::clone(&self.stream_semaphore)
            .try_acquire_owned()
            .ok()?;
        match Arc::clone(&self.global_semaphore).try_acquire_owned() {
            Ok(global) => Some(ConcurrencyPermit {
                _category: stream,
                _global: global,
            }),
            // `stream` drops here, releasing the category permit.
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
