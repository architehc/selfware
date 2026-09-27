//! Multi-Agent Interactive Mode
//!
//! Interactive CLI for the multi-agent chat system.

use std::io::{self, IsTerminal, Write};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::swarm::{Agent, AgentRole, Swarm, SwarmTask, TaskStatus};

use super::chat::MultiAgentChat;
use super::config::MultiAgentConfig;
use super::types::{
    AgentInstance, AgentResult, AgentStatus, MultiAgentEvent, MAX_CONCURRENT_AGENTS,
};
use super::view::{agent_color, render_summary_table, AgentLine, MultiChatView};

/// Classification of a `stdin().read_line` result for the interactive loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StdinRead {
    /// A line (possibly empty) was read.
    Line,
    /// EOF: 0 bytes read — Ctrl-D, terminal closed, or pipe ended.
    Eof,
    /// I/O error.
    Error,
}

/// Decide what a `read_line` result means for the interactive loop.
///
/// `Ok(0)` (EOF) used to slip past the `.is_err()` check, so the loop
/// reprinted the prompt forever on a hot loop — piped stdin without a
/// trailing `exit` livelocked at full CPU. EOF must exit the loop.
fn classify_stdin_read(result: &io::Result<usize>) -> StdinRead {
    match result {
        Ok(0) => StdinRead::Eof,
        Ok(_) => StdinRead::Line,
        Err(_) => StdinRead::Error,
    }
}

/// Exit words end the session. Matched case-insensitively so `Exit`/`EXIT`
/// don't fall through and get dispatched to every agent as a paid task.
///
/// The canonical set comes from the shared
/// [`crate::input::command_registry::is_exit_command`]; multi-chat
/// additionally keeps its historical bare `q` and case-insensitivity.
fn is_exit_word(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    crate::input::command_registry::is_exit_command(&lower) || lower == "q"
}

/// Build a swarm with one agent per configured role, named like the chat
/// agents. Returns the swarm and the swarm agent IDs indexed by chat-agent
/// position, so swarm agent i always corresponds to chat agent i.
fn build_role_swarm(roles: &[AgentRole]) -> (Swarm, Vec<String>) {
    let mut swarm = Swarm::new();
    let mut ids = Vec::with_capacity(roles.len());
    for (i, role) in roles.iter().enumerate() {
        let id = swarm.add_agent(Agent::new(format!("Agent-{}-{}", i, role.name()), *role));
        ids.push(id);
    }
    (swarm, ids)
}

/// Print an honest per-agent results summary: a table of each agent's role,
/// outcome, measured duration and provider-reported tokens (`—` when the
/// provider reported none), then provider-reported cost/reasoning per agent
/// when present, errors for failed agents, and the totals.
pub fn print_agent_summary(results: &[AgentResult]) {
    println!("\n{}", "Agent Results:".bright_cyan().bold());

    for (i, line) in render_summary_table(results).iter().enumerate() {
        if i < 2 {
            println!("  {}", line.dimmed());
        } else {
            println!("  {}", line);
        }
    }

    let mut sorted: Vec<&AgentResult> = results.iter().collect();
    sorted.sort_by_key(|r| r.agent_id);

    let mut any_usage = false;
    let mut total_tokens = 0usize;
    let mut total_cost = 0.0f64;
    let mut any_cost = false;
    let mut total_reasoning = 0usize;
    let mut any_reasoning = false;

    for result in sorted {
        let tag = super::view::agent_tag(result.agent_id, result.role);
        if let Some(usage) = &result.usage {
            any_usage = true;
            total_tokens += usage.total_tokens;
            let mut details = Vec::new();
            if let Some(r) = usage.reasoning_tokens() {
                any_reasoning = true;
                total_reasoning = total_reasoning.saturating_add(r);
                // Label an estimate as one (AGENTS.md §3/§4).
                if usage.is_estimated_reasoning() {
                    details.push(format!("~{} reasoning tokens (estimated)", r));
                } else {
                    details.push(format!("{} reasoning tokens", r));
                }
            }
            if let Some(cost) = usage.cost {
                any_cost = true;
                total_cost += cost;
                details.push(format!("${:.6}", cost));
            }
            if !details.is_empty() {
                println!("  {}: {}", tag, details.join(", "));
            }
        }
        if !result.success {
            if let Some(error) = &result.error {
                println!("  {} error: {}", tag, error);
            }
        }
    }

    if any_usage {
        let reasoning_str = if any_reasoning {
            format!(" ({} reasoning)", total_reasoning)
        } else {
            String::new()
        };
        if any_cost {
            println!(
                "  Total: {} tokens{}, ${:.6}",
                total_tokens, reasoning_str, total_cost
            );
        } else {
            println!("  Total: {} tokens{}", total_tokens, reasoning_str);
        }
    }
}

