//! Multi-Agent Collaboration Example
//!
//! This example demonstrates the multi-agent swarm system in Selfware.
//! Multiple specialized agents work together concurrently to solve a task.
//!
//! # Features Demonstrated
//!
//! - Concurrent agent execution (up to 16 agents)
//! - Role-based specialization (Architect, Coder, Tester, Reviewer)
//! - Event-driven progress tracking
//! - Result aggregation from multiple perspectives
//!
//! # Running this example
//!
//! ```bash
//! cargo run --example multi_agent
//! ```
//!
//! # Agent Roles
//!
//! - **Architect**: Designs high-level structure and patterns
//! - **Coder**: Implements features with clean, efficient code
//! - **Tester**: Creates comprehensive test cases
//! - **Reviewer**: Evaluates code quality and security
//! - **Documenter**: Writes clear documentation
//! - **Security**: Identifies vulnerabilities
//! - **Performance**: Optimizes for speed and efficiency

use anyhow::Result;
use selfware::config::Config;
use selfware::multiagent::{MultiAgentChat, MultiAgentConfig, MultiAgentEvent};
use selfware::swarm::AgentRole;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Selfware Multi-Agent Example ===\n");

    // Load configuration
    let config = Config::load(None)?;

    println!("Endpoint: {}", config.endpoint);
    println!("Model: {}", config.model);
    println!();

    // Configure the multi-agent system
    let agent_config = MultiAgentConfig {
        // Run 4 agents concurrently
        max_concurrency: 4,

        // Assign specialized roles
        roles: vec![
            AgentRole::Architect, // Designs the solution
            AgentRole::Coder,     // Implements the code
            AgentRole::Tester,    // Creates test cases
            AgentRole::Reviewer,  // Reviews for quality
        ],

        // Enable streaming for real-time output
        streaming: true,

        // Timeout per agent (2 minutes)
        timeout_secs: 120,

        // Generation parameters
        temperature: 0.7,
        max_tokens: 4096,

        // Failure policy
        failure_policy: selfware::multiagent::MultiAgentFailurePolicy::BestEffort,
    };

    println!("Multi-Agent Configuration:");
    println!("  Max concurrency: {}", agent_config.max_concurrency);
    println!(
        "  Roles: {:?}",
        agent_config
            .roles
            .iter()
            .map(|r| r.name())
            .collect::<Vec<_>>()
    );
    println!("  Timeout: {}s per agent", agent_config.timeout_secs);
    println!();

    // Create event channel for progress tracking
    let (event_tx, mut event_rx) = mpsc::channel::<MultiAgentEvent>(1000);

    // Create the multi-agent chat system
    let multi_agent = MultiAgentChat::new(&config, agent_config)?.with_events(event_tx);

    // Define the task for all agents
    let task = r#"
        Design and review a Rust function that finds all prime numbers up to N
        using the Sieve of Eratosthenes algorithm.

        Consider:
        - API design and function signature
        - Implementation efficiency
        - Edge cases and error handling
        - Test cases for validation
    "#;

    println!("Task:\n{}\n", task.trim());
    println!("--- Running Multi-Agent Task ---\n");

    // Spawn event handler task
    let event_handler = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                MultiAgentEvent::AgentStarted { name, task, .. } => {
                    println!(
                        "[START] {} - Working on: {}...",
                        name,
                        &task[..40.min(task.len())]
                    );
                }
                MultiAgentEvent::AgentToolCall { agent_id, tool } => {
                    println!("[TOOL]  Agent-{} calling: {}", agent_id, tool);
                }
                MultiAgentEvent::AgentCompleted { result, .. } => {
                    let status = if result.success { "OK" } else { "FAILED" };
                    println!(
                        "[DONE]  {} ({}) - {} in {:.2}s",
                        result.agent_name,
                        result.role.name(),
                        status,
                        result.duration.as_secs_f64()
                    );
                }
                MultiAgentEvent::AgentFailed { agent_id, error } => {
                    println!("[ERROR] Agent-{} failed: {}", agent_id, error);
                }
                MultiAgentEvent::AllCompleted {
                    results,
                    total_duration,
                } => {
                    let success_count = results.iter().filter(|r| r.success).count();
                    println!(
                        "\n[SUMMARY] {}/{} agents completed in {:.2}s",
                        success_count,
                        results.len(),
                        total_duration.as_secs_f64()
                    );
                    break;
                }
                _ => {}
            }
        }
    });

    // Run the task across all agents
    let results = multi_agent.run_task(task).await?;

    // Wait for event handler to finish
    let _ = event_handler.await;

    // Display aggregated results
    println!("\n--- Agent Responses ---\n");

    for result in &results {
        if result.success {
            println!("=== {} ({}) ===", result.agent_name, result.role.name());
            println!();

            // Show a preview of the response (first 500 chars)
            let preview = if result.content.len() > 500 {
                // UTF-8 safe truncation
                let mut end = 500;
                while end > 0 && !result.content.is_char_boundary(end) {
                    end -= 1;
                }
                format!(
                    "{}...\n[{} more characters]",
                    &result.content[..end],
                    result.content.len() - end
                )
            } else {
                result.content.clone()
            };
            println!("{}", preview);
            println!();
        } else {
            println!("=== {} (FAILED) ===", result.agent_name);
            if let Some(ref error) = result.error {
                println!("Error: {}", error);
            }
            println!();
        }
    }

    // Generate aggregated summary
    let summary = MultiAgentChat::aggregate_results(&results);
    println!("--- Aggregated Summary ---\n");
    println!(
        "Total response length: {} characters across {} agents",
        summary.len(),
        results.len()
    );

    Ok(())
}
