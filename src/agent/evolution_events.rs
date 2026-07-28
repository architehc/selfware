//! Evolution event bus: real-time telemetry for the DAG visualization.
//!
//! Extends the existing `AgentEvent` / `EventEmitter` infrastructure with
//! evolution-specific events: agent focus changes, throughput metrics,
//! graph topology updates, and tier assignment changes.
//!
//! The `EvolutionBus` is a broadcast channel that multiple subscribers can
//! listen to (TUI panel, web dashboard, metrics collector).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

/// Context tier assigned to a file based on its DAG distance from the focus
/// node. Consumed by the evolution event bus (`AgentFocus` / `TierUpdate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextTier {
    /// L1: Role + public surface count + dependency list (~50 tokens)
    Describe,
    /// L2: Struct/enum signatures, entry points, error patterns (~200 tokens)
    Work,
    /// L3: Import/export boundaries, trait impls, shared types (~400 tokens)
    Integrate,
    /// L4: Full architecture detail, unsafe blocks, test coverage (~800 tokens)
    Edit,
}

// ─── Event Types ────────────────────────────────────────────────────────────

/// Events emitted during evolution agent execution.
/// These flow through the broadcast bus to all subscribers.
#[derive(Debug, Clone)]
pub enum EvolutionEvent {
    /// Agent started working on a file.
    AgentFocus {
        agent_id: String,
        agent_role: String,
        file_path: String,
        tier: ContextTier,
    },

    /// Agent released focus (idle or moved to different file).
    AgentDefocus { agent_id: String },

    /// Agent produced streaming content.
    AgentStream {
        agent_id: String,
        content_preview: String,
        tokens: usize,
    },

    /// Tool was invoked by an agent.
    ToolInvoked {
        agent_id: String,
        tool_name: String,
        target_file: Option<String>,
    },

    /// Tool completed execution.
    ToolCompleted {
        agent_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },

    /// Agent state changed.
    AgentStateChange {
        agent_id: String,
        state: AgentActivityState,
    },

    /// Throughput snapshot (emitted periodically by the metrics collector).
    Throughput(ThroughputSnapshot),

    /// Tier assignments changed (after focus shift or graph update).
    TierUpdate {
        focus_node: String,
        total_files: usize,
        tiers: Vec<TierEntry>,
    },

    /// Build result from a worktree.
    BuildResult {
        agent_id: String,
        worktree: String,
        success: bool,
        error_count: usize,
    },
}

/// Current activity state of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActivityState {
    /// Waiting for work.
    Idle,
    /// Sending request to LLM, waiting for response.
    Thinking,
    /// Executing a tool call.
    ToolCall,
    /// Streaming response content.
    Streaming,
    /// Build/verify step running.
    Verifying,
    /// Agent completed its task.
    Done,
}

impl std::fmt::Display for AgentActivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Thinking => write!(f, "thinking"),
            Self::ToolCall => write!(f, "tool_call"),
            Self::Streaming => write!(f, "streaming"),
            Self::Verifying => write!(f, "verifying"),
            Self::Done => write!(f, "done"),
        }
    }
}

/// A single file's tier assignment for the TierUpdate event.
#[derive(Debug, Clone)]
pub struct TierEntry {
    pub file_path: String,
    pub tier: ContextTier,
    pub hops: usize,
}

/// Periodic throughput metrics snapshot.
#[derive(Debug, Clone)]
pub struct ThroughputSnapshot {
    pub tokens_in_per_sec: f64,
    pub tokens_out_per_sec: f64,
    pub concurrent_requests: usize,
    pub active_agents: Vec<String>,
    pub total_tokens_session: u64,
    pub timestamp: Instant,
}

// ─── Throughput Tracker ─────────────────────────────────────────────────────

