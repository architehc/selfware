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

use crate::analysis::tier_allocator::ContextTier;

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
mod tests {
    use super::*;

    #[test]
    fn test_throughput_tracker_basic() {
        let tracker = ThroughputTracker::new();
        tracker.record_tokens_in(100);
        tracker.record_tokens_out(50);

        let snap = tracker.snapshot(vec!["agent1".into()]);
        assert!(snap.tokens_in_per_sec >= 0.0);
        assert!(snap.tokens_out_per_sec >= 0.0);
        assert_eq!(snap.total_tokens_session, 150);
        assert_eq!(snap.active_agents, vec!["agent1"]);
    }

    #[test]
    fn test_throughput_tracker_concurrent_requests() {
        let tracker = ThroughputTracker::new();
        assert_eq!(tracker.concurrent_requests(), 0);
        tracker.request_started();
        tracker.request_started();
        assert_eq!(tracker.concurrent_requests(), 2);
        tracker.request_completed();
        assert_eq!(tracker.concurrent_requests(), 1);
    }

    #[test]
    fn test_throughput_tracker_rate_calculation() {
        let tracker = ThroughputTracker::new();

        // First snapshot establishes baseline.
        let _ = tracker.snapshot(vec![]);

        // Record some tokens.
        tracker.record_tokens_in(1000);
        tracker.record_tokens_out(500);

        // Second snapshot should show non-zero rate.
        let snap = tracker.snapshot(vec![]);
        // Rate depends on elapsed time, but totals should be correct.
        assert_eq!(snap.total_tokens_session, 1500);
    }

    #[test]
    fn test_throughput_tracker_default() {
        let tracker = ThroughputTracker::default();
        assert_eq!(tracker.concurrent_requests(), 0);
        assert_eq!(tracker.snapshot(vec![]).total_tokens_session, 0);
    }

    #[tokio::test]
    async fn test_evolution_bus_emit_receive() {
        let bus = EvolutionBus::new(16);
        let mut rx = bus.subscribe();

        bus.emit(EvolutionEvent::AgentFocus {
            agent_id: "coder".into(),
            agent_role: "Coder".into(),
            file_path: "src/main.rs".into(),
            tier: ContextTier::Edit,
        });

        let event = rx.recv().await.unwrap();
        match event {
            EvolutionEvent::AgentFocus {
                agent_id,
                file_path,
                tier,
                ..
            } => {
                assert_eq!(agent_id, "coder");
                assert_eq!(file_path, "src/main.rs");
                assert_eq!(tier, ContextTier::Edit);
            }
            _ => panic!("expected AgentFocus"),
        }
    }

