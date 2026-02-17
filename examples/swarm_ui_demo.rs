//! Agent Swarm UI Demo
//!
//! This example demonstrates the Qwen Code CLI-inspired UI for the agent swarm system.
//!
//! Run with:
//! ```bash
//! cargo run --example swarm_ui_demo --features tui
//! ```

#[cfg(feature = "tui")]
use selfware::orchestration::swarm::{Agent, AgentRole, Swarm};
#[cfg(feature = "tui")]
use selfware::ui::tui::{run_tui_swarm, run_tui_swarm_with_roles};

#[cfg(feature = "tui")]
fn main() -> anyhow::Result<()> {
    println!("🤖 Selfware Agent Swarm UI Demo");
    println!("================================\n");
    println!("This demo showcases the Qwen Code CLI-inspired UI for");
    println!("visualizing and interacting with agent swarms.\n");

    // Option 1: Use default dev swarm (4 agents: Architect, Coder, Tester, Reviewer)
    println!("Starting with default development swarm...");
    println!("Press '?' for help, 'q' to quit\n");

    run_tui_swarm()
}

#[cfg(not(feature = "tui"))]
fn main() {
    eprintln!("This example requires the 'tui' feature.");
    eprintln!("Run with: cargo run --example swarm_ui_demo --features tui");
    std::process::exit(1);
}

/// Example: Create a custom security-focused swarm
#[cfg(feature = "tui")]
#[allow(dead_code)]
fn run_security_swarm() -> anyhow::Result<()> {
    let roles = vec![
        AgentRole::Security,
        AgentRole::Security,
        AgentRole::Reviewer,
        AgentRole::Architect,
        AgentRole::Tester,
    ];

    run_tui_swarm_with_roles(roles)
}

/// Example: Create a custom swarm programmatically
#[cfg(feature = "tui")]
#[allow(dead_code)]
fn run_custom_swarm() -> anyhow::Result<()> {
    use std::sync::{Arc, RwLock};

    let mut swarm = Swarm::new();

    // Add specialized agents with expertise
    swarm.add_agent(
        Agent::new("Alice", AgentRole::Architect)
            .with_expertise("Microservices")
            .with_expertise("Event Sourcing"),
    );

    swarm.add_agent(
        Agent::new("Bob", AgentRole::Coder)
            .with_expertise("Rust")
            .with_expertise("Async Programming"),
    );

    swarm.add_agent(
        Agent::new("Carol", AgentRole::Tester)
            .with_expertise("Property-based Testing")
            .with_expertise("TDD"),
    );

    swarm.add_agent(
        Agent::new("Dave", AgentRole::Reviewer)
            .with_expertise("Code Quality")
            .with_expertise("Performance"),
    );

    swarm.add_agent(
        Agent::new("Eve", AgentRole::Security)
            .with_expertise("Cryptography")
            .with_expertise("OWASP"),
    );

    // Create a custom swarm UI (would need additional implementation)
    let _swarm = Arc::new(RwLock::new(swarm));

    println!("Custom swarm created with {} agents", 5);

    // For now, fall back to default
    run_tui_swarm()
}