/// Thread-safe counters for computing tokens/sec.
/// Call `record_tokens_in` / `record_tokens_out` from streaming callbacks,
/// then `snapshot()` on a 1-second timer to get the rate.
#[derive(Debug)]
pub struct ThroughputTracker {
    /// Tokens received from LLM (prompt echoes + completion).
    tokens_in: AtomicU64,
    /// Tokens sent to LLM (prompt tokens).
    tokens_out: AtomicU64,
    /// Currently in-flight requests.
    concurrent_requests: AtomicUsize,
    /// Session lifetime total.
    total_tokens: AtomicU64,
    /// Last snapshot timestamp.
    last_snapshot: std::sync::Mutex<Instant>,
    /// Tokens in at last snapshot.
    last_in: AtomicU64,
    /// Tokens out at last snapshot.
    last_out: AtomicU64,
}

impl ThroughputTracker {
    pub fn new() -> Self {
        Self {
            tokens_in: AtomicU64::new(0),
            tokens_out: AtomicU64::new(0),
            concurrent_requests: AtomicUsize::new(0),
            total_tokens: AtomicU64::new(0),
            last_snapshot: std::sync::Mutex::new(Instant::now()),
            last_in: AtomicU64::new(0),
            last_out: AtomicU64::new(0),
        }
    }

    /// Record incoming tokens (completion/response tokens).
    pub fn record_tokens_in(&self, count: usize) {
        self.tokens_in.fetch_add(count as u64, Ordering::Relaxed);
        self.total_tokens.fetch_add(count as u64, Ordering::Relaxed);
    }

    /// Record outgoing tokens (prompt tokens sent to LLM).
    pub fn record_tokens_out(&self, count: usize) {
        self.tokens_out.fetch_add(count as u64, Ordering::Relaxed);
        self.total_tokens.fetch_add(count as u64, Ordering::Relaxed);
    }

    /// Increment concurrent request count (call when request starts).
    pub fn request_started(&self) {
        self.concurrent_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement concurrent request count (call when request completes).
    pub fn request_completed(&self) {
        self.concurrent_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Current concurrent request count.
    pub fn concurrent_requests(&self) -> usize {
        self.concurrent_requests.load(Ordering::Relaxed)
    }

    /// Compute a throughput snapshot. Call this on a 1-second timer.
    pub fn snapshot(&self, active_agents: Vec<String>) -> ThroughputSnapshot {
        let now = Instant::now();
        let current_in = self.tokens_in.load(Ordering::Relaxed);
        let current_out = self.tokens_out.load(Ordering::Relaxed);

        let (elapsed_secs, prev_in, prev_out) = {
            let mut last = self.last_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            let elapsed = now.duration_since(*last).as_secs_f64().max(0.001);
            let prev_in = self.last_in.swap(current_in, Ordering::Relaxed);
            let prev_out = self.last_out.swap(current_out, Ordering::Relaxed);
            *last = now;
            (elapsed, prev_in, prev_out)
        };

        let delta_in = current_in.saturating_sub(prev_in) as f64;
        let delta_out = current_out.saturating_sub(prev_out) as f64;

        ThroughputSnapshot {
            tokens_in_per_sec: delta_in / elapsed_secs,
            tokens_out_per_sec: delta_out / elapsed_secs,
            concurrent_requests: self.concurrent_requests.load(Ordering::Relaxed),
            active_agents,
            total_tokens_session: self.total_tokens.load(Ordering::Relaxed),
            timestamp: now,
        }
    }
}

impl Default for ThroughputTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Broadcast Bus ──────────────────────────────────────────────────────────

/// The evolution event bus. Wraps a `tokio::sync::broadcast` channel.
///
/// Producers call `emit()`. Consumers call `subscribe()` to get a receiver.
/// Slow consumers that fall behind the buffer will miss events (lossy, not blocking).
#[derive(Clone)]
pub struct EvolutionBus {
    tx: broadcast::Sender<EvolutionEvent>,
    throughput: Arc<ThroughputTracker>,
}

impl EvolutionBus {
    /// Create a new bus with the given buffer capacity.
    /// 256 is enough for ~4 agents at 60 events/sec with 1-second consumer lag.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            throughput: Arc::new(ThroughputTracker::new()),
        }
    }