/// Format complete agent lines as `[tag] text`, one per line, with the tag
/// in the agent's stable colour. Returned as one string so the caller can
/// write the whole batch at once.
fn format_tagged_lines(view: &MultiChatView, lines: &[AgentLine]) -> String {
    let mut out = String::new();
    for line in lines {
        let tag = view
            .panels()
            .iter()
            .find(|p| p.agent_id == line.agent_id)
            .map(|p| p.tag())
            .unwrap_or_else(|| format!("agent·{}", line.agent_id + 1));
        out.push_str(&format!(
            "  {} {}\n",
            format!("[{}]", tag).color(agent_color(line.agent_id)),
            line.text
        ));
    }
    out
}

/// Print multi-chat events as per-agent tagged lines until the run ends.
///
/// Every event goes through a [`MultiChatView`], which keeps a separate log
/// per agent and releases only complete lines; each batch is written with a
/// single locked write, so no two agents are ever spliced together mid-line.
/// Ends on `AllCompleted` (printing `summary_label` with the success count)
/// or when the sender is dropped.
fn spawn_tagged_event_printer(
    mut view: MultiChatView,
    mut rx: mpsc::Receiver<MultiAgentEvent>,
    summary_label: &'static str,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let lines = view.apply(&event, Instant::now());
            if !lines.is_empty() {
                let text = format_tagged_lines(&view, &lines);
                let mut out = io::stdout().lock();
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
            }
            if let MultiAgentEvent::AllCompleted {
                results,
                total_duration,
            } = &event
            {
                let success_count = results.iter().filter(|r| r.success).count();
                println!(
                    "\n{} {}/{} agents completed in {:.2}s",
                    summary_label.bright_cyan(),
                    success_count,
                    results.len(),
                    total_duration.as_secs_f64()
                );
                break;
            }
        }
    })
}

impl MultiAgentChat {
    /// Run `task` across all agents, printing each agent's events and reply
    /// as `[tag] …` lines (see `spawn_tagged_event_printer`).
    ///
    /// The event sender is dropped when the run returns — including on an
    /// early error such as the workspace-question refusal — so the printer
    /// always terminates instead of leaking a task that waits forever.
    pub async fn run_task_tagged(
        &mut self,
        task: &str,
        summary_label: &'static str,
    ) -> Result<Vec<AgentResult>> {
        if self.agents.read().await.is_empty() {
            self.initialize_agents().await?;
        }
        let view = {
            let agents = self.agents.read().await;
            MultiChatView::new(agents.iter().map(|a| (a.id, a.role, a.name.clone())))
        };
        let (tx, rx) = mpsc::channel::<MultiAgentEvent>(1000);
        self.event_tx = Some(tx);
        let handle = spawn_tagged_event_printer(view, rx, summary_label);
        let result = self.run_task(task).await;
        self.event_tx = None;
        let _ = handle.await;
        result
    }

