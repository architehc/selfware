//! Multi-Agent Interactive Mode
//!
//! Interactive CLI for the multi-agent chat system.

use std::io::{self, Write};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::swarm::{
    create_dev_swarm, AgentRole, ConflictStrategy, DecisionStatus, Swarm, SwarmTask,
};

use super::chat::MultiAgentChat;
use super::config::MultiAgentConfig;
use super::types::{AgentInstance, AgentStatus, MultiAgentEvent, MAX_CONCURRENT_AGENTS};

impl MultiAgentChat {
    /// Run interactive multi-agent chat
    pub async fn interactive(&mut self) -> Result<()> {
        println!("{}", "🤖 Multi-Agent Chat System".bright_cyan().bold());
        println!(
            "Agents: {} | Max Concurrency: {}",
            self.config.roles.len(),
            self.config.max_concurrency
        );
        println!("Type 'exit' to quit, '/help' for commands\n");

        self.initialize_agents().await?;

        loop {
            print!("{} ", "🤖 ❯".bright_green());
            io::stdout().flush()?;

            let mut input = String::new();
            // Use block_in_place to prevent blocking the async runtime
            if tokio::task::block_in_place(|| io::stdin().read_line(&mut input)).is_err() {
                continue;
            }

            let input = input.trim();

            if matches!(input, "exit" | "quit" | "/exit" | "/quit" | "q" | "/q") {
                break;
            }

            if input == "/help" {
                println!("Commands:");
                println!("  /help        - Show this help");
                println!("  /agents      - List active agents");
                println!("  /status      - Show system status");
                println!("  /parallel N  - Set max concurrency (1-16)");
                println!("  /add <role>  - Add an agent (coder/tester/reviewer/etc)");
                println!("  /remove N    - Remove agent by ID");
                println!("  /clear       - Reset all agents");
                println!("  exit         - Exit chat");
                continue;
            }

            if input == "/agents" {
                let agents = self.agents.read().await;
                println!("Active agents:");
                for agent in agents.iter() {
                    println!(
                        "  [{:2}] {} ({}) - {:?}",
                        agent.id,
                        agent.name,
                        agent.role.name(),
                        agent.status
                    );
                }
                continue;
            }

            if input == "/status" {
                let agents = self.agents.read().await;
                let results = self.results.lock().await;
                println!("Status:");
                println!("  Agents: {}", agents.len());
                println!("  Max Concurrency: {}", self.config.max_concurrency);
                println!("  Completed Tasks: {}", results.len());
                continue;
            }

            if input.starts_with("/parallel ") {
                if let Some(value) = input.strip_prefix("/parallel ").map(str::trim) {
                    if let Ok(n) = value.parse::<usize>() {
                        let n = n.clamp(1, MAX_CONCURRENT_AGENTS);
                        self.config.max_concurrency = n;
                        self.semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(n));
                        println!("Max concurrency set to {}", n);
                    }
                } else {
                    println!("Usage: /parallel <1-{ }>", MAX_CONCURRENT_AGENTS);
                }
                continue;
            }

            if input.starts_with("/add ") {
                let Some(role_str) = input.strip_prefix("/add ").map(str::trim) else {
                    println!("Usage: /add <role>");
                    continue;
                };
                let role_str = role_str.to_lowercase();
                let role = match role_str.as_str() {
                    "architect" => Some(AgentRole::Architect),
                    "coder" => Some(AgentRole::Coder),
                    "tester" => Some(AgentRole::Tester),
                    "reviewer" => Some(AgentRole::Reviewer),
                    "documenter" => Some(AgentRole::Documenter),
                    "devops" => Some(AgentRole::DevOps),
                    "security" => Some(AgentRole::Security),
                    "performance" => Some(AgentRole::Performance),
                    "general" => Some(AgentRole::General),
                    _ => None,
                };
                if let Some(role) = role {
                    let mut agents = self.agents.write().await;
                    let id = agents.len();
                    agents.push(AgentInstance {
                        id,
                        role,
                        name: format!("Agent-{}-{}", id, role.name()),
                        messages: vec![crate::api::types::Message::system(role.system_prompt())],
                        status: AgentStatus::Idle,
                        last_heartbeat: Instant::now(),
                    });
                    println!("Added Agent-{}-{}", id, role.name());
                } else {
                    println!("Unknown role. Available: architect, coder, tester, reviewer, documenter, devops, security, performance, general");
                }
                continue;
            }

            if input.starts_with("/remove ") {
                if let Some(value) = input.strip_prefix("/remove ").map(str::trim) {
                    if let Ok(id) = value.parse::<usize>() {
                        let mut agents = self.agents.write().await;
                        if id < agents.len() {
                            let removed = agents.remove(id);
                            // Re-index remaining agents
                            for (i, agent) in agents.iter_mut().enumerate() {
                                agent.id = i;
                            }
                            println!("Removed {}", removed.name);
                        } else {
                            println!("Invalid agent ID");
                        }
                    }
                } else {
                    println!("Usage: /remove <id>");
                }
                continue;
            }

            if input == "/clear" {
                self.initialize_agents().await?;
                let mut results = self.results.lock().await;
                results.clear();
                println!("All agents reset");
                continue;
            }

            if input.is_empty() {
                continue;
            }

            // Run task across all agents
            println!("{}", "Running task across all agents...".bright_yellow());

            let start = Instant::now();

            // Create event channel for this run
            let (tx, mut rx) = mpsc::channel::<MultiAgentEvent>(1000);
            self.event_tx = Some(tx);

            // Spawn event handler
            let handle = tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        MultiAgentEvent::AgentStarted { name, .. } => {
                            println!("  {} {} started", "▶".bright_blue(), name);
                        }
                        MultiAgentEvent::AgentToolCall { agent_id, tool } => {
                            println!(
                                "  {} Agent-{} calling {}",
                                "🔧".bright_yellow(),
                                agent_id,
                                tool
                            );
                        }
                        MultiAgentEvent::AgentCompleted { result, .. } => {
                            let status = if result.success {
                                "✓".bright_green()
                            } else {
                                "✗".bright_red()
                            };
                            println!(
                                "  {} {} completed in {:.2}s",
                                status,
                                result.agent_name,
                                result.duration.as_secs_f64()
                            );
                        }
                        MultiAgentEvent::AgentFailed { agent_id, error } => {
                            println!(
                                "  {} Agent-{} failed: {}",
                                "✗".bright_red(),
                                agent_id,
                                error
                            );
                        }
                        MultiAgentEvent::AllCompleted {
                            results,
                            total_duration,
                        } => {
                            let success_count = results.iter().filter(|r| r.success).count();
                            println!(
                                "\n{} {}/{} agents completed in {:.2}s",
                                "Summary:".bright_cyan(),
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

            let results = self.run_task(input).await?;

            // Wait for event handler
            let _ = handle.await;

            // Print aggregated results (combines all agent outputs into a
            // single coherent summary rather than disconnected previews).
            let summary = Self::aggregate_results(&results);
            println!("\n{}", "Aggregated Result:".bright_cyan().bold());
            // Truncate long summaries for display (UTF-8 safe)
            let preview = if summary.len() > 2000 {
                let mut end = 2000;
                while end > 0 && !summary.is_char_boundary(end) {
                    end -= 1;
                }
                format!(
                    "{}...\n[{} more chars]",
                    &summary[..end],
                    summary.len() - end
                )
            } else {
                summary
            };
            println!("{}", preview);

            println!(
                "\n{} Total time: {:.2}s",
                "⏱".bright_yellow(),
                start.elapsed().as_secs_f64()
            );
        }

        Ok(())
    }

    /// Run interactive multi-agent chat with swarm coordinator orchestration.
    ///
    /// When `--coordinator` is set, the MultiChat handler routes here instead
    /// of the plain `interactive()` fan-out.  A `Swarm` is built via
    /// `create_dev_swarm()`, each user task is queued as a `SwarmTask`, the
    /// coordinator assigns it to role-matched agents, the existing per-agent
    /// execution (`run_task`) does the actual LLM work, results are fed back
    /// to the swarm via `complete_task`, and a consensus decision is created
    /// and resolved using the swarm's voting/conflict-resolution logic.
    pub async fn interactive_swarm(&mut self) -> Result<()> {
        use std::collections::HashMap;

        println!("{}", "🌐 Coordinator (Swarm) Mode".bright_cyan().bold());
        println!(
            "Swarm agents: {} | Max Concurrency: {}",
            self.config.roles.len(),
            self.config.max_concurrency
        );
        println!("Type 'exit' to quit, '/help' for commands\n");

        // Build the swarm coordinator pre-populated with dev agents
        // (Architect, Coder, Tester, Reviewer).
        let mut swarm = create_dev_swarm()
            .with_conflict_strategy(ConflictStrategy::ConfidenceWins)
            .with_consensus_threshold(0.5);

        self.initialize_agents().await?;

        loop {
            print!("{} ", "🌐 ❯".bright_green());
            io::stdout().flush()?;

            let mut input = String::new();
            if tokio::task::block_in_place(|| io::stdin().read_line(&mut input)).is_err() {
                continue;
            }

            let input = input.trim();

            if matches!(input, "exit" | "quit" | "/exit" | "/quit" | "q" | "/q") {
                break;
            }

            if input == "/help" {
                println!("Commands:");
                println!("  /help        - Show this help");
                println!("  /agents      - List swarm agents");
                println!("  /status      - Show swarm status");
                println!("  /parallel N  - Set max concurrency (1-16)");
                println!("  /clear       - Reset swarm and agents");
                println!("  exit         - Exit chat");
                continue;
            }

            if input == "/agents" {
                println!("Swarm agents:");
                for agent in swarm.list_agents() {
                    println!(
                        "  {} ({}) - {:?} | trust: {:.2} | tasks: {}",
                        agent.name,
                        agent.role.name(),
                        agent.status,
                        agent.trust_score,
                        agent.tasks_completed,
                    );
                }
                continue;
            }

            if input == "/status" {
                let stats = swarm.stats();
                println!("Swarm status:");
                println!("  Total agents: {}", stats.total_agents);
                println!("  Queued tasks: {}", stats.queued_tasks);
                println!("  Pending decisions: {}", stats.pending_decisions);
                println!("  Average trust: {:.2}", stats.average_trust);
                continue;
            }

            if input.starts_with("/parallel ") {
                if let Some(value) = input.strip_prefix("/parallel ").map(str::trim) {
                    if let Ok(n) = value.parse::<usize>() {
                        let n = n.clamp(1, MAX_CONCURRENT_AGENTS);
                        self.config.max_concurrency = n;
                        self.semaphore =
                            std::sync::Arc::new(tokio::sync::Semaphore::new(n));
                        println!("Max concurrency set to {}", n);
                    }
                }
                continue;
            }

            if input == "/clear" {
                self.initialize_agents().await?;
                {
                    let mut results = self.results.lock().await;
                    results.clear();
                }
                // Rebuild swarm with fresh agents
                swarm = create_dev_swarm()
                    .with_conflict_strategy(ConflictStrategy::ConfidenceWins)
                    .with_consensus_threshold(0.5);
                println!("Swarm and agents reset");
                continue;
            }

            if input.is_empty() {
                continue;
            }

            // --- Swarm-coordinated task execution ---

            println!("{}", "Running task via swarm coordinator...".bright_yellow());

            let start = Instant::now();

            // 1. Create a SwarmTask with the configured roles.
            let mut task = SwarmTask::new(input);
            for role in &self.config.roles {
                task = task.with_role(*role);
            }

            // 2. Queue the task in the swarm.
            swarm.queue_task(task)?;

            // 3. Pop the next task and assign to role-matched idle agents.
            let task_id = match swarm.next_task() {
                Some(id) => id,
                None => {
                    println!("  No task available from swarm queue");
                    continue;
                }
            };

            let assigned = swarm.assign_task(&task_id);
            println!(
                "  {} Swarm coordinator assigned {} agents to task",
                "📋".bright_blue(),
                assigned.len()
            );

            // 4. Execute the task using the existing per-agent execution path.
            let (tx, mut rx) = mpsc::channel::<MultiAgentEvent>(1000);
            self.event_tx = Some(tx);

            let handle = tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        MultiAgentEvent::AgentStarted { name, .. } => {
                            println!("  {} {} started", "▶".bright_blue(), name);
                        }
                        MultiAgentEvent::AgentCompleted { result, .. } => {
                            let status = if result.success {
                                "✓".bright_green()
                            } else {
                                "✗".bright_red()
                            };
                            println!(
                                "  {} {} completed in {:.2}s",
                                status,
                                result.agent_name,
                                result.duration.as_secs_f64()
                            );
                        }
                        MultiAgentEvent::AgentFailed { agent_id, error } => {
                            println!(
                                "  {} Agent-{} failed: {}",
                                "✗".bright_red(),
                                agent_id,
                                error
                            );
                        }
                        MultiAgentEvent::AllCompleted {
                            results,
                            total_duration,
                        } => {
                            let success_count =
                                results.iter().filter(|r| r.success).count();
                            println!(
                                "\n  {} {}/{} agents completed in {:.2}s",
                                "Swarm Summary:".bright_cyan(),
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

            let results = self.run_task(input).await?;
            let _ = handle.await;

            // 5. Feed results back to the swarm via complete_task.
            //    Map MultiAgentChat results to swarm agents by role.
            let role_to_agent_id: HashMap<AgentRole, String> = swarm
                .list_agents()
                .iter()
                .map(|a| (a.role, a.id.clone()))
                .collect();

            for result in &results {
                if let Some(agent_id) = role_to_agent_id.get(&result.role) {
                    swarm.complete_task(&task_id, agent_id, result.content.clone());
                }
            }

            // 6. Create a consensus decision and have agents vote.
            let successful_names: Vec<String> = results
                .iter()
                .filter(|r| r.success)
                .map(|r| r.agent_name.clone())
                .collect();

            if successful_names.len() > 1 {
                let decision_id = swarm.create_decision(
                    "Which agent's response best addresses the task?",
                    successful_names.clone(),
                );

                // Collect vote data (voter_id, choice, confidence, reasoning).
                // Each agent votes for its own response with moderate
                // confidence; the swarm's consensus threshold and conflict
                // strategy determine the final outcome.
                let vote_data: Vec<(String, String, f32, String)> = results
                    .iter()
                    .filter(|r| r.success)
                    .filter_map(|r| {
                        let voter_id = role_to_agent_id.get(&r.role)?.clone();
                        let reasoning: String = r.content.chars().take(200).collect();
                        Some((voter_id, r.agent_name.clone(), 0.7, reasoning))
                    })
                    .collect();

                for (voter_id, choice, confidence, reasoning) in &vote_data {
                    let _ = swarm.vote(
                        &decision_id,
                        voter_id,
                        choice.clone(),
                        *confidence,
                        reasoning.clone(),
                    );
                }

                // Resolve the decision via consensus threshold.
                let outcome = swarm.resolve_decision(&decision_id)?;

                // Check if it resulted in a conflict.
                let decision_status =
                    swarm.get_decision(&decision_id).map(|d| d.status);

                if decision_status == Some(DecisionStatus::Conflict) {
                    let conflict_outcome = swarm.resolve_conflict(&decision_id)?;
                    println!(
                        "  ⚡ Conflict detected — resolved via ConfidenceWins strategy: {}",
                        conflict_outcome.as_deref().unwrap_or("(human intervention needed)")
                    );
                } else {
                    println!(
                        "  ✓ Consensus reached: {}",
                        outcome.as_deref().unwrap_or("(no outcome)")
                    );
                }
            }

            // 7. Print the aggregated result.
            let summary = Self::aggregate_results(&results);
            println!("\n{}", "Aggregated Result:".bright_cyan().bold());
            let preview = if summary.len() > 2000 {
                let mut end = 2000;
                while end > 0 && !summary.is_char_boundary(end) {
                    end -= 1;
                }
                format!(
                    "{}...\n[{} more chars]",
                    &summary[..end],
                    summary.len() - end
                )
            } else {
                summary
            };
            println!("{}", preview);

            println!(
                "\n  {} Total time: {:.2}s",
                "⏱".bright_yellow(),
                start.elapsed().as_secs_f64()
            );
        }

        Ok(())
    }
}

/// Quick helper to run a task with default multi-agent config
pub async fn run_multiagent_task(
    config: &Config,
    task: &str,
    concurrency: usize,
) -> Result<Vec<crate::orchestration::multiagent::types::AgentResult>> {
    let agent_config = MultiAgentConfig::default().with_concurrency(concurrency);

    let chat = MultiAgentChat::new(config, agent_config)?;
    chat.run_task(task).await
}