    /// Emit an event to all subscribers.
    pub fn emit(&self, event: EvolutionEvent) {
        // Ignore send errors (no subscribers = nobody cares).
        let _ = self.tx.send(event);
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<EvolutionEvent> {
        self.tx.subscribe()
    }

    /// Get a reference to the throughput tracker for recording token counts.
    pub fn throughput(&self) -> &ThroughputTracker {
        &self.throughput
    }

    /// Emit a throughput snapshot. Call this on a 1-second timer.
    pub fn emit_throughput(&self, active_agents: Vec<String>) {
        let snapshot = self.throughput.snapshot(active_agents);
        self.emit(EvolutionEvent::Throughput(snapshot));
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EvolutionBus {
    fn default() -> Self {
        Self::new(256)
    }
}

// ─── Bridge to existing EventEmitter ────────────────────────────────────────

/// An `EventEmitter` implementation that forwards relevant `AgentEvent`s
/// to the `EvolutionBus` as `EvolutionEvent`s.
///
/// This bridges the existing agent event infrastructure with the new bus
/// without modifying any existing emit call sites.
pub struct EvolutionBridgeEmitter {
    /// The evolution bus to forward events to.
    bus: EvolutionBus,
    /// Agent ID to tag events with.
    agent_id: String,
    /// Inner emitter to delegate to (e.g. TuiEmitter or NoopEmitter).
    inner: Arc<dyn super::tui_events::EventEmitter>,
}

impl EvolutionBridgeEmitter {
    pub fn new(
        bus: EvolutionBus,
        agent_id: String,
        inner: Arc<dyn super::tui_events::EventEmitter>,
    ) -> Self {
        Self {
            bus,
            agent_id,
            inner,
        }
    }
}

impl super::tui_events::EventEmitter for EvolutionBridgeEmitter {
    fn emit(&self, event: super::tui_events::AgentEvent) {
        // Forward to the inner emitter (TUI or noop) unchanged.
        self.inner.emit(event.clone());

        // Also translate relevant events to EvolutionEvents.
        match event {
            super::tui_events::AgentEvent::ToolStarted { name } => {
                self.bus.emit(EvolutionEvent::ToolInvoked {
                    agent_id: self.agent_id.clone(),
                    tool_name: name,
                    target_file: None,
                });
                self.bus.emit(EvolutionEvent::AgentStateChange {
                    agent_id: self.agent_id.clone(),
                    state: AgentActivityState::ToolCall,
                });
            }
            super::tui_events::AgentEvent::ToolCompleted {
                name,
                success,
                duration_ms,
            } => {
                self.bus.emit(EvolutionEvent::ToolCompleted {
                    agent_id: self.agent_id.clone(),
                    tool_name: name,
                    success,
                    duration_ms,
                });
            }
            super::tui_events::AgentEvent::TokenUsage {
                prompt_tokens,
                completion_tokens,
            } => {
                self.bus
                    .throughput()
                    .record_tokens_out(prompt_tokens as usize);
                self.bus
                    .throughput()
                    .record_tokens_in(completion_tokens as usize);
            }
            super::tui_events::AgentEvent::AssistantDelta { ref text } => {
                self.bus.emit(EvolutionEvent::AgentStream {
                    agent_id: self.agent_id.clone(),
                    content_preview: text.chars().take(80).collect(),
                    tokens: text.len() / 4, // rough estimate
                });
            }
            super::tui_events::AgentEvent::Started => {
                self.bus.emit(EvolutionEvent::AgentStateChange {
                    agent_id: self.agent_id.clone(),
                    state: AgentActivityState::Thinking,
                });
            }
            super::tui_events::AgentEvent::Completed { .. } => {
                self.bus.emit(EvolutionEvent::AgentStateChange {
                    agent_id: self.agent_id.clone(),
                    state: AgentActivityState::Done,
                });
            }
            _ => {} // Other events don't map to evolution events
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/unit/agent/evolution_events/evolution_events_test.rs"]
mod tests;