    /// Run interactive multi-agent chat
    pub async fn interactive(&mut self) -> Result<()> {
        // Fail fast BEFORE any LLM call: on a non-terminal stdin this REPL
        // would spin on EOF (piped input without a trailing `exit` livelocked
        // at full CPU). Mirrors the Commands::Run guard in cli/mod.rs.
        if !io::stdin().is_terminal() {
            anyhow::bail!(
                "interactive multi-agent chat requires a terminal on stdin; \
                 piped or redirected input is not supported"
            );
        }

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
            let read = tokio::task::block_in_place(|| io::stdin().read_line(&mut input));
            match classify_stdin_read(&read) {
                // EOF (Ctrl-D / closed pipe): exit cleanly instead of
                // hot-looping on empty reads.
                StdinRead::Eof => break,
                StdinRead::Error => continue,
                StdinRead::Line => {}
            }

            let input = input.trim();

            if is_exit_word(input) {
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

            if input == "/parallel" || input.starts_with("/parallel ") {
                let value = input.strip_prefix("/parallel").unwrap_or("").trim();
                match value.parse::<usize>() {
                    Ok(n) => {
                        let n = n.clamp(1, MAX_CONCURRENT_AGENTS);
                        self.config.max_concurrency = n;
                        self.semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(n));
                        println!("Max concurrency set to {}", n);
                    }
                    Err(_) => println!("Usage: /parallel <1-{}>", MAX_CONCURRENT_AGENTS),
                }
                continue;
            }

            if input == "/add" || input.starts_with("/add ") {
                let role_str = input.strip_prefix("/add").unwrap_or("").trim();
                if role_str.is_empty() {
                    println!("Usage: /add <role>");
                    continue;
                }
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

            if input == "/remove" || input.starts_with("/remove ") {
                let value = input.strip_prefix("/remove").unwrap_or("").trim();
                match value.parse::<usize>() {
                    Ok(id) => {
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
                    Err(_) => println!("Usage: /remove <id>"),
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

            // Reject unrecognized slash commands instead of dispatching
            // them to every agent as a paid task.
            if input.starts_with('/') {
                println!("Unknown command: {}", input);
                println!("Type '/help' for available commands");
                continue;
            }

            // Run task across all agents
            println!("{}", "Running task across all agents...".bright_yellow());

            let start = Instant::now();

            let results = self.run_task_tagged(input, "Summary:").await?;

            // Honest per-agent results, then the aggregated output.
            print_agent_summary(&results);

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
    /// of the plain `interactive()` fan-out. A `Swarm` mirroring the chat
    /// fleet 1:1 is built; each user task is queued as a `SwarmTask` and the
    /// coordinator assigns it to role-matched idle agents. The assignment
    /// gates execution: an unassignable task makes no LLM calls. The existing
    /// per-agent execution (`run_task`) does the actual LLM work, results are
    /// fed back to the swarm with their real per-agent success flags via
    /// `complete_task` (which also returns agents to Idle for the next task),
    /// and an honest per-agent summary is printed. There is no
    /// voting/consensus step — it was removed because every agent self-voted
    /// at a fixed confidence and the "winner" was never used.
    pub async fn interactive_swarm(&mut self) -> Result<()> {
        // Same non-terminal guard as `interactive()` (see there).
        if !io::stdin().is_terminal() {
            anyhow::bail!(
                "interactive swarm chat requires a terminal on stdin; \
                 piped or redirected input is not supported"
            );
        }

        println!("{}", "🌐 Coordinator (Swarm) Mode".bright_cyan().bold());
        println!(
            "Swarm agents: {} | Max Concurrency: {}",
            self.config.roles.len(),
            self.config.max_concurrency
        );
        println!("Type 'exit' to quit, '/help' for commands\n");

        // Build the swarm 1:1 from the configured roles so swarm agent i
        // always corresponds to chat agent i. This makes the coordinator's
        // assignment authoritative: a full assignment is exactly the set of
        // agents `run_task` will execute.
        let (mut swarm, mut swarm_agent_ids) = build_role_swarm(&self.config.roles);

        self.initialize_agents().await?;

        loop {
            print!("{} ", "🌐 ❯".bright_green());
            io::stdout().flush()?;

            let mut input = String::new();
            let read = tokio::task::block_in_place(|| io::stdin().read_line(&mut input));
            match classify_stdin_read(&read) {
                // EOF (Ctrl-D / closed pipe): exit cleanly instead of
                // hot-looping on empty reads.
                StdinRead::Eof => break,
                StdinRead::Error => continue,
                StdinRead::Line => {}
            }

            let input = input.trim();

            if is_exit_word(input) {
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
                        "  {} ({}) - {:?} | trust: {:.2} | tasks: {} done / {} failed",
                        agent.name,
                        agent.role.name(),
                        agent.status,
                        agent.trust_score,
                        agent.tasks_completed,
                        agent.tasks_failed,
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

            if input == "/parallel" || input.starts_with("/parallel ") {
                let value = input.strip_prefix("/parallel").unwrap_or("").trim();
                match value.parse::<usize>() {
                    Ok(n) => {
                        let n = n.clamp(1, MAX_CONCURRENT_AGENTS);
                        self.config.max_concurrency = n;
                        self.semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(n));
                        println!("Max concurrency set to {}", n);
                    }
                    Err(_) => println!("Usage: /parallel <1-{}>", MAX_CONCURRENT_AGENTS),
                }
                continue;
            }

            if input == "/clear" {
                self.initialize_agents().await?;
                {
                    let mut results = self.results.lock().await;
                    results.clear();
                }
                // Rebuild the swarm 1:1 with fresh agents
                let (fresh_swarm, fresh_ids) = build_role_swarm(&self.config.roles);
                swarm = fresh_swarm;
                swarm_agent_ids = fresh_ids;
                println!("Swarm and agents reset");
                continue;
            }

            if input.is_empty() {
                continue;
            }

            // Reject unrecognized slash commands instead of dispatching
            // them to every agent as a paid task.
            if input.starts_with('/') {
                println!("Unknown command: {}", input);
                println!("Type '/help' for available commands");
                continue;
            }

            // --- Swarm-coordinated task execution ---

            println!(
                "{}",
                "Running task via swarm coordinator...".bright_yellow()
            );

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

            // 4. The coordinator's assignment GATES execution: when no
            //    agents are assignable, say so and make no LLM calls.
            let assigned = swarm.assign_task(&task_id);
            if assigned.is_empty() {
                println!(
                    "  {} No idle agents available for this task — not running it (no LLM calls made)",
                    "⚠".bright_yellow()
                );
                swarm.fail_task(&task_id);
                continue;
            }
            if assigned.len() < self.config.roles.len() {
                // Defensive: the swarm mirrors the chat fleet 1:1, so a
                // partial assignment should not happen. If it ever does,
                // don't run (and bill) a different set of agents than the
                // coordinator assigned.
                println!(
                    "  {} Coordinator assigned only {}/{} roles — task not run (no LLM calls made)",
                    "⚠".bright_yellow(),
                    assigned.len(),
                    self.config.roles.len()
                );
                swarm.fail_task(&task_id);
                continue;
            }
            println!(
                "  {} Swarm coordinator assigned {} agents to task",
                "📋".bright_blue(),
                assigned.len()
            );

            // 5. Execute the task using the existing per-agent execution path,
            //    streaming each agent's output as tagged lines.
            let results = self.run_task_tagged(input, "Swarm Summary:").await?;

            // 6. Feed results back to the swarm with the ACTUAL per-agent
            //    success flag so trust scores and failure counters reflect
            //    reality. `complete_task` also returns each agent to Idle so
            //    the next task can be assigned.
            for result in &results {
                if let Some(agent_id) = swarm_agent_ids.get(result.agent_id) {
                    swarm.complete_task(&task_id, agent_id, result.content.clone(), result.success);
                }
            }

            // 7. Settle the task: if any assigned agent never reported a
            //    result (e.g. cancelled mid-run), don't leave the SwarmTask
            //    stuck InProgress forever — mark it Failed and release its
            //    agents. Tasks already settled by `complete_task` (Completed,
            //    Partial, or Failed) keep their recorded outcome.
            let task_unsettled = swarm
                .get_task(&task_id)
                .map(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress))
                .unwrap_or(false);
            if task_unsettled {
                swarm.fail_task(&task_id);
            }

            // 8. Honest per-agent results (no fake consensus vote), then the
            //    aggregated output.
            print_agent_summary(&results);

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

#[cfg(test)]
#[path = "../../../tests/unit/orchestration/multiagent/interactive/interactive_test.rs"]
mod tests;