    #[tokio::test]
    async fn test_evolution_bus_multiple_subscribers() {
        let bus = EvolutionBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        assert_eq!(bus.subscriber_count(), 2);

        bus.emit(EvolutionEvent::AgentDefocus {
            agent_id: "test".into(),
        });

        // Both receivers should get the event.
        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(e1, EvolutionEvent::AgentDefocus { .. }));
        assert!(matches!(e2, EvolutionEvent::AgentDefocus { .. }));
    }

    #[tokio::test]
    async fn test_evolution_bus_no_subscribers_does_not_panic() {
        let bus = EvolutionBus::new(16);
        // No subscribers — emit should not panic.
        bus.emit(EvolutionEvent::BuildResult {
            agent_id: "test".into(),
            worktree: "/tmp/wt".into(),
            success: true,
            error_count: 0,
        });
    }

    #[tokio::test]
    async fn test_evolution_bus_throughput_emit() {
        let bus = EvolutionBus::new(16);
        let mut rx = bus.subscribe();

        bus.throughput().record_tokens_in(500);
        bus.emit_throughput(vec!["agent1".into()]);

        let event = rx.recv().await.unwrap();
        match event {
            EvolutionEvent::Throughput(snap) => {
                assert_eq!(snap.total_tokens_session, 500);
                assert_eq!(snap.active_agents, vec!["agent1"]);
            }
            _ => panic!("expected Throughput"),
        }
    }

    #[test]
    fn test_evolution_bus_default() {
        let bus = EvolutionBus::default();
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn test_agent_activity_state_display() {
        assert_eq!(format!("{}", AgentActivityState::Idle), "idle");
        assert_eq!(format!("{}", AgentActivityState::Thinking), "thinking");
        assert_eq!(format!("{}", AgentActivityState::ToolCall), "tool_call");
        assert_eq!(format!("{}", AgentActivityState::Streaming), "streaming");
        assert_eq!(format!("{}", AgentActivityState::Verifying), "verifying");
        assert_eq!(format!("{}", AgentActivityState::Done), "done");
    }

    #[test]
    fn test_bridge_emitter_forwards_to_inner() {
        use super::super::tui_events::{AgentEvent, EventEmitter};
        use std::sync::atomic::AtomicUsize;

        // Custom counter emitter to verify forwarding.
        struct CountingEmitter(AtomicUsize);
        impl EventEmitter for CountingEmitter {
            fn emit(&self, _event: AgentEvent) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let counter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
        let bus = EvolutionBus::new(16);
        let _rx = bus.subscribe();

        let bridge = EvolutionBridgeEmitter::new(bus, "test_agent".into(), counter.clone());

        // Emit a ToolStarted event through the bridge.
        bridge.emit(AgentEvent::ToolStarted {
            name: "file_edit".into(),
        });

        // Inner emitter should have received 1 event.
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);

        // Bus should also have received the translated event.
        // (We'd need tokio runtime to recv, but the send happened synchronously.)
    }

    #[test]
    fn test_tier_entry() {
        let entry = TierEntry {
            file_path: "src/main.rs".into(),
            tier: ContextTier::Edit,
            hops: 0,
        };
        assert_eq!(entry.tier, ContextTier::Edit);
        assert_eq!(entry.hops, 0);
    }

    #[test]
    fn test_throughput_snapshot_fields() {
        let snap = ThroughputSnapshot {
            tokens_in_per_sec: 100.0,
            tokens_out_per_sec: 50.0,
            concurrent_requests: 3,
            active_agents: vec!["a".into(), "b".into()],
            total_tokens_session: 10000,
            timestamp: Instant::now(),
        };
        assert_eq!(snap.concurrent_requests, 3);
        assert_eq!(snap.active_agents.len(), 2);
    }

    #[test]
    fn test_evolution_event_variants() {
        // Ensure all variants can be constructed and Debug-printed.
        let events: Vec<EvolutionEvent> = vec![
            EvolutionEvent::AgentFocus {
                agent_id: "a".into(),
                agent_role: "Coder".into(),
                file_path: "f.rs".into(),
                tier: ContextTier::Edit,
            },
            EvolutionEvent::AgentDefocus {
                agent_id: "a".into(),
            },
            EvolutionEvent::AgentStream {
                agent_id: "a".into(),
                content_preview: "hello".into(),
                tokens: 2,
            },
            EvolutionEvent::ToolInvoked {
                agent_id: "a".into(),
                tool_name: "file_edit".into(),
                target_file: Some("src/lib.rs".into()),
            },
            EvolutionEvent::ToolCompleted {
                agent_id: "a".into(),
                tool_name: "file_edit".into(),
                success: true,
                duration_ms: 42,
            },
            EvolutionEvent::AgentStateChange {
                agent_id: "a".into(),
                state: AgentActivityState::Thinking,
            },
            EvolutionEvent::Throughput(ThroughputSnapshot {
                tokens_in_per_sec: 0.0,
                tokens_out_per_sec: 0.0,
                concurrent_requests: 0,
                active_agents: vec![],
                total_tokens_session: 0,
                timestamp: Instant::now(),
            }),
            EvolutionEvent::TierUpdate {
                focus_node: "main".into(),
                total_files: 1,
                tiers: vec![TierEntry {
                    file_path: "src/main.rs".into(),
                    tier: ContextTier::Edit,
                    hops: 0,
                }],
            },
            EvolutionEvent::BuildResult {
                agent_id: "a".into(),
                worktree: "/tmp/wt".into(),
                success: false,
                error_count: 3,
            },
        ];

        for event in &events {
            let debug = format!("{:?}", event);
            assert!(!debug.is_empty());
        }
    }
}
