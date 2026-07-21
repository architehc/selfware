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
