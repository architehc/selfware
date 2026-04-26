//! Agent Swarm UI Demo
//!
//! This example demonstrates the Qwen Code CLI-inspired UI for the agent swarm system.
//!
//! Run with:
//! ```bash
//! cargo run --example swarm_ui_demo --features tui
//! ```

#[cfg(feature = "tui")]
use selfware::ui::tui::run_tui_swarm;

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

