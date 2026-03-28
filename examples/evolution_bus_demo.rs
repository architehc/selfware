//! Demo: Evolution event bus receiving live agent telemetry.
//!
//! Creates an EvolutionBus, attaches it to a mock scenario, and prints
//! events as they arrive. This demonstrates the full pipeline:
//!
//!   Agent emit_event(AgentEvent) → EvolutionBridgeEmitter → EvolutionBus → subscriber
//!
//! Usage: cargo run --example evolution_bus_demo

use selfware::agent::evolution_events::{
    AgentActivityState, EvolutionBus, EvolutionEvent,
};
use selfware::agent::tui_events::{AgentEvent, EventEmitter, NoopEmitter};
use selfware::agent::evolution_events::EvolutionBridgeEmitter;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    println!("=== Evolution Event Bus Demo ===\n");

    // 1. Create the bus
    let bus = EvolutionBus::new(64);
    let mut rx = bus.subscribe();
    println!("Bus created, 1 subscriber\n");

    // 2. Create a bridge emitter (wraps NoopEmitter as inner)
    let bridge = Arc::new(EvolutionBridgeEmitter::new(
        bus.clone(),
        "demo-agent".to_string(),
        Arc::new(NoopEmitter),
    ));

    // 3. Simulate agent events (what the real agent loop produces)
    let events_to_simulate = vec![
        AgentEvent::Started,
        AgentEvent::ToolStarted {
            name: "file_read".to_string(),
        },
        AgentEvent::ToolCompleted {
            name: "file_read".to_string(),
            success: true,
            duration_ms: 12,
        },
        AgentEvent::AssistantDelta {
            text: "I'll fix the bug in config/loader.rs by...".to_string(),
        },
        AgentEvent::TokenUsage {
            prompt_tokens: 4500,
            completion_tokens: 320,
        },
        AgentEvent::ToolStarted {
            name: "file_edit".to_string(),
        },
        AgentEvent::ToolCompleted {
            name: "file_edit".to_string(),
            success: true,
            duration_ms: 45,
        },
        AgentEvent::Completed {
            message: "Bug fixed successfully".to_string(),
        },
    ];

    // 4. Emit simulated events
    for event in &events_to_simulate {
        bridge.emit(event.clone());
    }

    // 5. Also emit a throughput snapshot
    bus.throughput().record_tokens_in(320);
    bus.throughput().record_tokens_out(4500);
    bus.emit_throughput(vec!["demo-agent".to_string()]);

    // 6. Drain and print all received EvolutionEvents
    println!("Received EvolutionEvents:");
    println!("{:-<60}", "");
    let mut count = 0;
    while let Ok(event) = rx.try_recv() {
        count += 1;
        match &event {
            EvolutionEvent::AgentStateChange { agent_id, state } => {
                println!("  [{:>2}] {} → {}", count, agent_id, state);
            }
            EvolutionEvent::ToolInvoked {
                agent_id,
                tool_name,
                ..
            } => {
                println!("  [{:>2}] {} called tool: {}", count, agent_id, tool_name);
            }
            EvolutionEvent::ToolCompleted {
                agent_id,
                tool_name,
                success,
                duration_ms,
            } => {
                let status = if *success { "ok" } else { "FAIL" };
                println!(
                    "  [{:>2}] {} tool done: {} ({}) {}ms",
                    count, agent_id, tool_name, status, duration_ms
                );
            }
            EvolutionEvent::AgentStream {
                agent_id,
                content_preview,
                tokens,
            } => {
                println!(
                    "  [{:>2}] {} stream: \"{}\" (~{} tok)",
                    count, agent_id, content_preview, tokens
                );
            }
            EvolutionEvent::Throughput(snap) => {
                println!(
                    "  [{:>2}] throughput: {:.0} tok/s in, {:.0} tok/s out, {} total",
                    count,
                    snap.tokens_in_per_sec,
                    snap.tokens_out_per_sec,
                    snap.total_tokens_session
                );
            }
            other => {
                println!("  [{:>2}] {:?}", count, other);
            }
        }
    }
    println!("{:-<60}", "");
    println!("\nTotal events received: {}", count);
    println!(
        "Throughput tracker: {} total tokens",
        bus.throughput().concurrent_requests()
    );
}
